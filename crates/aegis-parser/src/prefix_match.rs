//! Token-level prefix-pattern matching.
//!
//! The algorithm operates purely on a [`PrefixPattern`] (a sequence of
//! [`PatternToken`]s) and a slice of command tokens. It is decoupled from any
//! rule or scanner type so it can live at the parser layer; rule types graft
//! their own metadata on top by delegating here.

use aegis_types::{PatternToken, PrefixPattern};

/// Compare two tokens for prefix-rule equality.
///
/// Shell flags (tokens beginning with `-`) are compared case-sensitively;
/// everything else is compared case-insensitively so that SQL keywords and
/// command names match regardless of casing.
fn str_eq_maybe_case(a: &str, b: &str) -> bool {
    if a.starts_with('-') || b.starts_with('-') {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}

/// Check whether any token in `tokens` equals one of `candidates`.
///
/// Token comparison follows the same rule as prefix matching: flags are
/// compared case-sensitively, everything else case-insensitively. Position is
/// irrelevant — a candidate anywhere in the token list counts.
///
/// Used for a prefix rule's negative condition: a token whose presence
/// suppresses the rule (e.g. `--dry-run`) can appear at any position, so a
/// positional pattern token cannot express it.
pub fn contains_any_token(tokens: &[&str], candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| {
        tokens
            .iter()
            .any(|token| str_eq_maybe_case(token, candidate))
    })
}

/// Check whether `tokens` matches `pattern` as a token prefix.
///
/// Supports [`PatternToken::Single`], [`PatternToken::Alts`],
/// [`PatternToken::Any`], [`PatternToken::AnyStar`] and
/// [`PatternToken::ShortFlag`]. The pattern must be a prefix of `tokens` —
/// extra trailing tokens are allowed. Empty patterns never match.
pub fn matches_prefix(pattern: &PrefixPattern, tokens: &[&str]) -> bool {
    if pattern.is_empty() {
        return false;
    }
    matches_from(pattern, tokens, 0)
}

