use super::*;

#[test]
fn a_double_lt_inside_dollar_paren_within_double_quotes_is_a_marker() {
    let line = "\"$(cat <<'EOF'";
    assert_eq!(
        marker_fields(line),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: true,
            strip_tabs: false,
            operator_start: 7,
            delimiter_end: 14
        })
    );
}

#[test]
fn a_double_lt_inside_a_plain_double_quoted_string_is_not_a_marker() {
    let line = "\"<<EOF\"";
    assert!(marker_fields(line).is_none());
}

#[test]
fn a_double_lt_inside_a_backtick_body_is_a_marker() {
    let line = "`cat <<EOF`";
    assert_eq!(
        marker_fields(line),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 5,
            delimiter_end: 10
        })
    );
}

#[test]
fn a_double_lt_inside_nested_dollar_paren_command_substitutions_is_a_marker() {
    let line = "\"$(echo \"$(cat <<EOF)\")\"";
    assert_eq!(
        marker_fields(line),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 15,
            delimiter_end: 20
        })
    );
}

#[test]
fn a_heredoc_with_no_body_lines_suspends_marker_through_terminator() {
    let cmd = "cat <<EOF\nEOF\n";
    assert_eq!(
        heredoc_suspend_ranges(cmd),
        vec![cmd.find("<<").unwrap()..(cmd.rfind("EOF").unwrap() + "EOF".len())]
    );
}

#[test]
fn a_crlf_heredoc_suspends_marker_through_terminator() {
    let cmd = "cat <<EOF\r\nbody\r\nEOF\r\n";
    assert_eq!(
        heredoc_suspend_ranges(cmd),
        vec![cmd.find("<<").unwrap()..(cmd.rfind("EOF").unwrap() + "EOF".len())]
    );
}

#[test]
fn a_heredoc_terminator_with_no_trailing_newline_still_ends_the_suspend_range() {
    let cmd = "cat <<EOF\nbody\nEOF";
    assert_eq!(
        heredoc_suspend_ranges(cmd),
        vec![cmd.find("<<").unwrap()..(cmd.rfind("EOF").unwrap() + "EOF".len())]
    );
}

#[derive(Debug, PartialEq)]
struct MarkerFields {
    delimiter: String,
    is_nowdoc: bool,
    strip_tabs: bool,
    operator_start: usize,
    delimiter_end: usize,
}

fn marker_fields(line: &str) -> Option<MarkerFields> {
    find_heredoc_marker(line).map(|m| MarkerFields {
        delimiter: m.delimiter,
        is_nowdoc: m.is_nowdoc,
        strip_tabs: m.strip_tabs,
        operator_start: m.operator_start,
        delimiter_end: m.delimiter_end,
    })
}

#[test]
fn suspend_cursor_reports_membership_across_ranges_in_order() {
    let ranges = [2..4, 6..8];
    let mut cursor = SuspendCursor::new(&ranges);
    let hits: Vec<bool> = (0..10).map(|i| cursor.contains(i)).collect();
    assert_eq!(
        hits,
        vec![
            false, false, true, true, false, false, true, true, false, false
        ]
    );
}

#[test]
fn suspend_cursor_over_no_ranges_contains_nothing() {
    let mut cursor = SuspendCursor::new(&[]);
    assert!(!cursor.contains(0));
}

#[test]
fn suspend_cursor_skips_an_exhausted_range_when_asked_past_its_end() {
    let ranges = [0..1, 3..5];
    let mut cursor = SuspendCursor::new(&ranges);
    assert!(cursor.contains(3));
    assert!(!cursor.contains(5));
}

#[test]
fn double_lt_position_is_found_in_plain_text() {
    assert_eq!(find_unquoted_double_lt("cat <<EOF"), Some(4));
    assert_eq!(find_unquoted_double_lt("<<"), Some(0));
}

#[test]
fn a_single_lt_or_trailing_lt_is_not_a_double_lt() {
    assert_eq!(find_unquoted_double_lt("a<b"), None);
    assert_eq!(find_unquoted_double_lt("a <"), None);
    assert_eq!(find_unquoted_double_lt(""), None);
}

