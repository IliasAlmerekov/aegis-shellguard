use crate::nested::RecursiveScanLimit;
use aegis_parser::EndOfOptions;
use aegis_types::ParsedCommand;
use aegis_types::{Assessment, MatchResult, RiskLevel};

use super::prefix_scan::{
    REGEX_SUPERSEDED_PREFIXES, note_regex_match, prefix_id_superseded, uncertain_match,
};
use super::{Scanner, highlighting, pipeline_semantics, recursive};

const INEXACT_ANSI_C_ID: &str = "SCAN-005";

impl Scanner {
    pub fn assess(&self, cmd: &str) -> Assessment {
        let canonical = aegis_parser::canonical_ansi_c_quoting(cmd);
        let mut assessment = self.assess_canonical(&canonical);
        if let std::borrow::Cow::Owned(_) = canonical {
            for result in &mut assessment.matched {
                result.highlight_range = None;
            }
            assessment.highlight_ranges =
                highlighting::sorted_highlight_ranges(cmd, &assessment.matched);
            assessment.command.raw = cmd.to_string();
        }
        if aegis_parser::ansi_c_quoting_is_inexact(&canonical) {
            assessment.effect_opaque = true;
            assessment.risk = assessment.risk.max(RiskLevel::Warn);
            assessment.matched.push(uncertain_match(
                INEXACT_ANSI_C_ID,
                "ANSI-C quoted word ($'...') does not close or does not decode to valid text"
                    .to_string(),
                Some("Spell the word without ANSI-C escapes"),
            ));
        }
        assessment
    }

    fn assess_canonical(&self, cmd: &str) -> Assessment {
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
                false,
            );
        }

        let maybe_pipelines = cmd
            .contains('|')
            .then(|| aegis_parser::top_level_pipelines(cmd));
        let has_pipeline_chain = maybe_pipelines
            .as_ref()
            .map(|pipelines| pipelines.iter().any(|chain| chain.segments.len() > 1))
            .unwrap_or(false);

        let effect_opaque = super::effect_opaque::detect(cmd, &command, maybe_pipelines.as_deref());

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
            let tokens = aegis_parser::split_tokens(target);
            let token_refs: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();

            let prog = aegis_parser::effective_program(&token_refs).map(str::to_ascii_lowercase);
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

            if target == cmd && aegis_parser::has_multiple_top_level_segments(cmd) {
                continue;
            }

            let prefix_start = matched.len();
            for result in self.prefix_scan_effective_slices(&effective_slices, EndOfOptions::Ignore)
            {
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
                EndOfOptions::Ignore,
            );
            self.scan_aegis_option_candidates(
                &effective_slices,
                &mut matched,
                EndOfOptions::Ignore,
            );

            if let Some(quoted) = target_report.quoted_tokens(target) {
                self.drop_quote_split_matches(&mut matched, prefix_start, quoted);
            }
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
