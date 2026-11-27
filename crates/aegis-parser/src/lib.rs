mod aegis_options;
mod embedded_scripts;
mod env_launcher;
mod git_options;
mod heredoc_data_consumer;
mod list_segments;
mod nested_shells;
mod prefix_match;
mod redirection;
mod runner;
mod segmentation;
mod tokenizer;

pub use aegis_options::aegis_option_subcommand_start;
#[cfg(test)]
use aegis_types::InlineScript;
use aegis_types::ParsedCommand;
pub use embedded_scripts::{
    HeredocBody, extract_eval_payloads, extract_heredoc_bodies, extract_inline_scripts,
    extract_process_substitution_bodies, mask_inert_heredoc_substitution_markers,
    split_at_heredoc_marker,
};
use env_launcher::{env_prefix_lengths, env_split_string_tokens};
pub use git_options::{
    GitSubcommandStarts, GitTargetArgs, GitTargetReadings, MAX_GIT_OPTION_CANDIDATES,
    git_option_subcommand_starts, git_option_target_readings,
};
pub use list_segments::{ListSegment, ListSeparator};
pub use nested_shells::extract_nested_commands;
#[cfg(test)]
pub use prefix_match::matches_prefix_ending_options;
pub use prefix_match::{EndOfOptions, contains_any_token, matches_prefix, matches_prefix_with};
use redirection::redirection_token_len;
pub use redirection::{is_redirection_operator, starts_with_redirection_glyph};
pub use runner::Runner;
pub use segmentation::{
    ScanSegment, extract_command_substitution_bodies, has_multiple_top_level_segments,
    list_segments, logical_scan_segments, logical_segments, mask_command_substitutions,
    top_level_pipelines, unwrap_subshell_group,
};
pub use tokenizer::{
    ansi_c_quoting_is_inexact, canonical_ansi_c_quoting, canonical_ansi_c_quoting_marked,
    extract_prefix, split_tokens,
};

pub const COMMAND_STARTING_KEYWORDS: [&str; 9] = [
    "!", "if", "then", "elif", "else", "while", "until", "do", "time",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveTokenSlice<'a> {
    pub tokens: Vec<&'a str>,
    pub program: &'a str,
}

pub fn effective_token_slices<'a>(tokens: &[&'a str]) -> Vec<EffectiveTokenSlice<'a>> {
    effective_token_slices_checked(tokens).0
}

pub fn effective_token_slices_checked<'a>(
    tokens: &[&'a str],
) -> (Vec<EffectiveTokenSlice<'a>>, bool) {
    if tokens.is_empty() {
        return (Vec::new(), false);
    }

    let (starts, owned, truncated) = effective_program_resolution(tokens);
    let mut slices = Vec::with_capacity(starts.len() + owned.len());
    for start in starts {
        let Some(effective_tokens) = effective_tokens_at(tokens, start) else {
            continue;
        };
        let program = effective_tokens[0];
        slices.push(EffectiveTokenSlice {
            tokens: effective_tokens,
            program,
        });
    }
    for effective_tokens in owned {
        let Some(&program) = effective_tokens.first() else {
            continue;
        };
        slices.push(EffectiveTokenSlice {
            tokens: effective_tokens,
            program,
        });
    }
    (slices, truncated)
}

pub fn effective_program<'a>(tokens: &[&'a str]) -> Option<&'a str> {
    let (starts, owned, _truncated) = effective_program_resolution(tokens);
    if let Some(&start) = starts.first() {
        return tokens.get(start).map(|token| program_basename(token));
    }
    owned.into_iter().next()?.into_iter().next()
}

const ENV_SPLIT_MAX_DEPTH: u32 = 8;

fn effective_program_resolution<'a>(tokens: &[&'a str]) -> (Vec<usize>, Vec<Vec<&'a str>>, bool) {
    let mut starts = Vec::new();
    let mut owned = Vec::new();
    let mut truncated = false;
    collect_effective_program_indices(tokens, 0, &mut starts, &mut owned, 0, &mut truncated);
    starts.sort_unstable();
    starts.dedup();
    (starts, owned, truncated)
}