#[test]
fn a_double_lt_inside_single_quotes_is_not_found() {
    assert_eq!(find_unquoted_double_lt("echo '<<EOF'"), None);
    assert_eq!(find_unquoted_double_lt("echo '$(<<x'"), None);
}

#[test]
fn a_backslash_is_literal_inside_single_quotes_and_escapes_outside() {
    assert_eq!(find_unquoted_double_lt("'a\\' <<x"), Some(5));
    assert_eq!(find_unquoted_double_lt("echo \\<<EOF"), None);
}

#[test]
fn quote_characters_of_the_other_kind_do_not_toggle_quoting() {
    assert_eq!(find_unquoted_double_lt("\"it's <<x\""), None);
    assert_eq!(find_unquoted_double_lt("echo \"it's\" <<EOF"), Some(12));
    assert_eq!(find_unquoted_double_lt("echo '\"' <<EOF"), Some(9));
}

#[test]
fn a_backtick_inside_single_quotes_does_not_open_a_substitution() {
    assert_eq!(find_unquoted_double_lt("'`' <<x"), Some(4));
    assert_eq!(find_unquoted_double_lt("'`<<x'"), None);
}

#[test]
fn closing_a_backtick_restores_the_surrounding_double_quote() {
    assert_eq!(find_unquoted_double_lt("\"`a`\" <<x"), Some(6));
    assert_eq!(find_unquoted_double_lt("\"`a` <<x\""), None);
}

#[test]
fn a_backtick_inside_double_quotes_opens_an_unquoted_region() {
    assert_eq!(find_unquoted_double_lt("\"`<<x`\""), Some(2));
    assert_eq!(find_unquoted_double_lt("`\"<<x\"`"), None);
}

#[test]
fn a_backtick_pair_inside_dollar_paren_restores_the_outer_frame() {
    assert_eq!(find_unquoted_double_lt("\"$(`a`)\" <<x"), Some(9));
    assert_eq!(find_unquoted_double_lt("\"$(`a`) <<x\""), None);
}

#[test]
fn a_dollar_not_followed_by_a_paren_opens_nothing() {
    assert_eq!(find_unquoted_double_lt("echo $x <<EOF"), Some(8));
    assert_eq!(find_unquoted_double_lt("\"$x <<EOF\""), None);
    assert_eq!(find_unquoted_double_lt("echo $"), None);
}

#[test]
fn a_closed_dollar_paren_restores_the_surrounding_double_quote() {
    assert_eq!(find_unquoted_double_lt("\"$(a)\" <<x"), Some(7));
    assert_eq!(find_unquoted_double_lt("\"$(a) <<x\""), None);
}

#[test]
fn nested_parens_inside_dollar_paren_keep_the_substitution_open() {
    assert_eq!(find_unquoted_double_lt("\"$(a (b) <<x)\""), Some(9));
    assert_eq!(find_unquoted_double_lt("\"$(a (b) c) <<x\""), None);
    assert_eq!(find_unquoted_double_lt("\"$(a (b) c)\" <<x"), Some(13));
}

#[test]
fn parens_inside_quotes_in_a_substitution_do_not_change_its_depth() {
    assert_eq!(find_unquoted_double_lt("\"$(echo ')' <<x)\""), Some(12));
    assert_eq!(find_unquoted_double_lt("\"$(echo \"(\" ) <<x\""), None);
    assert_eq!(find_unquoted_double_lt("\"$(echo \")\" ) <<x\""), None);
    assert_eq!(find_unquoted_double_lt("$(echo ')') '<<x'"), None);
    assert_eq!(find_unquoted_double_lt("$(echo \")\") \"<<x\""), None);
}

#[test]
fn unbalanced_parens_outside_a_substitution_are_ignored() {
    assert_eq!(find_unquoted_double_lt(") (a <<x"), Some(5));
}

