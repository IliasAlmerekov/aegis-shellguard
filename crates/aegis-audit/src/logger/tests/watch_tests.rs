use super::*;

#[test]
fn watch_context_fields_round_trip_through_json() {
    let entry = AuditEntry::new(
        "git status",
        RiskLevel::Safe,
        vec![],
        Decision::AutoApproved,
        vec![],
        None,
        None,
    )
    .with_watch_context(
        Some("claude".to_string()),
        Some("/home/user/project".to_string()),
        Some("frame-42".to_string()),
    );

    let json = serde_json::to_string(&entry).unwrap();
    let back: AuditEntry = serde_json::from_str(&json).unwrap();

    let watch = match &back {
        AuditEntry::Watch(w) => w,
        _ => panic!("expected Watch variant after round-trip"),
    };
    assert_eq!(watch.source.as_deref(), Some("claude"));
    assert_eq!(back.as_base().cwd.as_deref(), Some("/home/user/project"));
    assert_eq!(watch.id.as_deref(), Some("frame-42"));
    assert!(json.contains(r#""transport":"watch""#));
}

#[test]
fn watch_context_fields_absent_when_not_set() {
    let entry = AuditEntry::new(
        "ls",
        RiskLevel::Safe,
        vec![],
        Decision::AutoApproved,
        vec![],
        None,
        None,
    );

    let json = serde_json::to_string(&entry).unwrap();
    assert!(!json.contains("source"), "source must be absent when None");
    assert!(
        !json.contains("transport"),
        "transport must be absent when None"
    );
}

#[test]
fn shell_wrapper_entry_records_cwd_without_becoming_a_watch_entry() {
    let entry = AuditEntry::new(
        "ls",
        RiskLevel::Safe,
        vec![],
        Decision::AutoApproved,
        vec![],
        None,
        None,
    )
    .with_cwd(Some("/home/user/repo".to_string()));

    let json = serde_json::to_string(&entry).unwrap();
    let back: AuditEntry = serde_json::from_str(&json).unwrap();

    assert!(matches!(back, AuditEntry::Decision(_)), "{json}");
    assert_eq!(back.as_base().cwd.as_deref(), Some("/home/user/repo"));
    assert!(json.contains(r#""cwd":"/home/user/repo""#));
}

/// Hashes produced by the code before #499, with `cwd` on watch entries only
/// and no `git_target`. Legacy lines must keep verifying (ADR-053).
#[test]
fn legacy_entries_keep_their_pre_499_integrity_hash() {
    let decision = entry(1, RiskLevel::Danger);
    assert_eq!(
        compute_entry_hash(&decision, None).unwrap(),
        "e16c88f784130538362ef2b3c404abc61bf5741865bac7fbd8ce6a74d4d4974a"
    );
    let watch = entry(2, RiskLevel::Danger).with_watch_context(
        Some("claude".into()),
        Some("/w/repo".into()),
        Some("f1".into()),
    );
    assert_eq!(
        compute_entry_hash(&watch, None).unwrap(),
        "e3cf9109d20d6f3bb6c2056450a669438353dedcb8737ff55db4dedf78f1a53f"
    );
}

#[test]
fn recorded_cwd_is_covered_by_the_integrity_hash() {
    let plain = entry(1, RiskLevel::Danger);
    let located = entry(1, RiskLevel::Danger).with_cwd(Some("/repo".to_string()));
    assert_ne!(
        compute_entry_hash(&plain, None).unwrap(),
        compute_entry_hash(&located, None).unwrap()
    );
}

#[test]
fn git_target_round_trips_and_is_absent_when_none() {
    let with = AuditSnapshot {
        plugin: "git".into(),
        snapshot_id: "id".into(),
        git_target: Some("/repo".into()),
    };
    let json = serde_json::to_string(&with).unwrap();
    assert!(json.contains(r#""git_target":"/repo""#));
    assert_eq!(serde_json::from_str::<AuditSnapshot>(&json).unwrap(), with);

    let without = AuditSnapshot {
        git_target: None,
        ..with
    };
    let json = serde_json::to_string(&without).unwrap();
    assert_eq!(json, r#"{"plugin":"git","snapshot_id":"id"}"#);
}
