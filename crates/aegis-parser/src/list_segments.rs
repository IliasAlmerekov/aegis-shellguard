//! Top-level list-segment splitting with separator tracking (#384).

use std::ops::Range;

use crate::PipelineChain;
use crate::embedded_scripts::heredoc_suspend_ranges;
use crate::segmentation::{ends_with_redirect_target, finalize_segment, split_pipeline_segments};

/// Walks a byte offset forward through sorted, non-overlapping opaque byte ranges
/// (such as `case` statements or heredoc bodies).
pub(crate) struct SuspendCursor<'a> {
    remaining: std::slice::Iter<'a, Range<usize>>,
    active: Option<&'a Range<usize>>,
}

impl<'a> SuspendCursor<'a> {
    pub(crate) fn new(ranges: &'a [Range<usize>]) -> Self {
        let mut remaining = ranges.iter();
        let active = remaining.next();
        Self { remaining, active }
    }

    /// Advances past any ranges that have already ended by `byte_idx`, then
    /// reports whether `byte_idx` falls inside the (possibly new) active
    /// one. `byte_idx` must be non-decreasing across calls on one cursor.
    pub(crate) fn contains(&mut self, byte_idx: usize) -> bool {
        while self.active.is_some_and(|range| range.end <= byte_idx) {
            self.active = self.remaining.next();
        }
        self.active.is_some_and(|range| range.contains(&byte_idx))
    }
}

/// The top-level shell list operator connecting one [`ListSegment`] to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSeparator {
    /// `&&` - logical AND.
    And,
    /// `||` - logical OR.
    Or,
    /// `;` - unconditional sequencing.
    Semicolon,
    /// A standalone background `&`.
    Background,
    /// A literal newline.
    Newline,
}

/// One top-level list segment: its full pipeline chain (a single stage when
/// the segment has no `|`) and the list operator connecting it to the next
/// segment, `None` for the last segment in the command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSegment {
    /// This segment's pipeline chain. `segments.len() == 1` for a plain
    /// (non-piped) command.
    pub pipeline: PipelineChain,
    /// The operator separating this segment from the next, `None` at the end.
    pub separator: Option<ListSeparator>,
}

/// Split `cmd` into top-level list segments delimited by `;`, `&&`, `||`,
/// background `&`, and newlines, each with its pipeline chain and trailing
/// separator (#384).
///
/// Unlike [`crate::top_level_pipelines`], single-stage groups are included.
pub fn list_segments(cmd: &str) -> Vec<ListSegment> {
    split_top_level_command_groups_with_separators(cmd)
        .into_iter()
        .map(|(raw_group, separator)| ListSegment {
            pipeline: PipelineChain {
                segments: split_pipeline_segments(&raw_group),
                raw: raw_group,
            },
            separator,
        })
        .collect()
}

/// Fold a completed word into the `case`/`esac` nesting scan and report
/// whether the next word stays in command position.
fn note_word_boundary(
    word: &str,
    word_start: usize,
    word_end: usize,
    was_command_position: bool,
    open_starts: &mut Vec<usize>,
    ranges: &mut Vec<Range<usize>>,
) -> bool {
    if !was_command_position {
        return false;
    }
    if word == "case" {
        open_starts.push(word_start);
        false
    } else if word == "esac" {
        if let Some(start) = open_starts.pop()
            && open_starts.is_empty()
        {
            ranges.push(start..word_end);
        }
        false
    } else {
        crate::COMMAND_STARTING_KEYWORDS.contains(&word)
    }
}

/// Byte ranges spanning outermost top-level `case ... esac` statements (#384).
///
/// Only `case`/`esac` in command position open or close a range so argument
/// words like `grep case f` never suspend list splitting. Unmatched `case`
/// produces no range.
pub(crate) fn case_statement_suspend_ranges(cmd: &str) -> Vec<Range<usize>> {
    if !cmd.contains("case") {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut open_starts: Vec<usize> = Vec::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    // `true` right before the *next* word starts means that word begins in
    // command position. Starts `true`: the very start of `cmd` is command
    // position, matching every other top-level list segment's own start.
    let mut command_position = true;
    let mut word_start: Option<usize> = None;

    let mut chars = cmd.char_indices();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                word_start.get_or_insert(idx);
                chars.next();
                continue;
            }
            '\'' if !in_double_quote && !in_backticks => {
                word_start.get_or_insert(idx);
                in_single_quote = !in_single_quote;
                continue;
            }
            '"' if !in_single_quote && !in_backticks => {
                word_start.get_or_insert(idx);
                in_double_quote = !in_double_quote;
                continue;
            }
            '`' if !in_single_quote => {
                word_start.get_or_insert(idx);
                in_backticks = !in_backticks;
                continue;
            }
            _ => {}
        }

        if in_single_quote || in_double_quote || in_backticks {
            // Inside a quoted/backticked span every byte — including
            // whitespace — stays part of the current word; a quoted string
            // is one token even when it embeds spaces.
            word_start.get_or_insert(idx);
            continue;
        }

        // A separator ends the current word *and* puts the next word in
        // command position; plain whitespace only ends the word.
        let is_separator = matches!(ch, ';' | '&' | '|' | '(' | ')' | '{') || ch == '\n';
        if ch.is_whitespace() || is_separator {
            if let Some(start) = word_start.take() {
                command_position = note_word_boundary(
                    &cmd[start..idx],
                    start,
                    idx,
                    command_position,
                    &mut open_starts,
                    &mut ranges,
                );
            }
            if is_separator {
                command_position = true;
            }
            continue;
        }

        word_start.get_or_insert(idx);
    }

    if let Some(start) = word_start.take() {
        note_word_boundary(
            &cmd[start..],
            start,
            cmd.len(),
            command_position,
            &mut open_starts,
            &mut ranges,
        );
    }

    ranges
}

/// Finalize `current` into `segments`, and if that pushed a new segment,
/// record `kind` as the separator immediately following it.
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

/// Split `cmd` into top-level command groups paired with their trailing [`ListSeparator`].
///
/// Heredoc and `case ... esac` spans stay opaque so internal separators do not split them (#384).
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
            // Heredoc marker, body, or terminator text: opaque data, carried
            // through verbatim with none of the separator/quote handling
            // below (a quoted delimiter's own quotes must not toggle outer
            // quote state, and a `&&`/`;` here must not split the segment).
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
            // Track `{ ...; }` groups and `f(){ ...; }` function bodies as single
            // units so the router sees the whole wrapper (ADR-022 §6, #384).
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
                    // Standalone background `&` (excludes `&>`, `&>>`, `>&`, `<&`).
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

    // Force the final segment's trailing separator to `None` (even for trailing `;`).
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
        list_segments(cmd)
            .into_iter()
            .map(|s| s.pipeline.raw)
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
        // Keeps the same-line `&&` chain in one segment for write-then-exec detection (#384).
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
        // Only command-position `case`/`esac` suspend splitting (#384).
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