fn collect_effective_program_indices<'a>(
    tokens: &[&'a str],
    index: usize,
    starts: &mut Vec<usize>,
    owned: &mut Vec<Vec<&'a str>>,
    depth: u32,
    truncated: &mut bool,
) {
    if index >= tokens.len() {
        return;
    }

    if starts_with_redirection_glyph(tokens[index]) {
        let len = redirection_token_len(tokens[index]).min(tokens.len() - index);
        collect_effective_program_indices(tokens, index + len, starts, owned, depth, truncated);
        return;
    }

    let assignment_prefix_len = leading_environment_assignment_prefix_len(&tokens[index..]);
    if assignment_prefix_len > 0 {
        collect_effective_program_indices(
            tokens,
            index + assignment_prefix_len,
            starts,
            owned,
            depth,
            truncated,
        );
        return;
    }

    if let Some(split) = env_split_string_tokens(&tokens[index..]) {
        if depth >= ENV_SPLIT_MAX_DEPTH {
            *truncated = true;
            return;
        }
        collect_effective_program_slices(&split, owned, depth + 1, truncated);
        return;
    }

    match launcher_prefix_lengths(&tokens[index..]) {
        Some(lengths) => {
            for len in lengths {
                if len == 0 {
                    starts.push(index);
                } else {
                    collect_effective_program_indices(
                        tokens,
                        index + len,
                        starts,
                        owned,
                        depth,
                        truncated,
                    );
                }
            }
        }
        None => starts.push(index),
    }
}

fn collect_effective_program_slices<'a>(
    tokens: &[&'a str],
    owned: &mut Vec<Vec<&'a str>>,
    depth: u32,
    truncated: &mut bool,
) {
    let mut starts = Vec::new();
    collect_effective_program_indices(tokens, 0, &mut starts, owned, depth, truncated);
    for start in starts {
        if let Some(effective_tokens) = effective_tokens_at(tokens, start) {
            owned.push(effective_tokens);
        }
    }
}

fn effective_tokens_at<'a>(tokens: &[&'a str], start: usize) -> Option<Vec<&'a str>> {
    let program = program_basename(tokens.get(start)?);
    let mut effective_tokens = Vec::with_capacity(tokens.len().saturating_sub(start));
    effective_tokens.push(program);
    effective_tokens.extend(tokens[start + 1..].iter().copied());
    Some(effective_tokens)
}

fn leading_environment_assignment_prefix_len(tokens: &[&str]) -> usize {
    tokens
        .iter()
        .take_while(|token| is_environment_assignment(token))
        .count()
}

