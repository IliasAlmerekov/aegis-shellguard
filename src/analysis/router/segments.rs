//! List-segment, pipeline-stage, and cwd-tracking routing (issue #384, ADR-022
//! §6). A child module of [`super`] (`router.rs`): every private item there is
//! visible here via `use super::*`, exactly as `router::tests` already relies
//! on for its own tests.
//!
//! [`super::route`] looks past a command's first effective token: this module
//! makes every top-level list segment of a compound command
//! ([`aegis_parser::list_segments`]) route independently — so
//! `true; python3 evil.py` still routes the second segment — and every stage
//! of a multi-stage pipeline within a segment, while tracking cwd changes
//! (`cd`/`pushd`/`popd`/`source`/`.`) across consecutive segments *and*
//! inside any grammar wrapper's own body, via the single recursive walk in
//! [`route_wrapped_stage`] (issue #384).

use super::*;

mod argv_walk;
mod direct;
mod dynamic_program;
mod executor_config;
mod runner;
mod unclaimed;
mod wrappers;
use argv_walk::{
    ArgvWalk, bare_stage_interpreter, env_chdir_prefix, leading_stdin_redirect_target,
    walk_interpreter_argv,
};
use direct::route_direct_stage;
use dynamic_program::{alias_value, is_dynamic_program_word};
use executor_config::{
    assignment_stage_executor_routes, env_prefix_executor_route, option_value_executor_route,
    skip_assignment_keyword,
};
use runner::{
    bare_python_script_route, opaque_runner_before_run, opaque_runner_option,
    package_executable_uncertain,
};
use unclaimed::unclaimed_interpreter_net;
use wrappers::{posix_function_definition_body, strip_trailing_redirection, wrapper_bodies};

/// Bound on wrapper-peeling recursion. Each level of
/// [`route_wrapped_stage`] re-scans its own (shrinking) body with
/// [`aegis_parser::list_segments`] and the wrapper-detection substring scans,
/// so an unbounded pathological nest such as `((((...))))` cost work
/// proportional to the *sum* of every level's remaining string length —
/// quadratic in nesting depth, and deep enough to blow the call stack before
/// that cost even mattered. Past this many levels, routing stops peeling and
/// records degradation instead of recursing further. Reads
/// `OrchestrationBudget::L1_DEFAULT.max_depth` (ADR-022 §7's cross-language
/// recursion-depth ceiling) rather than duplicating that number here.
const MAX_WRAP_DEPTH: u32 = crate::analysis::OrchestrationBudget::L1_DEFAULT.max_depth;

/// `true` when `command` contains a genuine heredoc marker (`<<WORD`,
/// `<<'WORD'`, `<<-WORD`, …) — never the unrelated single-line here-string
/// operator `<<<`, which `aegis_parser::extract_heredoc_bodies` already does
/// not match. Guarded by a cheap substring check first so the overwhelmingly
/// common heredoc-free command pays only that scan (route stays allocation-
/// light on the hot path, per `CONVENTION.md` §8).
pub(super) fn command_has_heredoc(command: &str) -> bool {
    command.contains("<<") && !aegis_parser::extract_heredoc_bodies(command).is_empty()
}

/// The router's one cwd-tracking type (ADR-022 §6): both the state threaded
/// across list segments (top-level and inside a wrapper body alike) and the
/// effect a single recognized `cd`-like segment has on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CwdState {
    /// No cwd effect is in play: relative targets are routed unchanged
    /// (resolved later against the real process/command cwd).
    Unset,
    /// A literal path is known to be the cwd for every segment routed under
    /// this state.
    Literal(PathBuf),
    /// A cwd-changing construct was seen whose effect cannot be trusted
    /// (dynamic path, wrong operator, a crossed separator, an unknown
    /// `source`d script, …) — every later relative target degrades, and this
    /// state never recovers.
    Degraded,
}

