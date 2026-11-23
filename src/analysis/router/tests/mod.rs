use super::*;

#[test]
fn explicit_interpreter_python3_inline_body_is_detected() {
    let targets = route(r#"python3 -c "print(1)""#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(1)".to_owned(),
        }]
    );
}

#[test]
fn versioned_basename_python3_11_normalizes_to_python() {
    let targets = route(r#"python3.11 -c "print(2)""#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(2)".to_owned(),
        }]
    );
}

#[test]
fn trusted_alias_resolves_to_its_canonical_interpreter() {
    let targets = route(r#"py -c "print(3)""#, &[("py", "python3")]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(3)".to_owned(),
        }]
    );
}

#[test]
fn untrusted_program_name_yields_no_targets() {
    assert_eq!(
        route(r#"py -c "print(3)""#, &[("other-alias", "python3")]),
        Vec::new()
    );
}

#[test]
fn effective_program_resolves_through_stacked_launcher_prefixes() {
    let targets = route(r#"sudo timeout 5 python3 -c "print(4)""#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(4)".to_owned(),
        }]
    );

    let targets = route(r#"env FOO=bar python3 -c "print(5)""#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(5)".to_owned(),
        }]
    );
}

#[test]
fn exact_registry_match_wins_over_a_conflicting_trusted_alias() {
    // ADR-022 §6: explicit interpreter takes precedence over trusted aliases.
    let targets = route(r#"python3 -c "print(6)""#, &[("python3", "bash")]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(6)".to_owned(),
        }]
    );
}

#[test]
fn explicit_interpreter_with_file_argument_is_a_script_file_route() {
    let targets = route("python3 script.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn inline_flag_wins_over_a_file_argument_on_the_same_line() {
    // ADR-022 §6: inline form takes precedence over a trailing file arg.
    let targets = route(r#"python3 -c "print(7)" ignored.py"#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(7)".to_owned(),
        }]
    );
}

#[test]
fn a_file_argument_before_the_inline_flag_ends_interpreter_option_parsing() {
    // ADR-022 §6: tokens after the script file belong to the script's argv, not the interpreter.
    let targets = route(r#"python3 malicious.py -c "print(1)""#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("malicious.py"),
        }]
    );
}

#[test]
fn a_spaced_redirection_target_is_not_misread_as_the_script_file() {
    for command in [
        "python3 > out.txt malicious.py",
        "python3 >> out.txt malicious.py",
        "python3 2> out.txt malicious.py",
        "python3 < in.txt malicious.py",
    ] {
        assert_eq!(
            route(command, &[]),
            vec![RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("malicious.py"),
            }],
            "command: {command}"
        );
    }
}

#[test]
fn a_self_contained_fd_duplication_redirection_consumes_only_its_own_token() {
    for command in ["python3 2>&1 script.py", "python3 >&2 script.py"] {
        assert_eq!(
            route(command, &[]),
            vec![RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("script.py"),
            }],
            "command: {command}"
        );
    }
}

#[test]
fn a_bash_combined_redirection_target_is_not_misread_as_the_script_file() {
    for command in [
        "python3 &> out.txt malicious.py",
        "python3 &>> out.txt malicious.py",
    ] {
        assert_eq!(
            route(command, &[]),
            vec![RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("malicious.py"),
            }],
            "command: {command}"
        );
    }
}

#[test]
fn quoted_heredoc_stdin_is_routed_as_inline_source() {
    let command = "bash <<'EOF'\nrm -rf /tmp/x\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Bash,
            source: "rm -rf /tmp/x".to_owned(),
        }]
    );
}

#[test]
fn expanding_heredoc_with_substitution_degrades_dynamically() {
    let command = "bash <<EOF\necho $HOME\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![
            unresolved_dynamic(),
            RoutedTarget::Dynamic {
                language: SourceLanguage::Bash,
                reason: DegradationReason::DynamicSource,
            },
        ]
    );
}

#[test]
fn literal_here_string_is_routed_as_inline_source() {
    let targets = route(r#"python3 <<< "print(1)""#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(1)".to_owned(),
        }]
    );
}

#[test]
fn script_file_argument_before_a_heredoc_marker_wins_over_stdin() {
    let command = "python3 script.py <<EOF\nprint(1)\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn script_file_argument_before_a_here_string_marker_wins_over_stdin() {
    let command = r#"python3 script.py <<< "print(1)""#;
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn heredoc_body_word_that_looks_like_a_glued_inline_flag_is_not_misread_as_one() {
    let command = "python3 <<'EOF'\n-crm -rf /\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "-crm -rf /".to_owned(),
        }]
    );
}

#[test]
fn here_string_body_word_that_looks_like_a_glued_inline_flag_is_not_misread_as_one() {
    let command = r#"node <<< "-ealert(1)""#;
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::JavaScript,
            source: "-ealert(1)".to_owned(),
        }]
    );
}

