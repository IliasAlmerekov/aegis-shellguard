//! `env` launcher-prefix parsing: recognizing `env -S`/`--split-string` and
//! the generic `env <assignments/flags> <program>` shape.

use crate::{is_environment_assignment, program_basename};

/// Recognize `env -S "<command line>" [ARGS...]` / `env --split-string
/// "<command line>" [ARGS...]` and its wider shapes (issue #384/#430):
/// GNU `env`'s split-string mode treats its value as a whole command line to
/// split into words — the program's argv, not a value preceding a further
/// program token — and appends any further `env` command-line tokens after
/// it as-is (real `env -S` argv semantics: `split(STRING) ++ ARGS`, matching
/// a shebang line such as `#!/usr/bin/env -S python3 -u`, which needs its
/// own script path appended by the kernel). Recognized: the value as a
/// separate token (`-S "cmd"`) or glued to the flag (`-S"cmd"`), the
/// `--split-string=<value>` equals form, and any of `env`'s own
/// environment-clearing/unset/chdir/assignment options ahead of the split
/// flag (`env -i -S "cmd"`, `env -u X -S "cmd"`, `env -C d1 -S "cmd"`,
/// `env FOO=1 -S "cmd"`). A shape this does not recognize (an unrecognized
/// flag ahead of `-S`, or a value that splits to nothing) returns [`None`] —
/// a caller that needs the split command's own program should treat that as
/// "not confidently split", not as "safe to skip": the generic env-prefix
/// handling in [`env_prefix_lengths`] it falls through to does not itself
/// know split-string semantics, so it must never resolve such a shape to an
/// in-range program token either (it would be the wrong token) — only to
/// nothing found at all, which the router degrades rather than silently
/// treating as an unrelated, ordinary command with no program (issue
/// #384/#430). `split_whitespace` reuses the original tokens' lifetime — no
/// allocation, and no quote-aware re-tokenization of the value (a narrower
/// but safe subset of GNU `env`'s own splitting).
pub(crate) fn env_split_string_tokens<'a>(tokens: &[&'a str]) -> Option<Vec<&'a str>> {
    let (env, tail) = tokens.split_first()?;
    if !program_basename(env).eq_ignore_ascii_case("env") {
        return None;
    }

    let mut index = 0;
    while index < tail.len() {
        if let Some((value, consumed)) = split_string_flag_value(tail, index) {
            let mut words: Vec<&str> = value.split_whitespace().collect();
            if words.is_empty() {
                return None;
            }
            words.extend_from_slice(&tail[index + consumed..]);
            return Some(words);
        }
        match env_split_string_leading_option_len(tail, index) {
            Some(len) => index += len,
            None => return None,
        }
    }
    None
}

/// The split-string flag at `tail[index]`, in any of its three recognized
/// shapes, as `(value, tokens consumed)`: `-S`/`--split-string` with the
/// value in the next token (`2`), or `-S<value>`/`--split-string=<value>`
/// glued into the flag's own token (`1`).
fn split_string_flag_value<'a>(tail: &[&'a str], index: usize) -> Option<(&'a str, usize)> {
    let token = tail[index];
    if let Some(value) = token.strip_prefix("--split-string=") {
        return Some((value, 1));
    }
    if let Some(value) = token.strip_prefix("-S") {
        if !value.is_empty() {
            return Some((value, 1));
        }
        return tail.get(index + 1).map(|value| (*value, 2));
    }
    if token == "--split-string" {
        return tail.get(index + 1).map(|value| (*value, 2));
    }
    None
}

/// Number of tokens consumed by a recognized `env` option ahead of the
/// split-string flag (an environment assignment, `-i`/`--ignore-
/// environment`, `-0`/`--null`, `-u`/`--unset NAME`, or `-C`/`--chdir DIR`),
/// or [`None`] for anything else — which stops [`env_split_string_tokens`]
/// from guessing past an option it does not recognize. `-C`/`--chdir` must
/// be recognized here, not left to fall through to [`env_prefix_lengths`]:
/// chained with `-S` (`env -C d1 -S "cmd"`), the generic handler's flag+
/// value skips land exactly on the split flag's own value token as if it
/// were a plain flag argument, walking the index straight past the last
/// token — an out-of-range program index that silently drops the whole
/// command instead of routing it (issue #384/#430).
fn env_split_string_leading_option_len(tail: &[&str], index: usize) -> Option<usize> {
    let token = tail[index];
    if is_environment_assignment(token) {
        return Some(1);
    }
    if matches!(token, "-i" | "--ignore-environment" | "-0" | "--null") {
        return Some(1);
    }
    if matches!(token, "-u" | "--unset" | "-C" | "--chdir") {
        return tail.get(index + 1).is_some().then_some(2);
    }
    if token.starts_with("--chdir=") {
        return Some(1);
    }
    None
}

pub(crate) fn env_prefix_lengths(tokens: &[&str]) -> Vec<usize> {
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index];
        if token.contains('=') || token == "-" {
            index += 1;
            continue;
        }
        if matches!(token, "-i" | "-0" | "--ignore-environment" | "--null") {
            index += 1;
            continue;
        }
        if matches!(
            token,
            "-u" | "--unset" | "-C" | "--chdir" | "-S" | "--split-string"
        ) {
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
    vec![index]
}

#[cfg(test)]
mod tests {
    use crate::{effective_program, effective_token_slices};

    #[test]
    fn effective_token_slices_split_env_dash_s_string_into_the_real_program() {
        let tokens = ["env", "-S", "python3 ./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
    }

    #[test]
    fn effective_program_splits_env_dash_s_string_too() {
        let tokens = ["env", "--split-string", "python3 ./evil.py"];
        assert_eq!(effective_program(&tokens), Some("python3"));
    }

    #[test]
    fn effective_token_slices_keep_env_no_arg_candidate_for_unknown_flag() {
        let tokens = ["env", "-X", "git", "reset", "--hard"];
        let slices = effective_token_slices(&tokens);
        let programs: Vec<&str> = slices.iter().map(|slice| slice.program).collect();

        assert!(programs.contains(&"git"));
    }
}
