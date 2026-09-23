//! Top-level list-segment splitting with separator tracking (issue #384).
//!
//! Split out of `segmentation.rs` to stay under the repo's 800-line file-size
//! budget. A sibling of `segmentation` under the crate root, so it reaches
//! `segmentation`'s shared scanning helpers through `pub(crate)`.

use crate::PipelineChain;
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
/// Unlike [`top_level_pipelines`], every group is returned even when it has
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

/// [`split_top_level_command_groups`], additionally recording the
/// [`ListSeparator`] immediately following each returned group (`None` for
/// the last group in `cmd`). The separator recorded for a group is whichever
/// one the scanner encountered right after finishing that group's text — an
/// empty group between two separators (e.g. `a;;b`, or a leading `;`) has no
/// preceding text to decorate, so no entry is recorded for it.
pub(crate) fn split_top_level_command_groups_with_separators(
    cmd: &str,
) -> Vec<(String, Option<ListSeparator>)> {
    let mut segments = Vec::new();
    let mut separators = Vec::new();
    let mut current = String::new();
    let mut chars = cmd.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut command_subst_depth = 0usize;

    // Finalize `current`, and if that pushed a new segment, record `kind` as
    // the separator immediately following it.
    macro_rules! finalize_with_separator {
        ($kind:expr) => {{
            let had_text = !current.trim().is_empty();
            finalize_segment(&mut current, &mut segments);
            if had_text {
                separators.push($kind);
            }
        }};
    }

    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                current.push(ch);
                if let Some(next) = chars.next() {
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
            '$' if !in_single_quote && !in_backticks && chars.peek() == Some(&'(') => {
                command_subst_depth += 1;
                current.push(ch);
                if let Some(next) = chars.next() {
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
            '\n' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                finalize_with_separator!(ListSeparator::Newline);
            }
            ';' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                finalize_with_separator!(ListSeparator::Semicolon);
            }
            '&' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                if chars.peek() == Some(&'&') {
                    // `&&` — logical AND
                    chars.next();
                    finalize_with_separator!(ListSeparator::And);
                } else if chars.peek() != Some(&'>') && !ends_with_redirect_target(&current) {
                    // Standalone background `&` — a command separator.
                    // Excludes redirect forms `&>` / `&>>` (peek is `>`) and
                    // unescaped `>&` / `<&` / `2>&1` / `3>&-` (preceding char is
                    // an unescaped `>` or `<`). An escaped `\>` is a literal arg.
                    finalize_with_separator!(ListSeparator::Background);
                } else {
                    // Part of a redirect operator — keep as an ordinary char.
                    current.push(ch);
                }
            }
            '|' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0
                && chars.peek() == Some(&'|') =>
            {
                chars.next();
                finalize_with_separator!(ListSeparator::Or);
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
