//! The directory each stage of a command runs in, once earlier `cd` steps are
//! applied (#525, ADR-050).
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

use std::cell::Cell;

use aegis_parser::ListSeparator;

use super::recursive;

/// Why a stage's directory could not be followed through the `cd` steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdUnresolved {
    /// `cd` with no path, `cd -`, an option, extra operands, `pushd` or `popd`.
    UnsupportedForm,
    /// A `cd` path holds an unquoted `$`, glob, brace expansion, backslash or
    /// `~`, or a `$`, backtick or backslash inside double quotes.
    DynamicPath,
    /// A relative `cd` while `CDPATH` is set in Aegis's own environment.
    CdPathSet,
    /// The `cd` may not have run before the stage: it is followed by anything
    /// but `&&`, or is itself reached through `||`.
    MayNotRun,
    /// A `cd` hides behind `eval`, `source`, `.`, a launcher or a nested shell.
    Opaque,
    /// An earlier stage may have turned on Bash's physical mode (`set -P`,
    /// `set -o physical`, `SHELLOPTS`), so `..` resolves through symlinks and
    /// the lexical walk would name the wrong directory.
    PhysicalMode,
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

impl StageDir {
    /// This directory after something makes it unknowable for `reason`. An
    /// already unresolved directory keeps its first reason.
    fn degrade(&self, reason: CdUnresolved) -> StageDir {
        match self {
            StageDir::Unresolved(_) => self.clone(),
            _ => StageDir::Unresolved(reason),
        }
    }
}

/// Whether `path`, taken as unquoted text, holds a character the shell would
/// expand or unquote, so its value is not known from the text. The quotes are
/// already gone from the text a caller passes here, so a quoted literal such
/// as `'x$y'` also reads as dynamic: this errs toward unresolved. A `cd`
/// operand is judged from the raw text with its quoting instead.
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
    let walker = Walker {
        cdpath_set: Cell::new(cdpath_set),
        physical: Cell::new(false),
    };
    walker.list(cmd, StageDir::Process, &mut stages, 0);
    stages
}

struct Walker {
    /// Whether an earlier stage may have turned on Bash's physical mode
    /// (`set -P`). Once on, it stays on.
    physical: Cell<bool>,
    /// Whether `CDPATH` may be set: in Aegis's own environment, or by an
    /// earlier stage of the command line. Once set, it stays set.
    cdpath_set: Cell<bool>,
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
        // A stage that names `CDPATH` may set or clear it for the stages after
        // it. This errs toward unresolved. The stage itself runs with the old
        // value (`CDPATH=x cd y` is a hidden `cd`, unresolved on its own).
        let touches_cdpath =
            raw.contains("CDPATH") || tokens.iter().any(|token| token.contains("CDPATH"));
        let result = self.stage_inner(raw, &tokens, state, incoming, out);
        if touches_cdpath {
            self.cdpath_set.set(true);
        }
        if sets_physical_mode(raw, &tokens) {
            self.physical.set(true);
        }
        result
    }

    fn stage_inner(
        &self,
        raw: &str,
        tokens: &[String],
        state: &StageDir,
        incoming: Option<ListSeparator>,
        out: &mut Vec<(String, StageDir)>,
    ) -> Option<StageDir> {
        if tokens.first().map(String::as_str) == Some("cd") {
            // The tokenizer drops quotes and backslashes, so the quoting is
            // read from the raw text.
            let operands = cd_operands(raw, tokens);
            // A substitution in an operand runs before the `cd` takes effect,
            // so what it holds belongs to the directory in effect before it.
            if raw.contains("$(") || raw.contains('`') || raw.contains("<(") {
                let before = match hidden_cd(raw, true) {
                    Some(reason) => state.degrade(reason),
                    None => state.clone(),
                };
                out.push((raw.to_string(), before));
            }
            if self.physical.get() {
                // `..` would follow symlinks, which the lexical walk does not.
                return Some(state.degrade(CdUnresolved::PhysicalMode));
            }
            return Some(self.apply_cd(state, &operands, incoming));
        }

        if let Some(reason) = hidden_cd(raw, false) {
            let dir = state.degrade(reason);
            out.push((raw.to_string(), dir.clone()));
            return Some(dir);
        }
        out.push((raw.to_string(), state.clone()));
        None
    }

    fn apply_cd(
        &self,
        state: &StageDir,
        operands: &[CdWord],
        incoming: Option<ListSeparator>,
    ) -> StageDir {
        let parsed = self.literal_path(operands);
        if let StageDir::Unresolved(_) = state {
            // A literal absolute `cd` reached through `&&` runs only when the
            // earlier steps succeeded, and lands in the same place whatever the
            // directory was before: it recovers a known directory.
            return match parsed {
                Ok(path) if incoming == Some(ListSeparator::And) && path.starts_with('/') => {
                    StageDir::Chain(vec![path])
                }
                _ => state.clone(),
            };
        }
        let path = match parsed {
            Ok(path) => path,
            Err(reason) => return StageDir::Unresolved(reason),
        };
        if incoming == Some(ListSeparator::Or) {
            return StageDir::Unresolved(CdUnresolved::MayNotRun);
        }
        match state {
            StageDir::Chain(steps) => {
                let mut steps = steps.clone();
                steps.push(path);
                StageDir::Chain(steps)
            }
            _ => StageDir::Chain(vec![path]),
        }
    }

    /// The literal path a `cd` with these operands names, or why it is not one.
    fn literal_path(&self, operands: &[CdWord]) -> Result<String, CdUnresolved> {
        let word = match operands {
            [word] if !word.value.starts_with('-') => word,
            [dashdash, word] if dashdash.value == "--" => word,
            _ => return Err(CdUnresolved::UnsupportedForm),
        };
        let path = &word.value;
        if path.is_empty() || path == "-" {
            return Err(CdUnresolved::UnsupportedForm);
        }
        if word.dynamic {
            return Err(CdUnresolved::DynamicPath);
        }
        // `CDPATH` is not searched for an absolute path, or one whose first
        // component is `.` or `..`.
        let searched = !path.starts_with('/')
            && path != "."
            && path != ".."
            && !path.starts_with("./")
            && !path.starts_with("../");
        if self.cdpath_set.get() && searched {
            return Err(CdUnresolved::CdPathSet);
        }
        Ok(path.clone())
    }
}