/// The router's HOME-tracking state (decision D1, GHSA-xj54), threaded
/// across list segments the same way [`CwdState`] is: `~/rest` in a later
/// [`unclaimed_interpreter_net`] launcher operand may use `ctx.home` only
/// while this stays `Trusted`. Unlike `CwdState`, there is no trusted "new
/// home" for a later segment to resolve against — an assignment or an
/// opaque construct only ever withholds the caller-supplied home, never
/// substitutes a different one — so this carries no `Literal`-shaped
/// variant and, once `Degraded`, never recovers, regardless of which
/// `Command separator` follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HomeState {
    /// No segment routed so far assigns, removes, or opaquely changes
    /// `HOME`: a later `~/rest` launcher operand may trust `ctx.home`.
    Trusted,
    /// An earlier segment assigned or removed `HOME`, or ran something the
    /// router cannot see inside (`eval`/`source`/`.`) — every later
    /// `~/rest` launcher operand degrades instead of trusting `ctx.home`.
    Degraded,
}

/// `true` when `tok` would itself assign or remove `HOME` if it ran as
/// shell text: a bare `HOME=value` assignment, or the plain `HOME` name an
/// `export`/`unset`/`read` argument list carries (decision D1, GHSA-xj54).
/// Used by [`unclaimed_interpreter_net`]'s operand scan, so a wrapper handed
/// this same shape as its own payload (`eval "HOME=/tmp/e"`) degrades
/// instead of reading it as an ordinary path-like candidate just because it
/// contains a `/`.
pub(super) fn token_is_home_assignment(tok: &str) -> bool {
    tok == "HOME" || tok.starts_with("HOME=")
}

/// Resolve already-tokenized `owned_tokens` to its `&str` view, its first
/// effective-program slice, and whether resolution ran into
/// `aegis_parser`'s `env -S`/`--split-string` nesting bound before it got
/// there — the `Vec<&str>` conversion plus
/// `effective_token_slices_checked().0.next()` idiom every stage-routing
/// call site that resolves one program per stage repeats (issue #384/#430;
/// truncation flag added #437 review, finding F1). Takes a borrow rather
/// than calling `aegis_parser::split_tokens` itself so a caller that still
/// needs the raw `&str` tokens afterward (an `env -C`/cwd scan past the
/// program, say) can keep them; a caller that needs neither the tokens nor
/// the truncation flag just ignores them.
fn effective_stage_slice<'a>(
    owned_tokens: &'a [String],
) -> (
    Vec<&'a str>,
    Option<aegis_parser::EffectiveTokenSlice<'a>>,
    bool,
) {
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    let (slices, truncated) = aegis_parser::effective_token_slices_checked(&tokens);
    let slice = slices.into_iter().next();
    (tokens, slice, truncated)
}

/// Recognize a segment (or pipeline stage) as a construct that changes the
/// cwd, through the same launcher/assignment stripping the router uses for
/// programs (`builtin cd`, `command cd`, `X=1 cd` all resolve to `cd`), and
/// report its effect: `Literal` only for the exact `cd -- <path>` shape,
/// `Degraded` for every other `cd`/`pushd`/`popd` shape and for `source`/`.`/
/// `eval` (an opaque script, or an arbitrary `eval`uated `cd`, may change the
/// cwd on its own — GHSA-xj54). Deliberately does *not* look
/// through a `{...}`/`(...)` wrapper — a wrapper that also routes to
/// something else must still have that something routed, which this narrow
/// token check cannot tell apart from a bare cwd change;
/// [`route_wrapped_stage`] handles a wrapper's own cwd effect instead.
fn parse_cd_like(stage_raw: &str) -> Option<CwdState> {
    let owned_tokens = aegis_parser::split_tokens(stage_raw);
    let (_tokens, slice, _truncated) = effective_stage_slice(&owned_tokens);
    let slice = slice?;

    match slice.program {
        "cd" => Some(match slice.tokens.get(1..) {
            Some([dashdash, path]) if *dashdash == "--" && is_literal_path(path) => {
                CwdState::Literal(PathBuf::from(*path))
            }
            _ => CwdState::Degraded,
        }),
        "pushd" | "popd" | "source" | "." | "eval" => Some(CwdState::Degraded),
        _ => None,
    }
}

