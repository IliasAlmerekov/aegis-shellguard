#![cfg(feature = "grammars")]

#[path = "common/corpus_harness.rs"]
mod corpus_harness;

use aegis_language::SourceLanguage;
use aegis_language::languages::javascript::analyze;
use aegis_language::operation::{OperandCertainty, OperationKind, OperationModifiers};
use corpus_harness::{ExpectedOp, assert_clean_no_ops, assert_malformed, assert_ops, bash_exec};

const FS_DELETE: &str = include_str!("corpora/javascript/fs_delete.js");
const FS_OVERWRITE: &str = include_str!("corpora/javascript/fs_overwrite.js");
const PERMS: &str = include_str!("corpora/javascript/perms.js");
const EXEC_SHELL: &str = include_str!("corpora/javascript/exec_shell.js");
const EXEC_JS: &str = include_str!("corpora/javascript/exec_js.js");
const NEGATIVES: &str = include_str!("corpora/javascript/negatives.js");
const DYNAMIC_OPERAND: &str = include_str!("corpora/javascript/dynamic_operand.js");
const MODERN_SYNTAX: &str = include_str!("corpora/javascript/modern_syntax.js");
const MALFORMED: &str = include_str!("corpora/javascript/malformed.js");

#[test]
fn fs_delete_emits_four_deletes_with_rmsync_recursive() {
    let expected = [
        ExpectedOp {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Known,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Known,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers {
                recursive: true,
                ..OperationModifiers::default()
            },
            certainty: OperandCertainty::Known,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers {
                recursive: true,
                ..OperationModifiers::default()
            },
            certainty: OperandCertainty::Known,
            payload: None,
        },
    ];
    assert_ops(analyze, FS_DELETE, &expected);
}

#[test]
fn fs_overwrite_emits_write_destructive_and_append() {
    let expected = [
        ExpectedOp {
            kind: OperationKind::FilesystemOverwrite,
            modifiers: OperationModifiers {
                destructive_mode: true,
                ..OperationModifiers::default()
            },
            certainty: OperandCertainty::Known,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::FilesystemOverwrite,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Known,
            payload: None,
        },
    ];
    assert_ops(analyze, FS_OVERWRITE, &expected);
}

#[test]
fn perms_emits_two_permission_or_ownership_changes() {
    let expected = [
        ExpectedOp {
            kind: OperationKind::PermissionOrOwnershipChange,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Known,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::PermissionOrOwnershipChange,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Known,
            payload: None,
        },
    ];
    assert_ops(analyze, PERMS, &expected);
}

#[test]
fn exec_shell_emits_three_code_executions_with_bash_payloads() {
    let expected = [
        bash_exec("rm -rf /tmp/x"),
        bash_exec("rm /tmp/y"),
        bash_exec("rm -rf /tmp/z"),
    ];
    assert_ops(analyze, EXEC_SHELL, &expected);
}

#[test]
fn exec_js_emits_two_code_executions_with_javascript_payloads() {
    let expected = [
        js_exec("fs.unlinkSync('x')"),
        js_exec("return fs.unlinkSync('y')"),
    ];
    assert_ops(analyze, EXEC_JS, &expected);
}

#[test]
fn negatives_emit_no_operations() {
    assert_clean_no_ops(analyze, NEGATIVES);
}

#[test]
fn dynamic_operand_emits_ops_with_dynamic_certainty_and_no_payload() {
    let expected = [
        ExpectedOp {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Dynamic,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::CodeExecution,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Dynamic,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::CodeExecution,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Dynamic,
            payload: None,
        },
        ExpectedOp {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Dynamic,
            payload: None,
        },
    ];
    assert_ops(analyze, DYNAMIC_OPERAND, &expected);
}

#[test]
fn modern_syntax_parses_cleanly_with_no_false_operations() {
    assert_clean_no_ops(analyze, MODERN_SYNTAX);
}

#[test]
fn malformed_source_records_parse_errors() {
    assert_malformed(analyze, MALFORMED);
}

fn js_exec(payload: &'static str) -> ExpectedOp {
    ExpectedOp {
        kind: OperationKind::CodeExecution,
        modifiers: OperationModifiers::default(),
        certainty: OperandCertainty::Known,
        payload: Some((SourceLanguage::JavaScript, payload)),
    }
}
