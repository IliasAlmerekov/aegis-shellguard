use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::LazyLock;

use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::language::SourceLanguage;
use crate::languages::common::span_for;
use crate::operation::{
    AdapterDegradation, AdapterResult, ByteSpan, DetectedOperation, NestedTarget, OperandCertainty,
    OperationKind, OperationModifiers,
};

#[expect(
    clippy::expect_used,
    reason = "a bundled query that fails to compile is a build-time bug; panicking at startup is intended"
)]
static CALLS_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &SourceLanguage::Bash.tree_sitter_language(),
        include_str!("../../queries/bash/calls.scm"),
    )
    .expect("bundled bash/calls.scm must compile against the pinned grammar")
});

thread_local! {
    #[expect(
        clippy::expect_used,
        reason = "a pinned grammar that is ABI-incompatible is a build-time bug; panicking at startup is intended"
    )]
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut parser = Parser::new();
        parser
            .set_language(&SourceLanguage::Bash.tree_sitter_language())
            .expect("pinned bash grammar is ABI-compatible with the runtime");
        parser
    });
}

#[must_use]
pub fn analyze(source: &str) -> AdapterResult {
    if !source.is_ascii() {
        return AdapterResult {
            operations: Vec::new(),
            parse_errors: 0,
            degradation: Some(AdapterDegradation::UnsupportedEncoding),
        };
    }

    let Some(tree) = PARSER.with(|cell| cell.borrow_mut().parse(source.as_bytes(), None)) else {
        return AdapterResult {
            operations: Vec::new(),
            parse_errors: 1,
            degradation: None,
        };
    };

    let parse_errors = u32::from(tree.root_node().has_error());
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let operations = collect_operations(root, bytes, &CALLS_QUERY);

    AdapterResult {
        operations,
        parse_errors,
        degradation: None,
    }
}