fn matches_from(pattern: &PrefixPattern, tokens: &[&str], pat_idx: usize) -> bool {
    if pat_idx == pattern.len() {
        return true;
    }
    match &pattern[pat_idx] {
        PatternToken::Single(s) => {
            if tokens.is_empty() || !str_eq_maybe_case(tokens[0], s.as_ref()) {
                return false;
            }
            matches_from(pattern, &tokens[1..], pat_idx + 1)
        }
        PatternToken::Alts(alts) => {
            if tokens.is_empty()
                || !alts
                    .iter()
                    .any(|a| str_eq_maybe_case(tokens[0], a.as_ref()))
            {
                return false;
            }
            matches_from(pattern, &tokens[1..], pat_idx + 1)
        }
        PatternToken::Any => {
            if tokens.is_empty() {
                return false;
            }
            matches_from(pattern, &tokens[1..], pat_idx + 1)
        }
        PatternToken::AnyStar => {
            for skip in 0..=tokens.len() {
                if matches_from(pattern, &tokens[skip..], pat_idx + 1) {
                    return true;
                }
            }
            false
        }
        PatternToken::ShortFlag { short, long } => {
            if tokens.is_empty() {
                return false;
            }
            let tok = tokens[0];
            let matches = long.contains(&tok)
                || (tok.starts_with('-') && !tok.starts_with("--") && tok.contains(*short));
            if !matches {
                return false;
            }
            matches_from(pattern, &tokens[1..], pat_idx + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{contains_any_token, matches_prefix};
    use aegis_types::PatternToken;
    use std::borrow::Cow;

    fn single(s: &'static str) -> PatternToken {
        PatternToken::Single(Cow::Borrowed(s))
    }

    fn alts(alts: &[&'static str]) -> PatternToken {
        PatternToken::Alts(alts.iter().map(|&s| Cow::Borrowed(s)).collect())
    }

    fn short_flag(short: char, long: &'static [&'static str]) -> PatternToken {
        PatternToken::ShortFlag { short, long }
    }

    #[test]
    fn empty_pattern_never_matches() {
        assert!(!matches_prefix(&vec![], &["anything"]));
        assert!(!matches_prefix(&vec![], &[]));
    }

    #[test]
    fn single_token_matches_as_prefix_with_trailing_tokens() {
        assert!(matches_prefix(&vec![single("rm")], &["rm", "-rf", "/"]));
    }

    #[test]
    fn fails_on_insufficient_tokens() {
        let pattern = vec![single("git"), single("push"), single("origin")];
        assert!(!matches_prefix(&pattern, &["git", "push"]));
    }

    #[test]
    fn alts_matches_any_alternative() {
        let pattern = vec![single("git"), single("push"), alts(&["--force", "-f"])];
        assert!(matches_prefix(&pattern, &["git", "push", "--force"]));
        assert!(matches_prefix(&pattern, &["git", "push", "-f"]));
        assert!(!matches_prefix(&pattern, &["git", "push", "--dry-run"]));
    }

    #[test]
    fn commands_are_case_insensitive_but_flags_are_case_sensitive() {
        assert!(matches_prefix(&vec![single("Git")], &["git"]));
        let flag = vec![single("git"), single("branch"), single("-D")];
        assert!(matches_prefix(&flag, &["git", "branch", "-D"]));
        assert!(!matches_prefix(&flag, &["git", "branch", "-d"]));
    }

    #[test]
    fn any_matches_exactly_one_token() {
        let pattern = vec![single("git"), PatternToken::Any, single("status")];
        assert!(matches_prefix(&pattern, &["git", "log", "status"]));
        assert!(!matches_prefix(&pattern, &["git", "status"]));
        assert!(!matches_prefix(&pattern, &["git", "a", "b", "status"]));
    }

    #[test]
    fn any_star_matches_zero_or_more_tokens() {
        let pattern = vec![single("git"), PatternToken::AnyStar, single("status")];
        assert!(matches_prefix(&pattern, &["git", "status"]));
        assert!(matches_prefix(&pattern, &["git", "log", "status"]));
        assert!(matches_prefix(&pattern, &["git", "a", "b", "c", "status"]));
        assert!(!matches_prefix(&pattern, &["git", "log"]));
    }

    #[test]
    fn short_flag_matches_alone() {
        let pattern = vec![single("chmod"), short_flag('R', &["--recursive"])];
        assert!(matches_prefix(&pattern, &["chmod", "-R"]));
    }

    #[test]
    fn short_flag_matches_inside_cluster() {
        let pattern = vec![single("chmod"), short_flag('R', &["--recursive"])];
        assert!(matches_prefix(&pattern, &["chmod", "-Rf"]));
        assert!(matches_prefix(&pattern, &["chmod", "-fR"]));
        assert!(matches_prefix(&pattern, &["chmod", "-fRv"]));
    }

    #[test]
    fn short_flag_matches_declared_long_form() {
        let pattern = vec![single("chmod"), short_flag('R', &["--recursive"])];
        assert!(matches_prefix(&pattern, &["chmod", "--recursive"]));
    }

    #[test]
    fn short_flag_is_case_sensitive() {
        let pattern = vec![single("chmod"), short_flag('R', &["--recursive"])];
        // `-r` is a mode expression (read), not the recursion flag.
        assert!(!matches_prefix(&pattern, &["chmod", "-r"]));
    }

    #[test]
    fn short_flag_rejects_non_synonym_long_flag() {
        let pattern = vec![single("chmod"), short_flag('R', &["--recursive"])];
        assert!(!matches_prefix(&pattern, &["chmod", "--recursive-ish"]));
    }

    #[test]
    fn short_flag_rejects_bare_non_flag_token() {
        let pattern = vec![single("chmod"), short_flag('R', &["--recursive"])];
        assert!(!matches_prefix(&pattern, &["chmod", "file"]));
    }

    // ── contains_any_token ────────────────────────────────────────────────

    #[test]
    fn contains_any_token_finds_candidate_at_any_position() {
        let candidates = ["--dry-run"];
        assert!(contains_any_token(
            &["npm", "publish", "--dry-run"],
            &candidates
        ));
        assert!(contains_any_token(
            &["npm", "--dry-run", "publish"],
            &candidates
        ));
        assert!(contains_any_token(
            &["npm", "publish", "--dry-run", "--access", "public"],
            &candidates
        ));
    }

    #[test]
    fn contains_any_token_is_false_without_a_candidate() {
        assert!(!contains_any_token(&["npm", "publish"], &["--dry-run"]));
    }

    #[test]
    fn contains_any_token_with_no_candidates_is_false() {
        assert!(!contains_any_token(&["npm", "publish", "--dry-run"], &[]));
    }

    #[test]
    fn contains_any_token_matches_any_of_several_candidates() {
        let candidates = ["--dry-run", "-n"];
        assert!(contains_any_token(&["npm", "publish", "-n"], &candidates));
    }

    #[test]
    fn contains_any_token_compares_flags_case_sensitively() {
        assert!(!contains_any_token(&["npm", "--DRY-RUN"], &["--dry-run"]));
    }

    #[test]
    fn contains_any_token_compares_non_flags_case_insensitively() {
        assert!(contains_any_token(&["psql", "READONLY"], &["readonly"]));
    }

    #[test]
    fn contains_any_token_requires_a_whole_token() {
        // A candidate embedded in a longer token is not a match: suppression is
        // token equality, not substring containment.
        assert!(!contains_any_token(
            &["npm", "publish", "--dry-run-please"],
            &["--dry-run"]
        ));
    }
}
