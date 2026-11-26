use super::*;

fn is_env_chdir_flag(tok: &str) -> bool {
    tok == "--chdir" || tok.starts_with("--chdir=") || tok.starts_with("-C")
}

pub(super) fn env_chdir_prefix(prefix: &[&str]) -> bool {
    prefix.iter().enumerate().any(|(idx, tok)| {
        let basename = tok.rsplit('/').next().unwrap_or(tok);
        basename.eq_ignore_ascii_case("env")
            && prefix[idx + 1..].iter().any(|t| is_env_chdir_flag(t))
    })
}

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
    NoSource,
    NoMatch,
}

pub(super) fn walk_interpreter_argv(interp: &Interpreter, rest: &[&str]) -> ArgvWalk {
    let marker_pos = rest.iter().position(|tok| tok.starts_with("<<"));
    let before_marker = marker_pos.map_or(rest, |idx| &rest[..idx]);

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

fn redirects_stdin_fd(fd: &str) -> bool {
    fd.is_empty() || fd == "0"
}

fn is_plain_input_redirect(tok: &str) -> bool {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    let fd = &tok[..tok.len() - after_fd.len()];
    after_fd == "<" && redirects_stdin_fd(fd)
}

fn glued_plain_input_redirect_target(tok: &str) -> Option<&str> {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    let fd = &tok[..tok.len() - after_fd.len()];
    if !redirects_stdin_fd(fd) {
        return None;
    }
    let target = after_fd.strip_prefix('<')?;
    (!target.is_empty() && !target.starts_with(['<', '&', '>'])).then_some(target)
}

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
