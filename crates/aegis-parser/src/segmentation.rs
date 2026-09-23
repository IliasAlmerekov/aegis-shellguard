use super::{PipelineChain, PipelineSegment, extract_nested_commands, split_tokens};
use crate::list_segments::{SuspendCursor, case_statement_suspend_ranges};

/// Split a raw shell command string into its logical segments.
///
/// Segments are delimited by top-level shell control operators (`&&`, `||`, `;`, `|`)
/// and newlines. Quoting and escaping are respected — operators inside quotes are not
/// separators.
///
/// The returned list is scan-oriented rather than execution-oriented:
/// - top-level command chains become separate segments
/// - shell wrappers such as env-prefix forms contribute an additional stripped segment
/// - leading grouping syntax, reserved words, case arms, and function headers
///   contribute an additional segment beginning with the program
/// - subshell groups and command substitutions contribute normalized inner segments,
///   even when a redirect or comment follows a closing subshell parenthesis
/// - quoted shell strings (for example `bash -c "cmd1 && cmd2"`) keep the outer segment
///   and also contribute the inner normalized commands
///
/// # Examples
///
/// ```text
/// "echo ok && rm -rf /"   → ["echo ok", "rm -rf /"]
/// "cmd1; cmd2; cmd3"       → ["cmd1", "cmd2", "cmd3"]
/// "bash -c 'a && b'"       → ["bash -c a && b", "a", "b"]
/// "echo $(rm -rf /)"       → ["echo $(rm -rf /)", "rm -rf /"]
/// ```
///
/// Used by the scanner as a normalization layer so that dangerous payloads
/// keep their command boundaries even when wrapped in shell syntax.
pub fn logical_segments(cmd: &str) -> Vec<String> {
    let mut segments = Vec::new();

    for raw_segment in split_top_level_segments(cmd) {
        collect_scan_segments(&raw_segment, &mut segments);
    }

    segments
}

/// Extract top-level pipeline chains from shell input.
///
/// Examples:
///
/// ```text
/// "echo ok | sh"                    → [["echo ok", "sh"]]
/// "a | b && c | d"                  → [["a", "b"], ["c", "d"]]
/// "echo 'x|y' | bash"               → [["echo x|y", "bash"]]
/// "echo ok && rm -rf /"             → []
/// ```
pub fn top_level_pipelines(cmd: &str) -> Vec<PipelineChain> {
    split_top_level_command_groups(cmd)
        .into_iter()
        .filter_map(|raw_group| {
            let segments = split_pipeline_segments(&raw_group);
            (segments.len() > 1).then_some(PipelineChain {
                raw: raw_group,
                segments,
            })
        })
        .collect()
}

fn collect_scan_segments(raw_segment: &str, segments: &mut Vec<String>) {
    if let Some(normalized) = normalize_segment(raw_segment) {
        push_unique(segments, normalized);
    }

    if let Some(stripped_env_command) = strip_env_prefix(raw_segment) {
        collect_scan_segments(&stripped_env_command, segments);
    }

    if let Some(stripped_syntax) = strip_leading_shell_syntax(raw_segment) {
        collect_scan_segments(&stripped_syntax, segments);
    }

    for nested in extract_nested_commands(raw_segment) {
        collect_scan_segments(&nested, segments);
    }

    if let Some(subshell_body) = unwrap_subshell_group(raw_segment) {
        // Re-split on the body's own top-level separators (issue #430) —
        // without this, `(true; git push --force origin main)` stayed one
        // unsplit string and the interpreter/program past the `;` was never
        // exposed as its own segment, the same gap `extract_command_substitution_bodies`
        // below already avoids for `$( )`/backtick bodies.
        for nested_segment in split_top_level_segments(&subshell_body) {
            collect_scan_segments(&nested_segment, segments);
        }
    }

    for body in extract_command_substitution_bodies(raw_segment) {
        for nested_segment in split_top_level_segments(&body) {
            collect_scan_segments(&nested_segment, segments);
        }
    }
}

