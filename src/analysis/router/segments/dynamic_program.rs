//! Program-word checks for the unclaimed-interpreter net (#384, #430).

/// `true` when a program word requires shell expansion (`$`, backticks, or brace list) (#384, #430).
pub(super) fn is_dynamic_program_word(word: &str) -> bool {
    word.contains('$')
        || word.contains('`')
        || (word.starts_with('{') && word.ends_with('}') && word.contains(','))
}

/// Last `alias` replacement for `name` in `scope`; keeps scanning so redefinitions match shell precedence (#384, #430, #437).
pub(super) fn alias_value(scope: &str, name: &str) -> Option<String> {
    let owned_tokens = aegis_parser::split_tokens(scope);
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();

    let mut index = 0;
    let mut latest = None;
    while index < tokens.len() {
        if tokens[index] != "alias" {
            index += 1;
            continue;
        }
        index += 1;
        while index < tokens.len() && !is_stage_separator(tokens[index]) {
            let arg = tokens[index];
            index += 1;
            if arg == "--" || arg == "-p" {
                continue;
            }
            if let Some((defined, value)) = arg.split_once('=')
                && defined == name
            {
                latest = Some(value.to_owned());
            }
        }
    }
    latest
}

fn is_stage_separator(token: &str) -> bool {
    matches!(token, ";" | "&&" | "||" | "|")
}
