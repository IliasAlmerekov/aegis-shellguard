use super::super::*;

#[test]
fn simple_command() {
    assert_eq!(split_tokens("echo hello"), vec!["echo", "hello"]);
}

#[test]
fn multiple_args() {
    assert_eq!(split_tokens("ls -la /tmp"), vec!["ls", "-la", "/tmp"]);
}

#[test]
fn single_quoted_token() {
    assert_eq!(
        split_tokens("echo 'hello world'"),
        vec!["echo", "hello world"]
    );
}

#[test]
fn single_quoted_dangerous_path() {
    assert_eq!(split_tokens("'rm -rf /'"), vec!["rm -rf /"]);
}

#[test]
fn double_quoted_token() {
    assert_eq!(
        split_tokens(r#"echo "hello world""#),
        vec!["echo", "hello world"]
    );
}

#[test]
fn double_ampersand_separator() {
    assert_eq!(split_tokens("cmd1 && cmd2"), vec!["cmd1", "&&", "cmd2"]);
}

#[test]
fn semicolon_separator() {
    assert_eq!(split_tokens("cmd1; cmd2"), vec!["cmd1", ";", "cmd2"]);
}

#[test]
fn semicolon_no_spaces() {
    assert_eq!(split_tokens("cmd1;cmd2"), vec!["cmd1", ";", "cmd2"]);
}

#[test]
fn pipe_separator() {
    assert_eq!(
        split_tokens("ls | grep foo"),
        vec!["ls", "|", "grep", "foo"]
    );
}

#[test]
fn double_pipe_separator() {
    assert_eq!(split_tokens("cmd1 || cmd2"), vec!["cmd1", "||", "cmd2"]);
}

#[test]
fn backslash_escaped_space() {
    assert_eq!(split_tokens(r"rm\ -rf\ /"), vec!["rm -rf /"]);
}

#[test]
fn ampersand_inside_double_quotes() {
    assert_eq!(
        split_tokens(r#"bash -c "cmd1 && cmd2""#),
        vec!["bash", "-c", "cmd1 && cmd2"]
    );
}

#[test]
fn single_quote_inside_double_quotes() {
    assert_eq!(split_tokens(r#""it's fine""#), vec!["it's fine"]);
}

#[test]
fn double_quote_inside_single_quotes() {
    assert_eq!(split_tokens(r#"'say "hi"'"#), vec![r#"say "hi""#]);
}

#[test]
fn empty_input() {
    assert_eq!(split_tokens(""), Vec::<String>::new());
}

#[test]
fn nested_bash_single_quote() {
    assert_eq!(
        extract_nested_commands("bash -c 'echo hello'"),
        vec!["echo hello"]
    );
}

#[test]
fn nested_sh_single_quote() {
    assert_eq!(
        extract_nested_commands("sh -c 'echo hello'"),
        vec!["echo hello"]
    );
}

#[test]
fn nested_double_quote_and_separator() {
    assert_eq!(
        extract_nested_commands(r#"bash -c "cmd1 && cmd2""#),
        vec!["cmd1", "cmd2"]
    );
}

#[test]
fn nested_semicolons_three_cmds() {
    assert_eq!(
        extract_nested_commands(r#"bash -c "cmd1; cmd2; cmd3""#),
        vec!["cmd1", "cmd2", "cmd3"]
    );
}

#[test]
fn nested_ansi_c_newline() {
    assert_eq!(
        extract_nested_commands("bash -c $'cmd1\\ncmd2'"),
        vec!["cmd1", "cmd2"]
    );
}

#[test]
fn nested_env_prefix_single_var() {
    assert_eq!(
        extract_nested_commands("env VAR=val bash -c 'echo hello'"),
        vec!["echo hello"]
    );
}

#[test]
fn nested_env_prefix_multi_var() {
    assert_eq!(
        extract_nested_commands("env A=1 B=2 bash -c 'cmd'"),
        vec!["cmd"]
    );
}

#[test]
fn nested_recursive_unwrap() {
    assert_eq!(
        extract_nested_commands(r#"bash -c 'bash -c "inner cmd"'"#),
        vec!["inner cmd"]
    );
}

#[test]
fn nested_non_shell_returns_empty() {
    assert_eq!(
        extract_nested_commands(r#"python -c 'print("hi")'"#),
        Vec::<String>::new()
    );
}

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

#[test]
fn heredoc_basic_body() {
    let cmd = "cat <<EOF\nrm -rf /\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    assert_eq!(bodies[0].body, "rm -rf /");
    assert!(!bodies[0].is_nowdoc);
}

#[test]
fn heredoc_nowdoc_flag() {
    let cmd = "cat <<'EOF'\nsome secret\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].body, "some secret");
    assert!(bodies[0].is_nowdoc);
}

#[test]
fn heredoc_multiline_body() {
    let cmd = "bash <<SCRIPT\necho hello\nrm -rf /tmp/foo\nSCRIPT";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].body, "echo hello\nrm -rf /tmp/foo");
}

#[test]
fn heredoc_delimiter_prefix_remains_in_interpreter_body() {
    let cmd = "bash <<'EOF'\nEOFx\necho still in body\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies[0].body, "EOFx\necho still in body");
}

#[test]
fn heredoc_delimiter_prefix_keeps_later_substitution_visible() {
    let cmd = "x=$(cat <<'EOF'\ntext\nEOF)\n$(rm -rf /)";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert!(masked.contains("$(rm -rf /)"));
}

#[test]
fn heredoc_delimiter_prefix_does_not_hide_later_list_segment() {
    let cmd = "x=$(cat <<'EOF'\ntext\nEOF)\necho later";
    let segments = list_segments(cmd);
    assert_eq!(segments.len(), 2);
}

#[test]
fn heredoc_strip_tabs_delimiter() {
    let cmd = "cat <<-EOF\n\trm -rf /\n\tEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    assert_eq!(bodies[0].body, "\trm -rf /");
}

#[test]
fn heredoc_no_match_returns_empty() {
    let bodies = extract_heredoc_bodies("echo hello world");
    assert!(bodies.is_empty());
}

#[test]
fn heredoc_double_quoted_delimiter_is_nowdoc() {
    let cmd = "cat <<\"EOF\"\nsome secret\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    assert_eq!(bodies[0].body, "some secret");
    assert!(bodies[0].is_nowdoc);
}

#[test]
fn heredoc_backslash_escaped_delimiter_is_nowdoc() {
    let cmd = "cat <<\\EOF\nsome secret\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].delimiter, "EOF");
    assert!(bodies[0].is_nowdoc);
}

#[test]
fn heredoc_nowdoc_to_cat_is_not_interpreter_target() {
    let cmd = "cat <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].target_is_interpreter);
}

#[test]
fn heredoc_nowdoc_to_bash_is_interpreter_target() {
    let cmd = "bash <<'EOF'\necho hi\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_is_interpreter);
}

#[test]
fn heredoc_nowdoc_to_python3_is_interpreter_target() {
    let cmd = "python3 <<'EOF'\nprint(1)\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_is_interpreter);
}

#[test]
fn heredoc_nowdoc_to_uppercase_interpreter_is_interpreter_target() {
    let cmd = "Bash <<'EOF'\necho hi\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].target_is_interpreter);
}

#[test]
fn mask_inert_heredoc_neutralizes_backticks_for_cat() {
    let cmd = "cat <<'EOF'\nmentions `python3 -c` as inert markdown code text\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert!(!masked.contains('`'));
}

#[test]
fn mask_inert_heredoc_leaves_interpreter_target_untouched() {
    let cmd = "bash <<'EOF'\necho `date`\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert_eq!(masked, cmd);
}

#[test]
fn mask_inert_heredoc_leaves_non_nowdoc_untouched() {
    let cmd = "cat <<EOF\n`rm -rf /`\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert_eq!(masked, cmd);
}

#[test]
fn mask_inert_heredoc_no_heredoc_is_noop() {
    let cmd = "echo `date`";
    assert_eq!(mask_inert_heredoc_substitution_markers(cmd), cmd);
}

#[test]
fn heredoc_cat_redirected_to_file_is_not_trusted() {
    let cmd = "cat >> notes.txt <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

#[test]
fn heredoc_cat_without_redirection_is_flagged_as_data_consumer() {
    let cmd = "cat <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].is_data_consumer_target);
}

#[test]
fn heredoc_tee_with_file_argument_is_not_trusted() {
    let cmd = "tee out.txt <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

#[test]
fn heredoc_interpreter_redirected_to_file_is_not_flagged_as_data_consumer() {
    let cmd = "bash > log.txt <<'EOF'\necho hi\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

#[test]
fn heredoc_cat_with_fd_duplication_is_flagged_as_data_consumer() {
    let cases = [
        "cat 2>&1 <<'EOF'\nsome text\nEOF",
        "cat >&2 <<'EOF'\nsome text\nEOF",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(
            bodies[0].is_data_consumer_target,
            "command {cmd:?}: expected is_data_consumer_target to be true"
        );
    }
}

#[test]
fn mask_inert_heredoc_keeps_file_write_body_visible() {
    let cmd = "cat >> notes.txt <<'EOF'\nrm -rf /\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert!(masked.contains("rm -rf"));
    assert_eq!(masked.lines().count(), cmd.lines().count());
}

#[test]
fn mask_inert_heredoc_blanks_full_body_for_cat_without_redirection() {
    let cmd = "cat <<'EOF'\nrm -rf /\nEOF";
    let masked = mask_inert_heredoc_substitution_markers(cmd);
    assert!(!masked.contains("rm -rf"));
}

#[test]
fn heredoc_jq_is_flagged_as_data_consumer() {
    let cmd = "jq -c . <<'JSON'\n{\"cmd\": \"python3 ./x\"}\nJSON";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].is_data_consumer_target);
}

#[test]
fn heredoc_file_write_after_semicolon_is_not_trusted() {
    let cmd = "true; cat > /tmp/x.sh <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

#[test]
fn heredoc_file_write_after_and_and_is_not_trusted() {
    let cmd = "true && cat > /tmp/x.sh <<'EOF'\nsome text\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

#[test]
fn heredoc_file_write_followed_by_another_command_is_not_flagged() {
    let cmd = "cat > /tmp/x.sh <<'EOF'\nsome text\nEOF\ntrue";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// An open quote or substitution on the marker line delays the body in bash.
#[test]
fn heredoc_open_construct_on_marker_line_is_not_flagged() {
    for cmd in [
        "cat <<'EOF' \"\n\"; rm -rf /\nEOF",
        "cat <<'EOF' '\n'; rm -rf /\nEOF",
        "cat <<'EOF' $'\n'; rm -rf /\nEOF",
        "cat <<'EOF' $(\nrm -rf /\n)\nEOF",
        "cat <<'EOF' `\nrm -rf /\n`\nEOF",
        "cat <<'EOF' ${x:-\n}; rm -rf /\nEOF",
        "x=$(cat <<'EOF' \"\n\"; rm -rf /\nEOF\n)",
    ] {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "{cmd:?}");
    }
}

#[test]
fn inline_script_python() {
    let scripts = extract_inline_scripts(r#"python3 -c "import os; os.system('cmd')""#);
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "python3");
    assert_eq!(scripts[0].body, "import os; os.system('cmd')");
}

#[test]
fn inline_script_node() {
    let scripts = extract_inline_scripts(r#"node -e "process.exit(1)""#);
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].interpreter, "node");
    assert_eq!(scripts[0].body, "process.exit(1)");
}

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

// Unquoted literal `$IFS` and `${IFS}` act as shell word-separators; quoted,
// escaped, and non-IFS variables stay untouched.
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

// Inside double quotes bash removes a backslash only before `$`, backtick,
// `"`, `\` and newline; before any other character the backslash stays.
#[test]
fn backslash_before_ordinary_char_in_double_quotes_is_kept() {
    assert_eq!(
        split_tokens(r#"echo "c\d" "\n""#),
        vec!["echo", r"c\d", r"\n"]
    );
}

#[test]
fn backslash_before_special_char_in_double_quotes_is_removed() {
    assert_eq!(
        split_tokens(r#"echo "e\$f" "g\"h" "i\\j" "k\`l""#),
        vec!["echo", "e$f", "g\"h", r"i\j", "k`l"]
    );
}

// A backslash-newline is a line continuation: bash removes both characters,
// outside quotes and inside double quotes alike.
#[test]
fn line_continuation_joins_the_word_outside_quotes() {
    assert_eq!(split_tokens("r\\\nm -rf x"), vec!["rm", "-rf", "x"]);
}

#[test]
fn line_continuation_joins_the_word_inside_double_quotes() {
    assert_eq!(split_tokens("echo \"a\\\nb\""), vec!["echo", "ab"]);
}

#[test]
fn backslash_newline_inside_single_quotes_stays() {
    assert_eq!(split_tokens("echo 'a\\\nb'"), vec!["echo", "a\\\nb"]);
}