pub(super) fn split_top_level_segments(cmd: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = cmd.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut command_subst_depth = 0usize;

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
                finalize_segment(&mut current, &mut segments);
            }
            ';' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                finalize_segment(&mut current, &mut segments);
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
                    finalize_segment(&mut current, &mut segments);
                } else if chars.peek() != Some(&'>') && !ends_with_redirect_target(&current) {
                    // Standalone background `&` — a command separator.
                    // Excludes redirect forms `&>` / `&>>` (peek is `>`) and
                    // unescaped `>&` / `<&` / `2>&1` / `3>&-` (preceding char is
                    // an unescaped `>` or `<`). An escaped `\>` is a literal arg.
                    finalize_segment(&mut current, &mut segments);
                } else {
                    // Part of a redirect operator — keep as an ordinary char.
                    current.push(ch);
                }
            }
            '|' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                finalize_segment(&mut current, &mut segments);
            }
            _ => current.push(ch),
        }
    }

    finalize_segment(&mut current, &mut segments);
    segments
}

fn split_top_level_command_groups(cmd: &str) -> Vec<String> {
    crate::list_segments::split_top_level_command_groups_with_separators(cmd)
        .into_iter()
        .map(|(raw, _separator)| raw)
        .collect()
}

pub(crate) fn split_pipeline_segments(raw_group: &str) -> Vec<PipelineSegment> {
    let mut raw_segments = Vec::new();
    let mut current = String::new();
    let case_ranges = case_statement_suspend_ranges(raw_group);
    let mut suspend = SuspendCursor::new(&case_ranges);
    let mut chars = raw_group.char_indices().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut command_subst_depth = 0usize;

    while let Some((byte_idx, ch)) = chars.next() {
        // A `case` pattern's own `)`/`|` (e.g. `x|y)`) is arm grammar, not a
        // pipe or a paren-close, so this whole command's byte span is opaque
        // here the same way it already is at the top-level list-segment
        // split — otherwise `case x in x|y) python3 evil.py;; esac` splits
        // into two bogus pipeline stages at the pattern's `|` (issue #384).
        if suspend.contains(byte_idx) {
            current.push(ch);
            continue;
        }
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
            '$' if !in_single_quote
                && !in_backticks
                && chars.peek().map(|&(_, c)| c) == Some('(') =>
            {
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
            '|' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0
                && chars.peek().map(|&(_, c)| c) != Some('|')
                && !ends_with_redirect_target(&current) =>
            {
                // `>|` (e.g. `>|out`, the noclobber-override redirect)
                // glues a `|` straight onto an unescaped `>`/`<` — that is
                // redirection syntax, not a pipeline separator (issue #384).
                finalize_segment(&mut current, &mut raw_segments);
            }
            _ => current.push(ch),
        }
    }

    finalize_segment(&mut current, &mut raw_segments);

    raw_segments
        .into_iter()
        .filter_map(|raw| {
            normalize_segment(&raw).map(|normalized| PipelineSegment { raw, normalized })
        })
        .collect()
}

/// Returns `true` when `current` (ignoring trailing whitespace) ends with an
/// *unescaped* redirect target char (`>` or `<`), meaning a following `&` is
/// part of a `>&` / `<&` file-descriptor duplication operator (`2>&1`, `3>&-`,
/// `cat 0<&3`, …) rather than a background command separator.
///
/// An escaped `\>` / `\<` is a literal argument character, not a redirect, so
/// this returns `false` and the `&` is treated as a separator. Escaping is
/// determined by the parity of the run of backslashes immediately preceding the
/// redirect char (odd = escaped).
///
/// Shared by both `split_top_level_segments` and `split_pipeline_segments`
/// so the security-critical background-`&` decision has a single source of truth.
pub(crate) fn ends_with_redirect_target(current: &str) -> bool {
    let mut rev = current.trim_end().chars().rev();
    match rev.next() {
        Some('>') | Some('<') => rev.take_while(|&c| c == '\\').count() % 2 == 0,
        _ => false,
    }
}

pub(crate) fn finalize_segment(current: &mut String, segments: &mut Vec<String>) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }
    current.clear();
}

