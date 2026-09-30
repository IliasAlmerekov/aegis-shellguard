//! Stage-wide checks for shell words whose value is known only at execution.

use super::*;
use std::borrow::Cow;

/// An unresolved program or state-bearing argv needs approval even when
/// another routing path found a nested source in the same stage.
pub(super) fn dynamic_stage_net(stage_raw: &str, ctx: &RouteContext<'_>) -> Option<RoutedTarget> {
    if !stage_raw.contains('$')
        && !stage_raw.contains('`')
        && !stage_raw.contains('{')
        && !stage_raw.contains("IFS=")
    {
        return None;
    }
    let without_comments = if let Some(marker) = stage_raw.find("<<") {
        // Strip comments up to the marker's physical line, including a
        // continued header. Later lines may be heredoc data, not shell code.
        let header_end = stage_raw[marker..]
            .find('\n')
            .map_or(stage_raw.len(), |end| marker + end + 1);
        let (header, body) = stage_raw.split_at(header_end);
        if header.contains('#') {
            let mut cleaned = strip_shell_comments(header).into_owned();
            cleaned.push_str(body);
            Cow::Owned(cleaned)
        } else {
            Cow::Borrowed(stage_raw)
        }
    } else {
        strip_shell_comments(stage_raw)
    };
    let stage_raw = without_comments.as_ref();
    let masked;
    let mut trusted_nowdoc_argument = false;
    let stage_raw = if stage_raw.contains("<<") {
        let bodies = aegis_parser::extract_heredoc_bodies(stage_raw);
        trusted_nowdoc_argument =
            bodies.len() == 1 && bodies[0].is_nowdoc && bodies[0].is_data_consumer_target;
        masked = aegis_parser::mask_inert_heredoc_substitution_markers(stage_raw);
        masked.as_str()
    } else {
        stage_raw
    };
    let tokens = aegis_parser::split_tokens(strip_trailing_redirection(stage_raw));
    let words: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let assignments = skip_assignment_keyword(&words);
    if assignments
        .iter()
        .take_while(|token| is_assignment_token(token))
        .any(|token| token.starts_with("IFS="))
    {
        return Some(unresolved());
    }
    let shell_expansion = has_active_expansion(stage_raw);
    for slice in aegis_parser::effective_token_slices(&words) {
        // env -S also expands ${VAR} when its value was single-quoted by
        // the outer shell. Limit the search to this candidate's launcher
        // prefix; a later literal operand named env is not a launcher.
        let split_string_expansion = env_split_string_expands_variable(&words, slice.program);
        let active_expansion = shell_expansion || split_string_expansion;
        if ifs_in_launcher_prefix(&words, slice.program) {
            return Some(unresolved());
        }
        if is_dynamic_program_word(slice.program)
            && (active_expansion || slice.program.starts_with('{'))
        {
            if slice.tokens.len() == 1
                && interactive_program_exception_allowed(slice.program, ctx.command)
            {
                continue;
            }
            return Some(unresolved());
        }
        const READ_ONLY: &[&str] = &[
            "echo", "printf", "ls", "cat", "head", "tail", "grep", "rg", "wc", "stat", "file",
            "basename", "dirname", "readlink", "realpath", "pwd",
        ];
        let program = slice.program.rsplit('/').next().unwrap_or(slice.program);
        let argv = &slice.tokens[1..];
        let stdin_exec_consumer =
            program == "xargs" || resolve_interpreter(program, ctx.trusted_aliases).is_some();
        if stdin_exec_consumer
            && (dynamic_plain_stdin_redirect(stage_raw)
                || (program == "xargs" && dynamic_here_string(stage_raw)))
            && active_expansion
        {
            return Some(unresolved());
        }
        if ((!READ_ONLY.contains(&program)
            && argv
                .iter()
                .any(|token| has_dynamic_argv(token, trusted_nowdoc_argument)))
            || (program == "printf" && dynamic_printf_state(argv, trusted_nowdoc_argument))
            || (program == "rg" && dynamic_rg_executor(argv, trusted_nowdoc_argument)))
            && active_expansion
        {
            return Some(unresolved());
        }
    }
    None
}

