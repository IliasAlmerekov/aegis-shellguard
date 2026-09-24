//! Interpreter-argv walking and the `env -C`/`--chdir` prefix scan, split out
//! of [`super`] (`segments.rs`) to keep that file under `CONVENTION.md`'s
//! file-size gate. A descendant of [`super::super`] (`router.rs`) as much as
//! `segments` is: every private item there is visible here via
//! `use super::*`, the same pattern `segments.rs`'s own doc comment
//! describes for `router::tests`.

use super::*;

/// `true` for an `env` `-C`/`--chdir` flag token: the spaced short form
/// (`-C`, its directory in the next token), the glued short form (`-Cd1`),
/// the spaced long form (`--chdir`), or the glued long form
/// (`--chdir=d1`) — every shape GNU `env` accepts (#437 review comment
/// 4091038686).
fn is_env_chdir_flag(tok: &str) -> bool {
    tok == "--chdir" || tok.starts_with("--chdir=") || tok.starts_with("-C")
}

/// `true` when `prefix` — the tokens routing consumed before the effective
/// program (assignments, launcher words, redirections) — carries an `env`
/// invocation with `-C`/`--chdir` anywhere in it, not only as its first
/// token: a launcher word ahead of `env` (`command env -C d1 python3
/// ./x.py`) still lets the chdir flag resolve the wrong cwd if routing only
/// checked `prefix[0]` (issue #384, #437 review comment 4091038686). A
/// cheap token-equality scan, not a full re-parse of `env`'s own option
/// grammar: routing only needs to know an `env` word and a chdir flag are
/// both present somewhere in the prefix it already resolved, not their
/// exact positions relative to each other.
pub(super) fn env_chdir_prefix(prefix: &[&str]) -> bool {
    prefix.iter().enumerate().any(|(idx, tok)| {
        let basename = tok.rsplit('/').next().unwrap_or(tok);
        basename.eq_ignore_ascii_case("env")
            && prefix[idx + 1..].iter().any(|t| is_env_chdir_flag(t))
    })
}

/// Resolve `stage` to its interpreter only if it has no source of its own —
/// the bare program token alone, its own flags followed by nothing but the
/// POSIX stdin sentinel `-` (`python3 -`, `python3 -u -`), or any other argv
/// shape [`walk_interpreter_argv`] itself cannot find a source in (e.g. a
/// redirection on a non-stdin descriptor, `python3 3<./benign.py`, #437
/// review comment 4091038714) — a real interpreter reads its script from
/// stdin in every such shape, the same as "reads piped stdin from the
/// previous stage" rather than "has its own source" (issue #384, #437
/// review comment 4091038647). Delegates to the same argv walk every other
/// call site uses rather than a second, narrower "no args of its own" check,
/// so a source [`walk_interpreter_argv`] would find (an inline body, a
/// script file, a genuine stdin redirect) is never misread as bare here.
pub(super) fn bare_stage_interpreter(
    stage: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<&'static Interpreter> {
    let owned_tokens = aegis_parser::split_tokens(stage);
    let (_tokens, slice, _truncated) = effective_stage_slice(&owned_tokens);
    let slice = slice?;
    let rest = &slice.tokens[1..];
    let interp = resolve_interpreter(slice.program, trusted_aliases)?;
    match walk_interpreter_argv(interp, rest) {
        ArgvWalk::NoMatch => Some(interp),
        ArgvWalk::Routed(_) | ArgvWalk::NoSource => None,
    }
}

/// The result of [`walk_interpreter_argv`] walking one interpreter
/// invocation's own argv.
pub(super) enum ArgvWalk {
    /// An inline body or a script-file argument was found.
    Routed(RoutedTarget),
    /// The interpreter's inline flag was present but carried no body — a
    /// definitive "not a source target", never falling back to stdin.
    NoSource,
    /// Nothing in argv itself routed; the caller decides its own stdin
    /// (heredoc/here-string) fallback.
    NoMatch,
}

