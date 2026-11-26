use super::*;

use crate::operation::{
    CERT_KNOWN, KIND_CODE_EXEC, KIND_FS_DELETE, MOD_RECURSIVE, OperandCertainty, OperationKind,
    OperationModifiers,
};

fn sample_result() -> AdapterResult {
    AdapterResult {
        parse_errors: 0,
        degradation: None,
        operations: vec![DetectedOperation {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers {
                recursive: true,
                forced: false,
                destructive_mode: false,
            },
            certainty: OperandCertainty::Known,
            span: ByteSpan {
                line: 3,
                column: 5,
                byte_start: 12,
                byte_end: 28,
            },
            payload: Some(NestedTarget {
                language: SourceLanguage::Python,
                source: "rm".to_string(),
                span: ByteSpan {
                    line: 3,
                    column: 10,
                    byte_start: 18,
                    byte_end: 20,
                },
            }),
        }],
    }
}

fn sample_result_bytes() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&0u32.to_le_bytes());
    v.push(0);
    v.extend_from_slice(&1u32.to_le_bytes());
    v.push(KIND_FS_DELETE);
    v.push(MOD_RECURSIVE);
    v.push(CERT_KNOWN);
    v.extend_from_slice(&3u32.to_le_bytes());
    v.extend_from_slice(&5u32.to_le_bytes());
    v.extend_from_slice(&12u32.to_le_bytes());
    v.extend_from_slice(&28u32.to_le_bytes());
    v.push(1);
    v.push(0x00);
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(b"rm");
    v.extend_from_slice(&3u32.to_le_bytes());
    v.extend_from_slice(&10u32.to_le_bytes());
    v.extend_from_slice(&18u32.to_le_bytes());
    v.extend_from_slice(&20u32.to_le_bytes());
    v
}

#[test]
fn encode_adapter_result_emits_the_specified_wire_format() {
    let got = encode_adapter_result(&sample_result()).expect("sample encodes");
    assert_eq!(
        got,
        sample_result_bytes(),
        "encoded AdapterResult must match the hand-derived wire format"
    );
}

#[test]
fn decode_adapter_result_recovers_the_hand_derived_bytes() {
    let got = decode_adapter_result(&sample_result_bytes()).expect("hand-derived bytes decode");
    assert_eq!(
        got,
        sample_result(),
        "decoded AdapterResult must match the hand-built literal"
    );
}

fn vocabulary_result() -> AdapterResult {
    let span = ByteSpan {
        line: 1,
        column: 1,
        byte_start: 0,
        byte_end: 4,
    };
    let payload_span = ByteSpan {
        line: 1,
        column: 6,
        byte_start: 5,
        byte_end: 7,
    };
    let py_payload = Some(NestedTarget {
        language: SourceLanguage::Python,
        source: "x".to_string(),
        span: payload_span,
    });
    let bash_payload = Some(NestedTarget {
        language: SourceLanguage::Bash,
        source: "rm x".to_string(),
        span: payload_span,
    });
    AdapterResult {
        parse_errors: 2,
        degradation: None,
        operations: vec![
            DetectedOperation {
                kind: OperationKind::FilesystemDelete,
                modifiers: OperationModifiers {
                    recursive: true,
                    forced: true,
                    destructive_mode: false,
                },
                certainty: OperandCertainty::Known,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::FilesystemOverwrite,
                modifiers: OperationModifiers {
                    recursive: false,
                    forced: false,
                    destructive_mode: true,
                },
                certainty: OperandCertainty::Partial,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::PermissionOrOwnershipChange,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Dynamic,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::DeviceOrCriticalWrite,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Known,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::DatabaseDestructive,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Known,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::CodeExecution,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Known,
                span,
                payload: py_payload.clone(),
            },
            DetectedOperation {
                kind: OperationKind::CodeExecution,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Dynamic,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::CloudDestructive,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Known,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::ContainerDestructive,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Known,
                span,
                payload: None,
            },
            DetectedOperation {
                kind: OperationKind::PackageDestructive,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Known,
                span,
                payload: bash_payload.clone(),
            },
        ],
    }
}