fn strip_shell_comments(text: &str) -> Cow<'_, str> {
    if !text.contains('#') {
        return Cow::Borrowed(text);
    }
    let mut output = String::with_capacity(text.len());
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut word_start = true;
    let mut comment = false;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if comment {
            if ch == '\n' {
                comment = false;
                word_start = true;
                output.push(ch);
            } else {
                output.push(' ');
            }
            continue;
        }
        match ch {
            '\\' if !single_quoted => {
                output.push(ch);
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
                word_start = false;
            }
            '\'' if !double_quoted => {
                single_quoted = !single_quoted;
                output.push(ch);
                word_start = false;
            }
            '"' if !single_quoted => {
                double_quoted = !double_quoted;
                output.push(ch);
                word_start = false;
            }
            '#' if !single_quoted && !double_quoted && word_start => {
                comment = true;
                output.push(' ');
            }
            _ => {
                output.push(ch);
                if !single_quoted && !double_quoted {
                    word_start = ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(' | ')');
                }
            }
        }
    }
    Cow::Owned(output)
}

fn env_split_string_expands_variable(words: &[&str], program: &str) -> bool {
    let program_index = words.iter().position(|word| {
        word.rsplit('/')
            .next()
            .is_some_and(|basename| std::ptr::eq(basename, program))
    });
    let prefix = program_index.map_or(words, |index| &words[..=index]);
    prefix.iter().enumerate().any(|(index, word)| {
        if word.rsplit('/').next() != Some("env") {
            return false;
        }
        let mut tail = prefix[index + 1..].iter().copied();
        while let Some(option) = tail.next() {
            if option == "--" {
                break;
            }
            let value = if matches!(option, "-S" | "--split-string") {
                tail.next()
            } else {
                option
                    .strip_prefix("--split-string=")
                    .or_else(|| option.strip_prefix("-S").filter(|value| !value.is_empty()))
            };
            if let Some(value) = value {
                return value.contains("${");
            }
        }
        false
    })
}

fn interactive_program_exception_allowed(program: &str, command: &str) -> bool {
    let name = match program {
        "$EDITOR" => "EDITOR",
        "$VISUAL" => "VISUAL",
        "$PAGER" => "PAGER",
        "$SHELL" => "SHELL",
        _ => return false,
    };
    !command_writes_shell_name(command, name) && !command_writes_shell_name(command, "IFS")
}

fn command_writes_shell_name(command: &str, name: &str) -> bool {
    for segment in aegis_parser::list_segments(command) {
        for stage in &segment.pipeline.segments {
            let owned_tokens = aegis_parser::split_tokens(&stage.raw);
            let words: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
            if words.iter().any(|token| token_assigns_name(token, name)) {
                return true;
            }
            for slice in aegis_parser::effective_token_slices(&words) {
                let program = slice.program.rsplit('/').next().unwrap_or(slice.program);
                if matches!(
                    program,
                    "export" | "read" | "declare" | "typeset" | "local" | "readonly"
                ) && slice.tokens[1..]
                    .iter()
                    .any(|token| *token == name || token_assigns_name(token, name))
                {
                    return true;
                }
            }
        }
    }
    false
}

fn token_assigns_name(token: &str, name: &str) -> bool {
    let Some((lhs, _value)) = token.split_once('=') else {
        return false;
    };
    let lhs = lhs.strip_suffix('+').unwrap_or(lhs);
    let assigned_name = match lhs.split_once('[') {
        Some((candidate, rest)) if rest.ends_with(']') => candidate,
        Some(_) => return false,
        None => lhs,
    };
    assigned_name == name
}

