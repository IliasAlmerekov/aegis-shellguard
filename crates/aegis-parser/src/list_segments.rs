use crate::PipelineChain;
use crate::redirection::ends_with_redirect_target;
use crate::tokenizer::suspend_ranges::{case_statement_suspend_ranges, heredoc_suspend_ranges};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSeparator {
    And,
    Or,
    Semicolon,
    Background,
    Newline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSegment {
    pub pipeline: PipelineChain,
    pub separator: Option<ListSeparator>,
}

pub(crate) fn finalize_segment(current: &mut String, segments: &mut Vec<String>) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }
    current.clear();
}

fn finalize_with_separator(
    kind: ListSeparator,
    current: &mut String,
    segments: &mut Vec<String>,
    separators: &mut Vec<ListSeparator>,
) {
    let had_text = !current.trim().is_empty();
    finalize_segment(current, segments);
    if had_text {
        separators.push(kind);
    }
}

pub(crate) fn split_top_level_command_groups_with_separators(
    cmd: &str,
) -> Vec<(String, Option<ListSeparator>)> {
    let mut suspend_ranges = heredoc_suspend_ranges(cmd);
    suspend_ranges.extend(case_statement_suspend_ranges(cmd));
    suspend_ranges.sort_by_key(|range| range.start);
    let mut suspend_ranges = suspend_ranges.iter();
    let mut active_suspend = suspend_ranges.next();

    let mut segments = Vec::new();
    let mut separators = Vec::new();
    let mut current = String::new();
    let mut chars = cmd.char_indices().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut command_subst_depth = 0usize;

    while let Some((byte_idx, ch)) = chars.next() {
        while active_suspend.is_some_and(|range| range.end <= byte_idx) {
            active_suspend = suspend_ranges.next();
        }
        if active_suspend.is_some_and(|range| range.contains(&byte_idx)) {
            current.push(ch);
            continue;
        }

        let next_char = chars.peek().map(|&(_, c)| c);

        match ch {
            '\\' if !in_single_quote => {
                current.push(ch);
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                }
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                current.push(ch);
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                current.push(ch);
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                current.push(ch);
            }
            '$' if !in_single_quote && !in_backticks && next_char == Some('(') => {
                command_subst_depth += 1;
                current.push(ch);
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                }
            }
            '(' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && command_subst_depth == 0 =>
            {
                paren_depth += 1;
                current.push(ch);
            }
            ')' if !in_single_quote
                && !in_backticks
                && (command_subst_depth > 0 || paren_depth > 0) =>
            {
                if command_subst_depth > 0 {
                    command_subst_depth -= 1;
                } else {
                    paren_depth -= 1;
                }
                current.push(ch);
            }
            '{' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && command_subst_depth == 0
                && (current.is_empty()
                    || current.ends_with(char::is_whitespace)
                    || current.ends_with(')'))
                && next_char.is_some_and(char::is_whitespace) =>
            {
                brace_depth += 1;
                current.push(ch);
            }
            '}' if !in_single_quote
                && !in_backticks
                && brace_depth > 0
                && (current.ends_with(char::is_whitespace) || current.ends_with(';')) =>
            {
                brace_depth -= 1;
                current.push(ch);
            }
            '\n' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && brace_depth == 0
                && command_subst_depth == 0 =>
            {
                finalize_with_separator(
                    ListSeparator::Newline,
                    &mut current,
                    &mut segments,
                    &mut separators,
                );
            }
            ';' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && brace_depth == 0
                && command_subst_depth == 0 =>
            {
                finalize_with_separator(
                    ListSeparator::Semicolon,
                    &mut current,
                    &mut segments,
                    &mut separators,
                );
            }
            '&' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && brace_depth == 0
                && command_subst_depth == 0 =>
            {
                if next_char == Some('&') {
                    chars.next();
                    finalize_with_separator(
                        ListSeparator::And,
                        &mut current,
                        &mut segments,
                        &mut separators,
                    );
                } else if next_char != Some('>') && !ends_with_redirect_target(&current) {
                    finalize_with_separator(
                        ListSeparator::Background,
                        &mut current,
                        &mut segments,
                        &mut separators,
                    );
                } else {
                    current.push(ch);
                }
            }
            '|' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && brace_depth == 0
                && command_subst_depth == 0
                && next_char == Some('|') =>
            {
                chars.next();
                finalize_with_separator(
                    ListSeparator::Or,
                    &mut current,
                    &mut segments,
                    &mut separators,
                );
            }
            _ => current.push(ch),
        }
    }

    finalize_segment(&mut current, &mut segments);

    let mut separators: Vec<Option<ListSeparator>> = separators.into_iter().map(Some).collect();
    separators.resize(segments.len(), None);
    if let Some(last) = separators.last_mut() {
        *last = None;
    }

    segments.into_iter().zip(separators).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_segments(cmd: &str) -> Vec<String> {
        split_top_level_command_groups_with_separators(cmd)
            .into_iter()
            .map(|(raw, _separator)| raw)
            .collect()
    }

    #[test]
    fn heredoc_body_line_does_not_split_on_an_embedded_separator() {
        let cmd = ": <<X\nhi; there\nX\ntrue; python3 ./evil.py";
        assert_eq!(
            raw_segments(cmd),
            vec![": <<X\nhi; there\nX", "true", "python3 ./evil.py"]
        );
    }

    #[test]
    fn heredoc_owning_segment_keeps_a_same_line_chained_exec_glued() {
        let cmd = "cat > f <<EOF && python3 f\nprint(1)\nEOF";
        assert_eq!(
            raw_segments(cmd),
            vec!["cat > f <<EOF && python3 f\nprint(1)\nEOF"]
        );
    }

    #[test]
    fn a_command_after_a_heredoc_terminator_is_its_own_segment() {
        let cmd = "cat <<X\nhi\nX\npython3 ./evil.py";
        assert_eq!(
            raw_segments(cmd),
            vec!["cat <<X\nhi\nX", "python3 ./evil.py"]
        );
    }

    #[test]
    fn a_separator_before_the_marker_on_the_same_line_still_splits() {
        let cmd = "true && python3 ./evil.py <<X\nhi\nX";
        assert_eq!(
            raw_segments(cmd),
            vec!["true", "python3 ./evil.py <<X\nhi\nX"]
        );
    }

    #[test]
    fn an_unterminated_heredoc_swallows_the_rest_of_the_command() {
        let cmd = "cat <<X\nhi\ntrue; python3 ./evil.py";
        assert_eq!(
            raw_segments(cmd),
            vec!["cat <<X\nhi\ntrue; python3 ./evil.py"]
        );
    }

    #[test]
    fn a_brace_group_stays_one_segment_despite_an_internal_semicolon() {
        let cmd = "{ cd -- d1; }; python3 ./sub/evil.py";
        assert_eq!(
            raw_segments(cmd),
            vec!["{ cd -- d1; }", "python3 ./sub/evil.py"]
        );
    }

    #[test]
    fn brace_expansion_with_no_surrounding_whitespace_is_not_treated_as_a_group() {
        let cmd = "echo {a,b}; python3 script.py";
        assert_eq!(raw_segments(cmd), vec!["echo {a,b}", "python3 script.py"]);
    }

    #[test]
    fn case_keyword_in_argument_position_does_not_suspend_splitting() {
        let cmd = "grep case f; python3 ./evil.py; grep esac f";
        assert_eq!(
            raw_segments(cmd),
            vec!["grep case f", "python3 ./evil.py", "grep esac f"]
        );
    }

    #[test]
    fn case_and_esac_as_echo_arguments_do_not_pair_up() {
        let cmd = "echo case x in; python3 ./evil.py; echo esac";
        assert_eq!(
            raw_segments(cmd),
            vec!["echo case x in", "python3 ./evil.py", "echo esac"]
        );
    }

    #[test]
    fn quoted_case_keyword_does_not_open_a_suspend_range() {
        let cmd = "echo \"case\" x in; python3 ./evil.py; echo esac";
        assert_eq!(
            raw_segments(cmd),
            vec!["echo \"case\" x in", "python3 ./evil.py", "echo esac"]
        );
    }

    #[test]
    fn a_quoted_heredoc_marker_does_not_suspend_splitting() {
        let cmd = "echo '<<EOF'; python3 ./evil.py";
        assert_eq!(raw_segments(cmd), vec!["echo '<<EOF'", "python3 ./evil.py"]);
    }

    #[test]
    fn case_used_as_a_filename_does_not_suspend_splitting() {
        let cmd = "cat case; python3 ./evil.py; cat esac";
        assert_eq!(
            raw_segments(cmd),
            vec!["cat case", "python3 ./evil.py", "cat esac"]
        );
    }

    #[test]
    fn esac_inside_a_quoted_string_does_not_close_a_case_statement_early() {
        let cmd = "case a in x) echo \"esac\";; esac; python3 ./evil.py";
        assert_eq!(
            raw_segments(cmd),
            vec!["case a in x) echo \"esac\";; esac", "python3 ./evil.py"]
        );
    }

    #[test]
    fn a_nested_case_statement_is_tracked_by_depth() {
        let cmd = "case a in x) (case b in y) true;; esac) ;; esac; python3 ./evil.py";
        assert_eq!(
            raw_segments(cmd),
            vec![
                "case a in x) (case b in y) true;; esac) ;; esac",
                "python3 ./evil.py"
            ]
        );
    }
}
