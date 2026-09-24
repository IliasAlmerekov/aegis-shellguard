//! Program-word checks for the unclaimed-interpreter net (issue #384/#430):
//! a program word reached only through shell expansion the router does not
//! perform, or through a shell `alias` defined earlier in the same command.
//! Split out of [`super::unclaimed`] to keep both files small and each
//! concern independently testable.

/// `true` when `word` — a stage's own effective program token — cannot be
/// trusted as a static program name without evaluating shell expansion the
/// router never performs: a variable reference (`$VAR`, `${VAR:-default}`),
/// a command or process substitution (`` `cmd` ``, `$(cmd)`), or a brace
/// list (`{python3,}`). Only the program word itself is checked — an
/// *operand* built the same way (`echo $HOME`, `ls {a,b}`) names nothing to
/// run and is unaffected (issue #384/#430).
pub(super) fn is_dynamic_program_word(word: &str) -> bool {
    word.contains('$')
        || word.contains('`')
        || (word.starts_with('{') && word.ends_with('}') && word.contains(','))
}

/// The replacement text an `alias` invocation gives `name` within `scope`,
/// the caller-supplied text considered "before" the call site — `alias
/// name=cmd`, `alias -- name=cmd` (`--` ends `alias`'s own option parsing,
/// letting a name that itself starts with `-` through), several names in
/// one call (`alias a=x b=y`), and a quoted name (the tokenizer strips the
/// quotes before this ever runs) all resolve the same way once the alias
/// stage's own words are walked instead of substring-matched against raw
/// text — a literal `"alias {name}="` search misses `alias -- name=cmd`
/// outright, since `--` sits between the two words (issue #384/#430). The
/// router does not track shell aliases, so it hands the replacement text
/// back rather than a bare yes/no: [`super::unclaimed`] decides whether
/// standing in for *this particular* text is opaque (a known interpreter, a
/// dynamic word) or as ordinary as any other program name (`alias
/// ll='ls -l'`) — issue #384/#430, this function no longer makes
/// that call itself.
///
/// `scope` is a byte range of the full command chosen by the caller, not
/// derived here by searching for the call site's own text: the same stage
/// text can appear more than once in one command (`run x; alias
/// run=python3; run x`), so a text search cannot tell *which* occurrence is
/// the one actually being routed and always lands on the first — silently
/// scoping a later, aliased call to whatever was true before the earlier
/// one (issue #437, F2). `name` can be redefined by more than one `alias`
/// call within `scope` (`alias run=echo; alias run=python3; run ./evil.py`,
/// review comment 4091038665); the shell honors whichever definition is
/// active at the call site — the last one written before it — so this keeps
/// scanning past a match instead of stopping at the first, and returns the
/// last one seen.
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

/// `true` for the separator tokens [`aegis_parser::split_tokens`] emits
/// between shell stages (`;`, `&&`, `||`, `|`) — an `alias` call's own
/// argument list ends there, the same boundary the router's own
/// stage-splitting already treats as the end of one stage and the start of
/// the next (issue #384/#430).
fn is_stage_separator(token: &str) -> bool {
    matches!(token, ";" | "&&" | "||" | "|")
}
