//! List-segment, pipeline-stage, and cwd-tracking routing (issue #384, ADR-022
//! §6). A child module of [`super`] (`router.rs`): every private item there is
//! visible here via `use super::*`, exactly as `router::tests` already relies
//! on for its own tests.
//!
//! [`super::route`] used to look only at a command's first effective token, so
//! `true; python3 evil.py` routed nothing. This module makes every top-level
//! list segment of a compound command ([`aegis_parser::list_segments`]) route
//! independently, and every stage of a multi-stage pipeline within a segment,
//! while tracking the one cwd change ADR-022 §6 allows (`cd -- <path> &&`) across
//! consecutive segments.

use super::*;

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
/// across top-level list segments and the effect a single recognized `cd`
/// segment has on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CwdState {
    /// No `cd`/`pushd`/`popd` effect is in play: relative targets are routed
    /// unchanged (resolved later against the real process/command cwd).
    Unset,
    /// A literal path is known to be the cwd for every segment routed under
    /// this state.
    Literal(PathBuf),
    /// A `cd`/`pushd`/`popd` was seen whose effect on the cwd cannot be
    /// trusted (dynamic path, wrong operator, a crossed separator, or a
    /// group/subshell wrapper this router cannot see into) — every later
    /// relative target degrades, and this state never recovers.
    Degraded,
}

/// Recognize a segment (or pipeline stage) as a `cd`/`pushd`/`popd`
/// invocation, through the same launcher/assignment stripping the router
/// uses for programs (`effective_token_slices` — `builtin cd`, `command cd`,
/// `X=1 cd` all resolve to `cd`), and report its cwd effect: `Literal` only
/// for the exact `cd -- <path>` shape with no globs/expansions, `Degraded`
/// for every other cd/pushd/popd shape — including one found anywhere inside
/// a `{ ...; }` group or `(...)` subshell wrapper, since whether it persists
/// to the caller's cwd depends on which wrapper it is, and this router
/// cannot resolve that without a real shell (ADR-022 §6).
fn parse_cd_like(stage_raw: &str) -> Option<CwdState> {
    if let Some(inner) = strip_group_or_subshell_wrapper(stage_raw) {
        return contains_cd_like_token(inner).then_some(CwdState::Degraded);
    }

    let owned_tokens = aegis_parser::split_tokens(stage_raw);
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&tokens)
        .into_iter()
        .next()?;

    match slice.program {
        "cd" => Some(match slice.tokens.get(1..) {
            Some([dashdash, path]) if *dashdash == "--" && is_literal_path(path) => {
                CwdState::Literal(PathBuf::from(*path))
            }
            _ => CwdState::Degraded,
        }),
        "pushd" | "popd" => Some(CwdState::Degraded),
        _ => None,
    }
}

/// `Some(inner)` when `raw`, trimmed, is wholly wrapped in a `{ ...; }` group
/// or a `(...)` subshell.
fn strip_group_or_subshell_wrapper(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .or_else(|| trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')')))
}

/// `true` when any top-level segment inside `inner` resolves to a
/// `cd`/`pushd`/`popd` effective program.
fn contains_cd_like_token(inner: &str) -> bool {
    aegis_parser::list_segments(inner).iter().any(|segment| {
        segment.pipeline.segments.iter().any(|stage| {
            let owned_tokens = aegis_parser::split_tokens(&stage.raw);
            let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
            aegis_parser::effective_token_slices(&tokens)
                .into_iter()
                .next()
                .is_some_and(|slice| matches!(slice.program, "cd" | "pushd" | "popd"))
        })
    })
}

/// Fold a recognized `cd`/`pushd`/`popd` segment's effect into the cwd state
/// carried to the next segment. Only an unbroken `&&` chain trusts the
/// result at all; a relative literal joins onto the current `Literal` base
/// (or becomes the base outright from `Unset`), an absolute literal replaces
/// it outright, and `Degraded` — on either side — never recovers (ADR-022 §6).
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
fn apply_cwd(target: RoutedTarget, cwd: &CwdState) -> Option<RoutedTarget> {
    match (target, cwd) {
        (t, CwdState::Unset) => Some(t),
        (RoutedTarget::ScriptFile { language, path }, CwdState::Literal(base))
            if path.is_relative() =>
        {
            Some(RoutedTarget::ScriptFile {
                language,
                path: base.join(path),
            })
        }
        (RoutedTarget::ScriptFile { language, path }, CwdState::Degraded) if path.is_relative() => {
            Some(RoutedTarget::Dynamic {
                language,
                reason: DegradationReason::DynamicSource,
            })
        }
        (RoutedTarget::DirectExec { path }, CwdState::Literal(base)) if path.is_relative() => {
            Some(RoutedTarget::DirectExec {
                path: base.join(path),
            })
        }
        (RoutedTarget::DirectExec { path }, CwdState::Degraded) if path.is_relative() => {
            Some(RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            })
        }
        (other, _) => Some(other),
    }
}

