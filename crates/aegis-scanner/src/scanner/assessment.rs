use std::sync::Arc;

use crate::nested::RecursiveScanLimit;
use aegis_types::ParsedCommand;
use aegis_types::{
    Assessment, Category, DetectionSource, MatchEvidence, MatchResult, Pattern, PatternSource,
    RiskLevel,
};

use super::{Scanner, highlighting, pipeline_semantics, recursive};

/// (regex id, prefix id) pairs where the regex already reports everything its
/// paired token-prefix rule would add for the same target, so the prefix rule
/// steps aside once the regex has matched (GHSA-7gcj-4f7x-7fxj / #415).
const REGEX_SUPERSEDED_PREFIXES: &[(&str, &str)] = &[("FS-001", "FS-020"), ("PS-006", "PS-008")];

/// Synthetic pattern id for the GHSA-7564 candidate-cap Warn, alongside
/// `SCAN-001`..`SCAN-003` below.
const GIT_OPTION_CAP_EXCEEDED_ID: &str = "SCAN-004";

/// Records that regex `id` matched the current target, for a later
/// [`prefix_id_superseded`] check.
fn note_regex_match(seen: &mut [bool; REGEX_SUPERSEDED_PREFIXES.len()], id: &str) {
    for (slot, (regex_id, _)) in seen.iter_mut().zip(REGEX_SUPERSEDED_PREFIXES) {
        if *regex_id == id {
            *slot = true;
        }
    }
}

/// Whether prefix rule `id` should step aside because its paired regex (see
/// [`REGEX_SUPERSEDED_PREFIXES`]) already matched the current target.
fn prefix_id_superseded(seen: &[bool; REGEX_SUPERSEDED_PREFIXES.len()], id: &str) -> bool {
    seen.iter()
        .zip(REGEX_SUPERSEDED_PREFIXES)
        .any(|(&matched, (_, prefix_id))| matched && *prefix_id == id)
}