fn dynamic_here_string(stage_raw: &str) -> bool {
    let tokens = aegis_parser::split_tokens(stage_raw);
    let words: Vec<&str> = tokens.iter().map(String::as_str).collect();
    words.iter().enumerate().any(|(index, token)| {
        let after_fd = token.trim_start_matches(|ch: char| ch.is_ascii_digit());
        let Some(glued_body) = after_fd.strip_prefix("<<<") else {
            return false;
        };
        let body = if glued_body.is_empty() {
            words.get(index + 1).copied()
        } else {
            Some(glued_body)
        };
        body.is_some_and(|body| has_dynamic_argv(body, false))
    })
}

fn dynamic_plain_stdin_redirect(stage_raw: &str) -> bool {
    let tokens = aegis_parser::split_tokens(stage_raw);
    let words: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let mut index = 0;
    while index < words.len() {
        let token = words[index];
        let after_fd = token.trim_start_matches(|ch: char| ch.is_ascii_digit());
        let fd = &token[..token.len() - after_fd.len()];
        if !fd.is_empty() && fd != "0" {
            index += 1;
            continue;
        }
        if after_fd == "<" {
            if words
                .get(index + 1)
                .is_some_and(|target| has_dynamic_argv(target, false))
            {
                return true;
            }
            index += 2;
            continue;
        }
        if let Some(target) = after_fd.strip_prefix('<')
            && !target.is_empty()
            && !target.starts_with(['<', '&', '>'])
            && has_dynamic_argv(target, false)
        {
            return true;
        }
        index += 1;
    }
    false
}

fn dynamic_printf_state(argv: &[&str], trusted_nowdoc_argument: bool) -> bool {
    let Some(&first) = argv.first() else {
        return false;
    };
    let first_dynamic = has_dynamic_argv(first, trusted_nowdoc_argument);
    if first_dynamic {
        return true;
    }
    let (format, rest) = if first == "--" {
        let Some((format, rest)) = argv[1..].split_first() else {
            return false;
        };
        (*format, rest)
    } else if first.starts_with("-v") {
        return argv[1..]
            .iter()
            .any(|word| has_dynamic_argv(word, trusted_nowdoc_argument));
    } else {
        (first, &argv[1..])
    };
    has_dynamic_argv(format, trusted_nowdoc_argument)
        || printf_writes_variable(format)
            && rest
                .iter()
                .any(|word| has_dynamic_argv(word, trusted_nowdoc_argument))
}

fn printf_writes_variable(format: &str) -> bool {
    let bytes = format.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        index += 1;
        if bytes.get(index) == Some(&b'%') {
            index += 1;
            continue;
        }
        while bytes.get(index).is_some_and(|byte| {
            byte.is_ascii_digit()
                || matches!(
                    byte,
                    b'-' | b'+'
                        | b' '
                        | b'#'
                        | b'0'
                        | b'\''
                        | b'.'
                        | b'*'
                        | b'$'
                        | b'h'
                        | b'j'
                        | b'l'
                        | b'L'
                        | b't'
                        | b'z'
                )
        }) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'n') {
            return true;
        }
    }
    false
}

fn ifs_in_launcher_prefix(words: &[&str], program: &str) -> bool {
    // Identity avoids mistaking an equal launcher option value for the program.
    let Some(program_index) = words.iter().position(|word| {
        word.rsplit('/')
            .next()
            .is_some_and(|basename| std::ptr::eq(basename, program))
    }) else {
        return env_split_string_assigns_ifs(words);
    };
    words[..program_index]
        .iter()
        .any(|word| word.starts_with("IFS="))
}

fn env_split_string_assigns_ifs(words: &[&str]) -> bool {
    words.iter().enumerate().any(|(index, word)| {
        if word.rsplit('/').next() != Some("env") {
            return false;
        }
        let mut tail = words[index + 1..].iter().copied();
        while let Some(option) = tail.next() {
            if option == "--" {
                break;
            }
            let value = if matches!(option, "-S" | "--split-string") {
                tail.next()
            } else {
                option
                    .strip_prefix("--split-string=")
                    .or_else(|| option.strip_prefix("-S").filter(|value| !value.is_empty()))
            };
            if let Some(value) = value {
                return aegis_parser::split_tokens(value)
                    .iter()
                    .take_while(|word| word.starts_with('-') || is_assignment_token(word))
                    .any(|word| word.starts_with("IFS="));
            }
        }
        false
    })
}

