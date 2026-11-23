//! Interpreter-argv walking and `env -C`/`--chdir` prefix scanning.

use super::*;

fn is_env_chdir_flag(tok: &str) -> bool {
    tok == "--chdir" || tok.starts_with("--chdir=") || tok.starts_with("-C")
}

/// `true` when `prefix` contains `env` followed by a `-C`/`--chdir` flag anywhere in the prefix (#384).
pub(super) fn env_chdir_prefix(prefix: &[&str]) -> bool {
    prefix.iter().enumerate().any(|(idx, tok)| {
        let basename = tok.rsplit('/').next().unwrap_or(tok);
        basename.eq_ignore_ascii_case("env")
            && prefix[idx + 1..].iter().any(|t| is_env_chdir_flag(t))
    })
}

/// Resolve `stage` to its interpreter only when argv carries no source of its own, so piped stdin is routed (#384).
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

pub(super) enum ArgvWalk {
    Routed(RoutedTarget),
    /// Inline flag present with no body; never falls back to stdin.
    NoSource,
    NoMatch,
}

/// Walk interpreter argv (`rest`), stopping option parsing at the first positional script argument (ADR-022 §6).
pub(super) fn walk_interpreter_argv(interp: &Interpreter, rest: &[&str]) -> ArgvWalk {
    // Stop at `<<`/`<<<`: tokens after the marker are heredoc/here-string body, not argv.
    let marker_pos = rest.iter().position(|tok| tok.starts_with("<<"));
    let before_marker = marker_pos.map_or(rest, |idx| &rest[..idx]);

    // Stdin redirect target is used only if no inline body or positional script is found (#384).
    let mut stdin_redirect_target: Option<&str> = None;

    let mut pos = 0;
    while pos < before_marker.len() {
        let tok = before_marker[pos];
        if let Some(source) = inline_body(tok, interp.inline_flag, before_marker, pos) {
            if source.is_empty() {
                return ArgvWalk::NoSource;
            }
            return ArgvWalk::Routed(RoutedTarget::Inline {
                language: interp.language,
                source,
            });
        }
        if aegis_parser::is_redirection_operator(tok) {
            // Skip both the redirection operator and its target so the target is not read as a script arg.
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

/// Only omitted fd or `0` redirects stdin; non-stdin fds (e.g. `3<file`) are not script source.
fn redirects_stdin_fd(fd: &str) -> bool {
    fd.is_empty() || fd == "0"
}

fn is_plain_input_redirect(tok: &str) -> bool {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    let fd = &tok[..tok.len() - after_fd.len()];
    after_fd == "<" && redirects_stdin_fd(fd)
}

/// Target of a glued stdin redirect (`<file`, `0<file`), excluding heredocs, dup-fds, and non-stdin fds (#384).
fn glued_plain_input_redirect_target(tok: &str) -> Option<&str> {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    let fd = &tok[..tok.len() - after_fd.len()];
    if !redirects_stdin_fd(fd) {
        return None;
    }
    let target = after_fd.strip_prefix('<')?;
    (!target.is_empty() && !target.starts_with(['<', '&', '>'])).then_some(target)
}

/// Last literal stdin redirect sitting before the program; later `<` overwrites earlier ones (#384).
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
