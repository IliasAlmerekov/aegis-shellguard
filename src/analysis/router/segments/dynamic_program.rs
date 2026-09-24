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

/// `true` when an `alias <name>=...` definition appears in `full_command`
/// before `stage_raw`'s own text within it (`alias runpy=python3; runpy
/// ./evil.py`) — the router does not track shell aliases, so a stage naming
/// one as its program is exactly as opaque as a variable reference (issue
/// #384/#430). Falls back to scanning the whole of `full_command` when
/// `stage_raw` cannot be located inside it verbatim (a body peeled out of a
/// wrapper by [`super::wrappers`] may have been re-derived rather than kept
/// as an exact substring) — the conservative direction, since a false match
/// only costs an extra prompt rather than a missed one.
pub(super) fn alias_defines_program(full_command: &str, stage_raw: &str, name: &str) -> bool {
    let scope = full_command
        .find(stage_raw)
        .map_or(full_command, |idx| &full_command[..idx]);
    scope.contains(&format!("alias {name}="))
}