impl Scanner {
    /// Assess a raw shell command and return a complete [`Assessment`].
    ///
    /// Pipeline:
    /// 1. Parse the command via [`aegis_parser::Parser::parse`] to preserve the original command contract.
    /// 2. Run [`Scanner::quick_scan`] on the raw command. If no keyword hits, return `Safe` immediately.
    /// 3. Build the recursive scan path via nested parsing helpers.
    /// 4. Run [`Scanner::full_scan`] on each discovered target and merge unique pattern matches.
    ///    Within the same target, a git slice with global options before its subcommand is
    ///    rescanned from each possible subcommand position (GHSA-7564).
    /// 5. Compute the maximum [`RiskLevel`] across all matched patterns and return.
    pub fn assess(&self, cmd: &str) -> Assessment {
        if cmd.len() > super::MAX_SCAN_COMMAND_LEN {
            return uncertain_assessment_without_parse(
                cmd,
                "SCAN-001",
                format!(
                    "scan input exceeded command length limit ({})",
                    super::MAX_SCAN_COMMAND_LEN
                ),
                Some(
                    "Review the command out-of-band or move the payload into a smaller, reviewed script file",
                ),
            );
        }

        let command = aegis_parser::Parser::parse(cmd);

        if let Some(script) = command
            .inline_scripts
            .iter()
            .find(|script| script.body.len() > super::MAX_INLINE_SCRIPT_LEN)
        {
            let interpreter = script.interpreter.clone();
            return uncertain_assessment(
                command,
                "SCAN-002",
                format!(
                    "scan input exceeded inline script length limit ({}) for {}",
                    super::MAX_INLINE_SCRIPT_LEN,
                    interpreter
                ),
                Some(
                    "Review the generated script separately or store it in a checked file before execution",
                ),
                // An inline-script invocation (`-c` / `-e`) is not effect-opaque:
                // its body is what exceeded the limit, and inline bodies are
                // extracted and scanned, not deferred to a file.
                false,
            );
        }

        // Pipeline detection needs the raw string (quoting-aware).
        let maybe_pipelines = cmd
            .contains('|')
            .then(|| aegis_parser::top_level_pipelines(cmd));
        let has_pipeline_chain = maybe_pipelines
            .as_ref()
            .map(|pipelines| pipelines.iter().any(|chain| chain.segments.len() > 1))
            .unwrap_or(false);

        // Effect-opacity is orthogonal to the quick-scan gate: a script-file
        // execution (`sh ./cleanup.sh`) is `Safe` to the quick scan yet still
        // effect-opaque, so it must be computed before the early return below.
        let effect_opaque = super::effect_opaque::detect(cmd, &command, maybe_pipelines.as_deref());

        // Use the normalized form as the primary scan target. It is free of quoting noise.
        if !self.quick_scan(&command.normalized) && !has_pipeline_chain {
            return Assessment {
                risk: RiskLevel::Safe,
                effect_opaque,
                matched: vec![],
                highlight_ranges: vec![],
                command,
                analysis: None,
            };
        }

        let mut matched = Vec::new();

        let target_report = recursive::scan_targets(cmd, &command);
        if let Some(limit_hit) = target_report.limit_hit {
            return uncertain_assessment(
                command,
                "SCAN-003",
                recursive_limit_description(limit_hit),
                Some(
                    "Reduce shell nesting depth or rewrite the command into a reviewed intermediate script",
                ),
                effect_opaque,
            );
        }

        for target in &target_report.targets {
            // Token-prefix scan: parsed tokens, not raw string. Reuse the same
            // token refs to derive the effective program key for indexed regexes.
            let tokens = aegis_parser::split_tokens(target);
            let token_refs: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();

            // Derive the program from the target's first token (lowercase) so
            // full_scan can use the by-program index on the fast path.
            let prog = aegis_parser::effective_program(&token_refs).map(str::to_ascii_lowercase);
            // Tracks which of REGEX_SUPERSEDED_PREFIXES' regex ids fired for
            // this target, so the paired prefix rule below can step aside.
            let mut regex_matched_for_target = [false; REGEX_SUPERSEDED_PREFIXES.len()];
            for pattern in self.full_scan(target, prog.as_deref()) {
                note_regex_match(&mut regex_matched_for_target, pattern.pattern.id.as_ref());
                if !matched
                    .iter()
                    .any(|existing: &MatchResult| existing.pattern.id == pattern.pattern.id)
                {
                    matched.push(pattern);
                }
            }

            let effective_slices = aegis_parser::effective_token_slices(&token_refs);
            for candidate in &effective_slices {
                let effective_target = candidate.tokens.join(" ");
                if effective_target == *target {
                    continue;
                }
                for pattern in self.full_scan(&effective_target, Some(candidate.program)) {
                    note_regex_match(&mut regex_matched_for_target, pattern.pattern.id.as_ref());
                    if !matched
                        .iter()
                        .any(|existing: &MatchResult| existing.pattern.id == pattern.pattern.id)
                    {
                        matched.push(pattern);
                    }
                }
            }

            for result in self.prefix_scan_effective_slices(&effective_slices) {
                if prefix_id_superseded(&regex_matched_for_target, result.pattern.id.as_ref()) {
                    continue;
                }
                if !matched
                    .iter()
                    .any(|existing: &MatchResult| existing.pattern.id == result.pattern.id)
                {
                    matched.push(result);
                }
            }

            self.scan_git_option_candidates(
                &effective_slices,
                &mut matched,
                &mut regex_matched_for_target,
            );
        }

        if let Some(pipelines) = maybe_pipelines {
            for evidence in pipeline_semantics::semantic_pipeline_matches(&pipelines) {
                if !matched
                    .iter()
                    .any(|existing: &MatchResult| existing.pattern.id == evidence.pattern.id)
                {
                    matched.push(evidence);
                }
            }
        }

        let risk = matched
            .iter()
            .map(|p| p.pattern.risk)
            .max()
            .unwrap_or(RiskLevel::Safe);
        let highlight_ranges = highlighting::sorted_highlight_ranges(cmd, &matched);

        Assessment {
            risk,
            effect_opaque,
            matched,
            highlight_ranges,
            command,
            analysis: None,
        }
    }