#[test]
fn a_single_quote_inside_a_substitution_is_scoped_to_it() {
    assert_eq!(find_unquoted_double_lt("'$(a)' <<x"), Some(7));
    assert_eq!(find_unquoted_double_lt("\"$(echo '\") <<x\""), None);
}

#[test]
fn case_ranges_are_empty_when_the_word_case_is_absent() {
    assert!(case_statement_suspend_ranges("echo hi").is_empty());
    assert!(case_statement_suspend_ranges("").is_empty());
}

#[test]
fn a_whole_case_statement_is_one_range() {
    let cmd = "case x in a) rm;; esac";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![0..cmd.len()]);
}

#[test]
fn a_case_statement_stops_at_its_esac_not_at_the_end_of_the_command() {
    let cmd = "case x in a) :;; esac; ls";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![0..21]);
}

#[test]
fn nested_case_statements_yield_one_outer_range() {
    let cmd = "case a in b) case c in d) :;; esac;; esac; ls";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![0..41]);
}

#[test]
fn an_esac_without_an_open_case_is_ignored() {
    assert!(case_statement_suspend_ranges("esac; case").is_empty());
    assert_eq!(
        case_statement_suspend_ranges("esac; case x in y) :;; esac"),
        vec![6..27]
    );
}

#[test]
fn a_case_without_esac_yields_no_range() {
    assert!(case_statement_suspend_ranges("case x in y) :;;").is_empty());
}

#[test]
fn case_not_in_command_position_is_ignored() {
    assert!(case_statement_suspend_ranges("echo case x in y) :;; esac").is_empty());
    assert!(case_statement_suspend_ranges("echo x case y in z) :;; esac").is_empty());
}

#[test]
fn an_esac_argument_does_not_close_the_case_statement() {
    let cmd = "case x in y) echo esac ;; esac";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![0..cmd.len()]);
}

#[test]
fn a_command_starting_keyword_puts_case_in_command_position() {
    for keyword in crate::COMMAND_STARTING_KEYWORDS {
        let cmd = format!("{keyword} case x in y) :;; esac");
        let start = keyword.len() + 1;
        assert_eq!(
            case_statement_suspend_ranges(&cmd),
            vec![start..cmd.len()],
            "{keyword}"
        );
    }
}

#[test]
fn every_separator_puts_case_in_command_position() {
    for sep in [";", "&", "|", "(", ")", "{", "\n"] {
        let cmd = format!("true {sep} case x in y) :;; esac");
        assert_eq!(
            case_statement_suspend_ranges(&cmd),
            vec![7..cmd.len()],
            "{sep:?}"
        );
    }
}

#[test]
fn case_right_after_a_separator_without_spaces_is_found() {
    let cmd = "a;case x in y) :;; esac";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![2..cmd.len()]);
}

#[test]
fn case_and_esac_words_repeated_directly_do_not_reopen() {
    let cmd = "case case in y) :;; esac";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![0..cmd.len()]);
    let two = "case x in y) :;; esac case z in w) :;; esac";
    assert_eq!(case_statement_suspend_ranges(two), vec![0..21]);
}

#[test]
fn quoted_or_escaped_case_words_are_not_keywords() {
    assert!(case_statement_suspend_ranges("'case' x in y) :;; esac").is_empty());
    assert!(case_statement_suspend_ranges("\"case\" x in y) :;; esac").is_empty());
    assert!(case_statement_suspend_ranges("\\case x in y) :;; esac").is_empty());
    assert!(case_statement_suspend_ranges("`case` x in y) :;; esac").is_empty());
}

#[test]
fn separators_inside_quotes_do_not_start_a_command() {
    assert!(case_statement_suspend_ranges("echo 'a ; case x in y) :;; esac '").is_empty());
    assert!(case_statement_suspend_ranges("echo \"a ; case x in y) :;; esac \"").is_empty());
    assert!(case_statement_suspend_ranges("echo `a ; case x in y) :;; esac `").is_empty());
}