fn collect_operations(root: Node, bytes: &[u8], query: &Query) -> Vec<DetectedOperation> {
    let mut operations = Vec::new();
    let mut cursor = QueryCursor::new();
    let capture_names = query.capture_names();
    let mut matches = cursor.matches(query, root, bytes);
    while let Some(m) = matches.next() {
        let mut cmd = None;
        let mut name = None;
        let mut redirect = None;
        for cap in m.captures {
            match capture_names[cap.index as usize] {
                "cmd" => cmd = Some(cap.node),
                "name" => name = Some(cap.node),
                "redirect" => redirect = Some(cap.node),
                _ => {}
            }
        }
        let op = match (cmd, name) {
            (Some(cmd_node), Some(name_node)) => interpret_command(name_node, cmd_node, bytes),
            _ => redirect.and_then(interpret_redirect),
        };
        if let Some(op) = op {
            operations.push(op);
        }
    }

    operations.sort_by_key(|op| (op.span.byte_start, op.span.byte_end));
    operations
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandClass {
    Rm,
    Rmdir,
    Unlink,
    Chmod,
    Chown,
    Chgrp,
    Tee,
    Eval,
    Source,
    ShellExec,
    InterpExec(SourceLanguage),
    PathExec,
}

fn classify_command(name: &str) -> Option<CommandClass> {
    Some(match name {
        "rm" => CommandClass::Rm,
        "rmdir" => CommandClass::Rmdir,
        "unlink" => CommandClass::Unlink,
        "chmod" => CommandClass::Chmod,
        "chown" => CommandClass::Chown,
        "chgrp" => CommandClass::Chgrp,
        "tee" => CommandClass::Tee,
        "eval" => CommandClass::Eval,
        "source" | "." => CommandClass::Source,
        "bash" | "sh" | "dash" | "ash" | "zsh" | "ksh" | "mksh" => CommandClass::ShellExec,
        _ if is_python(name) => CommandClass::InterpExec(SourceLanguage::Python),
        _ if is_node(name) => CommandClass::InterpExec(SourceLanguage::JavaScript),
        _ if name.contains('/') => CommandClass::PathExec,
        _ => return None,
    })
}

fn is_python(name: &str) -> bool {
    if name == "python" {
        return true;
    }
    let Some(rest) = name.strip_prefix("python") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn is_node(name: &str) -> bool {
    name == "node"
        || name == "nodejs"
        || (name.starts_with("node") && name[4..].chars().all(|c| c.is_ascii_digit()))
}

fn interpret_command(name_node: Node, cmd_node: Node, bytes: &[u8]) -> Option<DetectedOperation> {
    let name = word_text(name_node, bytes)?;
    let class = classify_command(&name)?;
    let args = command_args(cmd_node);
    let span = span_for(cmd_node);
    match class {
        CommandClass::Rm => {
            let modifiers = rm_modifiers(&args, bytes);
            let certainty = operand_certainty(&args);
            Some(DetectedOperation {
                kind: OperationKind::FilesystemDelete,
                modifiers,
                certainty,
                span,
                payload: None,
            })
        }
        CommandClass::Rmdir | CommandClass::Unlink => Some(DetectedOperation {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers::default(),
            certainty: operand_certainty(&args),
            span,
            payload: None,
        }),
        CommandClass::Chmod | CommandClass::Chown | CommandClass::Chgrp => {
            Some(DetectedOperation {
                kind: OperationKind::PermissionOrOwnershipChange,
                modifiers: OperationModifiers::default(),
                certainty: operand_certainty(&args),
                span,
                payload: None,
            })
        }
        CommandClass::Tee => Some(DetectedOperation {
            kind: OperationKind::FilesystemOverwrite,
            modifiers: OperationModifiers {
                destructive_mode: !(has_short_flag(&args, 'a', bytes)
                    || has_long_flag(&args, "append", bytes)),
                ..OperationModifiers::default()
            },
            certainty: operand_certainty(&args),
            span,
            payload: None,
        }),
        CommandClass::Eval => {
            let payload = (args.len() == 1)
                .then(|| args.first())
                .flatten()
                .and_then(|&n| literal_payload(n, bytes, SourceLanguage::Bash));
            Some(exec_op(payload, span))
        }
        CommandClass::Source | CommandClass::PathExec => Some(DetectedOperation {
            kind: OperationKind::CodeExecution,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Dynamic,
            span,
            payload: None,
        }),
        CommandClass::ShellExec => {
            let payload = arg_after_flag(&args, &["-c"], bytes)
                .and_then(|n| literal_payload(n, bytes, SourceLanguage::Bash));
            Some(exec_op(payload, span))
        }
        CommandClass::InterpExec(lang) => {
            let flags: &[&str] = if matches!(lang, SourceLanguage::JavaScript) {
                &["-e", "--eval"]
            } else {
                &["-c"]
            };
            let payload =
                arg_after_flag(&args, flags, bytes).and_then(|n| literal_payload(n, bytes, lang));
            Some(exec_op(payload, span))
        }
    }
}

fn exec_op(payload: Option<NestedTarget>, span: ByteSpan) -> DetectedOperation {
    let certainty = if payload.is_some() {
        OperandCertainty::Known
    } else {
        OperandCertainty::Dynamic
    };
    DetectedOperation {
        kind: OperationKind::CodeExecution,
        modifiers: OperationModifiers::default(),
        certainty,
        span,
        payload,
    }
}

fn interpret_redirect(node: Node) -> Option<DetectedOperation> {
    let destructive_mode = match redirect_operator(node)? {
        ">" | ">|" | "&>" => true,
        ">>" | "&>>" => false,
        _ => return None,
    };
    let destination = node.child_by_field_name("destination")?;
    let certainty = if has_expansion(destination) {
        OperandCertainty::Dynamic
    } else {
        OperandCertainty::Known
    };
    Some(DetectedOperation {
        kind: OperationKind::FilesystemOverwrite,
        modifiers: OperationModifiers {
            destructive_mode,
            ..OperationModifiers::default()
        },
        certainty,
        span: span_for(node),
        payload: None,
    })
}

fn redirect_operator(node: Node) -> Option<&'static str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            ">" | ">>" | ">|" | "&>" | "&>>" | "<" | "<&" | ">&" | "<&-" | ">&-" => {
                return Some(child.kind());
            }
            _ => {}
        }
    }
    None
}

