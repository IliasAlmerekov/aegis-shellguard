use std::cell::{Cell, RefCell};

use aegis_parser::ListSeparator;

use super::git_stages::AmbientEnv;
use super::recursive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdUnresolved {
    UnsupportedForm,
    DynamicPath,
    CdPathSet,
    MayNotRun,
    Opaque,
    PhysicalMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageDir {
    Process,
    Chain(Vec<String>),
    Unresolved(CdUnresolved),
}

impl StageDir {
    fn degrade(&self, reason: CdUnresolved) -> StageDir {
        match self {
            StageDir::Unresolved(_) => self.clone(),
            _ => StageDir::Unresolved(reason),
        }
    }
}

#[must_use]
pub fn path_is_dynamic(path: &str) -> bool {
    path.contains(['$', '*', '?', '[', ']', '~', '`', '{', '}', '\\'])
}

const MAX_GROUP_DEPTH: usize = 8;

const CD_LIKE: &[&str] = &["cd", "pushd", "popd", "eval", "source", "."];

pub(super) fn mentions_cd(targets: &[String]) -> bool {
    targets.iter().any(|target| {
        if !(target.contains("cd") || target.contains("pushd") || target.contains("popd")) {
            return false;
        }
        let tokens = aegis_parser::split_tokens(target);
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        wrapped_cd_like(&tokens).is_some_and(|program| matches!(program, "cd" | "pushd" | "popd"))
            || aegis_parser::effective_token_slices(&refs)
                .iter()
                .any(|slice| matches!(slice.program, "cd" | "pushd" | "popd"))
    })
}

pub(super) struct Walk {
    pub(super) stages: Vec<(String, StageDir)>,
    pub(super) pipelines: Vec<(Vec<String>, StageDir)>,
}

pub(super) fn stage_dirs(cmd: &str, ambient: AmbientEnv) -> Walk {
    let mut stages = Vec::new();
    let walker = Walker {
        cdpath_set: Cell::new(ambient.cdpath_set),
        physical: Cell::new(ambient.physical),
        lastpipe: Cell::new(ambient.lastpipe),
        cd_function: ambient.cd_function,
        pipelines: RefCell::new(Vec::new()),
    };
    walker.list(cmd, StageDir::Process, &mut stages, 0);
    Walk {
        stages,
        pipelines: walker.pipelines.into_inner(),
    }
}

struct Walker {
    physical: Cell<bool>,
    pipelines: RefCell<Vec<(Vec<String>, StageDir)>>,
    cdpath_set: Cell<bool>,
    lastpipe: Cell<bool>,
    cd_function: bool,
}

impl Walker {
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
            if !single {
                let stages = segment
                    .pipeline
                    .segments
                    .iter()
                    .map(|stage| stage.raw.trim().to_string())
                    .collect();
                self.pipelines.borrow_mut().push((stages, state.clone()));
            }
            let mut next = state.clone();
            let last_index = segment.pipeline.segments.len() - 1;
            for (index, stage) in segment.pipeline.segments.iter().enumerate() {
                let effect = self.stage(stage.raw.trim(), &state, incoming, out, depth);
                let in_shell = single || (index == last_index && self.lastpipe.get());
                if in_shell && let Some(effect) = effect {
                    next = effect;
                }
            }
            state = advance(next, segment.separator);
            incoming = segment.separator;
        }
        state
    }

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
        let touches_cdpath =
            raw.contains("CDPATH") || tokens.iter().any(|token| token.contains("CDPATH"));
        let result = self.stage_inner(raw, &tokens, state, incoming, out);
        if touches_cdpath {
            self.cdpath_set.set(true);
        }
        if raw.contains("lastpipe") || raw.contains("BASHOPTS") {
            self.lastpipe.set(true);
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
            let operands = cd_operands(raw, tokens);
            if raw.contains("$(") || raw.contains('`') || raw.contains("<(") {
                let before = match hidden_cd(raw, true) {
                    Some(reason) => state.degrade(reason),
                    None => state.clone(),
                };
                out.push((raw.to_string(), before));
            }
            if self.cd_function {
                return Some(state.degrade(CdUnresolved::Opaque));
            }
            if self.physical.get() {
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

struct CdWord {
    value: String,
    dynamic: bool,
    quoted: bool,
    redirect: Option<(usize, bool)>,
}

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
                redirect: None,
                value: token.clone(),
            })
            .collect(),
    )
}

