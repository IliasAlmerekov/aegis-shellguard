use super::*;

#[test]
fn categories_for_audit_errors_are_fixed_and_path_free() {
    const PRIVATE_AUDIT_PATH: &str = "/secret/from-AEGIS_AUDIT_PATH";
    let json_error = || serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
    let cases = [
        (
            AuditError::InsecureAuditArtifact {
                path: PRIVATE_AUDIT_PATH.to_string(),
                detail: "invalid permissions".to_string(),
            },
            "insecure audit artifact",
        ),
        (
            AuditError::Io(std::io::Error::other(PRIVATE_AUDIT_PATH)),
            "audit log i/o failure",
        ),
        (
            AuditError::WriteFailed {
                path: PRIVATE_AUDIT_PATH.to_string(),
                source: std::io::Error::other(PRIVATE_AUDIT_PATH),
            },
            "audit log i/o failure",
        ),
        (
            AuditError::Parse {
                path: PRIVATE_AUDIT_PATH.to_string(),
                line: Some(3),
                source: json_error(),
            },
            "unreadable audit log line",
        ),
        (
            AuditError::Serialize {
                source: json_error(),
            },
            "audit log error",
        ),
    ];

    for (error, expected) in cases {
        let category = audit_error_category(&error);
        assert_eq!(category, expected, "unexpected category for {error:?}");
        assert!(
            !category.contains(PRIVATE_AUDIT_PATH),
            "the Diagnostic stream must not expose an audit path"
        );
    }
}