fn command_args(cmd_node: Node) -> Vec<Node> {
    let mut cursor = cmd_node.walk();
    cmd_node
        .children_by_field_name("argument", &mut cursor)
        .collect()
}

fn rm_modifiers(args: &[Node], bytes: &[u8]) -> OperationModifiers {
    let mut mods = OperationModifiers::default();
    for arg in args {
        let Some(text) = word_text(*arg, bytes) else {
            continue;
        };
        let text = text.as_ref();
        if text == "--recursive" {
            mods.recursive = true;
            continue;
        }
        if text == "--force" {
            mods.forced = true;
            continue;
        }
        if let Some(rest) = text.strip_prefix('-') {
            if rest.is_empty() || rest.starts_with('-') {
                continue;
            }
            for c in rest.chars() {
                match c {
                    'r' | 'R' => mods.recursive = true,
                    'f' => mods.forced = true,
                    _ => {}
                }
            }
        }
    }
    mods
}

fn has_short_flag(args: &[Node], flag: char, bytes: &[u8]) -> bool {
    for arg in args {
        let Some(text) = word_text(*arg, bytes) else {
            continue;
        };
        let Some(rest) = text.strip_prefix('-') else {
            continue;
        };
        if !rest.is_empty() && !rest.starts_with('-') && rest.contains(flag) {
            return true;
        }
    }
    false
}

fn has_long_flag(args: &[Node], flag: &str, bytes: &[u8]) -> bool {
    let needle = format!("--{flag}");
    args.iter()
        .any(|arg| word_text(*arg, bytes).is_some_and(|text| text == needle.as_str()))
}

fn arg_after_flag<'a>(args: &[Node<'a>], flags: &[&str], bytes: &[u8]) -> Option<Node<'a>> {
    for (i, arg) in args.iter().enumerate() {
        let Some(text) = word_text(*arg, bytes) else {
            continue;
        };
        if flags.contains(&text.as_ref()) {
            return args.get(i + 1).copied();
        }
    }
    None
}

fn operand_certainty(args: &[Node]) -> OperandCertainty {
    if args.iter().any(|a| has_expansion(*a)) {
        OperandCertainty::Dynamic
    } else {
        OperandCertainty::Known
    }
}

fn has_expansion(node: Node) -> bool {
    match node.kind() {
        "simple_expansion"
        | "expansion"
        | "command_substitution"
        | "process_substitution"
        | "arithmetic_expansion" => return true,
        _ => {}
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor).any(has_expansion)
}

fn literal_payload(node: Node, bytes: &[u8], lang: SourceLanguage) -> Option<NestedTarget> {
    if has_expansion(node) {
        return None;
    }
    let source = literal_content(node, bytes)?;
    Some(NestedTarget {
        language: lang,
        source,
        span: span_for(node),
    })
}

fn literal_content(node: Node, bytes: &[u8]) -> Option<String> {
    let text = node.utf8_text(bytes).ok()?;
    match node.kind() {
        "raw_string" | "string" => {
            let end = text.len().checked_sub(1)?;
            Some(text.get(1..end)?.to_string())
        }
        "ansi_c_string" => {
            let end = text.len().checked_sub(1)?;
            Some(text.get(2..end)?.to_string())
        }
        "word" => Some(text.to_string()),
        _ => None,
    }
}

fn word_text<'a>(node: Node, bytes: &'a [u8]) -> Option<Cow<'a, str>> {
    let text = node.utf8_text(bytes).ok()?;
    if !text.contains(['\\', '\'', '"', '$']) {
        return Some(Cow::Borrowed(text));
    }
    match super::bash_word::shell_word(text) {
        Some(word) => Some(Cow::Owned(word)),
        None => Some(Cow::Borrowed(text)),
    }
}

#[cfg(test)]
#[path = "bash_tests.rs"]
mod tests;