/// `true` when `name` is a valid POSIX shell identifier: a leading letter or
/// underscore, then only alphanumerics or underscores. The one copy —
/// `executor_config` and `unclaimed` reach it as `super::is_shell_identifier`
/// rather than each keeping its own local copy of this three-line predicate,
/// since a private item in a module is already visible to its descendants.
fn is_shell_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `true` when `tok` writes a shell variable: `NAME=value`, `NAME+=value`,
/// or `NAME[idx]=value`. The `[idx]` and value text are read as opaque —
/// only the name ahead of `=` (and an optional trailing `+` or `[...]`
/// index) is validated.
fn is_assignment_token(tok: &str) -> bool {
    let Some((lhs, _value)) = tok.split_once('=') else {
        return false;
    };
    let lhs = lhs.strip_suffix('+').unwrap_or(lhs);
    let name = match lhs.split_once('[') {
        Some((name, rest)) if rest.ends_with(']') => name,
        Some(_) => return false,
        None => lhs,
    };
    is_shell_identifier(name)
}

/// `true` when every non-redirection token in `tokens` is an
/// [`is_assignment_token`] write, and at least one exists: a stage made only
/// of variable assignments, with or without a redirection glued in among
/// them (`HOME=/tmp/e 2>/dev/null`). The one "is this stage nothing but
/// assignments" check [`stage_degrades_home_trust`],
/// [`executor_config::assignment_stage_executor_routes`], and
/// [`unclaimed::is_pure_assignment_stage`] each used to duplicate
/// independently, none of them tolerating a redirection token (issue
/// #384/#430, GHSA-xj54, Rule A).
pub(super) fn is_assignment_only_stage(tokens: &[&str]) -> bool {
    let mut saw_assignment = false;
    let mut index = 0;
    while index < tokens.len() {
        let tok = tokens[index];
        if aegis_parser::starts_with_redirection_glyph(tok) {
            index += if aegis_parser::is_redirection_operator(tok) {
                2
            } else {
                1
            };
            continue;
        }
        if !is_assignment_token(tok) {
            return false;
        }
        saw_assignment = true;
        index += 1;
    }
    saw_assignment
}

/// Programs whose stage degrades [`HomeState`] whatever their own arguments
/// say (Rule A, GHSA-xj54): each can write, remove, or opaquely change shell
/// state — including `HOME` — in a way this router cannot see inside.
/// `printf` is handled separately in [`stage_degrades_home_trust`]: only
/// `-v` writes a variable, so a plain `printf '%s' x` is left off this list.
const HOME_OPAQUE_PROGRAMS: &[&str] = &[
    "read",
    "mapfile",
    "readarray",
    "getopts",
    "declare",
    "typeset",
    "local",
    "export",
    "readonly",
    "unset",
    "let",
    "eval",
    "source",
    ".",
];

/// `true` when `raw_tokens` opens a `for`/`select` loop header (`for HOME in
/// ...`, `select HOME in ...`): the loop variable is as much a write as an
/// assignment is (Rule A, GHSA-xj54). Checked on the stage's own first
/// token rather than through launcher-prefix stripping — neither reserved
/// word is itself a program a launcher would wrap.
pub(super) fn is_for_or_select_header(raw_tokens: &[&str]) -> bool {
    matches!(raw_tokens.first(), Some(&"for") | Some(&"select"))
}

/// Recognize a segment (or pipeline stage) whose effect on `HOME` a later
/// `~/rest` launcher operand cannot trust (Rule A, GHSA-xj54): a stage made
/// only of variable assignments ([`is_assignment_only_stage`]), a program
/// from [`HOME_OPAQUE_PROGRAMS`] (or `printf` carrying `-v`), or a
/// `for`/`select` loop header. Resolved through the same effective-program
/// logic [`parse_cd_like`] uses, so `builtin eval`, `command source`, and
/// `time export ...` all count.
///
/// This replaces the previous argument-level search for the literal name
/// `HOME`: any earlier variable-writing stage now withholds trust, not only
/// one that names `HOME` — routing cannot statically rule out an indirect
/// effect (an accepted false positive: `export PATH=/x; setsid ~/pyx` now
/// prompts even though `PATH` is not `HOME`).
fn stage_degrades_home_trust(stage_raw: &str) -> bool {
    let owned_tokens = aegis_parser::split_tokens(stage_raw);
    let (raw_tokens, slice, _truncated) = effective_stage_slice(&owned_tokens);

    if is_for_or_select_header(&raw_tokens) {
        return true;
    }

    if let Some(slice) = &slice {
        if slice.program == "printf" {
            // `-vNAME` (glued) writes a variable exactly as `-v NAME`
            // (spaced) does — getopts-style short-option gluing, round-3
            // review finding 4 — so either shape counts, not only the exact
            // `-v` token.
            return slice.tokens[1..].iter().any(|tok| tok.starts_with("-v"));
        }
        if HOME_OPAQUE_PROGRAMS.contains(&slice.program) {
            return true;
        }
    }

    is_assignment_only_stage(&raw_tokens)
}

