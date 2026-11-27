use std::ops::Range;

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

    pub(crate) fn contains(&mut self, byte_idx: usize) -> bool {
        while self.active.is_some_and(|range| range.end <= byte_idx) {
            self.active = self.remaining.next();
        }
        self.active.is_some_and(|range| range.contains(&byte_idx))
    }
}

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

pub(crate) fn case_statement_suspend_ranges(cmd: &str) -> Vec<Range<usize>> {
    if !cmd.contains("case") {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut open_starts: Vec<usize> = Vec::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
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
            word_start.get_or_insert(idx);
            continue;
        }

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

pub(crate) struct HeredocMarker {
    pub(crate) delimiter: String,
    pub(crate) is_nowdoc: bool,
    strip_tabs: bool,
    pub(crate) operator_start: usize,
    pub(crate) delimiter_end: usize,
}

pub(crate) struct HeredocSite<'a> {
    pub(crate) marker: &'a HeredocMarker,
    pub(crate) marker_line: &'a str,
    pub(crate) preceding_text: &'a str,
    pub(crate) body_range: Range<usize>,
    pub(crate) suspend_end: usize,
    pub(crate) terminator_is_clean: bool,
}

struct SubstitutionFrame {
    command_sub_depth: Option<usize>,
    saved_quotes: (bool, bool),
}

fn find_unquoted_double_lt(line: &str) -> Option<usize> {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut frames: Vec<SubstitutionFrame> = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !single_quote => {
                chars.next();
            }
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '`' if !single_quote => {
                if matches!(frames.last(), Some(f) if f.command_sub_depth.is_none()) {
                    if let Some(frame) = frames.pop() {
                        (single_quote, double_quote) = frame.saved_quotes;
                    }
                } else {
                    frames.push(SubstitutionFrame {
                        command_sub_depth: None,
                        saved_quotes: (single_quote, double_quote),
                    });
                    single_quote = false;
                    double_quote = false;
                }
            }
            '$' if !single_quote && chars.peek().map(|&(_, c)| c) == Some('(') => {
                chars.next();
                frames.push(SubstitutionFrame {
                    command_sub_depth: Some(1),
                    saved_quotes: (single_quote, double_quote),
                });
                single_quote = false;
                double_quote = false;
            }
            '(' if !single_quote && !double_quote => {
                if let Some(depth) = frames.last_mut().and_then(|f| f.command_sub_depth.as_mut()) {
                    *depth += 1;
                }
            }
            ')' if !single_quote && !double_quote => {
                let closed = if let Some(depth) =
                    frames.last_mut().and_then(|f| f.command_sub_depth.as_mut())
                {
                    *depth -= 1;
                    *depth == 0
                } else {
                    false
                };
                if closed && let Some(frame) = frames.pop() {
                    (single_quote, double_quote) = frame.saved_quotes;
                }
            }
            '<' if !single_quote && !double_quote && chars.peek().map(|&(_, c)| c) == Some('<') => {
                return Some(idx);
            }
            _ => {}
        }
    }

    None
}

pub(crate) fn find_heredoc_marker(line: &str) -> Option<HeredocMarker> {
    let operator_start = find_unquoted_double_lt(line)?;
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

pub(crate) fn heredoc_suspend_ranges(cmd: &str) -> Vec<Range<usize>> {
    if !cmd.contains("<<") {
        return Vec::new();
    }

    let indexed = indexed_lines(cmd);
    let lines: Vec<&str> = indexed.iter().map(|&(line, _)| line).collect();
    let mut ranges = Vec::new();

    walk_heredoc_sites(
        &lines,
        |HeredocSite {
             marker,
             body_range,
             suspend_end,
             ..
         }| {
            let Some(marker_line_index) = body_range.start.checked_sub(1) else {
                return;
            };
            let Some(&(_, marker_line_start)) = indexed.get(marker_line_index) else {
                return;
            };
            let start = marker_line_start + marker.operator_start;
            let end = match indexed.get(suspend_end) {
                Some(&(_, line_start)) if suspend_end < body_range.end => line_start,
                Some(&(terminator_line, terminator_start)) => {
                    terminator_start + terminator_line.len()
                }
                None => cmd.len(),
            };
            ranges.push(start..end);
        },
    );

    ranges
}

pub(crate) fn walk_heredoc_sites(lines: &[&str], mut on_site: impl FnMut(HeredocSite<'_>)) {
    if !lines.iter().any(|line| line.contains("<<")) {
        return;
    }

    let mut i = 0;
    let mut command_text = String::new();

    while i < lines.len() {
        if let Some(marker) = find_heredoc_marker(lines[i]) {
            let marker_line = lines[i];
            let preceding_len = command_text.len();
            command_text.push_str(marker_line);
            command_text.push('\n');
            i += 1;
            let body_start = i;

            let mut first_prefixed = None;
            let mut terminator_exact = false;
            while i < lines.len() {
                let candidate = if marker.strip_tabs {
                    lines[i].trim_start_matches('\t')
                } else {
                    lines[i]
                };
                terminator_exact = candidate == marker.delimiter;
                if terminator_exact {
                    break;
                }
                if first_prefixed.is_none() && candidate.starts_with(marker.delimiter.as_str()) {
                    first_prefixed = Some(i);
                }
                i += 1;
            }
            let body_end = i;
            let suspend_end = first_prefixed.unwrap_or(body_end);

            on_site(HeredocSite {
                marker: &marker,
                marker_line,
                preceding_text: &command_text[..preceding_len],
                body_range: body_start..body_end,
                suspend_end,
                terminator_is_clean: terminator_exact && first_prefixed.is_none(),
            });
        } else {
            command_text.push_str(lines[i]);
            command_text.push('\n');
        }
        i += 1;
    }
}

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

#[cfg(test)]
#[path = "../tests/suspend_ranges_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/double_lt_tests.rs"]
mod double_lt_tests;