fn dynamic_rg_executor(argv: &[&str], trusted_nowdoc_argument: bool) -> bool {
    let mut index = 0;
    while index < argv.len() {
        let word = argv[index];
        if word == "--" {
            break;
        }

        if word.starts_with('-')
            && let Some((option, value)) = word.split_once('=')
        {
            if has_dynamic_argv(option, trusted_nowdoc_argument) {
                return true;
            }
            if matches!(option, "--pre" | "--hostname-bin")
                && has_dynamic_argv(value, trusted_nowdoc_argument)
            {
                return true;
            }
            index += 1;
            continue;
        }

        if rg_short_option_has_glued_value(word) {
            index += 1;
            continue;
        }

        if has_dynamic_argv(word, trusted_nowdoc_argument) {
            return true;
        }

        if !word.starts_with('-') || word == "-" {
            break;
        }

        if matches!(word, "--pre" | "--hostname-bin")
            && argv
                .get(index + 1)
                .is_some_and(|value| has_dynamic_argv(value, trusted_nowdoc_argument))
        {
            return true;
        }

        index += if rg_option_takes_value(word) { 2 } else { 1 };
    }
    false
}

fn rg_short_option_has_glued_value(word: &str) -> bool {
    const OPTIONS: &[&str] = &[
        "-e", "-f", "-E", "-m", "-j", "-g", "-d", "-t", "-T", "-A", "-B", "-C", "-M", "-r",
    ];
    OPTIONS
        .iter()
        .any(|option| word.starts_with(option) && word.len() > option.len())
}

fn rg_option_takes_value(word: &str) -> bool {
    matches!(
        word,
        "-e" | "--regexp"
            | "-f"
            | "--file"
            | "--pre"
            | "--pre-glob"
            | "--dfa-size-limit"
            | "-E"
            | "--encoding"
            | "--engine"
            | "-m"
            | "--max-count"
            | "--regex-size-limit"
            | "-j"
            | "--threads"
            | "-g"
            | "--glob"
            | "--iglob"
            | "--ignore-file"
            | "-d"
            | "--max-depth"
            | "--max-filesize"
            | "-t"
            | "--type"
            | "-T"
            | "--type-not"
            | "--type-add"
            | "--type-clear"
            | "-A"
            | "--after-context"
            | "-B"
            | "--before-context"
            | "--color"
            | "--colors"
            | "-C"
            | "--context"
            | "--context-separator"
            | "--field-context-separator"
            | "--field-match-separator"
            | "--hostname-bin"
            | "--hyperlink-format"
            | "-M"
            | "--max-columns"
            | "--path-separator"
            | "-r"
            | "--replace"
            | "--sort"
            | "--sortr"
            | "--generate"
    )
}

fn unresolved() -> RoutedTarget {
    RoutedTarget::Unresolved {
        reason: DegradationReason::DynamicSource,
    }
}

fn has_dynamic_argv(word: &str, trusted_nowdoc_argument: bool) -> bool {
    let trusted_nowdoc_argument = trusted_nowdoc_argument
        && word.starts_with("$(cat <<")
        && word.ends_with(')')
        && word.matches("$(").count() == 1
        && !word.contains('`');
    if word.contains('`') {
        return true;
    }
    word.as_bytes().windows(2).any(|pair| {
        pair[0] == b'$'
            && (pair[1] == b'{'
                || pair[1] == b'_'
                || pair[1].is_ascii_alphanumeric()
                || b"@*?#$!-".contains(&pair[1])
                || pair[1] == b'(' && !trusted_nowdoc_argument)
    })
}

fn has_active_expansion(text: &str) -> bool {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !single_quoted => {
                chars.next();
            }
            '\'' if !double_quoted => single_quoted = !single_quoted,
            '"' if !single_quoted => double_quoted = !double_quoted,
            '$' | '`' if !single_quoted => return true,
            _ => {}
        }
    }
    false
}