#[test]
fn a_quote_of_the_other_kind_inside_quotes_does_not_toggle() {
    let cmd = "\"it's\"; case x in y) :;; esac";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![8..cmd.len()]);
    let single = "'say \"'; case x in y) :;; esac";
    assert_eq!(case_statement_suspend_ranges(single), vec![9..single.len()]);
    let backtick = "`it's \"`; case x in y) :;; esac";
    assert_eq!(
        case_statement_suspend_ranges(backtick),
        vec![10..backtick.len()]
    );
}

#[test]
fn a_backtick_inside_single_quotes_does_not_open_backticks() {
    let cmd = "'`'; case x in y) :;; esac";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![5..cmd.len()]);
}

#[test]
fn case_scan_treats_backslash_as_literal_in_single_quotes_and_escape_outside() {
    let literal = "'a\\'; case x in y) :;; esac";
    assert_eq!(
        case_statement_suspend_ranges(literal),
        vec![6..literal.len()]
    );
    let escaped = "echo \\'; case x in y) :;; esac";
    assert_eq!(
        case_statement_suspend_ranges(escaped),
        vec![9..escaped.len()]
    );
    assert!(case_statement_suspend_ranges("echo \\; case x in y) :;; esac").is_empty());
}

#[test]
fn a_double_quote_inside_backticks_does_not_toggle_double_quoting() {
    let cmd = "`\"`; case x in y) :;; esac";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![5..cmd.len()]);
}

#[test]
fn case_ranges_use_byte_offsets_after_multibyte_text() {
    let cmd = "é; case x in y) :;; esac é";
    assert_eq!(case_statement_suspend_ranges(cmd), vec![4..25]);
}

#[test]
fn a_trailing_case_word_at_the_end_of_input_is_handled() {
    assert!(case_statement_suspend_ranges("echo; case").is_empty());
    assert!(case_statement_suspend_ranges("case x in y) :;; esac2").is_empty());
}

#[test]
fn a_plain_word_marker_has_exact_fields() {
    assert_eq!(
        marker_fields("cat <<EOF"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 9
        })
    );
    assert_eq!(
        marker_fields("cat <<EOF > out"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 9
        })
    );
    assert_eq!(
        marker_fields("cat <<EOF_1-x"),
        Some(MarkerFields {
            delimiter: "EOF_1".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 11
        })
    );
}

#[test]
fn whitespace_after_the_operator_shifts_the_delimiter_end() {
    assert_eq!(
        marker_fields("cat << EOF"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 10
        })
    );
    assert_eq!(
        marker_fields("cat <<-  EOF"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: false,
            strip_tabs: true,
            operator_start: 4,
            delimiter_end: 12
        })
    );
}

#[test]
fn a_dash_after_the_operator_strips_tabs() {
    assert_eq!(
        marker_fields("cat <<-EOF"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: false,
            strip_tabs: true,
            operator_start: 4,
            delimiter_end: 10
        })
    );
}

#[test]
fn a_quoted_delimiter_is_a_nowdoc_with_exact_end() {
    assert_eq!(
        marker_fields("cat <<'EOF'"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: true,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 11
        })
    );
    assert_eq!(
        marker_fields("cat <<\"EOF\" x"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: true,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 11
        })
    );
    assert_eq!(
        marker_fields("cat << 'E F'"),
        Some(MarkerFields {
            delimiter: "E F".into(),
            is_nowdoc: true,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 12
        })
    );
    assert_eq!(
        marker_fields("cat <<-'EOF'"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: true,
            strip_tabs: true,
            operator_start: 4,
            delimiter_end: 12
        })
    );
}

#[test]
fn a_backslash_delimiter_is_a_nowdoc_with_exact_end() {
    assert_eq!(
        marker_fields("cat <<\\EOF"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: true,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 10
        })
    );
    assert_eq!(
        marker_fields("cat <<\\EOF;ls"),
        Some(MarkerFields {
            delimiter: "EOF".into(),
            is_nowdoc: true,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 10
        })
    );
}

#[test]
fn a_marker_with_no_usable_delimiter_is_none() {
    for line in [
        "cat <<",
        "cat <<;",
        "cat <<''",
        "cat <<\"\"",
        "cat <<\\",
        "cat <<\\;",
        "cat <<'EOF",
        "cat <<\"EOF",
        "cat <<-",
        "cat < file",
    ] {
        assert!(marker_fields(line).is_none(), "{line}");
    }
}

