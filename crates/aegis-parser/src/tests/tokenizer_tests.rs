use super::super::*;

// 1. Simple command — space-separated words
#[test]
fn simple_command() {
    assert_eq!(split_tokens("echo hello"), vec!["echo", "hello"]);
}

// 2. Multiple arguments
#[test]
fn multiple_args() {
    assert_eq!(split_tokens("ls -la /tmp"), vec!["ls", "-la", "/tmp"]);
}

// 3. Single-quoted string becomes one token
#[test]
fn single_quoted_token() {
    assert_eq!(
        split_tokens("echo 'hello world'"),
        vec!["echo", "hello world"]
    );
}

// 4. Dangerous path in single quotes is one token
#[test]
fn single_quoted_dangerous_path() {
    assert_eq!(split_tokens("'rm -rf /'"), vec!["rm -rf /"]);
}

// 5. Double-quoted string becomes one token
#[test]
fn double_quoted_token() {
    assert_eq!(
        split_tokens(r#"echo "hello world""#),
        vec!["echo", "hello world"]
    );
}

// 6. && is a separator token
#[test]
fn double_ampersand_separator() {
    assert_eq!(split_tokens("cmd1 && cmd2"), vec!["cmd1", "&&", "cmd2"]);
}

// 7. Semicolon is a separator token
#[test]
fn semicolon_separator() {
    assert_eq!(split_tokens("cmd1; cmd2"), vec!["cmd1", ";", "cmd2"]);
}

// 8. Semicolon without surrounding spaces
#[test]
fn semicolon_no_spaces() {
    assert_eq!(split_tokens("cmd1;cmd2"), vec!["cmd1", ";", "cmd2"]);
}

// 9. Pipe is a separator token
#[test]
fn pipe_separator() {
    assert_eq!(
        split_tokens("ls | grep foo"),
        vec!["ls", "|", "grep", "foo"]
    );
}

// 10. || is a separator token
#[test]
fn double_pipe_separator() {
    assert_eq!(split_tokens("cmd1 || cmd2"), vec!["cmd1", "||", "cmd2"]);
}

// 11. Backslash escapes a space (keeps words together)
#[test]
fn backslash_escaped_space() {
    assert_eq!(split_tokens(r"rm\ -rf\ /"), vec!["rm -rf /"]);
}

// 12. && inside double quotes is NOT a separator
#[test]
fn ampersand_inside_double_quotes() {
    assert_eq!(
        split_tokens(r#"bash -c "cmd1 && cmd2""#),
        vec!["bash", "-c", "cmd1 && cmd2"]
    );
}

// 13. Single quote inside double quotes is literal
#[test]
fn single_quote_inside_double_quotes() {
    assert_eq!(split_tokens(r#""it's fine""#), vec!["it's fine"]);
}

// 14. Double quote inside single quotes is literal
#[test]
fn double_quote_inside_single_quotes() {
    assert_eq!(split_tokens(r#"'say "hi"'"#), vec![r#"say "hi""#]);
}

// 15. Empty string produces no tokens
#[test]
fn empty_input() {
    assert_eq!(split_tokens(""), Vec::<String>::new());
}

// --- T2.2: extract_nested_commands ---

// 16. bash -c 'cmd' — single-quoted inner command
#[test]
fn nested_bash_single_quote() {
    assert_eq!(
        extract_nested_commands("bash -c 'echo hello'"),
        vec!["echo hello"]
    );
}

// 17. sh -c 'cmd' — sh variant is recognized
#[test]
fn nested_sh_single_quote() {
    assert_eq!(
        extract_nested_commands("sh -c 'echo hello'"),
        vec!["echo hello"]
    );
}

// 18. bash -c "cmd1 && cmd2" — && splits into two commands
#[test]
fn nested_double_quote_and_separator() {
    assert_eq!(
        extract_nested_commands(r#"bash -c "cmd1 && cmd2""#),
        vec!["cmd1", "cmd2"]
    );
}

// 19. bash -c "cmd1; cmd2; cmd3" — semicolons produce three commands
#[test]
fn nested_semicolons_three_cmds() {
    assert_eq!(
        extract_nested_commands(r#"bash -c "cmd1; cmd2; cmd3""#),
        vec!["cmd1", "cmd2", "cmd3"]
    );
}

// 20. bash -c $'cmd1\ncmd2' — ANSI-C $'...' quoting: \n becomes a newline separator
#[test]
fn nested_ansi_c_newline() {
    // In the Rust string literal "\\n" is a literal backslash-n, mirroring what the
    // shell passes when $'...' quoting embeds a \n escape.
    assert_eq!(
        extract_nested_commands("bash -c $'cmd1\\ncmd2'"),
        vec!["cmd1", "cmd2"]
    );
}

// 21. env VAR=val bash -c '...' — env prefix with one assignment
#[test]
fn nested_env_prefix_single_var() {
    assert_eq!(
        extract_nested_commands("env VAR=val bash -c 'echo hello'"),
        vec!["echo hello"]
    );
}

// 22. env A=1 B=2 bash -c '...' — env prefix with multiple assignments
#[test]
fn nested_env_prefix_multi_var() {
    assert_eq!(
        extract_nested_commands("env A=1 B=2 bash -c 'cmd'"),
        vec!["cmd"]
    );
}

// 23. bash -c 'bash -c "inner cmd"' — recursive unwrap preserves argument grouping
#[test]
fn nested_recursive_unwrap() {
    assert_eq!(
        extract_nested_commands(r#"bash -c 'bash -c "inner cmd"'"#),
        vec!["inner cmd"]
    );
}

// 24. python -c '...' — non-shell interpreter returns empty vec
#[test]
fn nested_non_shell_returns_empty() {
    assert_eq!(
        extract_nested_commands(r#"python -c 'print("hi")'"#),
        Vec::<String>::new()
    );
}

// 25. bash -c "cmd1 || cmd2" — || splits into two commands
#[test]
fn nested_double_pipe_separator() {
    assert_eq!(
        extract_nested_commands(r#"bash -c "cmd1 || cmd2""#),
        vec!["cmd1", "cmd2"]
    );
}

#[test]
fn top_level_pipelines_preserve_adjacent_pipeline_stages() {
    let pipelines = super::segmentation::top_level_pipelines("printf x | xargs rm -f | cat");
    assert_eq!(pipelines.len(), 1);
    assert_eq!(pipelines[0].segments.len(), 3);
    assert_eq!(pipelines[0].segments[1].normalized, "xargs rm -f");
}

#[test]
fn extract_nested_commands_unwraps_env_prefixed_shell_c() {
    assert_eq!(
        super::nested_shells::extract_nested_commands(
            "env FOO=bar bash -lc 'echo one && echo two'"
        ),
        vec!["echo one", "echo two"]
    );
}

#[test]
fn split_tokens_preserves_separator_tokens() {
    assert_eq!(
        split_tokens("echo hi && rm -rf /tmp/demo | cat"),
        vec!["echo", "hi", "&&", "rm", "-rf", "/tmp/demo", "|", "cat"]
    );
}

#[test]
fn parse_preserves_first_command_shape() {
    let parsed = Parser::parse("FOO=bar bash -c 'echo hi'");
    assert_eq!(parsed.program.as_deref(), Some("FOO=bar"));
    assert_eq!(parsed.raw, "FOO=bar bash -c 'echo hi'");
}

#[test]
fn extract_inline_scripts_preserves_python_c_payload() {
    let scripts = super::embedded_scripts::extract_inline_scripts("python -c 'print(1)'");
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "python");
    assert_eq!(scripts[0].body, "print(1)");
}

#[test]
fn extract_process_substitution_bodies_preserves_nested_command() {
    assert_eq!(
        super::embedded_scripts::extract_process_substitution_bodies(
            "diff <(git status) <(git diff)"
        ),
        vec!["git status", "git diff"]
    );
}

// ── T2.3: heredoc and inline script tests ────────────────────────────────

// 26. Basic heredoc — body between <<EOF and EOF is extracted
#[test]
fn heredoc_basic_body() {
    let cmd = "cat <<EOF\nrm -rf /\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    assert_eq!(bodies[0].body, "rm -rf /");
    assert!(!bodies[0].is_nowdoc);
}

// 27. Nowdoc — <<'EOF' is flagged as nowdoc, body is still extracted
#[test]
fn heredoc_nowdoc_flag() {
    let cmd = "cat <<'EOF'\nsome secret\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].body, "some secret");
    assert!(bodies[0].is_nowdoc);
}

// 28. Multi-line heredoc body — all lines joined with newline
#[test]
fn heredoc_multiline_body() {
    let cmd = "bash <<SCRIPT\necho hello\nrm -rf /tmp/foo\nSCRIPT";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].body, "echo hello\nrm -rf /tmp/foo");
}

// 29. Heredoc with tab-stripping (<<-) — leading tabs removed from delimiter line
#[test]
fn heredoc_strip_tabs_delimiter() {
    // The closing delimiter has a leading tab that should be stripped.
    let cmd = "cat <<-EOF\n\trm -rf /\n\tEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    // Body line is kept as-is (only the delimiter line is stripped for matching).
    assert_eq!(bodies[0].body, "\trm -rf /");
}

// 30. No heredoc — empty vec returned
#[test]
fn heredoc_no_match_returns_empty() {
    let bodies = extract_heredoc_bodies("echo hello world");
    assert!(bodies.is_empty());
}

// 31. Double-quoted delimiter (`<<"EOF"`) is nowdoc, same as single-quoted.
#[test]
fn heredoc_double_quoted_delimiter_is_nowdoc() {
    let cmd = "cat <<\"EOF\"\nsome secret\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    assert_eq!(bodies[0].body, "some secret");
    assert!(bodies[0].is_nowdoc);
}

// 32. Backslash-escaped delimiter (`<<\EOF`) is nowdoc.
#[test]
fn heredoc_backslash_escaped_delimiter_is_nowdoc() {
    let cmd = "cat <<\\EOF\nsome secret\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    assert!(bodies[0].is_nowdoc);
}

// 33. Nowdoc handed to a passive consumer (`cat`) is not interpreter-executed.
#[test]
fn heredoc_nowdoc_to_cat_is_not_interpreter_target() {
    let cmd = "cat <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].target_is_interpreter);
}

// 34. Nowdoc handed to a shell interpreter is still interpreter-executed —
// the interpreter runs the raw text as code regardless of quoting.
#[test]
fn heredoc_nowdoc_to_bash_is_interpreter_target() {
    let cmd = "bash <<'EOF'\necho hi\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_is_interpreter);
}

// 35. Nowdoc handed to python3 is also an interpreter target.
#[test]
fn heredoc_nowdoc_to_python3_is_interpreter_target() {
    let cmd = "python3 <<'EOF'\nprint(1)\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_is_interpreter);
}

// 35b. Interpreter-target detection is case-insensitive, matching the rest
// of this codebase's launcher/program matching (e.g. `eq_ignore_ascii_case`
// in `effective_token_slices`'s launcher checks).
#[test]
fn heredoc_nowdoc_to_uppercase_interpreter_is_interpreter_target() {
    let cmd = "Bash <<'EOF'\necho hi\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_is_interpreter);
}

// 36. Masking neutralizes backticks inside a nowdoc body sent to a passive
// consumer (`cat`) — issue #344's exact repro shape.
#[test]
fn mask_inert_heredoc_neutralizes_backticks_for_cat() {
    let cmd = "cat <<'EOF'\nmentions `python3 -c` as inert markdown code text\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert!(!masked.contains('`'));
}

// 37. Masking leaves a nowdoc body untouched when the target is an
// interpreter — it will execute the raw text regardless of quoting.
#[test]
fn mask_inert_heredoc_leaves_interpreter_target_untouched() {
    let cmd = "bash <<'EOF'\necho `date`\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert_eq!(masked, cmd);
}

// 38. Masking leaves a regular (non-nowdoc) heredoc untouched — bash does
// expand backticks there, so it's a live scan target.
#[test]
fn mask_inert_heredoc_leaves_non_nowdoc_untouched() {
    let cmd = "cat <<EOF\n`rm -rf /`\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert_eq!(masked, cmd);
}

// 39. Masking is a no-op on commands with no heredoc at all.
#[test]
fn mask_inert_heredoc_no_heredoc_is_noop() {
    let cmd = "echo `date`";
    assert_eq!(mask_inert_heredoc_substitution_markers(cmd), cmd);
}

// 40. Issue #357: a heredoc handed to `cat` whose stdout is redirected into
// a file is a data write, not a display or execution — the target is
// flagged accordingly.
#[test]
fn heredoc_cat_redirected_to_file_is_flagged() {
    let cmd = "cat >> notes.txt <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_redirects_to_file);
}

// 41. `cat` with no output redirection prints to the terminal — not a file
// write.
#[test]
fn heredoc_cat_without_redirection_is_not_flagged() {
    let cmd = "cat <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].target_redirects_to_file);
}

// 42. `tee` writes to its file argument directly, no shell redirection
// needed.
#[test]
fn heredoc_tee_with_file_argument_is_flagged() {
    let cmd = "tee out.txt <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_redirects_to_file);
}

// 43. An interpreter's stdout being redirected to a file doesn't change what
// it does with its stdin — it still executes the body.
#[test]
fn heredoc_interpreter_redirected_to_file_is_not_flagged_as_file_write() {
    let cmd = "bash > log.txt <<'EOF'\necho hi\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].target_redirects_to_file);
}

// 43b. Descriptor duplication (`2>&1`, `>&2`) never touches a file — the
// bare `>` inside it must not be mistaken for a stdout-to-file redirect.
#[test]
fn heredoc_cat_with_fd_duplication_is_not_flagged() {
    let cases = [
        "cat 2>&1 <<'EOF'\nsome text\nEOF",
        "cat >&2 <<'EOF'\nsome text\nEOF",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(
            !bodies[0].target_redirects_to_file,
            "command {cmd:?}: expected target_redirects_to_file to be false"
        );
    }
}

// 44. Masking blanks the entire nowdoc body — not just substitution markers
// — when its target writes it straight to a file.
#[test]
fn mask_inert_heredoc_blanks_full_body_for_file_redirected_cat() {
    let cmd = "cat >> notes.txt <<'EOF'\nrm -rf /\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert!(!masked.contains("rm -rf"));
    // Line structure (line count) is preserved so unrelated line-oriented
    // logic downstream isn't affected.
    assert_eq!(masked.lines().count(), cmd.lines().count());
}

// 45. Masking still only blanks substitution markers, not the whole body,
// when the same nowdoc target has no output redirection.
#[test]
fn mask_inert_heredoc_keeps_literal_text_without_redirection() {
    let cmd = "cat <<'EOF'\nrm -rf /\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert!(masked.contains("rm -rf"));
}

// 31. python -c "..." — inline Python script extracted
#[test]
fn inline_script_python() {
    let scripts = extract_inline_scripts(r#"python3 -c "import os; os.system('cmd')""#);
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "python3");
    assert_eq!(scripts[0].body, "import os; os.system('cmd')");
}

// 32. node -e "..." — inline Node.js script extracted
#[test]
fn inline_script_node() {
    let scripts = extract_inline_scripts(r#"node -e "process.exit(1)""#);
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "node");
    assert_eq!(scripts[0].body, "process.exit(1)");
}

// 33. ruby -e "..." — inline Ruby script extracted
#[test]
fn inline_script_ruby() {
    let scripts = extract_inline_scripts(r#"ruby -e "system('rm -rf /')""#);
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "ruby");
    assert_eq!(scripts[0].body, "system('rm -rf /')");
}

#[test]
fn inline_script_php() {
    let scripts = extract_inline_scripts(r#"php -r "system('rm -rf /')""#);
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "php");
    assert_eq!(scripts[0].body, "system('rm -rf /')");
}

#[test]
fn inline_script_lua() {
    let scripts = extract_inline_scripts(r#"lua -e "os.execute('rm -rf /')""#);
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "lua");
    assert_eq!(scripts[0].body, "os.execute('rm -rf /')");
}

#[test]
fn inline_script_stays_within_logical_segment_boundary() {
    let scripts = extract_inline_scripts(r#"ruby puts(1) && node -e "process.exit(1)""#);
    assert_eq!(
        scripts,
        vec![InlineScript {
            interpreter: "node".to_string(),
            body: "process.exit(1)".to_string(),
        }]
    );
}

#[test]
fn process_substitution_body_basic() {
    let bodies = extract_process_substitution_bodies(r#"source <(python3 -c "print(42)")"#);
    assert_eq!(bodies, vec![r#"python3 -c "print(42)""#]);
}

#[test]
fn process_substitution_body_nested_parens() {
    let bodies = extract_process_substitution_bodies(r#"bash <(printf '%s\n' "$(echo hi)")"#);
    assert_eq!(bodies, vec![r#"printf '%s\n' "$(echo hi)""#]);
}

#[test]
fn eval_payload_literal_string() {
    let payloads = extract_eval_payloads(r#"eval "python3 -c 'print(42)'"#);
    assert_eq!(payloads, vec![r#"python3 -c 'print(42)'"#]);
}

#[test]
fn eval_payload_with_env_prefix() {
    let payloads = extract_eval_payloads(r#"FOO=bar eval "$DEPLOY_CMD""#);
    assert_eq!(payloads, vec![r#"$DEPLOY_CMD"#]);
}

// ── C2: literal `$IFS` / `${IFS}` obfuscation ─────────────────────────────
//
// Unquoted literal `$IFS` and `${IFS}` expand to shell word-separators, so a
// destructive command split by them must tokenize like the whitespace form.
// Quoted, escaped, and non-IFS variable forms must remain untouched.
mod ifs_obfuscation {
    use super::*;

    #[test]
    fn split_tokens_treats_unquoted_dollar_ifs_as_separator() {
        assert_eq!(split_tokens("rm$IFS-rf$IFS/"), vec!["rm", "-rf", "/"]);
    }

    #[test]
    fn split_tokens_treats_unquoted_braced_ifs_as_separator() {
        assert_eq!(split_tokens("rm${IFS}-rf${IFS}/"), vec!["rm", "-rf", "/"]);
    }

    #[test]
    fn split_tokens_treats_ifs_between_program_and_argument_as_separator() {
        assert_eq!(
            split_tokens("dd${IFS}of=/dev/sda"),
            vec!["dd", "of=/dev/sda"]
        );
    }

    #[test]
    fn split_tokens_treats_leading_dollar_ifs_without_emitting_empty_token() {
        assert_eq!(split_tokens("$IFS-rf"), vec!["-rf"]);
    }

    #[test]
    fn split_tokens_treats_trailing_dollar_ifs_without_emitting_empty_token() {
        assert_eq!(split_tokens("rm$IFS"), vec!["rm"]);
    }

    #[test]
    fn split_tokens_collapses_repeated_dollar_ifs_runs() {
        assert_eq!(split_tokens("rm$IFS$IFS-rf"), vec!["rm", "-rf"]);
    }

    #[test]
    fn split_tokens_does_not_split_escaped_dollar_ifs() {
        assert_eq!(split_tokens(r"echo \$IFS"), vec!["echo", "$IFS"]);
    }

    #[test]
    fn split_tokens_does_not_split_single_quoted_dollar_ifs() {
        assert_eq!(split_tokens("echo '$IFS'"), vec!["echo", "$IFS"]);
    }

    #[test]
    fn split_tokens_does_not_split_double_quoted_dollar_ifs() {
        assert_eq!(split_tokens("echo \"$IFS\""), vec!["echo", "$IFS"]);
    }

    #[test]
    fn split_tokens_does_not_split_non_ifs_variables() {
        assert_eq!(split_tokens("echo$PATH/test"), vec!["echo$PATH/test"]);
    }

    #[test]
    fn split_tokens_ignores_partial_ifs_prefixes() {
        assert_eq!(split_tokens("echo$IF"), vec!["echo$IF"]);
    }

    #[test]
    fn split_tokens_ignores_partial_braced_ifs() {
        assert_eq!(split_tokens("echo${IFS"), vec!["echo${IFS"]);
    }

    #[test]
    fn split_tokens_does_not_split_ifs_prefixed_identifier() {
        // `$IFSHOME` references a different variable, not `$IFS`.
        assert_eq!(split_tokens("echo$IFSHOME"), vec!["echo$IFSHOME"]);
    }

    #[test]
    fn split_tokens_does_not_split_ifs_prefixed_by_digit() {
        // `$IFS2` is the identifier `IFS2`, so the digit extends the name.
        assert_eq!(split_tokens("echo$IFS2"), vec!["echo$IFS2"]);
    }

    #[test]
    fn split_tokens_splits_bare_ifs_before_punctuation() {
        // A `.` ends the variable name, so `$IFS` is a separator before `.txt`.
        assert_eq!(split_tokens("cat$IFS.txt"), vec!["cat", ".txt"]);
    }
}