/// Fold a recognized cwd-changing segment's effect into the cwd state carried
/// to the next segment. Only an unbroken `&&` chain trusts the result at all;
/// a relative literal joins onto the current `Literal` base (or becomes the
/// base outright from `Unset`), an absolute literal replaces it outright, and
/// `Degraded` — on either side — never recovers (ADR-022 §6).
fn fold_cd(
    current: &CwdState,
    effect: CwdState,
    separator: Option<aegis_parser::ListSeparator>,
) -> CwdState {
    if separator != Some(aegis_parser::ListSeparator::And) {
        return CwdState::Degraded;
    }
    match (current, effect) {
        (CwdState::Degraded, _) => CwdState::Degraded,
        (_, CwdState::Literal(new_path)) if new_path.is_absolute() => CwdState::Literal(new_path),
        (CwdState::Literal(base), CwdState::Literal(new_path)) => {
            CwdState::Literal(base.join(new_path))
        }
        (CwdState::Unset, CwdState::Literal(new_path)) => CwdState::Literal(new_path),
        _ => CwdState::Degraded,
    }
}

/// Carry `state` across the list operator following the segment it was just
/// applied to. A trusted `Literal` cwd survives only an unbroken `&&` chain;
/// crossing any other operator (`;`, `||`, background `&`, newline — or a
/// pipeline's `|`, handled by the caller before this is reached) degrades it.
/// `Unset` and `Degraded` are unaffected by any separator.
fn advance_across_separator(
    state: CwdState,
    separator: Option<aegis_parser::ListSeparator>,
) -> CwdState {
    match state {
        CwdState::Literal(_) if separator != Some(aegis_parser::ListSeparator::And) => {
            CwdState::Degraded
        }
        other => other,
    }
}

/// Rebase a target's relative path onto the current cwd state, or degrade it
/// when the cwd is unknown or untrusted. Absolute paths are unaffected
/// either way. A relative [`RoutedTarget::DirectExec`] retains an untyped
/// degradation: its language is only knowable from a shebang that must not
/// be read from an unknown cwd (ADR-022 §6, Iteration 10 P7).
fn apply_cwd(target: RoutedTarget, cwd: &CwdState) -> RoutedTarget {
    match (target, cwd) {
        (t, CwdState::Unset) => t,
        (RoutedTarget::ScriptFile { language, path }, CwdState::Literal(base))
            if path.is_relative() =>
        {
            RoutedTarget::ScriptFile {
                language,
                path: base.join(path),
            }
        }
        (RoutedTarget::ScriptFile { language, path }, CwdState::Degraded) if path.is_relative() => {
            RoutedTarget::Dynamic {
                language,
                reason: DegradationReason::DynamicSource,
            }
        }
        (RoutedTarget::DirectExec { path }, CwdState::Literal(base)) if path.is_relative() => {
            RoutedTarget::DirectExec {
                path: base.join(path),
            }
        }
        (RoutedTarget::DirectExec { path }, CwdState::Degraded) if path.is_relative() => {
            RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }
        }
        (RoutedTarget::LauncherOperand { path }, CwdState::Literal(base)) if path.is_relative() => {
            RoutedTarget::LauncherOperand {
                path: base.join(path),
            }
        }
        (RoutedTarget::LauncherOperand { path }, CwdState::Degraded) if path.is_relative() => {
            RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }
        }
        (other, _) => other,
    }
}