/// Walk an interpreter's own argv (`rest`, the effective token slice after
/// the program token) exactly as the interpreter itself would: it keeps
/// consuming flags (including the inline `-c`/`-e` body, which wins
/// immediately) and shell redirections (which the shell strips before exec —
/// the interpreter never sees them) until it hits the first positional
/// (non-flag, non-redirection) token, which is the script file and ends
/// option parsing right there — any flag-shaped token *after* it belongs to
/// the script's own argv, not the interpreter, and must not be misread as the
/// interpreter's inline flag (ADR-022 §6).
///
/// The single interpreter-argv walk shared by every routing call site.
pub(super) fn walk_interpreter_argv(interp: &Interpreter, rest: &[&str]) -> ArgvWalk {
    // The tokenizer has no heredoc-boundary awareness, so tokens *after* a
    // `<<WORD`/`<<<` marker are the heredoc/here-string *body*, not further
    // command arguments. Both the inline-flag scan and the file-argument scan
    // below must stop at the marker, or a crafted heredoc body could be
    // misread as the interpreter's own flag/argument instead of being
    // classified as stdin.
    let marker_pos = rest.iter().position(|tok| tok.starts_with("<<"));
    let before_marker = marker_pos.map_or(rest, |idx| &rest[..idx]);

    // A standalone `< file` with a literal target means the interpreter
    // reads its script from stdin, and stdin is exactly that file (issue
    // #384): `python3 < ./evil.py` is the same source as `python3 - <
    // ./evil.py`. Recorded here and only consulted if the walk below finds
    // no inline body or positional script argument of its own — either of
    // those wins outright, same as a real interpreter's own argv parsing.
    let mut stdin_redirect_target: Option<&str> = None;

    let mut pos = 0;
    while pos < before_marker.len() {
        let tok = before_marker[pos];
        if let Some(source) = inline_body(tok, interp.inline_flag, before_marker, pos) {
            if source.is_empty() {
                // Flag present but no inline body to analyze — not a source target.
                return ArgvWalk::NoSource;
            }
            return ArgvWalk::Routed(RoutedTarget::Inline {
                language: interp.language,
                source,
            });
        }
        if aegis_parser::is_redirection_operator(tok) {
            // A spaced-out redirection (`> file`, `2> file`, `>> file`) has
            // its target in the *next* token, which the interpreter never
            // sees either — skip both, not just the operator, or the target
            // filename would be misread as the script argument.
            if is_plain_input_redirect(tok)
                && let Some(target) = before_marker.get(pos + 1)
                && is_literal_path(target)
            {
                stdin_redirect_target = Some(target);
            }
            pos += 2;
            continue;
        }
        if let Some(target) = glued_plain_input_redirect_target(tok)
            && is_literal_path(target)
        {
            // A redirection with no space before its filename (`<file`,
            // `0<file`) is the same stdin source as the spaced form above,
            // just glued into one token by the tokenizer (issue #384).
            stdin_redirect_target = Some(target);
            pos += 1;
            continue;
        }
        if !tok.starts_with('-') && !tok.contains('<') && !tok.contains('>') {
            return ArgvWalk::Routed(RoutedTarget::ScriptFile {
                language: interp.language,
                path: PathBuf::from(tok),
            });
        }
        pos += 1;
    }

    match stdin_redirect_target {
        Some(path) => ArgvWalk::Routed(RoutedTarget::ScriptFile {
            language: interp.language,
            path: PathBuf::from(path),
        }),
        None => ArgvWalk::NoMatch,
    }
}

/// `true` when `tok`'s leading digit run (its redirected file descriptor, if
/// any) names stdin: no digits at all, or exactly `0`. A redirection on any
/// other descriptor (`3<file`) does not touch the process's stdin, so it
/// must never be read as the interpreter's script source (#437 review
/// comment 4091038714).
fn redirects_stdin_fd(fd: &str) -> bool {
    fd.is_empty() || fd == "0"
}

/// `true` for a standalone plain input redirection targeting stdin (`<`,
/// `0<`, but not `3<`) — an [`aegis_parser::is_redirection_operator`] token
/// with no `>`, no fd-duplication `&`, and a descriptor that is stdin itself
/// or omitted, the only shape whose target can mean "this file is the
/// interpreter's stdin source" (issue #384, #437 review comment 4091038714).
fn is_plain_input_redirect(tok: &str) -> bool {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    let fd = &tok[..tok.len() - after_fd.len()];
    after_fd == "<" && redirects_stdin_fd(fd)
}

/// The literal target of a plain input redirection glued to its own token
/// with no separating space (`<file`, `0<file`, but not `3<file`) — the same
/// stdin-only shape [`is_plain_input_redirect`] recognizes when spaced out,
/// but the tokenizer keeps this one glued because nothing splits it (issue
/// #384, #437 review comment 4091038714). `None` for anything else: a
/// redirect on a non-stdin descriptor, a duplication/dup-fd form (`<&3`), a
/// heredoc/here-string marker (`<<`, `<<<`, already excluded upstream by
/// the marker-boundary scan), an output redirection, or an empty target.
fn glued_plain_input_redirect_target(tok: &str) -> Option<&str> {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    let fd = &tok[..tok.len() - after_fd.len()];
    if !redirects_stdin_fd(fd) {
        return None;
    }
    let target = after_fd.strip_prefix('<')?;
    (!target.is_empty() && !target.starts_with(['<', '&', '>'])).then_some(target)
}

/// The literal target of a plain input redirection sitting *before* the
/// program (`<./evil.py python3`, `< ./evil.py python3`) — the shell
/// resolves stdin from it the same way regardless of which side of the
/// program name it sits on, but only the trailing form is visible to
/// [`walk_interpreter_argv`], which only ever sees `rest` (the tokens
/// *after* the program). Scans left to right and keeps the *last* match,
/// same as the shell itself: redirections apply in order, so a later `<`
/// overwrites stdin as far as the exec'd program is concerned, matching the
/// overwrite behavior [`walk_interpreter_argv`] already uses for redirects
/// after the program (issue #384, #437 review comment 4091038705).
pub(super) fn leading_stdin_redirect_target<'a>(prefix: &[&'a str]) -> Option<&'a str> {
    let mut pos = 0;
    let mut last_target = None;
    while pos < prefix.len() {
        let tok = prefix[pos];
        if aegis_parser::is_redirection_operator(tok) {
            if is_plain_input_redirect(tok)
                && let Some(target) = prefix.get(pos + 1)
                && is_literal_path(target)
            {
                last_target = Some(*target);
            }
            pos += 2;
            continue;
        }
        if let Some(target) = glued_plain_input_redirect_target(tok)
            && is_literal_path(target)
        {
            last_target = Some(target);
        }
        pos += 1;
    }
    last_target
}
