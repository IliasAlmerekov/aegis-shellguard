use std::sync::Arc;

use aegis_parser::EndOfOptions;
use aegis_types::{
    Category, DetectionSource, MatchEvidence, MatchResult, Pattern, PatternSource, RiskLevel,
};

use super::Scanner;

pub(super) const REGEX_SUPERSEDED_PREFIXES: &[(&str, &str)] =
    &[("FS-001", "FS-020"), ("PS-006", "PS-008")];

const GIT_OPTION_CAP_EXCEEDED_ID: &str = "SCAN-004";

pub(super) fn note_regex_match(seen: &mut [bool; REGEX_SUPERSEDED_PREFIXES.len()], id: &str) {
    for (slot, (regex_id, _)) in seen.iter_mut().zip(REGEX_SUPERSEDED_PREFIXES) {
        if *regex_id == id {
            *slot = true;
        }
    }
}

pub(super) fn prefix_id_superseded(
    seen: &[bool; REGEX_SUPERSEDED_PREFIXES.len()],
    id: &str,
) -> bool {
    seen.iter()
        .zip(REGEX_SUPERSEDED_PREFIXES)
        .any(|(&matched, (_, prefix_id))| matched && *prefix_id == id)
}

impl Scanner {
    pub(super) fn scan_aegis_option_candidates(
        &self,
        effective_slices: &[aegis_parser::EffectiveTokenSlice<'_>],
        matched: &mut Vec<MatchResult>,
        mode: EndOfOptions,
    ) {
        let Some(rules) = self.prefix_lookup("aegis") else {
            return;
        };
        for candidate in effective_slices {
            if !candidate.program.eq_ignore_ascii_case("aegis") {
                continue;
            }
            let Some(start) = aegis_parser::aegis_option_subcommand_start(&candidate.tokens) else {
                continue;
            };
            let aegis_tokens: Vec<&str> = std::iter::once(candidate.tokens[0])
                .chain(candidate.tokens[start..].iter().copied())
                .collect();
            let tail = candidate.tokens[start..].join(" ");
            for rule in rules {
                if matched
                    .iter()
                    .any(|existing: &MatchResult| existing.pattern.id.as_ref() == rule.id.as_ref())
                {
                    continue;
                }
                if rule.matches_tokens_with(&aegis_tokens, mode) {
                    let mut result = rule.to_match_result(&aegis_tokens);
                    result.matched_text = tail.clone();
                    matched.push(result);
                }
            }
        }
    }

    pub(super) fn scan_git_option_candidates(
        &self,
        effective_slices: &[aegis_parser::EffectiveTokenSlice<'_>],
        matched: &mut Vec<MatchResult>,
        regex_matched_for_target: &mut [bool; REGEX_SUPERSEDED_PREFIXES.len()],
        mode: EndOfOptions,
    ) {
        for candidate in effective_slices {
            if !candidate.program.eq_ignore_ascii_case("git") {
                continue;
            }

            let starts = match aegis_parser::git_option_subcommand_starts(&candidate.tokens) {
                aegis_parser::GitSubcommandStarts::Starts(starts) => starts,
                aegis_parser::GitSubcommandStarts::TooMany => {
                    if !matched.iter().any(|existing: &MatchResult| {
                        existing.pattern.id.as_ref() == GIT_OPTION_CAP_EXCEEDED_ID
                    }) {
                        matched.push(uncertain_match(
                            GIT_OPTION_CAP_EXCEEDED_ID,
                            format!(
                                "git command has more than {} unrecognized global-option candidates before its subcommand",
                                aegis_parser::MAX_GIT_OPTION_CANDIDATES
                            ),
                            Some(
                                "Rewrite the command with fewer or recognized git global options before the subcommand",
                            ),
                        ));
                    }
                    continue;
                }
            };

            for start in starts {
                let git_tokens: Vec<&str> = std::iter::once(candidate.tokens[0])
                    .chain(candidate.tokens[start..].iter().copied())
                    .collect();
                let tail = candidate.tokens[start..].join(" ");
                let joined = git_tokens.join(" ");

                for mut pattern in self.full_scan(&joined, Some("git")) {
                    note_regex_match(regex_matched_for_target, pattern.pattern.id.as_ref());
                    if matched
                        .iter()
                        .any(|existing: &MatchResult| existing.pattern.id == pattern.pattern.id)
                    {
                        continue;
                    }
                    pattern.highlight_range = None;
                    pattern.matched_text = tail.clone();
                    matched.push(pattern);
                }

                if let Some(rules) = self.prefix_lookup("git") {
                    for rule in rules {
                        if prefix_id_superseded(regex_matched_for_target, rule.id.as_ref())
                            || matched.iter().any(|existing: &MatchResult| {
                                existing.pattern.id.as_ref() == rule.id.as_ref()
                            })
                        {
                            continue;
                        }
                        if rule.matches_tokens_with(&git_tokens, mode) {
                            let mut result = rule.to_match_result(&git_tokens);
                            result.matched_text = tail.clone();
                            matched.push(result);
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn uncertain_match(
    id: &'static str,
    description: String,
    safe_alt: Option<&'static str>,
) -> MatchResult {
    MatchResult {
        pattern: Arc::new(Pattern {
            id: id.into(),
            category: Category::Process,
            risk: RiskLevel::Warn,
            pattern: id.into(),
            description: description.into(),
            safe_alt: safe_alt.map(Into::into),
            justification: None,
            source: PatternSource::Builtin,
        }),
        matched_text: String::new(),
        highlight_range: None,
        evidence: MatchEvidence::RegexPattern {
            source: DetectionSource::Builtin,
        },
    }
}