#[test]
fn glued_flag_lookalike_is_not_misread_as_inline_body() {
    assert_eq!(route("node -e-x", &[]), Vec::new());
}

#[test]
fn printf_percent_s_literal_producer_piped_to_interpreter_is_routed() {
    let targets = route(r#"printf '%s' 'print(1)' | python3"#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(1)".to_owned(),
        }]
    );
}

#[test]
fn other_pipeline_producer_degrades_dynamically() {
    let targets = route(r#"echo "$(whoami)" | python3"#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn pipeline_into_a_non_interpreter_yields_no_targets() {
    assert_eq!(route("echo hello | grep hello", &[]), Vec::new());
}

#[test]
fn heredoc_written_to_a_file_is_reused_when_the_same_command_executes_it() {
    // ADR-022 §6: reuse the in-hand heredoc body instead of re-reading from disk.
    let command = "cat > script.py <<'EOF' && python3 script.py\nprint(1)\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(1)".to_owned(),
        }]
    );
}

#[test]
fn heredoc_written_via_tee_is_also_reused() {
    let command = "tee script.py <<'EOF' && python3 script.py\nprint(2)\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(2)".to_owned(),
        }]
    );
}

#[test]
fn heredoc_written_then_executed_with_variable_expansion_degrades_dynamically() {
    let command = "cat > script.py <<EOF && python3 script.py\necho $HOME\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn heredoc_written_then_executed_with_a_mismatched_path_is_not_reused() {
    let command = "cat > script.py <<'EOF' && python3 other.py\nprint(1)\nEOF";
    assert_eq!(
        route(command, &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("other.py"),
        }]
    );
}

#[test]
fn heredoc_written_to_a_file_with_no_chained_exec_falls_back_to_current_behavior() {
    let command = "cat > script.py <<'EOF'\nprint(1)\nEOF";
    assert_eq!(route(command, &[]), Vec::new());
}

#[test]
fn heredoc_write_then_exec_with_a_third_chained_segment_is_not_reused() {
    let command = "cat > script.py <<'EOF' && python3 script.py && rm script.py\nprint(1)\nEOF";
    assert_eq!(
        route(command, &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn heredoc_marker_glued_directly_to_the_chained_operator_is_still_recognized() {
    let command = "cat > script.py <<EOF&& python3 script.py\nprint(3)\nEOF";
    let targets = route(command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(3)".to_owned(),
        }]
    );
}

#[test]
fn verified_shebang_env_form_resolves_language() {
    assert_eq!(
        verified_shebang_language("#!/usr/bin/env python3"),
        Some(SourceLanguage::Python)
    );
}

#[test]
fn verified_shebang_direct_form_resolves_language() {
    assert_eq!(
        verified_shebang_language("#!/usr/bin/python3"),
        Some(SourceLanguage::Python)
    );
}

#[test]
fn verified_shebang_unknown_interpreter_is_not_verified() {
    assert_eq!(verified_shebang_language("#!/usr/bin/env perl"), None);
}

#[test]
fn missing_shebang_is_not_verified() {
    assert_eq!(verified_shebang_language("print(1)"), None);
}

#[tokio::test]
async fn resolve_reads_a_script_file_route() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("script.py");
    std::fs::write(&path, "print(1)\n").unwrap();

    let results = resolve(
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: path.clone(),
        }],
        1024,
    )
    .await;

    assert_eq!(
        results,
        vec![Ok(SourceTarget {
            language: SourceLanguage::Python,
            source: "print(1)\n".to_owned(),
        })]
    );
}

#[tokio::test]
async fn resolve_degrades_a_missing_script_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.py");

    let results = resolve(
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path,
        }],
        1024,
    )
    .await;

    assert_eq!(
        results,
        vec![Err(UnresolvedTarget {
            language: Some(SourceLanguage::Python),
            reason: DegradationReason::UnsafeSource,
        })]
    );
}

#[test]
fn literal_cd_dashdash_prefix_rebases_a_relative_script_file() {
    let targets = route("cd -- /tmp/proj && python3 script.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("/tmp/proj/script.py"),
        }]
    );
}

#[test]
fn cd_prefix_with_a_glob_metacharacter_degrades_a_relative_script_file() {
    let targets = route("cd -- /tmp/pro?ect && python3 script.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn literal_cd_prefix_leaves_an_absolute_script_file_unaffected() {
    let targets = route("cd -- /tmp/proj && python3 /abs/script.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("/abs/script.py"),
        }]
    );
}

