mod ambient_env;
mod assessment;
mod cd_walk;
mod effect_opaque;
mod git_stages;
mod highlighting;
mod keywords;
mod pipeline_semantics;
mod prefix_scan;
mod quote_split;
mod recursive;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aho_corasick::AhoCorasick;
use regex::{Regex, RegexBuilder};

use crate::error::ScannerError;
#[cfg(test)]
use crate::nested::MAX_NESTED_SCAN_DEPTH;
use crate::patterns::PatternSet;
use aegis_types::{
    DetectionSource, HighlightRange, MatchEvidence, MatchResult, Pattern, PatternSource,
    PatternToken,
};

pub use crate::patterns::PrefixRule;
#[cfg(test)]
pub(crate) use ambient_env::AmbientEnv;
pub use cd_walk::{CdUnresolved, StageDir, path_is_dynamic};
pub use git_stages::{DangerStage, DangerStageTarget, GitStageReport};

const MAX_SCAN_COMMAND_LEN: usize = 64 * 1024;
const MAX_INLINE_SCRIPT_LEN: usize = 16 * 1024;

pub struct Scanner {
    ac: AhoCorasick,
    has_uncovered: bool,
    patterns: Vec<Arc<Pattern>>,
    regexes: Vec<Regex>,
    by_program: HashMap<String, Vec<usize>>,
    universal: Vec<usize>,
    prefix_by_program: Vec<(String, Vec<Arc<PrefixRule>>)>,
}

impl Scanner {
    fn compile_regex(pattern: &Pattern) -> Result<Regex, ScannerError> {
        let invalid = |reason: String| ScannerError::InvalidPattern {
            id: pattern.id.to_string(),
            reason,
        };
        if pattern.source == PatternSource::Builtin {
            return RegexBuilder::new(pattern.pattern.as_ref())
                .case_insensitive(true)
                .build()
                .map_err(|e| invalid(format!("invalid regex: {e}")));
        }
        crate::untrusted::check_pattern_length(pattern.pattern.as_ref()).map_err(&invalid)?;
        crate::untrusted::compile_untrusted_regex(pattern.pattern.as_ref())
            .map_err(|e| invalid(format!("invalid regex: {e}")))
    }

