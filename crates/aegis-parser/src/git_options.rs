use std::collections::VecDeque;

const NO_VALUE_OPTIONS: &[&str] = &[
    "-v",
    "--version",
    "-h",
    "--help",
    "--html-path",
    "--man-path",
    "--info-path",
    "-p",
    "--paginate",
    "-P",
    "--no-pager",
    "--bare",
    "--no-replace-objects",
    "--no-lazy-fetch",
    "--no-optional-locks",
    "--no-advice",
    "--literal-pathspecs",
    "--glob-pathspecs",
    "--noglob-pathspecs",
    "--icase-pathspecs",
];

const EQ_OR_NEXT_TOKEN_OPTIONS: &[&str] = &[
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
    "--attr-source",
];

pub const MAX_GIT_OPTION_CANDIDATES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitSubcommandStarts {
    Starts(Vec<usize>),
    TooMany,
}

enum OptionArity {
    NoValue,
    NextToken,
    EqOrNextToken,
    Unlisted,
}

fn classify(token: &str) -> OptionArity {
    if token == "-C" || token == "-c" {
        return OptionArity::NextToken;
    }

    let name = token.split('=').next().unwrap_or(token);

    if name == "--exec-path" {
        return OptionArity::NoValue;
    }

    if name == "--list-cmds" {
        return if token.contains('=') {
            OptionArity::NoValue
        } else {
            OptionArity::Unlisted
        };
    }

    if NO_VALUE_OPTIONS.contains(&token) {
        return OptionArity::NoValue;
    }

    if EQ_OR_NEXT_TOKEN_OPTIONS.contains(&name) {
        return OptionArity::EqOrNextToken;
    }

    OptionArity::Unlisted
}

fn successors(tokens: &[&str], pos: usize) -> [Option<usize>; 2] {
    let has_next = pos + 1 < tokens.len();

    match classify(tokens[pos]) {
        OptionArity::NoValue => [Some(pos + 1), None],
        OptionArity::NextToken => [has_next.then_some(pos + 2), None],
        OptionArity::EqOrNextToken => {
            if tokens[pos].contains('=') {
                [Some(pos + 1), None]
            } else {
                [has_next.then_some(pos + 2), None]
            }
        }
        OptionArity::Unlisted => [Some(pos + 1), has_next.then_some(pos + 2)],
    }
}

