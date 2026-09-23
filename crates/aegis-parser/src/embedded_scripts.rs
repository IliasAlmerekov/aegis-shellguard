use std::ops::Range;

use super::{segmentation::split_top_level_segments, split_tokens};

use aegis_types::InlineScript;

/// A heredoc (or nowdoc) body extracted from a multi-line command string.
#[derive(Debug, PartialEq)]
pub struct HeredocBody {
    /// The delimiter word (e.g., `EOF`, `SCRIPT`).
    pub delimiter: String,
    /// The lines of text between the opening marker and the closing delimiter.
    pub body: String,
    /// `true` when the delimiter was quoted (`<<'EOF'`, `<<"EOF"`, `<<\EOF`),
    /// indicating nowdoc semantics: bash performs zero expansion (including
    /// backtick/`$()` command substitution) on the body text.
    pub is_nowdoc: bool,
    /// `true` when the command directly ahead of the heredoc operator is a
    /// shell or scripting interpreter that will itself execute the body as
    /// code once it reads it from stdin (e.g. `bash <<'EOF'`, `python3
    /// <<'EOF'`). Nowdoc semantics only suppress bash's own expansion during
    /// heredoc construction — they say nothing about what the recipient
    /// program does with the literal text afterward.
    pub target_is_interpreter: bool,
    /// `true` when the heredoc feeds a command that copies stdin verbatim to
    /// a file (`cat >> notes.txt <<'EOF'`, `tee out.txt <<'EOF'`) rather than
    /// executing or displaying it. The body is pure data at rest once
    /// written; nothing in the pipeline ever runs or echoes it back.
    pub target_redirects_to_file: bool,
}

/// Programs that execute their stdin as code, independent of what bash's own
/// heredoc expansion does. A nowdoc body handed to one of these must still be
/// recursively scanned: the recipient interprets the raw text as a script.
const STDIN_EXECUTING_PROGRAMS: &[&str] = &[
    "bash", "sh", "zsh", "dash", "ash", "ksh", "python", "python3", "node", "nodejs", "ruby",
    "php", "lua", "perl",
];

/// Programs that copy their stdin verbatim to a file destination rather than
/// interpreting or echoing it back.
const STDIN_TO_FILE_PROGRAMS: &[&str] = &["cat", "tee"];

/// `true` when `line[..marker_start]` invokes `cat`/`tee` in a way whose
/// destination is a file rather than the terminal: `cat >> path`, `cat >
/// path`, or `tee path` (flags aside). Only the command token at the start
/// of the line is recognized, so a heredoc target reached through a
/// pipeline (`foo | cat >> out`) is intentionally not matched here.
fn heredoc_target_writes_to_file(line: &str, marker_start: usize) -> bool {
    let prefix = line[..marker_start].trim_end();
    let tokens = split_tokens(prefix);
    let Some(first) = tokens.first() else {
        return false;
    };
    let basename = first
        .rsplit_once('/')
        .map_or(first.as_str(), |(_, tail)| tail);

    if !STDIN_TO_FILE_PROGRAMS
        .iter()
        .any(|program| basename.eq_ignore_ascii_case(program))
    {
        return false;
    }

    match basename.to_ascii_lowercase().as_str() {
        "cat" => tokens
            .iter()
            .enumerate()
            .any(|(idx, token)| stdout_redirects_to_file(token, tokens.get(idx + 1))),
        "tee" => tokens.iter().skip(1).any(|token| !token.starts_with('-')),
        _ => false,
    }
}

/// `true` when `token` (with `next`, the token right after it, for the
/// bare-operator case) is an actual stdout-to-file redirect — `>`, `>>`, or
/// either glued to a filename — rather than file-descriptor duplication
/// like `2>&1` or `>&2`. Descriptor duplication never touches a file, so it
/// must not exempt a heredoc body from scanning.
fn stdout_redirects_to_file(token: &str, next: Option<&String>) -> bool {
    let Some(rest) = token.strip_prefix(">>").or_else(|| token.strip_prefix('>')) else {
        return false;
    };

    if !rest.is_empty() {
        return !rest.starts_with('&');
    }

    next.is_some_and(|next| !next.is_empty() && !next.starts_with('&'))
}

