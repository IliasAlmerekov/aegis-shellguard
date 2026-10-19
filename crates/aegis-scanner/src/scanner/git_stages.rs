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
use super::cd_walk::{StageDir, mentions_cd, stage_dirs};
use super::{Scanner, recursive};

/// Why a redirected git stage has no single option set to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DangerStageTarget {
    /// No git option redirects the stage: its working tree is the directory
    /// the stage runs in (#525). Used for a stage moved only by `cd`, git or
    /// not.
    Plain,
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

/// One `Danger` stage whose working tree is not the process cwd, or not
/// provably the process cwd: a git stage that redirects through options, or
/// any stage that runs after a `cd` (#525).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DangerStage {
    /// What the stage's options and environment say about its working tree.
    pub target: DangerStageTarget,
    /// The directory the stage runs in, once earlier `cd` steps are applied.
    pub dir: StageDir,
    /// Whether the stage's program is `git`. A non-git stage in a directory
    /// that is no repository needs no Snapshot; a git stage there is a gap.
    pub is_git: bool,
}

/// The parts of Aegis's own environment that change what a stage can be shown
/// to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AmbientEnv {
    /// `GIT_DIR` or `GIT_WORK_TREE` is set, so every spawned git inherits it
    /// and no git stage can be shown to act on the cwd.
    pub(crate) git_env: bool,
    /// `CDPATH` is set, so no relative `cd` can be followed.
    pub(crate) cdpath_set: bool,
}

impl AmbientEnv {
    fn from_process() -> Self {
        Self {
            git_env: ["GIT_DIR", "GIT_WORK_TREE"]
                .iter()
                .any(|name| std::env::var_os(name).is_some()),
            cdpath_set: std::env::var_os("CDPATH").is_some(),
        }
    }
}

/// The evidence one report gathers while it walks the stages.
#[derive(Default)]
struct StageEvidence {
    redirected: Vec<DangerStage>,
    redirected_ids: HashSet<String>,
    plain_ids: HashSet<String>,
}

/// The git Snapshot demand a command makes, stage by stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStageReport {
    /// `Danger` git stages that redirect their working tree.
    pub redirected: Vec<DangerStage>,
    /// Whether some part of the command that needs a Snapshot is not
    /// explained by `redirected`: a non-redirected `Danger` stage, an
    /// effect-opaque command, or a `Danger` match no git stage accounts for.
    pub needs_process_cwd: bool,
}

impl StageEvidence {
    fn push_stage(&mut self, stage: DangerStage) {
        if !self.redirected.contains(&stage) {
            self.redirected.push(stage);
        }
    }

    /// Sort the `Danger` stages among `targets` (the scan targets of `cmd`)
    /// into redirected ones and ones that need the process cwd. `dir` is the
    /// directory every stage of `cmd` runs in.
    fn collect(
        &mut self,
        scanner: &Scanner,
        cmd: &str,
        targets: &[String],
        dir: &StageDir,
        ambient_git_env: bool,
    ) {
        for target in targets {
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
                    // same pattern id (`git -C a log --format='rm -rf x'`),
                    // unless a `cd` moved the stage elsewhere (#525).
                    let ids = scanner.danger_ids(&slice);
                    if *dir == StageDir::Process {
                        self.plain_ids.extend(ids);
                    } else if !ids.is_empty() {
                        self.redirected_ids.extend(ids);
                        self.push_stage(DangerStage {
                            target: moved_stage_target(&token_refs, &slice),
                            dir: dir.clone(),
                            is_git: false,
                        });
                    }
                    continue;
                }
                let stage_target = stage_target(&token_refs, &slice, ambient_git_env);
                let ids = scanner.danger_ids(&slice);
                if ids.is_empty() {
                    continue;
                }
                match (stage_target, dir) {
                    (None, StageDir::Process) => self.plain_ids.extend(ids),
                    (stage_target, _) => {
                        self.redirected_ids.extend(ids);
                        self.push_stage(DangerStage {
                            target: stage_target.unwrap_or(DangerStageTarget::Plain),
                            dir: dir.clone(),
                            is_git: true,
                        });
                    }
                }
            }
        }
    }
}

