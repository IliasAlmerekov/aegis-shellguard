//! Git stages that need a Snapshot of the working tree git will act on (#471).
//!
//! `Scanner::assess` decides how risky a command is. It does not say which
//! directory a `git` stage will change: `git -C ../other reset --hard` is
//! `Danger`, but the tree at risk is `../other`, not the process cwd. This
//! module reuses the scanner's own stage enumeration and git global-option
//! walk (ADR-041) to report, for each `Danger` git stage that redirects its
//! working tree, the raw option values the planner needs to resolve a Git
//! target. It resolves nothing against the filesystem and takes no Snapshot.

use std::collections::HashSet;

use aegis_parser::{EffectiveTokenSlice, EndOfOptions, GitTargetArgs, GitTargetReadings};
use aegis_types::{Assessment, MatchResult, RiskLevel};

use super::assessment::REGEX_SUPERSEDED_PREFIXES;
use super::{Scanner, recursive};

/// Why a redirected git stage has no single option set to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitStageTarget {
    /// The `-C`/`--work-tree`/`--git-dir` values of every reading of the
    /// stage's global options. One entry for an ordinary command; several
    /// when an unlisted option makes two readings disagree.
    Options(Vec<GitTargetArgs>),
    /// The option walk exceeded its candidate budget.
    TooManyOptions,
    /// `GIT_DIR=` or `GIT_WORK_TREE=` sits in the stage's environment prefix,
    /// or in Aegis's own environment.
    EnvironmentRedirect,
    /// The stage sits behind a launcher whose argv the scanner cannot map
    /// back onto the stage's own tokens (an `env -S` split).
    UnmappedLauncher,
    /// A launcher before `git` changes the working directory (`env -C`,
    /// `env --chdir`, `sudo -D`, `sudo --chdir`), so every relative path in
    /// the stage resolves against a directory Aegis does not track.
    LauncherChangesDirectory,
}

/// One `Danger` git stage whose working tree is not the process cwd, or not
/// provably the process cwd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStage {
    /// What the stage's options and environment say about its working tree.
    pub target: GitStageTarget,
}

/// The git Snapshot demand a command makes, stage by stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStageReport {
    /// `Danger` git stages that redirect their working tree.
    pub redirected: Vec<GitStage>,
    /// Whether some part of the command that needs a Snapshot is not
    /// explained by `redirected`: a non-redirected `Danger` stage, an
    /// effect-opaque command, or a `Danger` match no git stage accounts for.
    pub needs_process_cwd: bool,
}