/// Resolve the basename-normalized program token immediately preceding the
/// heredoc operator on `line` (e.g. `bash` in `foo | bash <<'EOF'`).
fn heredoc_target_program(line: &str, marker_start: usize) -> Option<&str> {
    let prefix = line[..marker_start].trim_end();
    let end = prefix.len();
    let mut start = end;

    for (idx, ch) in prefix.char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' || ch == '/' {
            start = idx;
        } else {
            break;
        }
    }

    if start == end {
        return None;
    }

    let token = &prefix[start..end];
    let basename = token.rsplit_once('/').map_or(token, |(_, tail)| tail);
    if basename.is_empty() {
        None
    } else {
        Some(basename)
    }
}

/// Extract process-substitution bodies from shell forms like `<(...)` (input)
/// and `>(...)` (output, issue #384 G1) alike.
///
/// The returned strings are the shell commands inside the substitution,
/// without the surrounding `<(`/`>(` and `)`.
pub fn extract_process_substitution_bodies(cmd: &str) -> Vec<String> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut bodies = Vec::new();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;

    while i < chars.len() {
        match chars[i] {
            '\\' if !in_single_quote => {
                i = (i + 2).min(chars.len());
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                i += 1;
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                i += 1;
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                i += 1;
            }
            '<' | '>'
                if !in_single_quote
                    && !in_double_quote
                    && !in_backticks
                    && chars.get(i + 1) == Some(&'(') =>
            {
                if let Some((body, end_idx)) = extract_angle_paren_body(&chars, i) {
                    bodies.push(body);
                    i = end_idx + 1;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    bodies
}

fn extract_angle_paren_body(chars: &[char], start_idx: usize) -> Option<(String, usize)> {
    let mut body = String::new();
    let mut idx = start_idx + 2;
    let mut depth = 1usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;

    while idx < chars.len() {
        match chars[idx] {
            '\\' if !in_single_quote => {
                body.push(chars[idx]);
                idx += 1;
                if let Some(next) = chars.get(idx) {
                    body.push(*next);
                    idx += 1;
                }
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                body.push(chars[idx]);
                idx += 1;
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                body.push(chars[idx]);
                idx += 1;
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                body.push(chars[idx]);
                idx += 1;
            }
            '(' if !in_single_quote && !in_double_quote && !in_backticks => {
                depth += 1;
                body.push(chars[idx]);
                idx += 1;
            }
            ')' if !in_single_quote && !in_double_quote && !in_backticks => {
                depth -= 1;
                if depth == 0 {
                    return Some((body.trim().to_string(), idx));
                }
                body.push(chars[idx]);
                idx += 1;
            }
            _ => {
                body.push(chars[idx]);
                idx += 1;
            }
        }
    }

    None
}

/// Extract `eval` payload strings from logical shell segments.
///
/// This unwraps the arguments passed to `eval` so nested shell or interpreter
/// bodies can be analyzed recursively. Variable-only forms such as `eval "$VAR"`
/// remain opaque and are returned as-is when no literal body is available.
pub fn extract_eval_payloads(cmd: &str) -> Vec<String> {
    let mut payloads = Vec::new();

    for segment in split_top_level_segments(cmd) {
        let tokens = split_tokens(&segment);
        if tokens.is_empty() {
            continue;
        }

        let mut idx = 0;

        if tokens.get(idx).map(String::as_str) == Some("env") {
            idx += 1;
        }

        while let Some(token) = tokens.get(idx) {
            if token.contains('=') && !token.starts_with('-') {
                idx += 1;
            } else {
                break;
            }
        }

        if tokens.get(idx).map(String::as_str) == Some("eval") && idx + 1 < tokens.len() {
            payloads.push(tokens[idx + 1..].join(" "));
        }
    }

    payloads
}

// Private helper — result of scanning a single line for a heredoc operator.
struct HeredocMarker {
    delimiter: String,
    is_nowdoc: bool,
    strip_tabs: bool,
    /// Byte offset of `<<` within the line that produced this marker, so
    /// callers can locate the target-command token ahead of it without
    /// re-searching the line.
    operator_start: usize,
    /// Byte offset immediately after the delimiter spec (past the closing
    /// quote, or past the bare/escaped word) — the start of whatever else
    /// shares this physical line with the marker (e.g. a `&&`-chained exec).
    delimiter_end: usize,
}

/// Scan one line for a heredoc operator (`<<`) and return the parsed marker.
///
/// Recognises:
/// - `<<WORD`          — regular heredoc
/// - `<<'WORD'`        — nowdoc (single-quoted delimiter)
/// - `<<"WORD"`        — nowdoc (double-quoted delimiter)
/// - `<<\WORD`         — nowdoc (backslash-escaped delimiter)
/// - `<<-WORD`         — heredoc with leading-tab stripping
/// - `<<-'WORD'`       — nowdoc with leading-tab stripping
fn find_heredoc_marker(line: &str) -> Option<HeredocMarker> {
    let operator_start = line.find("<<")?;
    let after_op = &line[operator_start + 2..];
    let (strip_tabs, after_dash) = match after_op.strip_prefix('-') {
        Some(stripped) => (true, stripped),
        None => (false, after_op),
    };
    let trimmed = after_dash.trim_start();
    let spec_start =
        operator_start + 2 + (strip_tabs as usize) + (after_dash.len() - trimmed.len());

    let mk = |delimiter: String, is_nowdoc: bool, delimiter_end: usize| HeredocMarker {
        delimiter,
        is_nowdoc,
        strip_tabs,
        operator_start,
        delimiter_end,
    };

    if let Some(inner) = trimmed.strip_prefix('\'') {
        let close = inner.find('\'')?;
        let delim = &inner[..close];
        return (!delim.is_empty())
            .then(|| mk(delim.to_string(), true, spec_start + 1 + close + 1));
    }

    if let Some(inner) = trimmed.strip_prefix('"') {
        let close = inner.find('"')?;
        let delim = &inner[..close];
        return (!delim.is_empty())
            .then(|| mk(delim.to_string(), true, spec_start + 1 + close + 1));
    }

    if let Some(inner) = trimmed.strip_prefix('\\') {
        let word: String = inner
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let word_len = word.len();
        return (!word.is_empty()).then(|| mk(word, true, spec_start + 1 + word_len));
    }

    let word: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let word_len = word.len();

    (!word.is_empty()).then(|| mk(word, false, spec_start + word_len))
}

/// Split `line` at its first heredoc marker (`<<WORD`, `<<'WORD'`, or the
/// `<<-` tab-stripping variants), returning the text before the marker and
/// the text immediately after the delimiter spec (which, per shell grammar,
/// is still part of the same command line — e.g. a `&&`-chained command).
/// `None` when `line` carries no recognizable marker.
pub fn split_at_heredoc_marker(line: &str) -> Option<(&str, &str)> {
    let marker = find_heredoc_marker(line)?;
    Some((
        &line[..marker.operator_start],
        &line[marker.delimiter_end..],
    ))
}

/// Byte ranges in `cmd`, each spanning from a heredoc marker's `<<` operator
/// through the end of its terminator line (or through the end of `cmd`, for
/// a heredoc that never terminates) — the span list-segment splitting must
/// treat as one opaque unit rather than further command text. This keeps a
/// same-line `&&`-chained exec glued to the segment that owns the marker
/// (mirroring how the body itself is data, not further segments), and never
/// misreads a quoted delimiter's own quote characters as string quoting.
pub(crate) fn heredoc_suspend_ranges(cmd: &str) -> Vec<Range<usize>> {
    if !cmd.contains("<<") {
        return Vec::new();
    }

    let indexed = indexed_lines(cmd);
    let lines: Vec<&str> = indexed.iter().map(|&(line, _)| line).collect();
    let mut ranges = Vec::new();

    walk_heredocs(&lines, |marker, _interpreter, _redirects, body_range| {
        // `body_range.start` is always the marker line's own index plus one
        // (`walk_heredocs` never calls back before that increment), so this
        // never underflows in practice; `checked_sub`/`.get()` make that a
        // fact this can't panic on rather than one this merely relies on.
        let Some(marker_line_index) = body_range.start.checked_sub(1) else {
            return;
        };
        let Some(&(_, marker_line_start)) = indexed.get(marker_line_index) else {
            return;
        };
        let start = marker_line_start + marker.operator_start;
        let end = match indexed.get(body_range.end) {
            Some(&(terminator_line, terminator_start)) => terminator_start + terminator_line.len(),
            None => cmd.len(),
        };
        ranges.push(start..end);
    });

    ranges
}

/// `cmd` split the same way [`str::lines`] does — on `\n`, with one optional
/// trailing `\r` stripped from each line and no phantom empty final line when
/// `cmd` itself ends in `\n` — paired with each line's own byte offset into
/// `cmd`. The byte-offset companion [`str::lines`] does not provide, computed
/// by walking `cmd` once instead of recovering it from a line's pointer
/// address (issue #384 B6): pointer-offset recovery risks reading
/// uninitialized/foreign memory if a caller ever passes a `lines` value that
/// did not originate from this exact `cmd`, and no `unsafe` marks that risk
/// at the call site the way it would for a raw pointer read.
fn indexed_lines(cmd: &str) -> Vec<(&str, usize)> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (idx, ch) in cmd.char_indices() {
        if ch == '\n' {
            let raw = &cmd[start..idx];
            lines.push((raw.strip_suffix('\r').unwrap_or(raw), start));
            start = idx + 1;
        }
    }
    if start < cmd.len() {
        let raw = &cmd[start..];
        lines.push((raw.strip_suffix('\r').unwrap_or(raw), start));
    }
    lines
}

/// Walk `lines` for heredoc/nowdoc markers, invoking `on_heredoc` once per
/// occurrence with the parsed marker, whether its target reads stdin as
/// executable code, and the `[start, end)` line-index range of its body.
///
/// Centralizes the line-walking (marker detection, tab-stripped delimiter
/// matching, target-program resolution) shared by every heredoc-body
/// consumer below, so each of them only has to decide what to do with a
/// given body's line range.
fn walk_heredocs(
    lines: &[&str],
    mut on_heredoc: impl FnMut(&HeredocMarker, bool, bool, Range<usize>),
) {
    let mut i = 0;

    while i < lines.len() {
        if let Some(marker) = find_heredoc_marker(lines[i]) {
            let target_is_interpreter = heredoc_target_program(lines[i], marker.operator_start)
                .is_some_and(|program| {
                    STDIN_EXECUTING_PROGRAMS
                        .iter()
                        .any(|known| program.eq_ignore_ascii_case(known))
                });
            let target_redirects_to_file =
                heredoc_target_writes_to_file(lines[i], marker.operator_start);
            i += 1;
            let body_start = i;

            while i < lines.len() {
                let candidate = if marker.strip_tabs {
                    lines[i].trim_start_matches('\t')
                } else {
                    lines[i]
                };
                if candidate == marker.delimiter {
                    break;
                }
                i += 1;
            }

            on_heredoc(
                &marker,
                target_is_interpreter,
                target_redirects_to_file,
                body_start..i,
            );
        }
        i += 1;
    }
}

/// Extract all heredoc (and nowdoc) bodies from a multi-line command string.
///
/// Each call to this function scans `cmd` line by line, looking for `<<WORD`
/// or `<<'WORD'` markers. When found, it collects every subsequent line until
/// the closing delimiter appears on its own line (with leading tabs stripped
/// when `<<-` was used).
///
/// # Examples
///
/// ```text
/// cmd <<EOF
/// rm -rf /
/// EOF
/// ```
/// → `[HeredocBody { delimiter: "EOF", body: "rm -rf /", is_nowdoc: false }]`
pub fn extract_heredoc_bodies(cmd: &str) -> Vec<HeredocBody> {
    let mut bodies = Vec::new();
    let lines: Vec<&str> = cmd.lines().collect();

    walk_heredocs(
        &lines,
        |marker, target_is_interpreter, target_redirects_to_file, body_range| {
            bodies.push(HeredocBody {
                delimiter: marker.delimiter.clone(),
                body: lines[body_range].join("\n"),
                is_nowdoc: marker.is_nowdoc,
                target_is_interpreter,
                target_redirects_to_file,
            });
        },
    );

    bodies
}

/// Neutralize the inert parts of a nowdoc heredoc body handed to a
/// non-interpreter command (e.g. `cat <<'EOF'`).
///
/// Bash performs zero expansion on a nowdoc body, so backtick/`$(...)`
/// markers in it are inert text — a markdown code span in prose, not live
/// command substitution. Downstream segmentation (`logical_segments`) has no
/// heredoc awareness: it walks `cmd` line by line and would otherwise pull
/// the backtick/`$(...)` contents of these lines out as independent scan
/// targets, so a literal-text match (e.g. a dangerous-looking substring
/// quoted in a commit message) gets scanned as if it were about to execute.
/// Nowdoc bodies routed to an interpreter (`bash <<'EOF'`, `python3
/// <<'EOF'`) are left untouched — the interpreter will execute that text
/// verbatim once it reads it from stdin, marker quoting or not.
///
/// When the same nowdoc body is also handed to a command that writes stdin
/// straight to a file (`cat >> notes.txt <<'EOF'`, `tee out.txt <<'EOF'`),
/// the whole body is blanked rather than just its substitution markers: the
/// text is never executed or displayed, only written to disk, so a
/// dangerous-looking substring in it (a test fixture, a changelog entry) is
/// as inert as the markers are.
///
/// Only the returned copy is affected; the original command text used for
/// audit logging and highlighting is untouched.
pub fn mask_inert_heredoc_substitution_markers(cmd: &str) -> String {
    if !cmd.contains("<<") {
        return cmd.to_string();
    }

    let lines: Vec<&str> = cmd.lines().collect();
    let mut output_lines: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();

    walk_heredocs(
        &lines,
        |marker, target_is_interpreter, target_redirects_to_file, body_range| {
            if marker.is_nowdoc && !target_is_interpreter {
                for idx in body_range {
                    output_lines[idx] = if target_redirects_to_file {
                        " ".repeat(lines[idx].chars().count())
                    } else {
                        mask_substitution_markers(lines[idx])
                    };
                }
            }
        },
    );

    output_lines.join("\n")
}

/// Replace backtick and `$(` command-substitution openers with characters
/// that carry no shell meaning, so line-oriented segmentation downstream
/// stops recognizing them as command-substitution boundaries. Line length is
/// preserved so unrelated column-sensitive logic isn't affected.
fn mask_substitution_markers(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '`' {
            out.push(' ');
        } else if ch == '$' && chars.peek() == Some(&'(') {
            chars.next();
            out.push_str("$ ");
        } else {
            out.push(ch);
        }
    }

    out
}

/// Extract all inline scripts from interpreter invocations in `cmd`.
///
/// Recognises the following patterns (the script flag immediately precedes
/// the script body as the next shell token):
///
/// | Interpreter       | Flag |
/// |-------------------|------|
/// | `python` / `python3` | `-c` |
/// | `node` / `nodejs` | `-e` |
/// | `ruby`            | `-e` |
/// | `php`             | `-r` |
/// | `lua`             | `-e` |
/// | `perl`            | `-e` |
///
/// # Examples
///
/// ```text
/// python3 -c "import os; os.system('rm -rf /')"
/// ```
/// → `[InlineScript { interpreter: "python3", body: "import os; os.system('rm -rf /')" }]`
pub fn extract_inline_scripts(cmd: &str) -> Vec<InlineScript> {
    const INTERPRETERS: &[(&str, &str)] = &[
        ("python", "-c"),
        ("python3", "-c"),
        ("node", "-e"),
        ("nodejs", "-e"),
        ("ruby", "-e"),
        ("php", "-r"),
        ("lua", "-e"),
        ("perl", "-e"),
    ];

    let mut scripts = Vec::new();
    for segment in split_top_level_segments(cmd) {
        let tokens = split_tokens(&segment);
        let mut i = 0;

        while i < tokens.len() {
            for &(interp, flag) in INTERPRETERS {
                if tokens[i] == interp {
                    if let Some(rel) = tokens[i..].iter().position(|t| t == flag) {
                        let body_idx = i + rel + 1;
                        if let Some(body) = tokens.get(body_idx) {
                            scripts.push(InlineScript {
                                interpreter: interp.to_string(),
                                body: body.clone(),
                            });
                        }
                    }
                    break;
                }
            }
            i += 1;
        }
    }

    scripts
}

#[cfg(test)]
mod tests {
    use super::heredoc_suspend_ranges;

    // Boundaries `heredoc_suspend_ranges`' byte-offset tracking must hold at
    // (issue #384 B6): a heredoc with nothing between its marker and
    // terminator, one that never terminates because `cmd` ends on the marker
    // line itself, a CRLF-terminated command, and a terminated heredoc whose
    // last line carries no trailing newline at all.

    #[test]
    fn a_heredoc_with_no_body_lines_suspends_marker_through_terminator() {
        let cmd = "cat <<EOF\nEOF\n";
        assert_eq!(
            heredoc_suspend_ranges(cmd),
            vec![cmd.find("<<").unwrap()..(cmd.rfind("EOF").unwrap() + "EOF".len())]
        );
    }

    #[test]
    fn a_heredoc_marker_with_nothing_after_it_suspends_to_the_end_of_cmd() {
        let cmd = "cat <<EOF";
        assert_eq!(
            heredoc_suspend_ranges(cmd),
            vec![cmd.find("<<").unwrap()..cmd.len()]
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
}