/// Route one top-level [`aegis_parser::ListSegment`], mutating `cwd` for the
/// next segment and appending any produced targets (rebased/degraded per the
/// current cwd state) to `targets`, left to right, in order. Also the engine
/// [`route_wrapped_stage`] recurses into for a wrapper's own body, so a `cd`
/// nested behind a subshell, brace group, or reserved word folds with the
/// exact same rules a top-level walk uses (issue #384). Mutates `home`
/// alongside `cwd`, left to right in the same pass, so a later segment's
/// `~/rest` launcher operand sees whether an earlier one already put `HOME`
/// out of reach (decision D1, GHSA-xj54).
pub(super) fn route_list_segment(
    segment: &aegis_parser::ListSegment,
    ctx: &RouteContext<'_>,
    cwd: &mut CwdState,
    home: &mut HomeState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    let stages = &segment.pipeline.segments;

    if stages.len() == 1 {
        // Checked ahead of, and independently from, the cd-like short
        // circuit below: `source`/`.` matches both, and unlike a cd-like
        // match this never skips the stage's own routing (decision D1,
        // GHSA-xj54).
        if stage_degrades_home_trust(&stages[0].raw) {
            *home = HomeState::Degraded;
        }
        if let Some(effect) = parse_cd_like(&stages[0].raw) {
            *cwd = fold_cd(cwd, effect, segment.separator);
            return;
        }

        route_stage(&stages[0].raw, ctx, cwd, home, targets, depth);
        let current = std::mem::replace(cwd, CwdState::Unset);
        *cwd = advance_across_separator(current, segment.separator);
        return;
    }

    // A multi-stage pipeline: route every stage. A `cd`-like stage produces
    // no target of its own, but running in a pipeline subshell means its
    // outcome (and the pipeline's own cwd afterward) is never trustworthy
    // (ADR-022 §6) — every relative target in this same pipeline degrades,
    // and so does everything after it, same as crossing `;`/`||`/`&`. A
    // wrapper hidden in a stage is walked with its own scratch cwd (seeded
    // from the pipeline's cwd on entry) for the same reason: whatever it
    // finds inside is already resolved, and a cd effect there also taints
    // the whole pipeline.
    let mut pipeline_had_cd = false;
    let mut stage_targets = Vec::new();
    let mut wrapped_stage_targets = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        if stage_degrades_home_trust(&stage.raw) {
            *home = HomeState::Degraded;
        }
        if parse_cd_like(&stage.raw).is_some() {
            pipeline_had_cd = true;
            continue;
        }

        let mut scratch_cwd = cwd.clone();
        let mut scratch_home = *home;
        let wrapped_before = wrapped_stage_targets.len();
        route_wrapped_stage(
            &stage.raw,
            ctx,
            &mut scratch_cwd,
            &mut scratch_home,
            &mut wrapped_stage_targets,
            depth,
        );
        let wrapped_produced = wrapped_stage_targets.len() > wrapped_before;
        if scratch_cwd != *cwd {
            pipeline_had_cd = true;
        }
        if scratch_home == HomeState::Degraded {
            *home = HomeState::Degraded;
        }

        let routed = route_direct_stage(&stage.raw, ctx.trusted_aliases);
        if !routed.is_empty() {
            // A stage with its own script file, inline flag, or direct-exec
            // path routes exactly as it would standalone (ADR-022 §6),
            // whatever its position in the pipeline.
            stage_targets.extend(routed);
            continue;
        }
        if wrapped_produced {
            continue;
        }
        // A bare interpreter (no args of its own) reads whatever the
        // previous stage writes to stdout — only meaningful past the first
        // stage, which has no preceding producer to read from.
        if index > 0
            && let Some(bare_interp) = bare_stage_interpreter(&stage.raw, ctx.trusted_aliases)
        {
            // Only the narrow, exactly-two-stage `printf '%s' <literal> |
            // <interp>` shape has a statically recoverable producer; every
            // other producer (including a longer chain) is honestly Dynamic
            // rather than evaluated or guessed at (ADR-022 §6).
            if index == 1
                && stages.len() == 2
                && let Some(literal) = printf_percent_s_literal(&stages[0].raw)
            {
                stage_targets.push(RoutedTarget::Inline {
                    language: bare_interp.language,
                    source: literal,
                });
            } else {
                stage_targets.push(RoutedTarget::Dynamic {
                    language: bare_interp.language,
                    reason: DegradationReason::DynamicSource,
                });
            }
            continue;
        }
        // Nothing else claimed this stage: fall back to the fail-closed net
        // (issue #384/#430, ADR-022 §6 amendment) for a wrapper word the
        // launcher list does not enumerate.
        stage_targets.extend(unclaimed_interpreter_net(&stage.raw, ctx, *home));
    }

    if pipeline_had_cd {
        *cwd = CwdState::Degraded;
    }
    for target in stage_targets {
        push_unique(targets, apply_cwd(target, cwd));
    }
    // Already resolved against `scratch_cwd` above — pushed as-is, never
    // rebased a second time against the pipeline's own (possibly now
    // Degraded) cwd.
    for target in wrapped_stage_targets {
        push_unique(targets, target);
    }
    let current = std::mem::replace(cwd, CwdState::Unset);
    *cwd = advance_across_separator(current, segment.separator);
}

