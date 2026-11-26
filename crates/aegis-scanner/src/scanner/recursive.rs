use crate::nested::{RecursiveScanReport, recursive_scan_targets};
use aegis_parser::logical_scan_segments;
use aegis_types::ParsedCommand;

pub(super) fn scan_targets(cmd: &str, parsed: &ParsedCommand) -> RecursiveScanReport {
    if requires_recursive_scan(cmd) {
        return recursive_scan_targets(cmd);
    }

    let mut targets = vec![cmd.to_string()];

    let segments = logical_scan_segments(cmd);
    for segment in &segments {
        push_unique_target(&mut targets, segment.normalized.clone());
    }

    for script in &parsed.inline_scripts {
        push_unique_target(&mut targets, script.body.clone());
    }

    RecursiveScanReport {
        targets,
        limit_hit: None,
        segments,
    }
}

fn requires_recursive_scan(cmd: &str) -> bool {
    cmd.contains("<<")
        || cmd.contains("<(")
        || cmd.contains('`')
        || cmd
            .split(|c: char| c.is_whitespace() || matches!(c, ';' | '|' | '&'))
            .any(|token| token == "eval")
}

fn push_unique_target(targets: &mut Vec<String>, target: String) {
    if !target.is_empty() && !targets.iter().any(|existing| existing == &target) {
        targets.push(target);
    }
}
