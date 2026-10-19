//! The directory each stage of a command runs in, once earlier `cd` steps are
//! applied (#525, ADR-049).
//!
//! The walk reads only the command's own list structure: top-level segments,
//! their separators, pipelines and `( ... )` groups. It trusts a `cd` only when
//! the next stage provably runs after it succeeded, and otherwise reports why
//! the directory is unknown. It resolves nothing against the filesystem.
//!
//! This is deliberately a different rule from the router's `CwdState`
//! (ADR-022 section 6), which decides what to auto-approve and so trusts only
//! `cd -- <literal> &&`. Here a wrong answer only mis-aims a recovery point,
//! which the disclosure rules of ADR-048 already cover.

use aegis_parser::ListSeparator;

use super::recursive;

/// Why a stage's directory could not be followed through the `cd` steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdUnresolved {
    /// `cd` with no path, `cd -`, an option, extra operands, `pushd` or `popd`.
    UnsupportedForm,
    /// A `cd` path holds `$`, a glob, a brace expansion, a backslash or `~`.
    DynamicPath,
    /// A relative `cd` while `CDPATH` is set in Aegis's own environment.
    CdPathSet,
    /// The `cd` may not have run before the stage: it is followed by anything
    /// but `&&`, or is itself reached through `||`.
    MayNotRun,
    /// A `cd` hides behind `eval`, `source`, `.`, a launcher or a nested shell.
    Opaque,
}

/// The directory one stage runs in, relative to the process cwd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageDir {
    /// The process cwd: no `cd` step precedes the stage.
    Process,
    /// The literal `cd` paths applied in order, each relative to the previous
    /// result (an absolute path replaces it).
    Chain(Vec<String>),
    /// The directory cannot be known.
    Unresolved(CdUnresolved),
}

/// Whether `path` holds a character the shell would expand or unquote, so its
/// value is not known from the text.
#[must_use]
pub fn path_is_dynamic(path: &str) -> bool {
    path.contains(['$', '*', '?', '[', ']', '~', '`', '{', '}', '\\'])
}

const MAX_GROUP_DEPTH: usize = 8;

/// Programs that change the directory, or may, through text Aegis cannot see.
/// `eval`, `source` and `.` always count, with no `cd` word required: the
/// string or file they run can hold a `cd` that never appears in the command
/// line (`eval "$X"`, `source env.sh`). This errs toward unresolved.
const CD_LIKE: &[&str] = &["cd", "pushd", "popd", "eval", "source", "."];

/// Whether `target` holds a stage whose effective program is `cd`, `pushd` or
/// `popd`: the cheap trigger that turns the walk on.
pub(super) fn mentions_cd(targets: &[String]) -> bool {
    targets.iter().any(|target| {
        if !(target.contains("cd") || target.contains("pushd") || target.contains("popd")) {
            return false;
        }
        let tokens = aegis_parser::split_tokens(target);
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        aegis_parser::effective_token_slices(&refs)
            .iter()
            .any(|slice| matches!(slice.program, "cd" | "pushd" | "popd"))
    })
}

/// Split `cmd` into stage texts, each with the directory it runs in.
pub(super) fn stage_dirs(cmd: &str, cdpath_set: bool) -> Vec<(String, StageDir)> {
    let mut stages = Vec::new();
    let walker = Walker { cdpath_set };
    walker.list(cmd, StageDir::Process, &mut stages, 0);
    stages
}

struct Walker {
    cdpath_set: bool,
}

impl Walker {
    /// Walk one list of segments; returns the directory after the last one.
    fn list(
        &self,
        cmd: &str,
        start: StageDir,
        out: &mut Vec<(String, StageDir)>,
        depth: usize,
    ) -> StageDir {
        let mut state = start;
        let mut incoming: Option<ListSeparator> = None;
        for segment in aegis_parser::list_segments(cmd) {
            let single = segment.pipeline.segments.len() == 1;
            let mut next = state.clone();
            for stage in &segment.pipeline.segments {
                let effect = self.stage(stage.raw.trim(), &state, incoming, out, depth);
                // A `cd` in a pipeline runs in a subshell: no effect after it.
                if single && let Some(effect) = effect {
                    next = effect;
                }
            }
            state = advance(next, segment.separator);
            incoming = segment.separator;
        }
        state
    }