/// Add `target` to `targets` unless it is already present — list segments are
/// routed independently, so an identical route (e.g. the same script invoked
/// twice) would otherwise be duplicated.
fn push_unique(targets: &mut Vec<RoutedTarget>, target: RoutedTarget) {
    if !targets.contains(&target) {
        targets.push(target);
    }
}

/// Route one non-pipeline stage's own text: direct interpreter/file/
/// redirection argv routing, plus anything hidden behind a grammar wrapper.
/// A wrapper body's targets are already resolved through its own nested cwd
/// walk, so [`route_wrapped_stage`] pushes them into `targets` directly; only
/// the direct-routed targets still need `apply_cwd` against the caller's
/// current `cwd` here.
fn route_stage(
    stage_raw: &str,
    ctx: &RouteContext<'_>,
    cwd: &mut CwdState,
    home: &mut HomeState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    let direct = route_direct_stage(stage_raw, ctx.trusted_aliases);
    let mut claimed = !direct.is_empty();
    for target in direct {
        push_unique(targets, apply_cwd(target, cwd));
    }

    let mut wrapped_targets = Vec::new();
    route_wrapped_stage(stage_raw, ctx, cwd, home, &mut wrapped_targets, depth);
    claimed |= !wrapped_targets.is_empty();
    for target in wrapped_targets {
        push_unique(targets, target);
    }

    // Nothing else claimed this stage: fall back to the fail-closed net
    // (issue #384/#430, ADR-022 §6 amendment) for a wrapper word the
    // launcher list does not enumerate.
    if !claimed {
        for net_target in unclaimed_interpreter_net(stage_raw, ctx, *home) {
            push_unique(targets, apply_cwd(net_target, cwd));
        }
    }
}

/// Reserved words that can open a stage without being its own command
/// (grammar wrappers, issue #430) beyond `aegis_parser::COMMAND_STARTING_KEYWORDS`
/// (the shared list `strip_leading_shell_syntax` also reads): `case` and
/// `function` open a wrapper `stage_may_be_wrapped` must see through but
/// don't keep the *next* word in command position the way that list's own
/// members do, and `coproc` is this router's own addition for the same
/// reason.
const WRAPPER_ONLY_RESERVED_WORDS: &[&str] = &["case", "function", "coproc"];

