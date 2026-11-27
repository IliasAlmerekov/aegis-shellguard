use super::executor_value::route_executor_value;
use super::{HomeState, RouteContext, RoutedTarget, is_assignment_only_stage, is_shell_identifier};
use aegis_types::DegradationReason;

const EXECUTOR_ENV_VARS: &[&str] = &[
    "PAGER",
    "GIT_PAGER",
    "MANPAGER",
    "EDITOR",
    "VISUAL",
    "GIT_EDITOR",
    "GIT_SSH_COMMAND",
    "GIT_SSH",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "GIT_EXTERNAL_DIFF",
    "GIT_SEQUENCE_EDITOR",
    "GIT_PROXY_COMMAND",
    "BROWSER",
    "LESSOPEN",
    "LESSCLOSE",
];

pub(super) fn env_prefix_executor_route(
    raw_tokens: &[&str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> Vec<RoutedTarget> {
    let mut routes = Vec::new();
    for token in env_launcher_tail(raw_tokens) {
        let Some((name, value)) = token.split_once('=') else {
            break;
        };
        if !is_shell_identifier(name) {
            break;
        }
        if EXECUTOR_ENV_VARS.contains(&name)
            && let Some(route) = route_executor_value(value, home, ctx)
        {
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return vec![route];
            }
            routes.push(route);
        }
    }
    routes
}

fn env_launcher_tail<'a>(raw_tokens: &'a [&'a str]) -> &'a [&'a str] {
    let Some((&first, rest)) = raw_tokens.split_first() else {
        return raw_tokens;
    };
    if first.rsplit('/').next() != Some("env") {
        return raw_tokens;
    }
    let mut index = 0;
    while index < rest.len() {
        let token = rest[index];
        if matches!(token, "-i" | "-0" | "--ignore-environment" | "--null") {
            index += 1;
        } else if matches!(token, "-u" | "--unset" | "-C" | "--chdir") {
            index += 2.min(rest.len() - index);
        } else if token.starts_with("--chdir=") || token.starts_with("--unset=") {
            index += 1;
        } else {
            break;
        }
    }
    &rest[index..]
}

pub(super) fn skip_assignment_keyword<'a>(tokens: &'a [&'a str]) -> &'a [&'a str] {
    match tokens {
        [keyword, rest @ ..]
            if matches!(
                *keyword,
                "export" | "readonly" | "declare" | "typeset" | "local"
            ) =>
        {
            let flags = rest.iter().take_while(|tok| tok.starts_with('-')).count();
            &rest[flags..]
        }
        _ => tokens,
    }
}

pub(super) fn assignment_stage_executor_routes(
    raw_tokens: &[&str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> Vec<RoutedTarget> {
    let tokens = skip_assignment_keyword(raw_tokens);
    if !is_assignment_only_stage(tokens) {
        return Vec::new();
    }
    let mut routes = Vec::new();
    for token in tokens {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if !is_shell_identifier(name) || !EXECUTOR_ENV_VARS.contains(&name) {
            continue;
        }
        let Some(route) = route_executor_value(value, home, ctx) else {
            continue;
        };
        if matches!(route, RoutedTarget::Unresolved { .. }) {
            return vec![route];
        }
        routes.push(route);
    }
    routes
}

pub(super) fn option_value_executor_route<'a>(
    program: &str,
    raw_tokens: &[&str],
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    match program {
        "git" => git_config_executor_route(raw_tokens, tokens, home, ctx),
        "man" => man_pager_executor_route(tokens, home, ctx),
        "ssh" | "scp" | "sftp" => ssh_option_executor_route(tokens, home, ctx),
        "rg" => long_option_executor_route("--pre", tokens, home, ctx),
        "ag" => long_option_executor_route("--pager", tokens, home, ctx),
        _ => (Vec::new(), Vec::new()),
    }
}

fn ssh_option_executor_route<'a>(
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    for (idx, token) in tokens.iter().enumerate() {
        let Some(group) = token.strip_prefix('-').filter(|g| !g.starts_with('-')) else {
            continue;
        };
        let Some((_, glued)) = group.split_once('o') else {
            continue;
        };
        let separate_token = glued.is_empty();
        let option = if separate_token {
            tokens.get(idx + 1).copied()
        } else {
            Some(glued)
        };
        let Some((key, value)) = option.and_then(ssh_option_key_value) else {
            continue;
        };
        if is_ssh_executor_option_key(key)
            && let Some(route) = route_executor_value(value, home, ctx)
        {
            if separate_token && let Some(&next) = tokens.get(idx + 1) {
                consumed.push(next);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}

fn is_ssh_executor_option_key(key: &str) -> bool {
    ["ProxyCommand", "LocalCommand", "KnownHostsCommand"]
        .into_iter()
        .any(|executor_key| key.eq_ignore_ascii_case(executor_key))
}

fn ssh_option_key_value(option: &str) -> Option<(&str, &str)> {
    let option = option.trim_start();
    let key_end = option.find(|c: char| c == '=' || c.is_whitespace())?;
    let (key, rest) = option.split_at(key_end);
    let rest = rest.trim_start();
    let value = rest.strip_prefix('=').unwrap_or(rest).trim_start();
    Some((key, value))
}

fn is_executor_config_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    let exact = matches!(
        lower.as_str(),
        "core.pager"
            | "core.editor"
            | "core.sshcommand"
            | "core.askpass"
            | "core.gitproxy"
            | "core.fsmonitor"
            | "diff.external"
            | "credential.helper"
            | "sequence.editor"
            | "gpg.program"
            | "uploadpack.packobjectshook"
            | "sendemail.sendmailcmd"
    );
    let family = lower.starts_with("alias.")
        || family_middle_segment(&lower, "credential.", ".helper").is_some()
        || family_middle_segment(&lower, "gpg.", ".program").is_some()
        || [
            ("filter.", ".clean"),
            ("filter.", ".smudge"),
            ("filter.", ".process"),
            ("diff.", ".command"),
            ("diff.", ".textconv"),
            ("merge.", ".driver"),
        ]
        .into_iter()
        .any(|(prefix, suffix)| family_middle_segment(&lower, prefix, suffix).is_some());
    exact || family
}

fn family_middle_segment<'a>(key: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    key.strip_prefix(prefix)?.strip_suffix(suffix)
}

fn git_config_executor_route<'a>(
    raw_tokens: &[&str],
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        let (assignment, names_an_env_var, separate_token) = if tok == "-c" {
            (iter.next().copied(), false, true)
        } else if tok == "--config-env" {
            (iter.next().copied(), true, true)
        } else if let Some(rest) = tok.strip_prefix("-c") {
            (Some(rest), false, false)
        } else {
            (tok.strip_prefix("--config-env="), true, false)
        };
        let Some((key, value)) = assignment.and_then(|a| a.split_once('=')) else {
            continue;
        };
        if !is_executor_config_key(key) {
            continue;
        }
        let route = if names_an_env_var {
            match assignment_prefix_value(raw_tokens, value) {
                Some(resolved) => route_executor_value(resolved, home, ctx),
                None => Some(RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                }),
            }
        } else {
            route_executor_value(value, home, ctx)
        };
        if let Some(route) = route {
            if separate_token && let Some(a) = assignment {
                consumed.push(a);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}

fn assignment_prefix_value<'a>(raw_tokens: &'a [&'a str], name: &str) -> Option<&'a str> {
    for token in env_launcher_tail(raw_tokens) {
        let Some((defined, value)) = token.split_once('=') else {
            break;
        };
        if !is_shell_identifier(defined) {
            break;
        }
        if defined == name {
            return Some(value);
        }
    }
    None
}

fn man_pager_executor_route<'a>(
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        let separate_token = tok == "-P" || tok == "--pager";
        let value = if separate_token {
            iter.next().copied()
        } else {
            tok.strip_prefix("--pager=")
                .or_else(|| tok.strip_prefix("-P").filter(|v| !v.is_empty()))
        };
        if let Some(route) = value.and_then(|value| route_executor_value(value, home, ctx)) {
            if separate_token && let Some(v) = value {
                consumed.push(v);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}

fn long_option_executor_route<'a>(
    flag: &str,
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let glued_prefix = format!("{flag}=");
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        if tok == "--" {
            break;
        }
        let separate_token = tok == flag;
        let value = if separate_token {
            iter.next().copied()
        } else {
            tok.strip_prefix(glued_prefix.as_str())
        };
        if let Some(route) = value.and_then(|value| route_executor_value(value, home, ctx)) {
            if separate_token && let Some(v) = value {
                consumed.push(v);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}
