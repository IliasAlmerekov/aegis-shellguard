use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::language::SourceLanguage;
use crate::languages::common::span_for;
use crate::operation::{
    DetectedOperation, NestedTarget, OperandCertainty, OperationKind, OperationModifiers,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecLang {
    Js,
    Bash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecArg {
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallClass {
    Op(OperationKind, OperationModifiers),
    RmSync,
    Exec(ExecLang, ExecArg),
    ExecArgv,
}

pub(crate) fn classify_path(path: &str) -> Option<CallClass> {
    Some(match path {
        "fs.unlinkSync" | "fs.rmdirSync" => CallClass::Op(
            OperationKind::FilesystemDelete,
            OperationModifiers::default(),
        ),
        "fs.rmSync" => CallClass::RmSync,
        "fs.writeFileSync" => CallClass::Op(
            OperationKind::FilesystemOverwrite,
            OperationModifiers {
                destructive_mode: true,
                ..OperationModifiers::default()
            },
        ),
        "fs.appendFileSync" => CallClass::Op(
            OperationKind::FilesystemOverwrite,
            OperationModifiers::default(),
        ),
        "fs.chmodSync" | "fs.chownSync" => CallClass::Op(
            OperationKind::PermissionOrOwnershipChange,
            OperationModifiers::default(),
        ),
        "eval" => CallClass::Exec(ExecLang::Js, ExecArg::First),
        "Function" => CallClass::Exec(ExecLang::Js, ExecArg::Last),
        "child_process.exec" | "child_process.execSync" => {
            CallClass::Exec(ExecLang::Bash, ExecArg::First)
        }
        "child_process.spawn"
        | "child_process.spawnSync"
        | "child_process.execFile"
        | "child_process.execFileSync"
        | "child_process.fork" => CallClass::ExecArgv,
        _ => return None,
    })
}

pub(crate) fn collect_operations(
    root: Node,
    bytes: &[u8],
    query: &Query,
) -> Vec<DetectedOperation> {
    let mut operations = Vec::new();
    let mut cursor = QueryCursor::new();
    let capture_names = query.capture_names();
    let mut matches = cursor.matches(query, root, bytes);
    while let Some(m) = matches.next() {
        let mut call = None;
        let mut args = None;
        let mut obj = None;
        let mut attr = None;
        let mut fname = None;
        for cap in m.captures {
            let name = capture_names[cap.index as usize];
            match name {
                "call" => call = Some(cap.node),
                "args" => args = Some(cap.node),
                "obj" => obj = Some(cap.node),
                "attr" => attr = Some(cap.node),
                "fname" => fname = Some(cap.node),
                _ => {}
            }
        }
        let (Some(call_node), Some(args_node)) = (call, args) else {
            continue;
        };

        let path = if let (Some(obj), Some(attr)) = (obj, attr) {
            format!(
                "{}.{}",
                obj.utf8_text(bytes).unwrap_or_default(),
                attr.utf8_text(bytes).unwrap_or_default()
            )
        } else if let Some(fname) = fname {
            fname.utf8_text(bytes).unwrap_or_default().to_string()
        } else {
            continue;
        };

        let Some(class) = classify_path(&path) else {
            continue;
        };

        if let Some(op) = interpret(&class, call_node, args_node, bytes) {
            operations.push(op);
        }
    }

    operations.sort_by_key(|op| (op.span.byte_start, op.span.byte_end));
    operations
}

fn interpret(
    class: &CallClass,
    call_node: Node,
    args_node: Node,
    bytes: &[u8],
) -> Option<DetectedOperation> {
    let span = span_for(call_node);
    match class {
        CallClass::Op(kind, mods) => {
            let operand = first_positional_arg(args_node);
            let certainty = operand_certainty(operand);
            Some(DetectedOperation {
                kind: *kind,
                modifiers: *mods,
                certainty,
                span,
                payload: None,
            })
        }
        CallClass::RmSync => {
            let operand = first_positional_arg(args_node);
            let certainty = operand_certainty(operand);
            let recursive = recursive_option(args_node, bytes);
            Some(DetectedOperation {
                kind: OperationKind::FilesystemDelete,
                modifiers: OperationModifiers {
                    recursive,
                    ..OperationModifiers::default()
                },
                certainty,
                span,
                payload: None,
            })
        }
        CallClass::Exec(lang, arg_pos) => {
            let operand = match arg_pos {
                ExecArg::First => first_positional_arg(args_node),
                ExecArg::Last => last_positional_arg(args_node),
            };
            let payload = operand.and_then(|n| {
                string_literal_content(n, bytes).map(|content| NestedTarget {
                    language: exec_language(*lang),
                    source: content,
                    span: span_for(n),
                })
            });
            let certainty = if payload.is_some() {
                OperandCertainty::Known
            } else {
                OperandCertainty::Dynamic
            };
            Some(DetectedOperation {
                kind: OperationKind::CodeExecution,
                modifiers: OperationModifiers::default(),
                certainty,
                span,
                payload,
            })
        }
        CallClass::ExecArgv => Some(DetectedOperation {
            kind: OperationKind::CodeExecution,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Dynamic,
            span,
            payload: None,
        }),
    }
}

fn exec_language(lang: ExecLang) -> SourceLanguage {
    match lang {
        ExecLang::Js => SourceLanguage::JavaScript,
        ExecLang::Bash => SourceLanguage::Bash,
    }
}

fn first_positional_arg(args_node: Node) -> Option<Node> {
    let mut cursor = args_node.walk();
    args_node.named_children(&mut cursor).next()
}

fn last_positional_arg(args_node: Node) -> Option<Node> {
    let mut cursor = args_node.walk();
    args_node.named_children(&mut cursor).last()
}

fn operand_certainty(arg: Option<Node>) -> OperandCertainty {
    match arg {
        Some(n) if is_string_literal(n) => OperandCertainty::Known,
        _ => OperandCertainty::Dynamic,
    }
}

fn is_string_literal(node: Node) -> bool {
    match node.kind() {
        "string" => true,
        "template_string" => {
            let mut cursor = node.walk();
            !node
                .named_children(&mut cursor)
                .any(|c| c.kind() == "template_substitution")
        }
        _ => false,
    }
}

fn string_literal_content(node: Node, bytes: &[u8]) -> Option<String> {
    match node.kind() {
        "string" => {
            let mut cursor = node.walk();
            let mut content = String::new();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "string_fragment" | "escape_sequence" => {
                        content.push_str(child.utf8_text(bytes).ok()?);
                    }
                    _ => return None,
                }
            }
            Some(content)
        }
        "template_string" => {
            let mut cursor = node.walk();
            let mut content = String::new();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "string_fragment" | "escape_sequence" => {
                        content.push_str(child.utf8_text(bytes).ok()?);
                    }
                    "template_substitution" => return None,
                    _ => return None,
                }
            }
            Some(content)
        }
        _ => None,
    }
}

fn recursive_option(args_node: Node, bytes: &[u8]) -> bool {
    let mut cursor = args_node.walk();
    for arg in args_node.named_children(&mut cursor) {
        if arg.kind() != "object" {
            continue;
        }
        let mut pair_cursor = arg.walk();
        for pair in arg.named_children(&mut pair_cursor) {
            if pair.kind() != "pair" {
                continue;
            }
            let Some(key) = pair.child_by_field_name("key") else {
                continue;
            };

            let key_is_recursive = key.utf8_text(bytes).ok() == Some("recursive")
                || (key.kind() == "string"
                    && string_literal_content(key, bytes).as_deref() == Some("recursive"));
            if !key_is_recursive {
                continue;
            }
            let Some(value) = pair.child_by_field_name("value") else {
                continue;
            };
            if value.kind() == "true" {
                return true;
            }
        }
    }
    false
}
