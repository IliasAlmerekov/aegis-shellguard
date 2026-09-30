//! Stage-wide checks for shell words whose value is known only at execution.

use super::*;

/// An unresolved program or state-bearing argv needs approval even when
/// another routing path found a nested source in the same stage.
pub(super) fn dynamic_stage_net(stage_raw: &str) -> Option<RoutedTarget> {
    if !stage_raw.contains('$')
        && !stage_raw.contains('`')
        && !stage_raw.contains('{')
        && !stage_raw.contains("IFS=")
    {
        return None;
    }
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
        || words.iter().any(|token| token.starts_with("IFS="))
            && (words
                .first()
                .is_some_and(|word| word.rsplit('/').next() == Some("env"))
                || words
                    .first()
                    .is_some_and(|word| word.rsplit('/').next() == Some("sudo"))
                    && words
                        .get(1)
                        .is_some_and(|word| word.rsplit('/').next() == Some("env")))
    {
        return Some(unresolved());
    }
    let slice = aegis_parser::effective_token_slices(&words)
        .into_iter()
        .next()?;
    if ifs_in_launcher_prefix(&words, slice.program) {
        return Some(unresolved());
    }
    if is_dynamic_program_word(slice.program)
        && (has_active_expansion(stage_raw) || slice.program.starts_with('{'))
    {
        if slice.tokens.len() == 1
            && matches!(slice.program, "$EDITOR" | "$VISUAL" | "$PAGER" | "$SHELL")
        {
            return None;
        }
        return Some(unresolved());
    }
    const READ_ONLY: &[&str] = &[
        "echo", "printf", "ls", "cat", "head", "tail", "grep", "rg", "wc", "stat", "file",
        "basename", "dirname", "readlink", "realpath", "pwd",
    ];
    let program = slice.program.rsplit('/').next().unwrap_or(slice.program);
    let argv = &slice.tokens[1..];
    if ((!READ_ONLY.contains(&program)
        && argv
            .iter()
            .any(|token| has_dynamic_argv(token, trusted_nowdoc_argument)))
        || (program == "rg" && dynamic_rg_preprocessor(argv, trusted_nowdoc_argument)))
        && has_active_expansion(stage_raw)
    {
        return Some(unresolved());
    }
    None
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

fn dynamic_rg_preprocessor(argv: &[&str], trusted_nowdoc_argument: bool) -> bool {
    let mut words = argv.iter().copied();
    while let Some(word) = words.next() {
        if word == "--" {
            break;
        }
        if word == "--pre" {
            if words
                .next()
                .is_some_and(|value| has_dynamic_argv(value, trusted_nowdoc_argument))
            {
                return true;
            }
        } else if let Some(value) = word.strip_prefix("--pre=")
            && has_dynamic_argv(value, trusted_nowdoc_argument)
        {
            return true;
        }
    }
    false
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