fn without_redirections(words: Vec<CdWord>) -> Vec<CdWord> {
    let mut kept = Vec::new();
    let mut skip_target = false;
    for word in words {
        if skip_target {
            skip_target = false;
            continue;
        }
        if let Some((at, prefix_dynamic)) = word.redirect
            && at > 0
            && !word.value[..at]
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'&')
        {
            let rest = word.value[at..].trim_start_matches(['<', '>', '&', '|']);
            skip_target = rest.is_empty();
            kept.push(CdWord {
                value: word.value[..at].to_string(),
                dynamic: prefix_dynamic,
                quoted: word.quoted,
                redirect: None,
            });
            continue;
        }
        if word.redirect.is_none() && word.quoted {
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

fn raw_words(raw: &str) -> Vec<CdWord> {
    let mut words = Vec::new();
    let mut value = String::new();
    let (mut dynamic, mut quoted) = (false, false);
    let mut redirect: Option<(usize, bool)> = None;
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
                        redirect: redirect.take(),
                    });
                }
            }
            c => {
                if !single && !double && matches!(c, '<' | '>') && redirect.is_none() {
                    redirect = Some((value.len(), dynamic));
                }
                dynamic |= if single {
                    false
                } else if double {
                    matches!(c, '$' | '`' | '\\')
                } else {
                    path_is_dynamic(c.encode_utf8(&mut [0; 4])) || matches!(c, '(' | ')')
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
            redirect,
        });
    }
    words
}

fn advance(state: StageDir, separator: Option<ListSeparator>) -> StageDir {
    match (&state, separator) {
        (StageDir::Chain(_), Some(sep)) if sep != ListSeparator::And => {
            StageDir::Unresolved(CdUnresolved::MayNotRun)
        }
        _ => state,
    }
}

fn hidden_cd(raw: &str, is_cd_stage: bool) -> Option<CdUnresolved> {
    let parsed = aegis_parser::Parser::parse(raw);
    let report = recursive::scan_targets(raw, &parsed);
    let mut reason = None;
    for target in &report.targets {
        let tokens = aegis_parser::split_tokens(target);
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let wrapped = wrapped_cd_like(&tokens);
        let slices = aegis_parser::effective_token_slices(&refs);
        let programs = slices.iter().map(|slice| slice.program).chain(wrapped);
        for program in programs {
            if !CD_LIKE.contains(&program) {
                continue;
            }
            if is_cd_stage
                && program == "cd"
                && (target.contains("$(") || target.contains('`') || target.contains("<("))
            {
                continue;
            }
            if target == raw && matches!(program, "pushd" | "popd") {
                return Some(CdUnresolved::UnsupportedForm);
            }
            reason = Some(CdUnresolved::Opaque);
        }
    }
    reason
}

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

fn wrapped_cd_like(tokens: &[String]) -> Option<&str> {
    let mut words = tokens
        .iter()
        .map(String::as_str)
        .skip_while(|token| !token.starts_with('-') && token.contains('='));
    let wrapper = words.next()?;
    let wrapper = wrapper.rsplit('/').next().unwrap_or(wrapper);
    if !(wrapper.eq_ignore_ascii_case("command") || wrapper.eq_ignore_ascii_case("builtin")) {
        return None;
    }
    words
        .find(|token| !token.starts_with('-'))
        .filter(|program| CD_LIKE.contains(program))
}