fn normalize_segment(raw_segment: &str) -> Option<String> {
    let tokens = split_tokens(raw_segment);
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

fn push_unique(segments: &mut Vec<String>, segment: String) {
    if !segment.is_empty() && !segments.iter().any(|existing| existing == &segment) {
        segments.push(segment);
    }
}

fn strip_env_prefix(raw_segment: &str) -> Option<String> {
    let tokens = split_tokens(raw_segment);
    if tokens.is_empty() {
        return None;
    }

    let mut idx = 0;
    let mut stripped_any = false;

    if tokens.get(idx).map(String::as_str) == Some("env") {
        idx += 1;
        stripped_any = true;
    }

    while let Some(token) = tokens.get(idx) {
        if token.contains('=') && !token.starts_with('-') {
            idx += 1;
            stripped_any = true;
        } else {
            break;
        }
    }

    if stripped_any && idx < tokens.len() {
        Some(tokens[idx..].join(" "))
    } else {
        None
    }
}

fn strip_leading_shell_syntax(raw_segment: &str) -> Option<String> {
    let segment = raw_segment.trim_start();

    if strip_shell_keyword(segment, "case").is_some() {
        let tokens = split_tokens(segment);
        if tokens.get(2).is_some_and(|token| token == "in") && tokens.len() > 3 {
            return Some(tokens[3..].join(" "));
        }
    }

    if let Some(header) = strip_shell_keyword(segment, "function")
        && let Some((name, body)) = header.split_once('{')
        && !name.trim().is_empty()
        && !name.trim().chars().any(char::is_whitespace)
    {
        return Some(body.trim_start().to_string());
    }

    for prefix in std::iter::once("{").chain(crate::COMMAND_STARTING_KEYWORDS) {
        if let Some(rest) = strip_shell_keyword(segment, prefix) {
            return Some(rest.to_string());
        }
    }

    if let Some((pattern, rest)) = segment.split_once(')')
        && !pattern.is_empty()
        && !pattern.chars().any(char::is_whitespace)
        && rest.chars().next().is_some_and(char::is_whitespace)
    {
        return Some(rest.trim_start().to_string());
    }
    None
}

fn strip_shell_keyword<'a>(segment: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = segment.strip_prefix(keyword)?;
    rest.chars()
        .next()
        .is_some_and(char::is_whitespace)
        .then(|| rest.trim_start())
}