    /// Rescan each git candidate slice for a subcommand hidden behind a
    /// global option (GHSA-7564): the option shifts the subcommand off
    /// position 1, so no `GIT-*` rule above ever sees it. Resolves where the
    /// subcommand actually starts and re-runs both scan mechanisms there,
    /// pushing matches (and the `SCAN-004` cap warning) into `matched`.
    fn scan_git_option_candidates(
        &self,
        effective_slices: &[aegis_parser::EffectiveTokenSlice<'_>],
        matched: &mut Vec<MatchResult>,
        regex_matched_for_target: &mut [bool; REGEX_SUPERSEDED_PREFIXES.len()],
    ) {
        for candidate in effective_slices {
            if !candidate.program.eq_ignore_ascii_case("git") {
                continue;
            }

            let starts = match aegis_parser::git_option_subcommand_starts(&candidate.tokens) {
                aegis_parser::GitSubcommandStarts::Starts(starts) => starts,
                aegis_parser::GitSubcommandStarts::TooMany => {
                    // Too many unrecognized options to scan without the
                    // quadratic cost the cap exists to avoid. Fail closed
                    // with a Warn instead of scanning none of them.
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
                // The candidate string is synthetic, so a byte offset a
                // regex found inside it does not point at the raw
                // command. Report the subcommand-onward span instead: it
                // is always a real substring of the raw command, or
                // `sorted_highlight_ranges` skips the highlight rather
                // than guessing.
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
                        if rule.matches_tokens(&git_tokens) {
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

fn uncertain_assessment_without_parse(
    cmd: &str,
    id: &'static str,
    description: String,
    safe_alt: Option<&'static str>,
) -> Assessment {
    uncertain_assessment(
        ParsedCommand {
            program: None,
            argv: Vec::new(),
            normalized: cmd.to_string(),
            inline_scripts: Vec::new(),
            raw: cmd.to_string(),
        },
        id,
        description,
        safe_alt,
        // No parsed command to classify, so no effect-opacity claim.
        false,
    )
}

fn uncertain_assessment(
    command: ParsedCommand,
    id: &'static str,
    description: String,
    safe_alt: Option<&'static str>,
    effect_opaque: bool,
) -> Assessment {
    let matched = vec![uncertain_match(id, description, safe_alt)];
    let highlight_ranges = highlighting::sorted_highlight_ranges(&command.raw, &matched);

    Assessment {
        risk: RiskLevel::Warn,
        effect_opaque,
        matched,
        highlight_ranges,
        command,
        analysis: None,
    }
}

fn uncertain_match(
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
        // The synthetic scan-limit marker is `Pattern`-family (regex-shaped),
        // not a `Token-prefix rule` or Language-aware rule, so it maps to the
        // `RegexPattern` mechanism with a built-in source.
        evidence: MatchEvidence::RegexPattern {
            source: DetectionSource::Builtin,
        },
    }
}

fn recursive_limit_description(limit: RecursiveScanLimit) -> String {
    match limit {
        RecursiveScanLimit::DepthExceeded { limit } => {
            format!("scan input exceeded recursive parsing depth limit ({limit})")
        }
    }
}

#[cfg(test)]
pub(super) fn assess_for_tests(scanner: &Scanner, cmd: &str) -> Assessment {
    scanner.assess(cmd)
}

#[cfg(test)]
mod tests {
    #[test]
    fn uncertain_match_produces_pattern_with_no_justification() {
        let result = super::uncertain_match("SCAN-001", "desc".to_string(), None);
        assert!(result.pattern.justification.is_none());
    }
}