#[test]
fn cd_without_dashdash_degrades_a_relative_script_file() {
    let targets = route("cd /tmp/proj && python3 script.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn cd_with_command_substitution_degrades_a_relative_script_file() {
    let targets = route("cd -- $(mktemp -d) && python3 script.py", &[]);
    assert_eq!(
        targets,
        vec![
            unresolved_dynamic(),
            RoutedTarget::Dynamic {
                language: SourceLanguage::Python,
                reason: DegradationReason::DynamicSource,
            },
        ]
    );
}

#[test]
fn dynamic_cd_preserves_degradation_for_a_relative_direct_exec() {
    // ADR-022 §6: relative DirectExec under unknown cwd must degrade untyped.
    let targets = route("cd -- $(mktemp -d) && ./script.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn direct_exec_of_a_relative_path_is_routed_pending_shebang_verification() {
    assert_eq!(
        route("./script.py", &[]),
        vec![RoutedTarget::DirectExec {
            path: PathBuf::from("./script.py"),
        }]
    );
}

#[test]
fn route_never_touches_the_filesystem_for_a_script_file_target() {
    // Behavioral pin only: a nonexistent path yields the same route as an existing one.
    assert_eq!(
        route("python3 /this/directory/does/not/exist/script.py", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("/this/directory/does/not/exist/script.py"),
        }]
    );
}

#[test]
fn route_never_touches_the_filesystem_for_a_direct_exec_target() {
    assert_eq!(
        route("/this/directory/does/not/exist/script", &[]),
        vec![RoutedTarget::DirectExec {
            path: PathBuf::from("/this/directory/does/not/exist/script"),
        }]
    );
}

#[test]
fn bare_program_name_without_a_path_is_not_a_direct_exec_candidate() {
    assert_eq!(route("myscript", &[]), Vec::new());
}

#[test]
fn direct_exec_of_a_relative_path_is_routed_through_a_launcher_prefix() {
    for command in [
        "sudo ./script.py",
        "timeout 5 ./script.py",
        "env FOO=bar ./script.py",
    ] {
        assert_eq!(
            route(command, &[]),
            vec![RoutedTarget::DirectExec {
                path: PathBuf::from("./script.py"),
            }],
            "command: {command}"
        );
    }
}

#[tokio::test]
async fn resolve_verifies_shebang_before_treating_direct_exec_as_a_target() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("script.py");
    std::fs::write(&path, "#!/usr/bin/env python3\nprint(1)\n").unwrap();

    let results = resolve(vec![RoutedTarget::DirectExec { path: path.clone() }], 1024).await;

    assert_eq!(
        results,
        vec![Ok(SourceTarget {
            language: SourceLanguage::Python,
            source: "#!/usr/bin/env python3\nprint(1)\n".to_owned(),
        })]
    );
}

#[tokio::test]
async fn resolve_drops_a_direct_exec_target_with_no_verified_shebang() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binary");
    std::fs::write(&path, b"\x7fELF-not-really-but-no-shebang").unwrap();

    let results = resolve(vec![RoutedTarget::DirectExec { path }], 1024).await;

    assert_eq!(results, Vec::new());
}

#[tokio::test]
async fn resolve_for_analysis_treats_an_oversized_direct_exec_target_as_not_applicable() {
    // Regression test for #345: compiled binaries invoked by path must not degrade.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actionlint");
    std::fs::write(&path, vec![0xffu8; 2048]).unwrap();

    let resolution = resolve_for_analysis(
        RoutedTarget::DirectExec { path },
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;

    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn resolve_for_analysis_treats_a_non_utf8_direct_exec_target_as_not_applicable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actionlint");
    std::fs::write(&path, [0x7f, 0x45, 0x4c, 0x46, 0xff, 0xfe]).unwrap();

    let resolution = resolve_for_analysis(
        RoutedTarget::DirectExec { path },
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;

    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn resolve_degrades_an_oversized_script_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.py");
    std::fs::write(&path, "x".repeat(2048)).unwrap();

    let results = resolve(
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path,
        }],
        1024,
    )
    .await;

    assert_eq!(
        results,
        vec![Err(UnresolvedTarget {
            language: Some(SourceLanguage::Python),
            reason: DegradationReason::LimitExceeded,
        })]
    );
}

/// The degradation every unclaimed-interpreter net test expects.
fn unresolved_dynamic() -> RoutedTarget {
    RoutedTarget::Unresolved {
        reason: DegradationReason::DynamicSource,
    }
}

mod alias_repeated_calls;
mod ansi_c_quoting;
mod command_string_prefix_words;
mod cwd;
mod env_launcher_shapes;
mod executor_config_keys;
mod executor_option_values;
mod glued_option_values;
mod interpreter_stdin_redirect;
mod issue_396;
mod multi_word_executor_values;
mod redirection_placement;
mod search_tool_executor_values;
mod segments;
mod shell_grammar_and_launchers;
mod unclaimed_interpreter_net;
mod unclaimed_launcher_operands;
mod wrapped;
mod wrapper_grammar;
mod wrapper_trailing_redirection;