/// Route one top-level [`aegis_parser::ListSegment`], mutating `cwd` for the
/// next segment and appending any produced targets (rebased/degraded per the
/// current cwd state) to `targets`, left to right, in order.
pub(super) fn route_list_segment(
    segment: &aegis_parser::ListSegment,
    trusted_aliases: &[(&str, &str)],
    cwd: &mut CwdState,
    targets: &mut Vec<RoutedTarget>,
) {
    let stages = &segment.pipeline.segments;

    if stages.len() == 1 {
        if let Some(effect) = parse_cd_like(&stages[0].raw) {
            *cwd = fold_cd(cwd, effect, segment.separator);
            return;
        }

        for target in route_single_stage(&stages[0].raw, trusted_aliases) {
            if let Some(routed) = apply_cwd(target, cwd) {
                push_unique(targets, routed);
            }
        }
        let current = std::mem::replace(cwd, CwdState::Unset);
        *cwd = advance_across_separator(current, segment.separator);
        return;
    }

    // A multi-stage pipeline: route every stage. A `cd`-like stage produces
    // no target of its own, but running in a pipeline subshell means its
    // outcome (and the pipeline's own cwd afterward) is never trustworthy
    // (ADR-022 §6) — every relative target in this same pipeline degrades,
    // and so does everything after it, same as crossing `;`/`||`/`&`.
    let mut pipeline_had_cd = false;
    let mut stage_targets = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        if parse_cd_like(&stage.raw).is_some() {
            pipeline_had_cd = true;
            continue;
        }

        let routed = route_single_stage(&stage.raw, trusted_aliases);
        if !routed.is_empty() {
            // A stage with its own script file, inline flag, or direct-exec
            // path routes exactly as it would standalone (ADR-022 §6),
            // whatever its position in the pipeline.
            stage_targets.extend(routed);
            continue;
        }
        if index == 0 {
            // No preceding stage to pipe stdin from.
            continue;
        }
        let Some(bare_interp) = bare_stage_interpreter(&stage.raw, trusted_aliases) else {
            continue;
        };
        // A bare interpreter reads whatever the previous stage writes to
        // stdout. Only the narrow, exactly-two-stage `printf '%s' <literal> |
        // <interp>` shape has a statically recoverable producer; every other
        // producer (including a longer chain) is honestly Dynamic rather than
        // evaluated or guessed at (ADR-022 §6).
        if index == 1
            && stages.len() == 2
            && let Some(literal) = printf_percent_s_literal(&stages[0].raw)
        {
            stage_targets.push(RoutedTarget::Inline {
                language: bare_interp.language,
                source: literal,
            });
            continue;
        }
        stage_targets.push(RoutedTarget::Dynamic {
            language: bare_interp.language,
            reason: DegradationReason::DynamicSource,
        });
    }

    if pipeline_had_cd {
        *cwd = CwdState::Degraded;
    }
    for target in stage_targets {
        if let Some(routed) = apply_cwd(target, cwd) {
            push_unique(targets, routed);
        }
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

/// Resolve `stage` (one pipeline stage's raw text) to its own route: the
/// narrow heredoc-write-then-exec reuse shape first (when `stage` owns a
/// heredoc marker), then explicit interpreter inline/file/redirection argv
/// walk, then heredoc/here-string stdin fallback, then a bare path-like
/// direct-exec candidate. The 2-stage `producer | interp` fallback is the
/// caller's concern (see [`route_list_segment`]'s multi-stage branch).
fn route_single_stage(stage: &str, trusted_aliases: &[(&str, &str)]) -> Vec<RoutedTarget> {
    if command_has_heredoc(stage)
        && let Some(targets) = heredoc_write_then_exec_reuse(stage, trusted_aliases)
    {
        return targets;
    }

    let owned_tokens = aegis_parser::split_tokens(stage);
    if owned_tokens.is_empty() {
        return Vec::new();
    }
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();

    let Some(slice) = aegis_parser::effective_token_slices(&tokens)
        .into_iter()
        .next()
    else {
        return Vec::new();
    };

    let Some(interp) = resolve_interpreter(slice.program, trusted_aliases) else {
        let effective_start = tokens.len() - slice.tokens.len();
        return direct_exec_route(tokens[effective_start])
            .into_iter()
            .collect();
    };

    let rest = &slice.tokens[1..];
    match walk_interpreter_argv(interp, rest) {
        ArgvWalk::Routed(target) => vec![target],
        ArgvWalk::NoSource => Vec::new(),
        ArgvWalk::NoMatch => {
            if let Some(stdin_route) =
                heredoc::heredoc_stdin(stage).or_else(|| heredoc::here_string_stdin(rest))
            {
                vec![stdin_target(interp.language, stdin_route)]
            } else {
                Vec::new()
            }
        }
    }
}

/// Resolve `stage` to its interpreter only if it is bare (program token
/// only, no flags or arguments of its own) — the shape that means "reads
/// piped stdin from the previous stage" rather than "has its own source".
fn bare_stage_interpreter(
    stage: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<&'static Interpreter> {
    let owned_tokens = aegis_parser::split_tokens(stage);
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&tokens)
        .into_iter()
        .next()?;
    if slice.tokens.len() > 1 {
        return None;
    }
    resolve_interpreter(slice.program, trusted_aliases)
}