/// `Some(inner)` when `raw_segment`, trimmed, starts with a `(...)` subshell —
/// the raw text between the matching parens, quote/backtick-aware, ignoring
/// anything after the close (a redirect, `&&`-chained command, ...). Returns
/// the exact source bytes, not a dequoted/normalized copy, so a caller that
/// needs to route an inline interpreter body found inside keeps its original
/// quoting.
pub fn unwrap_subshell_group(raw_segment: &str) -> Option<String> {
    let trimmed = raw_segment.trim();
    if !trimmed.starts_with('(') {
        return None;
    }

    let case_ranges = case_statement_suspend_ranges(trimmed);
    let mut suspend = SuspendCursor::new(&case_ranges);
    let mut chars = trimmed.char_indices().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut command_subst_depth = 0usize;
    let mut close_idx = None;

    while let Some((idx, ch)) = chars.next() {
        // A `case` arm's own `)` (e.g. `x)`) is arm grammar, not this
        // wrapper's closing paren — without this guard a nested
        // `case ... esac` would close the subshell at the pattern's first
        // `)` and drop everything from the arm body onward (issue #384).
        if suspend.contains(idx) {
            continue;
        }
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
            }
            '$' if !in_single_quote
                && !in_backticks
                && chars.peek().map(|&(_, c)| c) == Some('(') =>
            {
                command_subst_depth += 1;
                chars.next();
            }
            '(' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && command_subst_depth == 0 =>
            {
                paren_depth += 1;
            }
            ')' if !in_single_quote
                && !in_backticks
                && (command_subst_depth > 0 || (!in_double_quote && paren_depth > 0)) =>
            {
                if command_subst_depth > 0 {
                    command_subst_depth -= 1;
                } else {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        close_idx = Some(idx);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(close_idx) = close_idx {
        let inner = trimmed[1..close_idx].trim();
        if !inner.is_empty() {
            return Some(inner.to_string());
        }
    }

    None
}

/// Every top-level `$(...)`/backtick command-substitution body in
/// `raw_segment`, quote/nesting-aware, in the exact source bytes rather than
/// a dequoted/normalized copy.
pub fn extract_command_substitution_bodies(raw_segment: &str) -> Vec<String> {
    let case_ranges = case_statement_suspend_ranges(raw_segment);
    let mut bodies = Vec::new();
    let mut idx = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while idx < raw_segment.len() {
        let Some(ch) = raw_segment[idx..].chars().next() else {
            break;
        };
        let ch_len = ch.len_utf8();

        match ch {
            '\\' if !in_single_quote => {
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '$' if !in_single_quote && raw_segment[idx + ch_len..].starts_with('(') => {
                if let Some((body, end_idx)) =
                    extract_dollar_paren_body(raw_segment, idx, &case_ranges)
                {
                    bodies.push(body);
                    idx = end_idx;
                    continue;
                }
            }
            '`' if !in_single_quote => {
                if let Some((body, end_idx)) = extract_backtick_body(raw_segment, idx) {
                    bodies.push(body);
                    idx = end_idx;
                    continue;
                }
            }
            _ => {}
        }
        idx += ch_len;
    }

    bodies
}

/// `Some((body, end_idx))` when a `$(...)` command substitution starts at
/// `raw_segment[start_idx..]`, `body` its trimmed inner text and `end_idx`
/// the byte offset right past its closing `)`. `case_ranges` — precomputed
/// once by the caller over the whole `raw_segment` — marks every top-level
/// `case ... esac` span so its arms' own `)` (`x)`) never misreads as this
/// substitution's close (issue #384).
fn extract_dollar_paren_body(
    raw_segment: &str,
    start_idx: usize,
    case_ranges: &[std::ops::Range<usize>],
) -> Option<(String, usize)> {
    let mut suspend = SuspendCursor::new(case_ranges);
    let body_start = start_idx + 2;
    let mut idx = body_start;
    let mut depth = 1usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;

    while idx < raw_segment.len() {
        let ch = raw_segment[idx..].chars().next()?;
        let ch_len = ch.len_utf8();

        if suspend.contains(idx) {
            idx += ch_len;
            continue;
        }

        match ch {
            '\\' if !in_single_quote => {
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '\'' if !in_double_quote && !in_backticks => in_single_quote = !in_single_quote,
            '"' if !in_single_quote && !in_backticks => in_double_quote = !in_double_quote,
            '`' if !in_single_quote => in_backticks = !in_backticks,
            '$' if !in_single_quote
                && !in_backticks
                && raw_segment[idx + ch_len..].starts_with('(') =>
            {
                depth += 1;
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '(' if !in_single_quote && !in_double_quote && !in_backticks => depth += 1,
            ')' if !in_single_quote && !in_backticks && !in_double_quote => {
                depth -= 1;
                if depth == 0 {
                    return Some((
                        raw_segment[body_start..idx].trim().to_string(),
                        idx + ch_len,
                    ));
                }
            }
            _ => {}
        }
        idx += ch_len;
    }

    None
}

/// `Some((body, end_idx))` when a backtick command substitution starts at
/// `raw_segment[start_idx..]`, `body` its trimmed inner text and `end_idx`
/// the byte offset right past its closing backtick. No `case`-arm awareness
/// needed here: unlike `$(...)`, a backtick body's end is the next
/// unescaped backtick, not paren depth, so a case pattern's `)` cannot
/// close it early.
fn extract_backtick_body(raw_segment: &str, start_idx: usize) -> Option<(String, usize)> {
    let body_start = start_idx + 1;
    let mut idx = body_start;

    while idx < raw_segment.len() {
        let ch = raw_segment[idx..].chars().next()?;
        let ch_len = ch.len_utf8();

        match ch {
            '\\' => {
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '`' => {
                return Some((
                    raw_segment[body_start..idx].trim().to_string(),
                    idx + ch_len,
                ));
            }
            _ => {}
        }
        idx += ch_len;
    }

    None
}