#[test]
fn encode_then_decode_round_trips_the_full_vocabulary() {
    let original = vocabulary_result();
    let encoded = encode_adapter_result(&original).expect("vocabulary encodes");
    let decoded = decode_adapter_result(&encoded).expect("vocabulary decodes");
    assert_eq!(
        decoded, original,
        "every kind, certainty, modifier, and payload case must round-trip"
    );
}

#[test]
fn encode_then_decode_round_trips_unsupported_encoding_degradation() {
    let original = AdapterResult {
        operations: Vec::new(),
        parse_errors: 0,
        degradation: Some(AdapterDegradation::UnsupportedEncoding),
    };
    let encoded = encode_adapter_result(&original).expect("degradation encodes");
    let decoded = decode_adapter_result(&encoded).expect("degradation decodes");
    assert_eq!(decoded, original);
}

#[test]
fn decode_adapter_result_rejects_an_empty_buffer() {
    assert_eq!(
        decode_adapter_result(&[]),
        Err(DecodeError::InvalidPayload("parse_errors")),
        "a buffer missing even the parse_errors field must be rejected"
    );
}

#[test]
fn decode_adapter_result_rejects_an_unknown_degradation_tag() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0xFF);
    assert_eq!(
        decode_adapter_result(&buf),
        Err(DecodeError::InvalidPayload("unknown adapter degradation"))
    );
}

#[test]
fn decode_adapter_result_rejects_a_truncated_operation() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.push(KIND_FS_DELETE);
    assert_eq!(
        decode_adapter_result(&buf),
        Err(DecodeError::InvalidPayload("modifiers")),
        "an operation missing its remaining fields must be rejected, not panic"
    );
}

#[test]
fn decode_adapter_result_rejects_an_unknown_kind_discriminant() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.push(0xFF);
    buf.push(0);
    buf.push(CERT_KNOWN);
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    assert_eq!(
        decode_adapter_result(&buf),
        Err(DecodeError::InvalidPayload(
            "unknown operation kind discriminant"
        )),
        "an unknown kind discriminant must be rejected"
    );
}

#[test]
fn decode_adapter_result_rejects_an_unknown_certainty_discriminant() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.push(KIND_FS_DELETE);
    buf.push(0);
    buf.push(0xFF);
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    assert_eq!(
        decode_adapter_result(&buf),
        Err(DecodeError::InvalidPayload(
            "unknown certainty discriminant"
        )),
        "an unknown certainty discriminant must be rejected"
    );
}

#[test]
fn decode_adapter_result_rejects_an_invalid_has_payload_flag() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.push(KIND_FS_DELETE);
    buf.push(0);
    buf.push(CERT_KNOWN);
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(2);
    assert_eq!(
        decode_adapter_result(&buf),
        Err(DecodeError::InvalidPayload("has_payload must be 0 or 1")),
        "a has_payload flag other than 0/1 must be rejected"
    );
}

#[test]
fn decode_adapter_result_rejects_an_unknown_payload_language_tag() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.push(KIND_CODE_EXEC);
    buf.push(0);
    buf.push(CERT_KNOWN);
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(1);
    buf.push(0xFF);
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_adapter_result(&buf),
        Err(DecodeError::InvalidPayload("unknown payload language tag")),
        "an unknown payload language tag must be rejected"
    );
}

#[test]
fn decode_adapter_result_rejects_a_payload_source_len_exceeding_remaining_bytes() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(0);
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.push(KIND_CODE_EXEC);
    buf.push(0);
    buf.push(CERT_KNOWN);
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(1);
    buf.push(0x00);
    buf.extend_from_slice(&100u32.to_le_bytes());
    assert_eq!(
        decode_adapter_result(&buf),
        Err(DecodeError::InvalidPayload("payload.source")),
        "a source length exceeding the remaining bytes must be rejected"
    );
}

#[test]
fn decode_adapter_result_rejects_trailing_bytes() {
    let mut encoded = encode_adapter_result(&sample_result()).expect("sample encodes");
    encoded.push(0xFF);
    assert_eq!(
        decode_adapter_result(&encoded),
        Err(DecodeError::InvalidPayload(
            "trailing bytes after AdapterResult"
        )),
        "trailing bytes after the declared operations must be rejected"
    );
}