/// `true` when `word` prefixes `trimmed` as a whole reserved word, not just a
/// literal substring (`iffy` must not match `if`).
fn stage_starts_with_reserved_word(trimmed: &str, word: &str) -> bool {
    trimmed
        .strip_prefix(word)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// `true` when `stage_raw` may hide a routable command behind shell grammar
/// [`route_direct_stage`] cannot see through: a subshell, a brace group, a
/// command substitution/backtick, or a reserved-word prefix. Cheap prefix/
/// substring checks only, so the no-wrapper hot path stays allocation-light
/// (issue #430, `CONVENTION.md` §8).
fn stage_may_be_wrapped(stage_raw: &str) -> bool {
    let trimmed = stage_raw.trim_start();
    trimmed.starts_with('(')
        || trimmed.starts_with('{')
        || stage_raw.contains("$(")
        || stage_raw.contains('`')
        || stage_raw.contains("<(")
        || stage_raw.contains(">(")
        || aegis_parser::COMMAND_STARTING_KEYWORDS
            .iter()
            .chain(WRAPPER_ONLY_RESERVED_WORDS)
            .any(|kw| stage_starts_with_reserved_word(trimmed, kw))
        || (command_has_heredoc(stage_raw) && heredoc_marker_line_tail(stage_raw).is_some())
        || posix_function_definition_body(trimmed).is_some()
}

/// The chained-command tail glued onto a heredoc marker's own physical line
/// (issue #384) — e.g. `&& python3 ./evil.py` in `cat <<A && python3
/// ./evil.py`, or `; python3 ./evil.py` in `cat <<A; python3 ./evil.py`.
/// `aegis_parser::list_segments` deliberately keeps this glued to the
/// heredoc-owning segment (the body itself must stay data, not further
/// segments, and the marker's own line must stay one unit for
/// `heredoc_write_then_exec_reuse` below); this walks that same tail and
/// routes it, since `route_direct_stage` cannot — that text never resolves
/// to a routable program on its own. `None` when `stage_raw` owns no
/// heredoc marker, or nothing follows the marker(s) on its opening line.
fn heredoc_marker_line_tail(stage_raw: &str) -> Option<&str> {
    let first_line = stage_raw.lines().next()?;
    let (_, mut after) = aegis_parser::split_at_heredoc_marker(first_line)?;
    while let Some((_, next_after)) = aegis_parser::split_at_heredoc_marker(after) {
        // A further stacked marker (`cat <<A <<B; ...`) on the same line —
        // keep walking past every marker before looking for the tail.
        after = next_after;
    }
    let trimmed = after.trim_start();
    let without_operator = trimmed
        .strip_prefix("&&")
        .or_else(|| trimmed.strip_prefix("||"))
        .or_else(|| trimmed.strip_prefix(';'))
        .unwrap_or(trimmed)
        .trim_start();
    (!without_operator.is_empty()).then_some(without_operator)
}

/// Route targets hidden behind a grammar wrapper (issue #430): a subshell,
/// brace group, command substitution/backtick, or reserved-word prefix. Each
/// [`wrapper_bodies`] entry is real shell source, so it is walked exactly
/// like a top-level command — split into [`aegis_parser::list_segments`] and
/// routed through [`route_list_segment`] with its own scratch `body_cwd`,
/// seeded from the caller's `cwd` on entry so a `cd` inside the wrapper joins
/// onto whatever cwd was already known, not a blank slate. Recursing back
/// into wrapper detection is safe: a wrapper body is always strictly shorter
/// than the text it was peeled from.
///
/// The wrapper's own targets are pushed as the body walk already resolved
/// them. If the body's final cwd differs from what it started with, some
/// cwd-changing construct ran inside it: a brace group or reserved-word body
/// runs in the *current* shell, so that persists to whatever follows this
/// wrapper and the caller's `cwd` must degrade to match. A subshell's or
/// `$(...)`'s cd does not persist, but degrading here anyway is the safe
/// direction — one mechanism instead of a per-wrapper-kind special case
/// (ADR-022 §6, issue #384). `body_home` is seeded and folded back the same
/// way: once anything inside the body degrades it, the caller's `home` never
/// recovers either (decision D1, GHSA-xj54).
fn route_wrapped_stage(
    stage_raw: &str,
    ctx: &RouteContext<'_>,
    cwd: &mut CwdState,
    home: &mut HomeState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    if !stage_may_be_wrapped(stage_raw) {
        return;
    }

    if depth >= MAX_WRAP_DEPTH {
        // Something is still hidden behind this wrapper that routing refuses
        // to keep peeling into (issue #384): recursing further would cost
        // work proportional to the whole remaining string at every
        // additional level, and deep enough nesting overflows the call stack
        // outright. Degrade honestly rather than silently treat the
        // unexamined body as safe (fail-closed, CONVENTION.md §2).
        push_unique(
            targets,
            RoutedTarget::Unresolved {
                reason: DegradationReason::LimitExceeded,
            },
        );
        return;
    }

    for body in wrapper_bodies(stage_raw, ctx.trusted_aliases) {
        let mut body_cwd = cwd.clone();
        let mut body_home = *home;
        let mut body_targets = Vec::new();
        for segment in aegis_parser::list_segments(&body) {
            route_list_segment(
                &segment,
                ctx,
                &mut body_cwd,
                &mut body_home,
                &mut body_targets,
                depth + 1,
            );
        }
        for target in body_targets {
            push_unique(targets, target);
        }
        if body_cwd != *cwd {
            *cwd = CwdState::Degraded;
        }
        if body_home == HomeState::Degraded {
            *home = HomeState::Degraded;
        }
    }
}
