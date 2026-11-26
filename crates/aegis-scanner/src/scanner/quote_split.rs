use aegis_parser::EndOfOptions;
use aegis_types::{MatchEvidence, MatchResult};

use super::Scanner;
use super::assessment::REGEX_SUPERSEDED_PREFIXES;

impl Scanner {
    pub(super) fn drop_quote_split_matches(
        &self,
        matched: &mut Vec<MatchResult>,
        from: usize,
        quoted: &[String],
    ) {
        if matched.len() <= from {
            return;
        }
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

    fn prefix_ids(&self, tokens: &[String]) -> Vec<String> {
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let slices = aegis_parser::effective_token_slices(&refs);
        let mut found = self.prefix_scan_effective_slices(&slices, EndOfOptions::Honor);
        let mut regex_matched = [false; REGEX_SUPERSEDED_PREFIXES.len()];
        self.scan_git_option_candidates(
            &slices,
            &mut found,
            &mut regex_matched,
            EndOfOptions::Honor,
        );
        self.scan_aegis_option_candidates(&slices, &mut found, EndOfOptions::Honor);
        found
            .into_iter()
            .filter(|result| matches!(result.evidence, MatchEvidence::TokenPrefixRule { .. }))
            .map(|result| result.pattern.id.to_string())
            .collect()
    }

    fn prefix_programs<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a str> {
        self.prefix_by_program
            .iter()
            .filter(move |(_, rules)| rules.iter().any(|rule| rule.id.as_ref() == id))
            .map(|(program, _)| program.as_str())
    }
}
