//! Drop token-prefix matches that only exist because a normalized scan target
//! lost its quote boundaries (#484).
//!
//! A logical segment reaches the scanner as its words joined by spaces, so
//! re-tokenizing `git push origin 'x --force'` yields a standalone `--force`
//! that git never receives. That same re-split is also how a command string
//! handed to a runner (`watch 'git clean -fdx'`) reaches the prefix rules, so
//! the lossy tokens stay the source of matches. This module only removes the
//! matches that the segment's quote-preserving tokens contradict.

use aegis_types::{MatchEvidence, MatchResult};

use super::Scanner;
use super::assessment::REGEX_SUPERSEDED_PREFIXES;

impl Scanner {
    /// Remove token-prefix matches in `matched[from..]` that the
    /// quote-preserving `quoted` tokens of the same segment do not support.
    ///
    /// A match stays when the same rule also matches `quoted`, or when a
    /// quoted token that re-tokenizes into several words names the rule's
    /// program: that token may be a command string some runner executes.
    pub(super) fn drop_quote_split_matches(
        &self,
        matched: &mut Vec<MatchResult>,
        from: usize,
        quoted: &[String],
    ) {
        let multi_word_tokens: Vec<String> = quoted
            .iter()
            .filter(|token| aegis_parser::split_tokens(token) != [token.as_str()])
            .map(|token| token.to_ascii_lowercase())
            .collect();
        let quoted_ids = self.prefix_ids(quoted);

        let mut later = matched.split_off(from);
        later.retain(|result| {
            if !matches!(result.evidence, MatchEvidence::TokenPrefixRule { .. }) {
                return true;
            }
            let id = result.pattern.id.as_ref();
            quoted_ids.iter().any(|quoted_id| quoted_id == id)
                || self.prefix_programs(id).any(|program| {
                    multi_word_tokens
                        .iter()
                        .any(|token| token.contains(program))
                })
        });
        matched.append(&mut later);
    }

    /// Ids of every token-prefix rule that matches `tokens`, through the same
    /// launcher stripping and git/aegis global-option skipping as `assess`.
    fn prefix_ids(&self, tokens: &[String]) -> Vec<String> {
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let slices = aegis_parser::effective_token_slices(&refs);
        let mut found = self.prefix_scan_effective_slices(&slices);
        let mut regex_matched = [false; REGEX_SUPERSEDED_PREFIXES.len()];
        self.scan_git_option_candidates(&slices, &mut found, &mut regex_matched);
        self.scan_aegis_option_candidates(&slices, &mut found);
        found
            .into_iter()
            .filter(|result| matches!(result.evidence, MatchEvidence::TokenPrefixRule { .. }))
            .map(|result| result.pattern.id.to_string())
            .collect()
    }

    /// The lowercase programs a token-prefix rule is indexed under.
    fn prefix_programs<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a str> {
        self.prefix_by_program
            .iter()
            .filter(move |(_, rules)| rules.iter().any(|rule| rule.id.as_ref() == id))
            .map(|(program, _)| program.as_str())
    }
}