pub fn git_option_subcommand_starts(tokens: &[&str]) -> GitSubcommandStarts {
    if tokens.len() < 2 || !tokens[1].starts_with('-') {
        return GitSubcommandStarts::Starts(Vec::new());
    }

    let mut visited = vec![false; tokens.len()];
    let mut queue = VecDeque::new();
    let mut starts = Vec::new();

    visited[1] = true;
    queue.push_back(1);

    while let Some(pos) = queue.pop_front() {
        if !tokens[pos].starts_with('-') {
            starts.push(pos);
            if starts.len() > MAX_GIT_OPTION_CANDIDATES {
                return GitSubcommandStarts::TooMany;
            }
            continue;
        }

        for succ in successors(tokens, pos).into_iter().flatten() {
            if succ < tokens.len() && !visited[succ] {
                visited[succ] = true;
                queue.push_back(succ);
            }
        }
    }

    starts.sort_unstable();
    GitSubcommandStarts::Starts(starts)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct GitTargetArgs {
    pub dash_c: Vec<String>,
    pub work_tree: Option<String>,
    pub git_dir: Option<String>,
    pub bare: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitTargetReadings {
    Readings(Vec<GitTargetArgs>),
    TooMany,
}

const MAX_TARGET_WALK_STATES: usize = 1024;

fn apply_target_option(tokens: &[&str], pos: usize, args: &GitTargetArgs) -> GitTargetArgs {
    let token = tokens[pos];
    let mut next = args.clone();
    if token == "--bare" {
        next.bare = true;
        return next;
    }
    if token == "-C" {
        if let Some(value) = tokens.get(pos + 1) {
            next.dash_c.push((*value).to_string());
        }
        return next;
    }
    let (name, glued) = match token.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (token, None),
    };
    if name != "--work-tree" && name != "--git-dir" {
        return next;
    }
    let value = match glued {
        Some(value) => Some(value),
        None => tokens.get(pos + 1).copied(),
    };
    if let Some(value) = value {
        let value = value.to_string();
        if name == "--work-tree" {
            next.work_tree = Some(value);
        } else {
            next.git_dir = Some(value);
        }
    }
    next
}

pub fn git_option_target_readings(tokens: &[&str]) -> GitTargetReadings {
    use std::collections::HashSet;

    if tokens.get(1).is_none_or(|token| !token.starts_with('-')) {
        return GitTargetReadings::Readings(Vec::new());
    }

    let mut seen: HashSet<(usize, GitTargetArgs)> = HashSet::new();
    let mut stack = vec![(1usize, GitTargetArgs::default())];
    let mut readings: Vec<GitTargetArgs> = Vec::new();

    seen.insert((1, GitTargetArgs::default()));
    while let Some((pos, args)) = stack.pop() {
        if !tokens[pos].starts_with('-') {
            if !readings.contains(&args) {
                readings.push(args);
            }
            continue;
        }

        let carried = apply_target_option(tokens, pos, &args);
        for succ in successors(tokens, pos).into_iter().flatten() {
            if succ >= tokens.len() {
                continue;
            }
            let state = (succ, carried.clone());
            if seen.contains(&state) {
                continue;
            }
            if seen.len() >= MAX_TARGET_WALK_STATES {
                return GitTargetReadings::TooMany;
            }
            seen.insert(state.clone());
            stack.push(state);
        }
    }

    GitTargetReadings::Readings(readings)
}

#[cfg(test)]
mod tests {
    use super::{GitSubcommandStarts, MAX_GIT_OPTION_CANDIDATES, git_option_subcommand_starts};

    fn starts(tokens: &[&str]) -> Vec<usize> {
        match git_option_subcommand_starts(tokens) {
            GitSubcommandStarts::Starts(starts) => starts,
            GitSubcommandStarts::TooMany => panic!("unexpected TooMany for {tokens:?}"),
        }
    }

    #[test]
    fn empty_and_single_token_slices_produce_nothing() {
        assert_eq!(starts(&[]), Vec::<usize>::new());
        assert_eq!(starts(&["git"]), Vec::<usize>::new());
    }

    #[test]
    fn plain_subcommand_produces_nothing() {
        assert_eq!(starts(&["git", "reset", "--hard"]), Vec::<usize>::new());
    }

    #[test]
    fn dash_capital_c_takes_the_next_token_as_its_value() {
        assert_eq!(starts(&["git", "-C", ".", "reset", "--hard"]), vec![3]);
    }

    #[test]
    fn dash_c_takes_the_next_token_as_its_value() {
        assert_eq!(starts(&["git", "-c", "core.editor=vim", "commit"]), vec![3]);
    }

    #[test]
    fn git_dir_takes_a_glued_or_separate_value() {
        assert_eq!(
            starts(&["git", "--git-dir=.git", "branch", "-D", "x"]),
            vec![2]
        );
        assert_eq!(
            starts(&["git", "--git-dir", ".git", "branch", "-D", "x"]),
            vec![3]
        );
    }

    #[test]
    fn work_tree_takes_the_next_token_when_bare() {
        assert_eq!(
            starts(&["git", "--work-tree", "x", "reset", "--hard"]),
            vec![3]
        );
    }

    #[test]
    fn no_value_options_consume_only_themselves() {
        assert_eq!(starts(&["git", "--no-pager", "log"]), vec![2]);
        assert_eq!(starts(&["git", "-P", "log"]), vec![2]);
    }

    #[test]
    fn chain_of_several_known_options_resolves_to_one_start() {
        let tokens = [
            "git",
            "-C",
            ".",
            "-c",
            "x=y",
            "--no-pager",
            "reset",
            "--hard",
        ];
        assert_eq!(starts(&tokens), vec![6]);
    }

    #[test]
    fn bare_list_cmds_falls_to_the_unlisted_rule() {
        let tokens = ["git", "--list-cmds", "reset", "--hard"];
        assert_eq!(starts(&tokens), vec![2]);
    }

    #[test]
    fn list_cmds_with_value_consumes_only_itself() {
        assert_eq!(
            starts(&["git", "--list-cmds=main", "reset", "--hard"]),
            vec![2]
        );
    }

    #[test]
    fn bare_exec_path_consumes_only_itself() {
        assert_eq!(starts(&["git", "--exec-path", "reset", "--hard"]), vec![2]);
    }

    #[test]
    fn exec_path_with_glued_value_consumes_only_itself() {
        assert_eq!(
            starts(&["git", "--exec-path=/usr/lib/git-core", "reset", "--hard"]),
            vec![2]
        );
    }

    #[test]
    fn dangling_option_with_no_following_token_produces_nothing() {
        assert_eq!(starts(&["git", "-C"]), Vec::<usize>::new());
        assert_eq!(starts(&["git", "--work-tree"]), Vec::<usize>::new());
    }

    #[test]
    fn print_and_exit_option_with_no_following_token_produces_nothing() {
        assert_eq!(starts(&["git", "--version"]), Vec::<usize>::new());
    }

    #[test]
    fn glued_dash_capital_c_short_form_is_treated_as_unlisted_and_still_finds_reset() {
        assert_eq!(starts(&["git", "-C/tmp", "reset", "--hard"]), vec![2]);
    }

    #[test]
    fn glued_dash_c_short_form_is_treated_as_unlisted_and_still_finds_reset() {
        assert_eq!(starts(&["git", "-cfoo=bar", "reset", "--hard"]), vec![2]);
    }

    #[test]
    fn unlisted_option_yields_both_readings() {
        assert_eq!(starts(&["git", "--newopt", "reset", "--hard"]), vec![2]);
        assert_eq!(
            starts(&["git", "--newopt", "x", "reset", "--hard"]),
            vec![2, 3]
        );
    }

    #[test]
    fn unlisted_option_chain_yields_linear_not_exponential_candidates() {
        let mut tokens: Vec<String> = vec!["git".to_string()];
        for i in 0..2000 {
            tokens.push(format!("--unknown-{i}"));
        }
        tokens.push("reset".to_string());
        tokens.push("--hard".to_string());
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        let result = starts(&token_refs);

        assert!(
            result.len() <= token_refs.len(),
            "candidate count must stay linear in token count: got {} starts for {} tokens",
            result.len(),
            token_refs.len()
        );
    }

    fn alternating_unlisted_option_value_tokens(pairs: usize) -> Vec<String> {
        let mut tokens: Vec<String> = vec!["git".to_string()];
        for i in 0..pairs {
            tokens.push(format!("--o{i}"));
            tokens.push(format!("v{i}"));
        }
        tokens.push("reset".to_string());
        tokens.push("--hard".to_string());
        tokens
    }

    #[test]
    fn exactly_the_cap_worth_of_candidates_still_resolves() {
        let tokens = alternating_unlisted_option_value_tokens(MAX_GIT_OPTION_CANDIDATES - 1);
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        let result = starts(&token_refs);
        assert_eq!(result.len(), MAX_GIT_OPTION_CANDIDATES);
    }

    #[test]
    fn one_more_than_the_cap_reports_too_many() {
        let tokens = alternating_unlisted_option_value_tokens(MAX_GIT_OPTION_CANDIDATES);
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        assert_eq!(
            git_option_subcommand_starts(&token_refs),
            GitSubcommandStarts::TooMany
        );
    }

    #[test]
    fn two_thousand_unlisted_option_value_pairs_report_too_many_fast() {
        let tokens = alternating_unlisted_option_value_tokens(2000);
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        let started = std::time::Instant::now();
        let result = git_option_subcommand_starts(&token_refs);
        let elapsed = started.elapsed();

        assert_eq!(result, GitSubcommandStarts::TooMany);
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "must bail out at the cap, not walk all 2000 pairs: took {elapsed:?}"
        );
    }

    mod target_readings {
        use super::super::{GitTargetArgs, GitTargetReadings, git_option_target_readings};

        fn readings(tokens: &[&str]) -> Vec<GitTargetArgs> {
            match git_option_target_readings(tokens) {
                GitTargetReadings::Readings(readings) => readings,
                GitTargetReadings::TooMany => panic!("unexpected TooMany for {tokens:?}"),
            }
        }

        fn args(dash_c: &[&str], work_tree: Option<&str>, git_dir: Option<&str>) -> GitTargetArgs {
            GitTargetArgs {
                dash_c: dash_c.iter().map(|s| (*s).to_string()).collect(),
                work_tree: work_tree.map(str::to_string),
                git_dir: git_dir.map(str::to_string),
                ..GitTargetArgs::default()
            }
        }

        #[test]
        fn bare_option_is_recorded_in_the_reading() {
            let reading = readings(&["git", "-C", "r.git", "--bare", "filter-branch"]);

            assert!(reading[0].bare);
        }

        #[test]
        fn empty_and_single_token_slices_have_no_readings() {
            assert!(readings(&[]).is_empty());
            assert!(readings(&["git"]).is_empty());
        }

        #[test]
        fn plain_subcommand_has_no_readings() {
            assert!(readings(&["git", "reset", "--hard"]).is_empty());
        }

        #[test]
        fn dash_c_chain_is_kept_in_command_order() {
            assert_eq!(
                readings(&["git", "-C", "a", "-C", "b", "reset", "--hard"]),
                vec![args(&["a", "b"], None, None)]
            );
        }

        #[test]
        fn work_tree_after_dash_c_is_kept_separately() {
            assert_eq!(
                readings(&["git", "-C", "a", "--work-tree", "w", "reset", "--hard"]),
                vec![args(&["a"], Some("w"), None)]
            );
        }

        #[test]
        fn work_tree_and_git_dir_accept_glued_and_separate_values() {
            assert_eq!(
                readings(&["git", "--work-tree=w", "--git-dir=w/.git", "reset"]),
                vec![args(&[], Some("w"), Some("w/.git"))]
            );
            assert_eq!(
                readings(&["git", "--work-tree", "w", "--git-dir", "w/.git", "reset"]),
                vec![args(&[], Some("w"), Some("w/.git"))]
            );
        }

        #[test]
        fn last_work_tree_wins() {
            assert_eq!(
                readings(&["git", "--work-tree=a", "--work-tree=b", "reset"]),
                vec![args(&[], Some("b"), None)]
            );
        }

        #[test]
        fn options_that_do_not_move_the_target_leave_empty_args() {
            assert_eq!(
                readings(&["git", "-c", "x=y", "--no-pager", "reset", "--hard"]),
                vec![args(&[], None, None)]
            );
        }

        #[test]
        fn unlisted_option_that_does_not_touch_the_target_adds_no_reading() {
            assert_eq!(
                readings(&["git", "--newopt", "x", "reset", "--hard"]),
                vec![args(&[], None, None)]
            );
        }

        #[test]
        fn unlisted_option_swallowing_dash_c_gives_two_different_readings() {
            let found = readings(&["git", "--newopt", "-C", "a", "reset", "--hard"]);
            assert!(found.contains(&args(&["a"], None, None)));
            assert!(found.contains(&args(&[], None, None)));
        }

        #[test]
        fn dangling_dash_c_has_no_reading() {
            assert!(readings(&["git", "-C"]).is_empty());
        }

        #[test]
        fn hostile_unlisted_chain_stays_bounded() {
            let mut tokens: Vec<String> = vec!["git".to_string()];
            for i in 0..2000 {
                tokens.push(format!("--unknown-{i}"));
                tokens.push("-C".to_string());
                tokens.push(format!("d{i}"));
            }
            tokens.push("reset".to_string());
            let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
            assert_eq!(
                git_option_target_readings(&refs),
                GitTargetReadings::TooMany
            );
        }
    }
}