    /// Record one stage; returns the directory after it when it is a `cd`-like
    /// construct that changes the current shell's directory.
    fn stage(
        &self,
        raw: &str,
        state: &StageDir,
        incoming: Option<ListSeparator>,
        out: &mut Vec<(String, StageDir)>,
        depth: usize,
    ) -> Option<StageDir> {
        if depth >= MAX_GROUP_DEPTH {
            out.push((raw.to_string(), StageDir::Unresolved(CdUnresolved::Opaque)));
            return Some(StageDir::Unresolved(CdUnresolved::Opaque));
        }
        if let Some(inner) = aegis_parser::unwrap_subshell_group(raw) {
            self.list(&inner, state.clone(), out, depth + 1);
            return None;
        }

        let tokens = aegis_parser::split_tokens(raw);
        if tokens.first().map(String::as_str) == Some("cd") {
            // The tokenizer drops backslashes, so look at the raw text.
            if raw.contains('\\') {
                return Some(match state {
                    StageDir::Unresolved(_) => state.clone(),
                    _ => StageDir::Unresolved(CdUnresolved::DynamicPath),
                });
            }
            return Some(self.apply_cd(state, &tokens[1..], incoming));
        }

        if let Some(reason) = hidden_cd(raw) {
            let dir = match state {
                StageDir::Unresolved(_) => state.clone(),
                _ => StageDir::Unresolved(reason),
            };
            out.push((raw.to_string(), dir.clone()));
            return Some(dir);
        }
        out.push((raw.to_string(), state.clone()));
        None
    }

    fn apply_cd(
        &self,
        state: &StageDir,
        operands: &[String],
        incoming: Option<ListSeparator>,
    ) -> StageDir {
        if matches!(state, StageDir::Unresolved(_)) {
            return state.clone();
        }
        let path = match operands {
            [path] if !path.starts_with('-') => path,
            [dashdash, path] if dashdash == "--" => path,
            _ => return StageDir::Unresolved(CdUnresolved::UnsupportedForm),
        };
        if path.is_empty() || path == "-" {
            return StageDir::Unresolved(CdUnresolved::UnsupportedForm);
        }
        if path_is_dynamic(path) {
            return StageDir::Unresolved(CdUnresolved::DynamicPath);
        }
        if self.cdpath_set && !path.starts_with('/') {
            return StageDir::Unresolved(CdUnresolved::CdPathSet);
        }
        if incoming == Some(ListSeparator::Or) {
            return StageDir::Unresolved(CdUnresolved::MayNotRun);
        }
        match state {
            StageDir::Chain(steps) => {
                let mut steps = steps.clone();
                steps.push(path.clone());
                StageDir::Chain(steps)
            }
            _ => StageDir::Chain(vec![path.clone()]),
        }
    }
}

/// A resolved directory survives only an unbroken `&&` chain.
fn advance(state: StageDir, separator: Option<ListSeparator>) -> StageDir {
    match (&state, separator) {
        (StageDir::Chain(_), Some(sep)) if sep != ListSeparator::And => {
            StageDir::Unresolved(CdUnresolved::MayNotRun)
        }
        _ => state,
    }
}

/// Why `raw`, an ordinary stage, may change the directory behind Aegis's back:
/// `pushd`/`popd`, or any `cd`, `eval`, `source` or `.` it holds directly or in
/// a nested shell, substitution or payload.
///
/// Precedence: a stage that is itself `pushd` or `popd` is `UnsupportedForm`,
/// whatever else it holds; any other hit is `Opaque`.
fn hidden_cd(raw: &str) -> Option<CdUnresolved> {
    let parsed = aegis_parser::Parser::parse(raw);
    let report = recursive::scan_targets(raw, &parsed);
    let mut reason = None;
    for target in &report.targets {
        let tokens = aegis_parser::split_tokens(target);
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        for slice in aegis_parser::effective_token_slices(&refs) {
            if !CD_LIKE.contains(&slice.program) {
                continue;
            }
            if target == raw && matches!(slice.program, "pushd" | "popd") {
                return Some(CdUnresolved::UnsupportedForm);
            }
            reason = Some(CdUnresolved::Opaque);
        }
    }
    reason
}
