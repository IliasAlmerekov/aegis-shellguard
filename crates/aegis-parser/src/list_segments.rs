//! Top-level list-segment splitting with separator tracking (issue #384).
//!
//! Split out of `segmentation.rs` to stay under the repo's 800-line file-size
//! budget. A sibling of `segmentation` under the crate root, so it reaches
//! `segmentation`'s shared scanning helpers through `pub(crate)`.

use std::ops::Range;

use crate::PipelineChain;
use crate::embedded_scripts::heredoc_suspend_ranges;
use crate::segmentation::{ends_with_redirect_target, finalize_segment, split_pipeline_segments};

/// The top-level shell list operator connecting one [`ListSegment`] to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSeparator {
    /// `&&` — logical AND.
    And,
    /// `||` — logical OR.
    Or,
    /// `;` — unconditional sequencing.
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

/// Split `cmd` into top-level list segments — delimited by `;`, `&&`, `||`,
/// background `&`, and newlines — each carrying its own pipeline chain
/// (`|`-delimited stages) and the operator that follows it.
///
/// Unlike [`crate::top_level_pipelines`], every group is returned even when it has
/// only one pipeline stage: a caller that must route every top-level command
/// of a compound command, not only its multi-stage pipelines, uses this
/// instead (issue #384).
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

/// Byte ranges spanning a whole top-level `case ... esac` statement (issue
/// #384 G1): a `case` block's own `;;`/`;;&`/`;&` arm terminators and any `&`
/// inside an arm body are case grammar, not list operators, so top-level
/// splitting must not treat them as one — only the router's own
/// `case`-keyword wrapper handling (which re-parses each arm as its own list)
/// should ever see them as boundaries. Word-boundary and quote/backtick
/// aware; nesting is depth-counted so only the outermost `case`/`esac` pair
/// produces a range, since that range already spans every statement nested
/// inside it. A `case` with no matching `esac` (or vice versa) produces no
/// range at all — the same fail-open gap as the rest of this crate's raw
/// scans for malformed input.
fn case_statement_suspend_ranges(cmd: &str) -> Vec<Range<usize>> {
    if !cmd.contains("case") {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut open_starts: Vec<usize> = Vec::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut at_word_boundary = true;

    let mut chars = cmd.char_indices();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
                at_word_boundary = false;
                continue;
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                at_word_boundary = false;
                continue;
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                at_word_boundary = false;
                continue;
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                at_word_boundary = false;
                continue;
            }
            _ => {}
        }

        if !in_single_quote && !in_double_quote && !in_backticks && at_word_boundary {
            if cmd[idx..]
                .strip_prefix("case")
                .is_some_and(|rest| rest.starts_with(char::is_whitespace))
            {
                open_starts.push(idx);
            } else if let Some(rest) = cmd[idx..].strip_prefix("esac")
                && rest
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '_')
                && let Some(start) = open_starts.pop()
                && open_starts.is_empty()
            {
                ranges.push(start..idx + "esac".len());
            }
        }

        at_word_boundary = ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(');
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

/// [`split_top_level_command_groups`], additionally recording the
/// [`ListSeparator`] immediately following each returned group (`None` for
/// the last group in `cmd`). The separator recorded for a group is whichever
/// one the scanner encountered right after finishing that group's text — an
/// empty group between two separators (e.g. `a;;b`, or a leading `;`) has no
/// preceding text to decorate, so no entry is recorded for it.
///
/// A heredoc marker (issue #384) suspends list-operator recognition from its
/// `<<` through the end of its terminator line: the body is data, never
/// further segments, and anything chained on the marker's own physical line
/// (e.g. `cat > f <<EOF && python3 f`) stays glued to the segment that owns
/// the marker rather than splitting there.
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
            // `{ ...; }` grouping syntax: real shell grammar requires `{` to
            // stand alone as a word followed by whitespace, and `}` to stand
            // alone right after a `;`/newline/whitespace — tracked the same
            // way as `(...)` so a `cd` inside the group is never split away
            // from the braces that make it a single unit to route (ADR-022
            // §6, issue #384 S3: a group's `cd` persists to the caller's
            // cwd, unlike a subshell's, and the router must see the whole
            // wrapper to tell the two apart). A `{` immediately after `)`
            // additionally opens a POSIX function body (`f(){ ...; }`, issue
            // #384 G1): unlike a bare grouping `{`, real shell grammar allows
            // no whitespace there, so this is the one preceding character a
            // grouping `{` itself could never have — no ambiguity between
            // the two shapes.
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
                    // `&&` — logical AND
                    chars.next();
                    finalize_with_separator(
                        ListSeparator::And,
                        &mut current,
                        &mut segments,
                        &mut separators,
                    );
                } else if next_char != Some('>') && !ends_with_redirect_target(&current) {
                    // Standalone background `&` — a command separator.
                    // Excludes redirect forms `&>` / `&>>` (peek is `>`) and
                    // unescaped `>&` / `<&` / `2>&1` / `3>&-` (preceding char is
                    // an unescaped `>` or `<`). An escaped `\>` is a literal arg.
                    finalize_with_separator(
                        ListSeparator::Background,
                        &mut current,
                        &mut segments,
                        &mut separators,
                    );
                } else {
                    // Part of a redirect operator — keep as an ordinary char.
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

    // Every recorded separator decorates the segment pushed right before it,
    // so `separators` and `segments` are already in lockstep, modulo the
    // final segment (pushed by the unconditional `finalize_segment` call
    // above, with no separator tracked for it) and a command that ends right
    // on a separator (e.g. `"a;"`), where the last recorded separator would
    // otherwise dangle off the end with nothing after it. Either way, the
    // true last segment never has a following separator, so it is forced to
    // `None` explicitly rather than relied on to fall out of the arithmetic.
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
        // Issue #384: a `;` inside a heredoc body used to be indistinguishable
        // from a real command separator, so the body could slice a later
        // command away from its owning segment.
        let cmd = ": <<X\nhi; there\nX\ntrue; python3 ./evil.py";
        assert_eq!(
            raw_segments(cmd),
            vec![": <<X\nhi; there\nX", "true", "python3 ./evil.py"]
        );
    }

    #[test]
    fn heredoc_owning_segment_keeps_a_same_line_chained_exec_glued() {
        // The `&&`-chained exec on the marker's own line must not become a
        // separate segment: `heredoc_write_then_exec_reuse` (router.rs) needs
        // the whole thing as one unit to recognize the write-then-exec shape.
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
        // Issue #384 S3: without this, `{ cd -- d1; }` used to split at the
        // internal `;`, so the router never saw the whole group and could
        // not tell a `cd` that persists to the caller's cwd from one it
        // cannot resolve.
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
}