#[test]
fn delimiter_end_counts_bytes_of_multibyte_delimiters() {
    assert_eq!(
        marker_fields("cat <<é"),
        Some(MarkerFields {
            delimiter: "é".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 8
        })
    );
    assert_eq!(
        marker_fields("cat <<\\é"),
        Some(MarkerFields {
            delimiter: "é".into(),
            is_nowdoc: true,
            strip_tabs: false,
            operator_start: 4,
            delimiter_end: 9
        })
    );
}

#[test]
fn the_arithmetic_shift_marker_has_exact_fields() {
    assert_eq!(
        marker_fields("echo $((1 << 2))"),
        Some(MarkerFields {
            delimiter: "2".into(),
            is_nowdoc: false,
            strip_tabs: false,
            operator_start: 10,
            delimiter_end: 14
        })
    );
}

#[derive(Debug, PartialEq)]
struct SiteFields {
    marker_line: String,
    preceding_text: String,
    body_range: std::ops::Range<usize>,
    suspend_end: usize,
    terminator_is_clean: bool,
}

fn sites(lines: &[&str]) -> Vec<SiteFields> {
    let mut out = Vec::new();
    walk_heredoc_sites(lines, |site| {
        out.push(SiteFields {
            marker_line: site.marker_line.to_string(),
            preceding_text: site.preceding_text.to_string(),
            body_range: site.body_range,
            suspend_end: site.suspend_end,
            terminator_is_clean: site.terminator_is_clean,
        });
    });
    out
}

#[test]
fn lines_without_a_double_lt_produce_no_sites() {
    assert!(sites(&["echo hi", "ls"]).is_empty());
    assert!(sites(&["echo <<", "ls"]).is_empty());
}

#[test]
fn a_terminated_heredoc_has_a_clean_site() {
    assert_eq!(
        sites(&["cat <<EOF", "a", "EOF", "ls"]),
        vec![SiteFields {
            marker_line: "cat <<EOF".into(),
            preceding_text: "".into(),
            body_range: 1..2,
            suspend_end: 2,
            terminator_is_clean: true
        }]
    );
}

#[test]
fn preceding_text_collects_earlier_lines_and_marker_lines_only() {
    assert_eq!(
        sites(&["echo hi", "cat <<EOF", "x", "EOF"]),
        vec![SiteFields {
            marker_line: "cat <<EOF".into(),
            preceding_text: "echo hi\n".into(),
            body_range: 2..3,
            suspend_end: 3,
            terminator_is_clean: true
        }]
    );
    assert_eq!(
        sites(&["cat <<A", "x", "A", "cat <<B", "y", "B"]),
        vec![
            SiteFields {
                marker_line: "cat <<A".into(),
                preceding_text: "".into(),
                body_range: 1..2,
                suspend_end: 2,
                terminator_is_clean: true
            },
            SiteFields {
                marker_line: "cat <<B".into(),
                preceding_text: "cat <<A\n".into(),
                body_range: 4..5,
                suspend_end: 5,
                terminator_is_clean: true
            }
        ]
    );
}

#[test]
fn an_unterminated_heredoc_runs_to_the_last_line_and_is_not_clean() {
    assert_eq!(
        sites(&["cat <<EOF", "a", "b"]),
        vec![SiteFields {
            marker_line: "cat <<EOF".into(),
            preceding_text: "".into(),
            body_range: 1..3,
            suspend_end: 3,
            terminator_is_clean: false
        }]
    );
    assert_eq!(
        sites(&["cat <<EOF"]),
        vec![SiteFields {
            marker_line: "cat <<EOF".into(),
            preceding_text: "".into(),
            body_range: 1..1,
            suspend_end: 1,
            terminator_is_clean: false
        }]
    );
}