/// The result of [`walk_interpreter_argv`] walking one interpreter
/// invocation's own argv.
pub(super) enum ArgvWalk {
    /// An inline body or a script-file argument was found.
    Routed(RoutedTarget),
    /// The interpreter's inline flag was present but carried no body — a
    /// definitive "not a source target", never falling back to stdin.
    NoSource,
    /// Nothing in argv itself routed; the caller decides its own stdin
    /// (heredoc/here-string) fallback.
    NoMatch,
}

/// Walk an interpreter's own argv (`rest`, the effective token slice after
/// the program token) exactly as the interpreter itself would: it keeps
/// consuming flags (including the inline `-c`/`-e` body, which wins
/// immediately) and shell redirections (which the shell strips before exec —
/// the interpreter never sees them) until it hits the first positional
/// (non-flag, non-redirection) token, which is the script file and ends
/// option parsing right there — any flag-shaped token *after* it belongs to
/// the script's own argv, not the interpreter, and must not be misread as the
/// interpreter's inline flag (ADR-022 §6).
///
/// The single interpreter-argv walk shared by every routing call site.
pub(super) fn walk_interpreter_argv(interp: &Interpreter, rest: &[&str]) -> ArgvWalk {
    // The tokenizer has no heredoc-boundary awareness, so tokens *after* a
    // `<<WORD`/`<<<` marker are the heredoc/here-string *body*, not further
    // command arguments. Both the inline-flag scan and the file-argument scan
    // below must stop at the marker, or a crafted heredoc body could be
    // misread as the interpreter's own flag/argument instead of being
    // classified as stdin.
    let marker_pos = rest.iter().position(|tok| tok.starts_with("<<"));
    let before_marker = marker_pos.map_or(rest, |idx| &rest[..idx]);

    let mut pos = 0;
    while pos < before_marker.len() {
        let tok = before_marker[pos];
        if let Some(source) = inline_body(tok, interp.inline_flag, before_marker, pos) {
            if source.is_empty() {
                // Flag present but no inline body to analyze — not a source target.
                return ArgvWalk::NoSource;
            }
            return ArgvWalk::Routed(RoutedTarget::Inline {
                language: interp.language,
                source,
            });
        }
        if is_redirection_operator(tok) {
            // A spaced-out redirection (`> file`, `2> file`, `>> file`) has
            // its target in the *next* token, which the interpreter never
            // sees either — skip both, not just the operator, or the target
            // filename would be misread as the script argument.
            pos += 2;
            continue;
        }
        if !tok.starts_with('-') && !tok.contains('<') && !tok.contains('>') {
            return ArgvWalk::Routed(RoutedTarget::ScriptFile {
                language: interp.language,
                path: PathBuf::from(tok),
            });
        }
        pos += 1;
    }

    ArgvWalk::NoMatch
}

/// A standalone shell redirection operator token (`>`, `>>`, `<`, `2>`, …) —
/// an optional leading file-descriptor number followed by nothing but `<`/`>`
/// characters. A glued form (`>out.txt`, `2>&1`) is not standalone — it
/// carries its own target in the same token and needs no extra token
/// skipped, so it is deliberately excluded here.
fn is_redirection_operator(tok: &str) -> bool {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    // `&>`/`&>>` (bash's combined stdout+stderr redirection) carry one
    // leading `&` before the `<`/`>` run; a glued fd-duplication form like
    // `>&2`/`2>&1` has `&` *after* the `<`/`>` instead and is deliberately
    // left unmatched here — it carries its own target in the same token, so
    // the generic "contains `<`/`>`" fallback already skips just that one
    // token, which is correct.
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    !after_amp.is_empty() && after_amp.chars().all(|c| c == '<' || c == '>')
}
