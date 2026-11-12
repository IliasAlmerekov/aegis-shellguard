use super::*;

#[test]
fn policy_star_is_a_config_error_pointing_at_rules() {
    let home = TempDir::new().unwrap();
    let aegis_dir = home.path().join(".aegis");
    fs::create_dir_all(&aegis_dir).unwrap();
    let policy_path = aegis_dir.join("policy.star");
    fs::write(&policy_path, "").unwrap();

    let err = match RuntimeContext::new_with_policy_path(
        AegisConfig::default(),
        test_handle(),
        Some(&policy_path),
    ) {
        Ok(_) => panic!("an existing policy.star must fail closed"),
        Err(err) => err,
    };

    assert_eq!(
        err.to_string(),
        format!(
            "config error: policy.star at {} is no longer supported: Aegis reads Policy rules only from [[rules]] in aegis.toml. Move the rules there and delete the file.",
            policy_path.display()
        )
    );
    assert!(err.is_config_fault());
}

#[test]
fn policy_star_outside_a_dot_aegis_dir_is_still_a_config_error() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.star");
    fs::write(&policy_path, "").unwrap();

    let err = match RuntimeContext::new_with_policy_path(
        AegisConfig::default(),
        test_handle(),
        Some(&policy_path),
    ) {
        Ok(_) => panic!("an existing policy.star must fail closed"),
        Err(err) => err,
    };

    assert!(matches!(err, AegisError::Config(_)));
    assert!(err.is_config_fault());
    let message = err.to_string();
    assert!(message.contains(&policy_path.display().to_string()));
    assert!(message.contains("[[rules]]"));
}