impl Scanner {
    /// Report which `Danger` git stages of `cmd` redirect their working tree.
    ///
    /// `assessment` must be the assessment of `cmd`. Called only when a
    /// recovery backstop applies, never on the safe path.
    pub fn git_stage_report(&self, cmd: &str, assessment: &Assessment) -> GitStageReport {
        let ambient = ["GIT_DIR", "GIT_WORK_TREE"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        self.git_stage_report_with_env(cmd, assessment, ambient)
    }

    /// [`Self::git_stage_report`] with the process environment passed in:
    /// `ambient_git_env` says Aegis's own environment sets `GIT_DIR` or
    /// `GIT_WORK_TREE`, which every spawned git inherits, so no git stage
    /// can be shown to act on the cwd.
    pub fn git_stage_report_with_env(
        &self,
        cmd: &str,
        assessment: &Assessment,
        ambient_git_env: bool,
    ) -> GitStageReport {
        // Read stage boundaries from the words bash runs, as `assess` does.
        let canonical = aegis_parser::canonical_ansi_c_quoting(cmd);
        let cmd = canonical.as_ref();
        let parsed = aegis_parser::Parser::parse(cmd);
        let target_report = recursive::scan_targets(cmd, &parsed);
        if target_report.limit_hit.is_some() {
            return GitStageReport {
                redirected: Vec::new(),
                needs_process_cwd: true,
            };
        }

        let mut redirected: Vec<GitStage> = Vec::new();
        let mut redirected_ids: HashSet<String> = HashSet::new();
        let mut plain_ids: HashSet<String> = HashSet::new();

        for target in &target_report.targets {
            // The raw command joins every segment into one token list; each
            // segment is its own target, so skip it as `assess` does.
            if target == cmd && aegis_parser::has_multiple_top_level_segments(cmd) {
                continue;
            }
            let tokens = aegis_parser::split_tokens(target);
            let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

            for slice in aegis_parser::effective_token_slices(&token_refs) {
                if !slice.program.eq_ignore_ascii_case("git") {
                    // A Danger match in any other stage needs the process cwd
                    // Snapshot, even when a redirected git stage matches the
                    // same pattern id (`git -C a log --format='rm -rf x'`).
                    plain_ids.extend(self.danger_ids(&slice));
                    continue;
                }
                let stage_target = stage_target(&token_refs, &slice, ambient_git_env);
                let ids = self.danger_ids(&slice);
                if ids.is_empty() {
                    continue;
                }
                match stage_target {
                    Some(stage_target) => {
                        redirected_ids.extend(ids);
                        let stage = GitStage {
                            target: stage_target,
                        };
                        if !redirected.contains(&stage) {
                            redirected.push(stage);
                        }
                    }
                    None => plain_ids.extend(ids),
                }
            }
        }

        let needs_process_cwd = assessment.effect_opaque
            || assessment.matched.iter().any(|matched| {
                matched.pattern.risk == RiskLevel::Danger
                    && (!redirected_ids.contains(matched.pattern.id.as_ref())
                        || plain_ids.contains(matched.pattern.id.as_ref()))
            });

        GitStageReport {
            redirected,
            needs_process_cwd,
        }
    }

    /// Ids of the `Danger` patterns one git slice matches, reading global
    /// options the way `assess` does (ADR-041).
    fn danger_ids(&self, slice: &EffectiveTokenSlice<'_>) -> Vec<String> {
        let mut matched: Vec<MatchResult> =
            self.prefix_scan_effective_slices(std::slice::from_ref(slice), EndOfOptions::Ignore);
        matched.extend(self.full_scan(&slice.tokens.join(" "), Some(slice.program)));
        let mut seen = [false; REGEX_SUPERSEDED_PREFIXES.len()];
        self.scan_git_option_candidates(
            std::slice::from_ref(slice),
            &mut matched,
            &mut seen,
            EndOfOptions::Ignore,
        );
        matched
            .into_iter()
            .filter(|m| m.pattern.risk == RiskLevel::Danger)
            .map(|m| m.pattern.id.to_string())
            .collect()
    }
}

/// `Some` when the stage's working tree is redirected or cannot be shown to
/// be the process cwd; `None` for a plain stage.
fn stage_target(
    token_refs: &[&str],
    slice: &EffectiveTokenSlice<'_>,
    ambient_git_env: bool,
) -> Option<GitStageTarget> {
    let tail_len = slice.tokens.len() - 1;
    let tail_start = token_refs.len().checked_sub(tail_len);
    let mapped = tail_start.is_some_and(|start| token_refs[start..] == slice.tokens[1..]);
    if !mapped {
        return Some(GitStageTarget::UnmappedLauncher);
    }
    let prefix = &token_refs[..token_refs.len() - tail_len];
    if ambient_git_env
        || prefix
            .iter()
            .any(|token| token.starts_with("GIT_DIR=") || token.starts_with("GIT_WORK_TREE="))
    {
        return Some(GitStageTarget::EnvironmentRedirect);
    }
    if prefix_changes_directory(prefix) {
        return Some(GitStageTarget::LauncherChangesDirectory);
    }
    match aegis_parser::git_option_target_readings(&slice.tokens) {
        GitTargetReadings::TooMany => Some(GitStageTarget::TooManyOptions),
        GitTargetReadings::Readings(readings) => {
            if readings
                .iter()
                .all(|args| *args == GitTargetArgs::default())
            {
                None
            } else {
                Some(GitStageTarget::Options(readings))
            }
        }
    }
}

/// Whether a launcher in `prefix` runs the command in another directory.
///
/// Reads `env -C DIR`, `-CDIR`, `--chdir DIR`, `--chdir=DIR` and the `sudo`
/// forms `-D DIR`, `--chdir=DIR`. Short options may be clustered, so any short
/// option token holding the letter counts. This leans toward reporting a
/// change: an unresolved Git target is disclosed, never silently wrong.
fn prefix_changes_directory(prefix: &[&str]) -> bool {
    let has_launcher = |name: &str| {
        prefix.iter().any(|token| {
            token
                .rsplit('/')
                .next()
                .unwrap_or(token)
                .eq_ignore_ascii_case(name)
        })
    };
    let (env, sudo) = (has_launcher("env"), has_launcher("sudo"));
    prefix.iter().any(|token| {
        let long = *token == "--chdir" || token.starts_with("--chdir=");
        let short = |letter: char| {
            token.starts_with('-') && !token.starts_with("--") && token.contains(letter)
        };
        ((env || sudo) && long) || (env && short('C')) || (sudo && short('D'))
    })
}