#[test]
fn a_prefixed_line_before_the_terminator_ends_suspension_early_and_is_not_clean() {
    assert_eq!(
        sites(&["cat <<EOF", "a", "EOFX", "b", "EOF"]),
        vec![SiteFields {
            marker_line: "cat <<EOF".into(),
            preceding_text: "".into(),
            body_range: 1..4,
            suspend_end: 2,
            terminator_is_clean: false
        }]
    );
    assert_eq!(
        sites(&["cat <<EOF", "EOFX"]),
        vec![SiteFields {
            marker_line: "cat <<EOF".into(),
            preceding_text: "".into(),
            body_range: 1..2,
            suspend_end: 1,
            terminator_is_clean: false
        }]
    );
}

#[test]
fn leading_tabs_on_the_terminator_count_only_with_a_dash_marker() {
    assert_eq!(
        sites(&["cat <<-EOF", "a", "\t\tEOF"]),
        vec![SiteFields {
            marker_line: "cat <<-EOF".into(),
            preceding_text: "".into(),
            body_range: 1..2,
            suspend_end: 2,
            terminator_is_clean: true
        }]
    );
    assert_eq!(
        sites(&["cat <<EOF", "\tEOF"]),
        vec![SiteFields {
            marker_line: "cat <<EOF".into(),
            preceding_text: "".into(),
            body_range: 1..2,
            suspend_end: 2,
            terminator_is_clean: false
        }]
    );
}

#[test]
fn a_heredoc_with_body_and_following_command_ends_at_the_terminator() {
    let cmd = "cat <<EOF\nbody\nEOF\nls";
    assert_eq!(heredoc_suspend_ranges(cmd), vec![4..18]);
}

#[test]
fn a_prefixed_line_ends_the_suspend_range_at_that_line_start() {
    let cmd = "cat <<EOF\na\nEOFX\nb\nEOF\n";
    assert_eq!(heredoc_suspend_ranges(cmd), vec![4..12]);
}

#[test]
fn a_heredoc_on_a_later_line_starts_at_its_operator() {
    let cmd = "echo x\ncat <<EOF\nb\nEOF";
    assert_eq!(heredoc_suspend_ranges(cmd), vec![11..22]);
}

#[test]
fn an_unterminated_heredoc_suspends_to_the_end_of_cmd() {
    let cmd = "cat <<EOF\nb\n";
    assert_eq!(heredoc_suspend_ranges(cmd), vec![4..12]);
}

#[test]
fn two_heredocs_yield_two_ranges() {
    let cmd = "cat <<A\nx\nA\ncat <<B\ny\nB";
    assert_eq!(heredoc_suspend_ranges(cmd), vec![4..11, 16..23]);
}

#[test]
fn a_double_lt_that_is_not_a_marker_yields_no_range() {
    assert!(heredoc_suspend_ranges("echo <<\nls").is_empty());
}

#[test]
fn a_command_without_a_double_lt_yields_no_heredoc_range() {
    assert!(heredoc_suspend_ranges("echo hi").is_empty());
}

#[test]
fn indexed_lines_pairs_each_line_with_its_byte_start() {
    assert_eq!(indexed_lines("a\nb\n"), vec![("a", 0), ("b", 2)]);
    assert_eq!(indexed_lines("a\n\nb"), vec![("a", 0), ("", 2), ("b", 3)]);
    assert_eq!(indexed_lines("é\nx"), vec![("é", 0), ("x", 3)]);
}

#[test]
fn indexed_lines_has_no_empty_tail_after_a_final_newline() {
    assert_eq!(indexed_lines("a\n"), vec![("a", 0)]);
    assert_eq!(indexed_lines("\n"), vec![("", 0)]);
    assert!(indexed_lines("").is_empty());
}

#[test]
fn indexed_lines_strips_one_trailing_carriage_return_per_line() {
    assert_eq!(indexed_lines("a\r\nb"), vec![("a", 0), ("b", 3)]);
    assert_eq!(indexed_lines("a\r"), vec![("a", 0)]);
    assert_eq!(indexed_lines("a\r\r\nb\r"), vec![("a\r", 0), ("b", 4)]);
}
