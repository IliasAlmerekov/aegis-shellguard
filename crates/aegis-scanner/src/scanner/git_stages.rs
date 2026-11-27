use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;

use aegis_parser::{EffectiveTokenSlice, EndOfOptions, GitTargetArgs, GitTargetReadings};
use aegis_types::{Assessment, MatchResult, RiskLevel};

use super::assessment::REGEX_SUPERSEDED_PREFIXES;
use super::cd_walk::{CD_PROGRAMS, CdUnresolved, StageDir, mentions_cd, stage_dirs};
use super::{Scanner, pipeline_semantics, recursive};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DangerStageTarget {
    Plain,
    Options(Vec<GitTargetArgs>),
    TooManyOptions,
    EnvironmentRedirect,
    UnmappedLauncher,
    LauncherChangesDirectory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DangerStage {
    pub target: DangerStageTarget,
    pub dir: StageDir,
    pub is_git: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AmbientEnv {
    pub(crate) git_env: bool,
    pub(crate) cdpath_set: bool,
    pub(crate) physical: bool,
    pub(crate) cd_function: bool,
    pub(crate) lastpipe: bool,
    pub(crate) zsh: bool,
}

impl AmbientEnv {
    fn from_process() -> Self {
        Self {
            git_env: ["GIT_DIR", "GIT_WORK_TREE"]
                .iter()
                .any(|name| std::env::var_os(name).is_some()),
            cdpath_set: std::env::var_os("CDPATH").is_some(),
            physical: env_text("SHELLOPTS").contains("physical"),
            cd_function: std::env::var_os("BASH_FUNC_cd%%").is_some(),
            lastpipe: env_text("BASHOPTS").contains("lastpipe"),
            zsh: shell_is_zsh(
                std::env::var_os("AEGIS_REAL_SHELL").as_deref(),
                std::env::var_os("SHELL").as_deref(),
            ),
        }
    }
}

pub(crate) fn shell_is_zsh(real_shell: Option<&OsStr>, shell: Option<&OsStr>) -> bool {
    [real_shell, shell]
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
        .is_some_and(|path| Path::new(path).file_name() == Some(OsStr::new("zsh")))
}

fn env_text(name: &str) -> String {
    std::env::var_os(name)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[derive(Default)]
struct StageEvidence {
    redirected: Vec<DangerStage>,
    redirected_ids: HashSet<String>,
    plain_ids: HashSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStageReport {
    pub redirected: Vec<DangerStage>,
    pub needs_process_cwd: bool,
}

impl StageEvidence {
    fn push_stage(&mut self, stage: DangerStage) {
        if !self.redirected.contains(&stage) {
            self.redirected.push(stage);
        }
    }

    fn collect_pipeline(&mut self, scanner: &Scanner, stages: &[String], dir: &StageDir) {
        let pipeline = stages.join(" | ");
        let chains = aegis_parser::top_level_pipelines(&pipeline);
        let mut matched = pipeline_semantics::semantic_pipeline_matches(&chains);
        for stage in stages {
            let tokens = aegis_parser::split_tokens(stage);
            let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
            for slice in aegis_parser::effective_token_slices(&refs) {
                matched.extend(scanner.full_scan(&pipeline, Some(slice.program)));
            }
        }
        let ids: Vec<String> = matched
            .into_iter()
            .filter(|matched| matched.pattern.risk == RiskLevel::Danger)
            .map(|matched| matched.pattern.id.to_string())
            .collect();
        if ids.is_empty() {
            return;
        }
        if *dir == StageDir::Process {
            self.plain_ids.extend(ids);
        } else {
            self.redirected_ids.extend(ids);
            self.push_stage(DangerStage {
                target: DangerStageTarget::Plain,
                dir: dir.clone(),
                is_git: false,
            });
        }
    }

    fn attribute_unmatched(&mut self, assessment: &Assessment, stages: &[(String, StageDir)]) {
        let unmatched: Vec<String> = assessment
            .matched
            .iter()
            .filter(|matched| matched.pattern.risk == RiskLevel::Danger)
            .map(|matched| matched.pattern.id.to_string())
            .filter(|id| !self.redirected_ids.contains(id) && !self.plain_ids.contains(id))
            .collect();
        if unmatched.is_empty() {
            return;
        }
        let mut in_process_cwd = stages.is_empty();
        for (_, dir) in stages {
            if *dir == StageDir::Process {
                in_process_cwd = true;
            } else {
                self.redirected_ids.extend(unmatched.iter().cloned());
                self.push_stage(DangerStage {
                    target: DangerStageTarget::Plain,
                    dir: dir.clone(),
                    is_git: false,
                });
            }
        }
        if in_process_cwd {
            self.plain_ids.extend(unmatched);
        }
    }

    fn collect(
        &mut self,
        scanner: &Scanner,
        cmd: &str,
        targets: &[String],
        dir: &StageDir,
        ambient_git_env: bool,
    ) {
        for target in targets {
            if target == cmd && aegis_parser::has_multiple_top_level_segments(cmd) {
                continue;
            }
            let tokens = aegis_parser::split_tokens(target);
            let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

            for slice in aegis_parser::effective_token_slices(&token_refs) {
                if !slice.program.eq_ignore_ascii_case("git") {
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

fn limit_hit_report(mentions_cd: bool) -> GitStageReport {
    let redirected = if mentions_cd {
        vec![DangerStage {
            target: DangerStageTarget::Plain,
            dir: StageDir::Unresolved(CdUnresolved::Opaque),
            is_git: false,
        }]
    } else {
        Vec::new()
    };
    GitStageReport {
        redirected,
        needs_process_cwd: true,
    }
}

impl Scanner {
    pub fn git_stage_report(&self, cmd: &str, assessment: &Assessment) -> GitStageReport {
        self.git_stage_report_with(cmd, assessment, AmbientEnv::from_process())
    }

    pub(crate) fn git_stage_report_with(
        &self,
        cmd: &str,
        assessment: &Assessment,
        ambient: AmbientEnv,
    ) -> GitStageReport {
        let canonical = aegis_parser::canonical_ansi_c_quoting(cmd);
        let cmd = canonical.as_ref();
        let parsed = aegis_parser::Parser::parse(cmd);
        let target_report = recursive::scan_targets(cmd, &parsed);
        if target_report.limit_hit.is_some() {
            return limit_hit_report(CD_PROGRAMS.iter().any(|word| cmd.contains(word)));
        }

        let mut evidence = StageEvidence::default();
        if mentions_cd(&target_report.targets) {
            let walk = stage_dirs(cmd, ambient);
            for (stage, dir) in &walk.stages {
                let parsed = aegis_parser::Parser::parse(stage);
                let stage_report = recursive::scan_targets(stage, &parsed);
                if stage_report.limit_hit.is_some() {
                    return limit_hit_report(true);
                }
                evidence.collect(self, stage, &stage_report.targets, dir, ambient.git_env);
            }
            for (pipeline, dir) in walk.pipelines {
                evidence.collect_pipeline(self, &pipeline, &dir);
            }
            evidence.attribute_unmatched(assessment, &walk.stages);
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

fn moved_stage_target(token_refs: &[&str], slice: &EffectiveTokenSlice<'_>) -> DangerStageTarget {
    match launcher_prefix(token_refs, slice) {
        None => DangerStageTarget::UnmappedLauncher,
        Some(prefix) if prefix_changes_directory(prefix) => {
            DangerStageTarget::LauncherChangesDirectory
        }
        Some(_) => DangerStageTarget::Plain,
    }
}

fn launcher_prefix<'a, 'b>(
    token_refs: &'a [&'b str],
    slice: &EffectiveTokenSlice<'_>,
) -> Option<&'a [&'b str]> {
    let tail_len = slice.tokens.len() - 1;
    let start = token_refs.len().checked_sub(tail_len)?;
    (token_refs[start..] == slice.tokens[1..]).then(|| &token_refs[..start])
}

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
        let login = *token == "--login" || short('i');
        ((env || sudo) && long) || (env && short('C')) || (sudo && (short('D') || login))
    })
}