    pub fn try_new(patterns: PatternSet) -> Result<Self, ScannerError> {
        TRY_NEW_CALLS.fetch_add(1, Ordering::Relaxed);
        let effective_patterns = patterns.patterns();

        let mut keywords: Vec<String> = Vec::new();
        let mut has_uncovered = false;
        let mut by_program: HashMap<String, Vec<usize>> = HashMap::new();
        let mut universal: Vec<usize> = Vec::new();
        let mut flat_patterns: Vec<Arc<Pattern>> = Vec::with_capacity(effective_patterns.len());
        let mut regexes: Vec<Regex> = Vec::with_capacity(effective_patterns.len());

        for p in effective_patterns {
            let idx = flat_patterns.len();
            flat_patterns.push(Arc::clone(p));

            let rx = Self::compile_regex(p)?;
            regexes.push(rx);

            let kws = keywords::extract_keywords(&p.pattern);
            if kws.is_empty() {
                has_uncovered = true;
            } else {
                keywords.extend(kws);
            }

            let prog_keys = keywords::derive_program_keys(&p.pattern);
            if prog_keys.is_empty() {
                universal.push(idx);
            } else {
                for key in &prog_keys {
                    by_program.entry(key.clone()).or_default().push(idx);
                }
            }
        }

        for rule in patterns.prefix_rules() {
            match rule.pattern.first() {
                Some(PatternToken::Single(s)) => keywords.push(s.as_ref().to_string()),
                Some(PatternToken::Alts(alts)) => {
                    for s in alts {
                        keywords.push(s.as_ref().to_string());
                    }
                }
                Some(PatternToken::Any)
                | Some(PatternToken::AnyStar)
                | Some(PatternToken::ShortFlag { .. })
                | None => continue,
            }
        }

        keywords.sort_unstable();
        keywords.dedup();

        #[expect(
            clippy::expect_used,
            reason = "keywords come from compiled-in patterns, so a build failure is a startup bug"
        )]
        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(&keywords)
            .expect("all extracted keywords are valid byte strings");

        let mut prefix_by_program: Vec<(String, Vec<Arc<PrefixRule>>)> = Vec::new();
        for rule in patterns.prefix_rules() {
            let keys: Vec<&str> = match rule.pattern.first() {
                Some(PatternToken::Single(first)) => vec![&**first],
                Some(PatternToken::Alts(alts)) => alts.iter().map(|s| &**s).collect(),
                Some(PatternToken::Any)
                | Some(PatternToken::AnyStar)
                | Some(PatternToken::ShortFlag { .. })
                | None => continue,
            };
            for key in keys {
                let lower = key.to_ascii_lowercase();
                if let Some((_, rules)) = prefix_by_program.iter_mut().find(|(k, _)| k == &lower) {
                    rules.push(Arc::clone(rule));
                } else {
                    prefix_by_program.push((lower, vec![Arc::clone(rule)]));
                }
            }
        }

        Ok(Scanner {
            ac,
            has_uncovered,
            patterns: flat_patterns,
            regexes,
            by_program,
            universal,
            prefix_by_program,
        })
    }

    pub fn quick_scan(&self, cmd: &str) -> bool {
        if self.has_uncovered || self.ac.is_match(cmd) {
            return true;
        }
        if let Some(prog) = cmd.split_whitespace().next()
            && self.prefix_lookup(prog).is_some()
        {
            return true;
        }
        false
    }

    pub fn full_scan(&self, cmd: &str, program: Option<&str>) -> Vec<MatchResult> {
        let program_patterns: &[usize] = program
            .and_then(|prog| self.by_program.get(&prog.to_ascii_lowercase()))
            .map(Vec::as_slice)
            .unwrap_or_default();

        self.universal
            .iter()
            .chain(program_patterns.iter())
            .filter_map(|&idx| self.match_one(cmd, idx))
            .collect()
    }

    fn match_one(&self, cmd: &str, idx: usize) -> Option<MatchResult> {
        let pattern = &self.patterns[idx];
        self.regexes[idx].find(cmd).map(|m| MatchResult {
            pattern: Arc::clone(pattern),
            matched_text: m.as_str().to_string(),
            highlight_range: Some(HighlightRange {
                start: m.start(),
                end: m.end(),
            }),
            evidence: MatchEvidence::RegexPattern {
                source: DetectionSource::from(pattern.source),
            },
        })
    }

    #[cfg(test)]
    pub fn prefix_scan(&self, tokens: &[&str]) -> Vec<MatchResult> {
        if tokens.is_empty() {
            return vec![];
        }
        let candidates = aegis_parser::effective_token_slices(tokens);
        self.prefix_scan_effective_slices(&candidates, aegis_parser::EndOfOptions::Ignore)
    }

    fn prefix_scan_effective_slices(
        &self,
        candidates: &[aegis_parser::EffectiveTokenSlice<'_>],
        mode: aegis_parser::EndOfOptions,
    ) -> Vec<MatchResult> {
        candidates
            .iter()
            .flat_map(|candidate| {
                self.prefix_lookup(candidate.program)
                    .map_or(&[] as &[Arc<PrefixRule>], Vec::as_slice)
                    .iter()
                    .filter_map(move |rule| {
                        if rule.matches_tokens_with(&candidate.tokens, mode) {
                            Some(rule.to_match_result(&candidate.tokens))
                        } else {
                            None
                        }
                    })
            })
            .collect()
    }

    fn prefix_lookup(&self, prog: &str) -> Option<&Vec<Arc<PrefixRule>>> {
        self.prefix_by_program
            .iter()
            .find(|(key, _)| str_eq_ignore_ascii_case(key, prog))
            .map(|(_, rules)| rules)
    }

    #[cfg(test)]
    pub(crate) fn indexed_program_count(&self, program: &str) -> usize {
        self.by_program.get(program).map_or(0, Vec::len)
    }

    #[cfg(test)]
    pub(crate) fn prefix_indexed_program_count(&self, program: &str) -> usize {
        self.prefix_lookup(program).map_or(0, Vec::len)
    }
}

static TRY_NEW_CALLS: AtomicUsize = AtomicUsize::new(0);

#[doc(hidden)]
pub fn try_new_call_count_for_tests() -> usize {
    TRY_NEW_CALLS.load(Ordering::Relaxed)
}

fn str_eq_ignore_ascii_case(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .all(|(a, b)| a.eq_ignore_ascii_case(&b))
}

#[cfg(test)]
mod tests;
