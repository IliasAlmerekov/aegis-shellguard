use super::*;

pub(super) fn basename(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

pub(super) fn route_path_like_candidate(
    word: &str,
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> RoutedTarget {
    if token_is_home_assignment(word) {
        return RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        };
    }

    let path = if let Some(rest) = word.strip_prefix("~/").map(|r| r.trim_start_matches('/')) {
        match ctx
            .home
            .filter(|_| is_literal_path(rest) && home == HomeState::Trusted)
        {
            Some(home_dir) => home_dir.join(rest),
            None => {
                return RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                };
            }
        }
    } else if is_literal_path(word) {
        PathBuf::from(word)
    } else {
        return RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        };
    };
    RoutedTarget::LauncherOperand { path }
}

pub(super) fn command_value_leading_word(value: &str) -> &str {
    if value.contains(char::is_whitespace) {
        skip_command_string_prefix_words(value)
            .next()
            .unwrap_or(value)
    } else {
        value
    }
}

pub(super) fn route_executor_value(
    value: &str,
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> Option<RoutedTarget> {
    let stripped = value
        .strip_prefix('|')
        .or_else(|| value.strip_prefix('!'))
        .unwrap_or(value);
    if token_names_an_interpreter(stripped, ctx.trusted_aliases) {
        return Some(RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        });
    }
    let candidate = command_value_leading_word(stripped);
    if !candidate.contains('/') || candidate.contains("://") {
        return None;
    }
    Some(route_path_like_candidate(candidate, home, ctx))
}

const COMMAND_STRING_PREFIX_WORDS: &[&str] =
    &["exec", "command", "builtin", "nohup", "time", "nice", "env"];

fn skip_command_string_prefix_words(tok: &str) -> std::str::SplitWhitespace<'_> {
    let mut words = tok.split_whitespace();
    loop {
        let mut lookahead = words.clone();
        let Some(word) = lookahead.next() else {
            break;
        };
        if COMMAND_STRING_PREFIX_WORDS.contains(&word)
            || word
                .split_once('=')
                .is_some_and(|(name, _)| is_shell_identifier(name))
        {
            words = lookahead;
        } else {
            break;
        }
    }
    words
}

pub(super) fn token_names_an_interpreter(tok: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    text_names_an_interpreter(tok, trusted_aliases)
        || d4_value(tok).is_some_and(|value| text_names_an_interpreter(value, trusted_aliases))
}

pub(super) fn d4_value(tok: &str) -> Option<&str> {
    let (name, mut value) = tok.split_once('=')?;
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    while let Some((inner_name, inner_value)) = value.split_once('=') {
        if inner_name.is_empty() || inner_name.contains(char::is_whitespace) {
            break;
        }
        value = inner_value;
    }
    Some(value)
}

fn text_names_an_interpreter(text: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    resolve_interpreter(basename(text), trusted_aliases).is_some()
        || (text.contains(char::is_whitespace)
            && skip_command_string_prefix_words(text)
                .next()
                .is_some_and(|word| resolve_interpreter(basename(word), trusted_aliases).is_some()))
}
