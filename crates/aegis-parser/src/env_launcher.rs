use crate::{is_environment_assignment, program_basename};

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
        index += env_split_string_leading_option_len(tail, index)?;
    }
    None
}

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
    use crate::{effective_program, effective_token_slices, effective_token_slices_checked};

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

    #[test]
    fn effective_token_slices_split_env_dash_s_after_leading_assignment() {
        let tokens = ["FOO=1", "env", "-S", "python3 ./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
    }

    #[test]
    fn effective_token_slices_split_env_dash_s_after_sudo_prefix() {
        let tokens = ["sudo", "env", "-S", "python3 ./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
    }

    #[test]
    fn effective_program_splits_env_dash_s_after_sudo_prefix() {
        let tokens = ["sudo", "env", "-S", "python3 ./evil.py"];
        assert_eq!(effective_program(&tokens), Some("python3"));
    }

    #[test]
    fn effective_token_slices_split_env_dash_s_nested_twice() {
        let tokens = ["sudo", "env", "-S", "env -S python3 ./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
        assert!(!effective_token_slices_checked(&tokens).1);
    }

    fn env_s_chain_tokens(repeats: usize) -> Vec<&'static str> {
        let mut tokens = vec!["env", "-S"];
        tokens.extend(std::iter::repeat_n("env -S", repeats));
        tokens.push("true");
        tokens
    }

    #[test]
    fn effective_program_resolves_an_env_dash_s_chain_exactly_at_the_nesting_bound() {
        let tokens = env_s_chain_tokens(crate::ENV_SPLIT_MAX_DEPTH as usize - 1);
        let refs: Vec<&str> = tokens.to_vec();

        assert!(!effective_token_slices_checked(&refs).1);
        assert_eq!(effective_program(&refs), Some("true"));
    }

    #[test]
    fn effective_program_resolution_fails_closed_past_the_env_dash_s_nesting_bound() {
        let tokens = env_s_chain_tokens(crate::ENV_SPLIT_MAX_DEPTH as usize);
        let refs: Vec<&str> = tokens.to_vec();

        assert!(effective_token_slices_checked(&refs).1);
    }

    #[test]
    fn effective_program_resolution_of_a_deep_env_dash_s_chain_does_not_overflow_the_stack() {
        let handle = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let tokens = env_s_chain_tokens(3000);
                let refs: Vec<&str> = tokens.to_vec();
                effective_token_slices_checked(&refs).1
            })
            .expect("failed to spawn the small-stack test thread");

        let truncated = handle.join().expect("resolution overflowed the stack");
        assert!(truncated, "a 3000-deep chain must exceed the nesting bound");
    }
}