pub(crate) fn is_environment_assignment(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn launcher_prefix_lengths(tokens: &[&str]) -> Option<Vec<usize>> {
    let launcher = program_basename(tokens.first().copied()?);
    if launcher.eq_ignore_ascii_case("rtk") {
        return Some(rtk_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("nohup")
        || launcher.eq_ignore_ascii_case("time")
        || launcher.eq_ignore_ascii_case("command")
        || launcher.eq_ignore_ascii_case("doas")
        || launcher.eq_ignore_ascii_case("exec")
        || launcher.eq_ignore_ascii_case("builtin")
        || launcher.eq_ignore_ascii_case("coproc")
    {
        return Some(vec![1]);
    }

    if launcher.eq_ignore_ascii_case("timeout") {
        return Some(timeout_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("nice") {
        return Some(vec![nice_prefix_len(tokens)]);
    }

    if launcher.eq_ignore_ascii_case("sudo") {
        return Some(sudo_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("env") {
        return Some(env_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("xargs") {
        return Some(vec![xargs_prefix_len(tokens)]);
    }

    if let Some(prefix_len) =
        Runner::from_program(launcher).and_then(|runner| runner.command_prefix_len(tokens))
    {
        return Some(vec![prefix_len]);
    }

    None
}

fn xargs_prefix_len(tokens: &[&str]) -> usize {
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "--" {
            index += 1;
            break;
        }
        if !token.starts_with('-') {
            break;
        }
        index += 1;
        if matches!(
            token,
            "-a" | "--arg-file"
                | "-d"
                | "--delimiter"
                | "-E"
                | "-I"
                | "-i"
                | "--replace"
                | "-L"
                | "--max-lines"
                | "-l"
                | "-n"
                | "--max-args"
                | "-P"
                | "--max-procs"
                | "-s"
                | "--max-chars"
        ) && index < tokens.len()
        {
            index += 1;
        }
    }
    index
}

fn rtk_prefix_lengths(tokens: &[&str]) -> Vec<usize> {
    match tokens.get(1) {
        Some(sub) if sub.eq_ignore_ascii_case("proxy") => vec![2],
        _ => vec![1],
    }
}

fn sudo_prefix_lengths(tokens: &[&str]) -> Vec<usize> {
    sudo_prefix_lengths_from(tokens, 1)
}

fn sudo_prefix_lengths_from(tokens: &[&str], mut index: usize) -> Vec<usize> {
    while index < tokens.len() {
        let token = tokens[index];
        if token.contains('=') {
            index += 1;
            continue;
        }
        if !token.starts_with('-') || token == "-" {
            break;
        }
        index += 1;
        if matches!(
            token,
            "-u" | "--user"
                | "-g"
                | "--group"
                | "-h"
                | "--host"
                | "-p"
                | "--prompt"
                | "-C"
                | "--close-from"
                | "-T"
                | "--command-timeout"
        ) && index < tokens.len()
        {
            index += 1;
        } else if index < tokens.len() {
            let mut candidates = sudo_prefix_lengths_from(tokens, index);
            if index < tokens.len() {
                candidates.push(index + 1);
            }
            candidates.sort_unstable();
            candidates.dedup();
            return candidates;
        }
    }
    vec![index]
}

fn timeout_prefix_lengths(tokens: &[&str]) -> Vec<usize> {
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "--" {
            index += 1;
            break;
        }
        if matches!(
            token,
            "-v" | "--verbose" | "--foreground" | "--preserve-status"
        ) || token.starts_with("--signal=")
            || token.starts_with("--kill-after=")
            || short_timeout_option_with_value(token)
        {
            index += 1;
            continue;
        }
        if matches!(token, "-s" | "--signal" | "-k" | "--kill-after") {
            index += 2.min(tokens.len() - index);
            continue;
        }
        if token.starts_with('-') {
            return if index + 1 < tokens.len() {
                vec![index + 1, index + 2]
            } else {
                vec![index + 1]
            };
        }
        break;
    }
    vec![(index + 1).min(tokens.len())]
}

fn short_timeout_option_with_value(token: &str) -> bool {
    token.len() > 2 && (token.starts_with("-s") || token.starts_with("-k"))
}

fn nice_prefix_len(tokens: &[&str]) -> usize {
    if tokens.len() > 2 && matches!(tokens[1], "-n" | "--adjustment") {
        3
    } else if tokens.len() > 1 && tokens[1].starts_with('-') && tokens[1].len() > 1 {
        2
    } else {
        1
    }
}

pub(crate) fn program_basename(token: &str) -> &str {
    token
        .rsplit_once('/')
        .map_or(token, |(_, basename)| basename)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineSegment {
    pub raw: String,
    pub normalized: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineChain {
    pub raw: String,
    pub segments: Vec<PipelineSegment>,
}

pub struct Parser;

impl Parser {
    pub fn parse(cmd: &str) -> ParsedCommand {
        let tokens = split_tokens(cmd);

        let first_cmd: Vec<&String> = tokens
            .iter()
            .take_while(|t| !matches!(t.as_str(), ";" | "&&" | "||" | "|"))
            .collect();

        let program = first_cmd.first().map(|s| s.to_string());
        let argv: Vec<String> = first_cmd.iter().skip(1).map(|s| s.to_string()).collect();

        let normalized = tokens.join(" ");

        let inline_scripts = extract_inline_scripts(cmd);

        ParsedCommand {
            program,
            argv,
            normalized,
            inline_scripts,
            raw: cmd.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) mod corpus_hash;
    mod heredoc_data_consumer_tests;
    mod parsing_tests;
    mod quote_tracking_tests;
    mod runner_tests;
    mod segments_logical_tests;
    mod tokenizer_tests;

    #[test]
    fn effective_token_slices_strip_launchers_and_absolute_paths() {
        let tokens = [
            "sudo",
            "env",
            "FOO=bar",
            "rtk",
            "/usr/bin/git",
            "reset",
            "--hard",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_strip_shell_launchers() {
        for launcher in ["exec", "builtin", "coproc"] {
            let tokens = [launcher, "git", "push", "--force"];
            let slices = effective_token_slices(&tokens);
            assert_eq!(slices[0].program, "git", "{launcher}");
            assert_eq!(slices[0].tokens, vec!["git", "push", "--force"]);
        }
    }

    #[test]
    fn effective_token_slices_strip_xargs_launcher() {
        let tokens = ["xargs", "python3", "./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
    }

    #[test]
    fn effective_token_slices_strip_xargs_flags_before_the_program() {
        let tokens = ["xargs", "-n1", "python3", "./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
    }

    #[test]
    fn effective_token_slices_strip_rtk_proxy_meta_command() {
        let tokens = ["rtk", "proxy", "git", "rebase", "origin/main"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "rebase", "origin/main"]);
    }

    #[test]
    fn effective_token_slices_strip_rtk_proxy_meta_command_case_insensitively() {
        let tokens = ["rtk", "PROXY", "git", "rebase", "origin/main"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "rebase", "origin/main"]);
    }

    #[test]
    fn effective_token_slices_treat_rtk_own_subcommand_as_program() {
        let tokens = ["rtk", "gain", "--history"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "gain");
        assert_eq!(slices[0].tokens, vec!["gain", "--history"]);
    }

    #[test]
    fn effective_token_slices_skip_launcher_options() {
        let tokens = [
            "sudo",
            "-u",
            "root",
            "timeout",
            "5s",
            "/bin/kill",
            "-9",
            "1",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "kill");
        assert_eq!(slices[0].tokens, vec!["kill", "-9", "1"]);
    }

    #[test]
    fn effective_token_slices_parse_timeout_options() {
        let tokens = [
            "timeout",
            "-s",
            "KILL",
            "-k",
            "10s",
            "5s",
            "/usr/bin/git",
            "reset",
            "--hard",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_keep_conservative_unknown_sudo_flag_candidates() {
        let tokens = ["sudo", "--new-opt", "value", "git", "reset", "--hard"];
        let slices = effective_token_slices(&tokens);
        let programs: Vec<&str> = slices.iter().map(|slice| slice.program).collect();

        assert!(programs.contains(&"value"));
        assert!(programs.contains(&"git"));
    }

    #[test]
    fn effective_token_slices_keep_scanning_after_unknown_sudo_flags() {
        let tokens = ["sudo", "-n", "-u", "postgres", "psql", "-c", "DROP TABLE t"];
        let slices = effective_token_slices(&tokens);
        let programs: Vec<&str> = slices.iter().map(|slice| slice.program).collect();

        assert!(programs.contains(&"psql"));
    }

    #[test]
    fn effective_token_slices_skip_sudo_environment_assignment() {
        let tokens = ["sudo", "FOO=bar", "git", "reset", "--hard"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_skip_leading_environment_assignments() {
        let tokens = [
            "CARGO_TARGET_DIR=/tmp/aegis",
            "FOO=x",
            "git",
            "reset",
            "--hard",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_keeps_non_shell_assignment_as_a_program() {
        let tokens = ["1FOO=not-a-command", "echo", "hello"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "1FOO=not-a-command");
        assert_eq!(
            slices[0].tokens,
            vec!["1FOO=not-a-command", "echo", "hello"]
        );
    }
}