/// One operand of a `cd`: its text after quote removal, and whether the shell
/// would still change it (expand a variable, glob, brace or `~`, or unescape).
struct CdWord {
    value: String,
    dynamic: bool,
    /// Whether any part of the word was quoted.
    quoted: bool,
}

/// The operands of the `cd` stage `raw`, whose tokens are `tokens`.
///
/// Reads the raw text so quoting counts: single quotes make everything
/// literal; inside double quotes only `$`, a backtick and a backslash expand;
/// unquoted, any of those or a glob, brace or `~` expands, and a backslash
/// always counts as dynamic. When the raw words do not line up with the
/// tokenizer's tokens, every operand counts as dynamic: unresolved whenever
/// unsure.
fn cd_operands(raw: &str, tokens: &[String]) -> Vec<CdWord> {
    let words = raw_words(raw);
    let aligned = words
        .iter()
        .map(|word| word.value.as_str())
        .eq(tokens.iter().map(String::as_str));
    if aligned {
        return without_redirections(words.into_iter().skip(1).collect());
    }
    without_redirections(
        tokens[1..]
            .iter()
            .map(|token| CdWord {
                dynamic: true,
                quoted: false,
                value: token.clone(),
            })
            .collect(),
    )
}

/// Drop the redirections (`2>/dev/null`, `> file`, `&>x`, `< in`) from the
/// words of a command: they are syntax, not arguments. A quoted word is always
/// an argument.
fn without_redirections(words: Vec<CdWord>) -> Vec<CdWord> {
    let mut kept = Vec::new();
    let mut skip_target = false;
    for word in words {
        if skip_target {
            skip_target = false;
            continue;
        }
        if word.quoted {
            kept.push(word);
            continue;
        }
        let operator = word.value.trim_start_matches(|c: char| c.is_ascii_digit());
        let operator = operator.strip_prefix('&').unwrap_or(operator);
        if operator.starts_with(['<', '>']) {
            let rest = operator.trim_start_matches(['<', '>', '&', '|']);
            skip_target = rest.is_empty();
            continue;
        }
        kept.push(word);
    }
    kept
}

/// Split `raw` into whitespace-separated words the way the tokenizer does
/// for plain words, tracking which characters were left unquoted.
fn raw_words(raw: &str) -> Vec<CdWord> {
    let mut words = Vec::new();
    let mut value = String::new();
    let (mut dynamic, mut quoted) = (false, false);
    let (mut single, mut double) = (false, false);
    for ch in raw.chars() {
        match ch {
            '\'' if !double => {
                single = !single;
                quoted = true;
            }
            '"' if !single => {
                double = !double;
                quoted = true;
            }
            c if c.is_whitespace() && !single && !double => {
                if !value.is_empty() {
                    words.push(CdWord {
                        value: std::mem::take(&mut value),
                        dynamic: std::mem::take(&mut dynamic),
                        quoted: std::mem::take(&mut quoted),
                    });
                }
            }
            c => {
                dynamic |= if single {
                    false
                } else if double {
                    matches!(c, '$' | '`' | '\\')
                } else {
                    path_is_dynamic(c.encode_utf8(&mut [0; 4]))
                };
                value.push(c);
            }
        }
    }
    if !value.is_empty() {
        words.push(CdWord {
            value,
            dynamic,
            quoted,
        });
    }
    words
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
/// For a `cd` stage (`is_cd_stage`), the stage's own `cd` is skipped.
///
/// Precedence: a stage that is itself `pushd` or `popd` is `UnsupportedForm`,
/// whatever else it holds; any other hit is `Opaque`.
fn hidden_cd(raw: &str, is_cd_stage: bool) -> Option<CdUnresolved> {
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
            // The `cd` stage's own `cd` is not hidden: only what its operands
            // hold counts.
            if is_cd_stage
                && slice.program == "cd"
                && (target.contains("$(") || target.contains('`') || target.contains("<("))
            {
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

/// Whether `raw` (tokens `tokens`) may turn on Bash's physical directory mode
/// for the stages after it: `set -P`, `set -eP`, `set -o physical`, a `set`
/// with a word Aegis cannot read, or any mention of `SHELLOPTS`. This errs
/// toward true.
fn sets_physical_mode(raw: &str, tokens: &[String]) -> bool {
    if raw.contains("SHELLOPTS") {
        return true;
    }
    let Some(set_at) = tokens.iter().position(|token| token == "set") else {
        return false;
    };
    tokens[set_at + 1..].iter().any(|token| {
        token == "physical"
            || token.contains('$')
            || (token.starts_with('-') && !token.starts_with("--") && token.contains('P'))
    })
}