impl Scanner {
    /// Report which `Danger` git stages of `cmd` redirect their working tree.
    ///
    /// `assessment` must be the assessment of `cmd`. Called only when a
    /// recovery backstop applies, never on the safe path.
    pub fn git_stage_report(&self, cmd: &str, assessment: &Assessment) -> GitStageReport {
        self.git_stage_report_with(cmd, assessment, AmbientEnv::from_process())
    }

    /// [`Self::git_stage_report`] with Aegis's own environment passed in.
    pub(crate) fn git_stage_report_with(
        &self,
        cmd: &str,
        assessment: &Assessment,
        ambient: AmbientEnv,
    ) -> GitStageReport {
        let parsed = aegis_parser::Parser::parse(cmd);
        let target_report = recursive::scan_targets(cmd, &parsed);
        if target_report.limit_hit.is_some() {
            return GitStageReport {
                redirected: Vec::new(),
                needs_process_cwd: true,
            };
        }

        let mut evidence = StageEvidence::default();
        if mentions_cd(&target_report.targets) {
            // #525: walk the command's list structure so each stage knows the
            // directory it runs in. Runs only when a `cd` is present.
            for (stage, dir) in stage_dirs(cmd, ambient.cdpath_set) {
                let parsed = aegis_parser::Parser::parse(&stage);
                let stage_report = recursive::scan_targets(&stage, &parsed);
                if stage_report.limit_hit.is_some() {
                    return GitStageReport {
                        redirected: Vec::new(),
                        needs_process_cwd: true,
                    };
                }
                evidence.collect(self, &stage, &stage_report.targets, &dir, ambient.git_env);
            }
        } else {
            evidence.collect(
                self,
                cmd,
                &target_report.targets,
                &StageDir::Process,
                ambient.git_env,
            );
        }

        let StageEvidence {
            redirected,
            redirected_ids,
            plain_ids,
        } = evidence;
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

/// The target of a non-git stage that a `cd` moved: plain, unless a launcher
/// in front of it changes the directory again (`env -C`, `sudo -D`) or cannot
/// be mapped back onto the stage's own tokens (#525).
fn moved_stage_target(token_refs: &[&str], slice: &EffectiveTokenSlice<'_>) -> DangerStageTarget {
    match launcher_prefix(token_refs, slice) {
        None => DangerStageTarget::UnmappedLauncher,
        Some(prefix) if prefix_changes_directory(prefix) => {
            DangerStageTarget::LauncherChangesDirectory
        }
        Some(_) => DangerStageTarget::Plain,
    }
}

/// The tokens in front of the effective program, or `None` when the slice does
/// not map back onto the tail of the stage's own tokens (an `env -S` split).
fn launcher_prefix<'a, 'b>(
    token_refs: &'a [&'b str],
    slice: &EffectiveTokenSlice<'_>,
) -> Option<&'a [&'b str]> {
    let tail_len = slice.tokens.len() - 1;
    let start = token_refs.len().checked_sub(tail_len)?;
    (token_refs[start..] == slice.tokens[1..]).then(|| &token_refs[..start])
}

/// `Some` when the stage's working tree is redirected or cannot be shown to
/// be the process cwd; `None` for a plain stage.
fn stage_target(
    token_refs: &[&str],
    slice: &EffectiveTokenSlice<'_>,
    ambient_git_env: bool,
) -> Option<DangerStageTarget> {
    let Some(prefix) = launcher_prefix(token_refs, slice) else {
        return Some(DangerStageTarget::UnmappedLauncher);
    };
    if ambient_git_env
        || prefix
            .iter()
            .any(|token| token.starts_with("GIT_DIR=") || token.starts_with("GIT_WORK_TREE="))
    {
        return Some(DangerStageTarget::EnvironmentRedirect);
    }
    if prefix_changes_directory(prefix) {
        return Some(DangerStageTarget::LauncherChangesDirectory);
    }
    match aegis_parser::git_option_target_readings(&slice.tokens) {
        GitTargetReadings::TooMany => Some(DangerStageTarget::TooManyOptions),
        GitTargetReadings::Readings(readings) => {
            if readings
                .iter()
                .all(|args| *args == GitTargetArgs::default())
            {
                None
            } else {
                Some(DangerStageTarget::Options(readings))
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
