use std::collections::{HashSet, VecDeque};

use aegis_parser::{
    Parser, ScanSegment, extract_eval_payloads, extract_heredoc_bodies,
    extract_process_substitution_bodies, logical_segments, mask_inert_heredoc_substitution_markers,
};

pub(crate) const MAX_NESTED_SCAN_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursiveScanLimit {
    DepthExceeded { limit: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecursiveScanReport {
    pub targets: Vec<String>,
    pub limit_hit: Option<RecursiveScanLimit>,
    pub segments: Vec<ScanSegment>,
}

impl RecursiveScanReport {
    pub fn quoted_tokens(&self, target: &str) -> Option<&[String]> {
        let mut found = self
            .segments
            .iter()
            .filter(|segment| segment.normalized == target)
            .map(|segment| segment.tokens.as_slice());
        let first = found.next()?;
        found.all(|tokens| tokens == first).then_some(first)
    }
}

pub fn recursive_scan_targets(cmd: &str) -> RecursiveScanReport {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([(cmd.trim().to_string(), 0usize)]);
    let mut limit_hit = None;

    while let Some((candidate, depth)) = queue.pop_front() {
        if candidate.is_empty() || !seen.insert(candidate.clone()) {
            continue;
        }

        targets.push(mask_inert_heredoc_substitution_markers(&candidate));

        if depth >= MAX_NESTED_SCAN_DEPTH {
            limit_hit.get_or_insert(RecursiveScanLimit::DepthExceeded {
                limit: MAX_NESTED_SCAN_DEPTH,
            });
            continue;
        }

        for target in expand_nested_targets(&candidate) {
            let trimmed = target.trim();
            if !trimmed.is_empty() && !seen.contains(trimmed) {
                queue.push_back((trimmed.to_string(), depth + 1));
            }
        }
    }

    RecursiveScanReport {
        targets,
        limit_hit,
        segments: Vec::new(),
    }
}

fn expand_nested_targets(cmd: &str) -> Vec<String> {
    let sanitized = mask_inert_heredoc_substitution_markers(cmd);
    let mut targets = logical_segments(&sanitized);
    let parsed = Parser::parse(cmd);

    for script in parsed.inline_scripts {
        targets.push(script.body);
    }

    for heredoc in extract_heredoc_bodies(cmd) {
        if heredoc.is_nowdoc && !heredoc.target_is_interpreter {
            continue;
        }
        targets.push(heredoc.body);
    }

    for body in extract_process_substitution_bodies(cmd) {
        targets.push(body);
    }

    for payload in extract_eval_payloads(cmd) {
        targets.push(payload);
    }

    targets
}

#[cfg(test)]
mod tests {
    use super::{MAX_NESTED_SCAN_DEPTH, RecursiveScanLimit, recursive_scan_targets};

    #[test]
    fn recursive_targets_include_inline_script_body_from_nested_shell() {
        let targets = recursive_scan_targets(r#"bash -c 'python3 -c "print(42)"'"#);
        assert!(targets.targets.iter().any(|target| target == "print(42)"));
    }

    #[test]
    fn recursive_targets_include_heredoc_body_and_nested_inline_script() {
        let cmd = "bash <<'EOF'\npython3 -c \"print(42)\"\nEOF";
        let targets = recursive_scan_targets(cmd);

        assert!(
            targets
                .targets
                .iter()
                .any(|target| target == r#"python3 -c "print(42)""#)
        );
        assert!(targets.targets.iter().any(|target| target == "print(42)"));
    }

    #[test]
    fn recursive_targets_include_process_substitution_body() {
        let targets = recursive_scan_targets(r#"source <(python3 -c "print(42)")"#);

        assert!(
            targets
                .targets
                .iter()
                .any(|target| target == r#"python3 -c "print(42)""#)
        );
        assert!(targets.targets.iter().any(|target| target == "print(42)"));
    }

    #[test]
    fn recursive_targets_include_eval_payload() {
        let targets = recursive_scan_targets(r#"eval "python3 -c 'print(42)'"#);

        assert!(
            targets
                .targets
                .iter()
                .any(|target| target == "python3 -c 'print(42)'")
        );
        assert!(targets.targets.iter().any(|target| target == "print(42)"));
    }

    #[test]
    fn recursive_targets_report_depth_limit_when_nesting_exceeds_cap() {
        let mut cmd = "eval \"printf hi\"".to_string();
        for _ in 0..=MAX_NESTED_SCAN_DEPTH {
            cmd = format!("eval \"{cmd}\"");
        }

        let report = recursive_scan_targets(&cmd);

        assert_eq!(
            report.limit_hit,
            Some(RecursiveScanLimit::DepthExceeded {
                limit: MAX_NESTED_SCAN_DEPTH,
            })
        );
    }
}
