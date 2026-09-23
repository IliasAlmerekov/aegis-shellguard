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

mod wrappers;
use wrappers::{posix_function_definition_body, wrapper_bodies};

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

/// Resolve already-tokenized `owned_tokens` to its `&str` view and its first
/// effective-program slice — the `Vec<&str>` conversion plus
/// `effective_token_slices().next()` idiom every stage-routing call site
/// that resolves one program per stage repeats (issue #384/#430). Takes
/// a borrow rather than calling `aegis_parser::split_tokens` itself so a
/// caller that still needs the raw `&str` tokens afterward (an `env -C`/cwd
/// scan past the program, say) can keep them; one that doesn't just ignores
/// the first element of the pair.
fn effective_stage_slice<'a>(
    owned_tokens: &'a [String],
) -> (Vec<&'a str>, Option<aegis_parser::EffectiveTokenSlice<'a>>) {
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&tokens)
        .into_iter()
        .next();
    (tokens, slice)
}

/// Recognize a segment (or pipeline stage) as a construct that changes the
/// cwd, through the same launcher/assignment stripping the router uses for
/// programs (`builtin cd`, `command cd`, `X=1 cd` all resolve to `cd`), and
/// report its effect: `Literal` only for the exact `cd -- <path>` shape,
/// `Degraded` for every other `cd`/`pushd`/`popd` shape and for `source`/`.`
/// (an opaque script may `cd` on its own). Deliberately does *not* look
/// through a `{...}`/`(...)` wrapper — a wrapper that also routes to
/// something else must still have that something routed, which this narrow
/// token check cannot tell apart from a bare cwd change;
/// [`route_wrapped_stage`] handles a wrapper's own cwd effect instead.
fn parse_cd_like(stage_raw: &str) -> Option<CwdState> {
    let owned_tokens = aegis_parser::split_tokens(stage_raw);
    let (_tokens, slice) = effective_stage_slice(&owned_tokens);
    let slice = slice?;

    match slice.program {
        "cd" => Some(match slice.tokens.get(1..) {
            Some([dashdash, path]) if *dashdash == "--" && is_literal_path(path) => {
                CwdState::Literal(PathBuf::from(*path))
            }
            _ => CwdState::Degraded,
        }),
        "pushd" | "popd" | "source" | "." => Some(CwdState::Degraded),
        _ => None,
    }
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
        (other, _) => other,
    }
}

/// Route one top-level [`aegis_parser::ListSegment`], mutating `cwd` for the
/// next segment and appending any produced targets (rebased/degraded per the
/// current cwd state) to `targets`, left to right, in order. Also the engine
/// [`route_wrapped_stage`] recurses into for a wrapper's own body, so a `cd`
/// nested behind a subshell, brace group, or reserved word folds with the
/// exact same rules a top-level walk uses (issue #384).
pub(super) fn route_list_segment(
    segment: &aegis_parser::ListSegment,
    trusted_aliases: &[(&str, &str)],
    cwd: &mut CwdState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    let stages = &segment.pipeline.segments;

    if stages.len() == 1 {
        if let Some(effect) = parse_cd_like(&stages[0].raw) {
            *cwd = fold_cd(cwd, effect, segment.separator);
            return;
        }

        route_stage(&stages[0].raw, trusted_aliases, cwd, targets, depth);
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
        if parse_cd_like(&stage.raw).is_some() {
            pipeline_had_cd = true;
            continue;
        }

        let mut scratch_cwd = cwd.clone();
        let wrapped_before = wrapped_stage_targets.len();
        route_wrapped_stage(
            &stage.raw,
            trusted_aliases,
            &mut scratch_cwd,
            &mut wrapped_stage_targets,
            depth,
        );
        let wrapped_produced = wrapped_stage_targets.len() > wrapped_before;
        if scratch_cwd != *cwd {
            pipeline_had_cd = true;
        }

        let routed = route_direct_stage(&stage.raw, trusted_aliases);
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
    trusted_aliases: &[(&str, &str)],
    cwd: &mut CwdState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    for target in route_direct_stage(stage_raw, trusted_aliases) {
        push_unique(targets, apply_cwd(target, cwd));
    }
    route_wrapped_stage(stage_raw, trusted_aliases, cwd, targets, depth);
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
/// (ADR-022 §6, issue #384).
fn route_wrapped_stage(
    stage_raw: &str,
    trusted_aliases: &[(&str, &str)],
    cwd: &mut CwdState,
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

    for body in wrapper_bodies(stage_raw, trusted_aliases) {
        let mut body_cwd = cwd.clone();
        let mut body_targets = Vec::new();
        for segment in aegis_parser::list_segments(&body) {
            route_list_segment(
                &segment,
                trusted_aliases,
                &mut body_cwd,
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
    }
}

/// Resolve `stage` (one pipeline stage's raw text) to its own route without
/// looking through any grammar wrapper: the narrow heredoc-write-then-exec
/// reuse shape first (when `stage` owns a heredoc marker), then explicit
/// interpreter inline/file/redirection argv walk, then heredoc/here-string
/// stdin fallback, then a bare path-like direct-exec candidate.
fn route_direct_stage(stage: &str, trusted_aliases: &[(&str, &str)]) -> Vec<RoutedTarget> {
    if command_has_heredoc(stage)
        && let Some(targets) = heredoc_write_then_exec_reuse(stage, trusted_aliases)
    {
        return targets;
    }

    let owned_tokens = aegis_parser::split_tokens(stage);
    if owned_tokens.is_empty() {
        return Vec::new();
    }
    // A redirection anywhere before the program (`>out python3 x.py`,
    // `FOO=1 >out python3 x.py`, `env >out python3 x.py`) is shell syntax
    // attached to the stage, not an argument of the program that follows it
    // — the shell strips it before argv0 resolution, so routing must too.
    // `aegis_parser::effective_token_slices` handles this at every step
    // (issue #384), not only a redirection at position 0.
    let (tokens, slice) = effective_stage_slice(&owned_tokens);
    let Some(slice) = slice else {
        return Vec::new();
    };

    let effective_start = tokens.len() - slice.tokens.len();
    // `env -C DIR`/`--chdir[=]DIR` changes the cwd for that one child
    // process only, not the shell's own — a relative target must degrade
    // rather than resolve against the shell's own cwd (issue #384).
    let env_cwd = if env_chdir_prefix(&tokens[..effective_start]) {
        CwdState::Degraded
    } else {
        CwdState::Unset
    };

    let Some(interp) = resolve_interpreter(slice.program, trusted_aliases) else {
        let routed = direct_exec_route(tokens[effective_start]);
        return routed
            .into_iter()
            .map(|target| apply_cwd(target, &env_cwd))
            .collect();
    };

    let rest = &slice.tokens[1..];
    let routed = match walk_interpreter_argv(interp, rest) {
        ArgvWalk::Routed(target) => vec![target],
        ArgvWalk::NoSource => Vec::new(),
        ArgvWalk::NoMatch => {
            if let Some(stdin_route) =
                heredoc::heredoc_stdin(stage).or_else(|| heredoc::here_string_stdin(rest))
            {
                vec![stdin_target(interp.language, stdin_route)]
            } else if let Some(path) = leading_stdin_redirect_target(&tokens[..effective_start]) {
                vec![RoutedTarget::ScriptFile {
                    language: interp.language,
                    path: PathBuf::from(path),
                }]
            } else {
                Vec::new()
            }
        }
    };
    routed
        .into_iter()
        .map(|target| apply_cwd(target, &env_cwd))
        .collect()
}

/// `true` when `prefix` — the tokens routing consumed before the effective
/// program (assignments, launcher words, redirections) — is an `env`
/// invocation carrying `-C`/`--chdir` (issue #384). A cheap token-
/// equality scan, not a full re-parse of `env`'s own option grammar: routing
/// only needs to know a chdir flag is present somewhere in the prefix it
/// already resolved, not its exact position.
fn env_chdir_prefix(prefix: &[&str]) -> bool {
    let Some(first) = prefix.first() else {
        return false;
    };
    let basename = first.rsplit('/').next().unwrap_or(first);
    if !basename.eq_ignore_ascii_case("env") {
        return false;
    }
    prefix[1..]
        .iter()
        .any(|tok| *tok == "-C" || *tok == "--chdir" || tok.starts_with("--chdir="))
}

/// Resolve `stage` to its interpreter only if it is bare (program token
/// only, no flags or arguments of its own) — the shape that means "reads
/// piped stdin from the previous stage" rather than "has its own source".
fn bare_stage_interpreter(
    stage: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<&'static Interpreter> {
    let owned_tokens = aegis_parser::split_tokens(stage);
    let (_tokens, slice) = effective_stage_slice(&owned_tokens);
    let slice = slice?;
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

    // A standalone `< file` with a literal target means the interpreter
    // reads its script from stdin, and stdin is exactly that file (issue
    // #384): `python3 < ./evil.py` is the same source as `python3 - <
    // ./evil.py`. Recorded here and only consulted if the walk below finds
    // no inline body or positional script argument of its own — either of
    // those wins outright, same as a real interpreter's own argv parsing.
    let mut stdin_redirect_target: Option<&str> = None;

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
        if aegis_parser::is_redirection_operator(tok) {
            // A spaced-out redirection (`> file`, `2> file`, `>> file`) has
            // its target in the *next* token, which the interpreter never
            // sees either — skip both, not just the operator, or the target
            // filename would be misread as the script argument.
            if is_plain_input_redirect(tok)
                && let Some(target) = before_marker.get(pos + 1)
                && is_literal_path(target)
            {
                stdin_redirect_target = Some(target);
            }
            pos += 2;
            continue;
        }
        if let Some(target) = glued_plain_input_redirect_target(tok)
            && is_literal_path(target)
        {
            // A redirection with no space before its filename (`<file`,
            // `0<file`) is the same stdin source as the spaced form above,
            // just glued into one token by the tokenizer (issue #384).
            stdin_redirect_target = Some(target);
            pos += 1;
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

    match stdin_redirect_target {
        Some(path) => ArgvWalk::Routed(RoutedTarget::ScriptFile {
            language: interp.language,
            path: PathBuf::from(path),
        }),
        None => ArgvWalk::NoMatch,
    }
}

/// `true` for a standalone plain input redirection (`<`, `3<`, …) — an
/// [`aegis_parser::is_redirection_operator`] token with no `>` and no
/// fd-duplication `&`, the only shape whose target can mean "this file is
/// the interpreter's stdin source" (issue #384).
fn is_plain_input_redirect(tok: &str) -> bool {
    tok.trim_start_matches(|c: char| c.is_ascii_digit()) == "<"
}

/// The literal target of a plain input redirection glued to its own token
/// with no separating space (`<file`, `0<file`) — the same shape
/// [`is_plain_input_redirect`] recognizes when spaced out, but the
/// tokenizer keeps this one glued because nothing splits it (issue #384).
/// `None` for anything else: a duplication/dup-fd form (`<&3`), a
/// heredoc/here-string marker (`<<`, `<<<`, already excluded upstream by
/// the marker-boundary scan), an output redirection, or an empty target.
fn glued_plain_input_redirect_target(tok: &str) -> Option<&str> {
    let after_fd = tok.trim_start_matches(|c: char| c.is_ascii_digit());
    let target = after_fd.strip_prefix('<')?;
    (!target.is_empty() && !target.starts_with(['<', '&', '>'])).then_some(target)
}

/// The literal target of a plain input redirection sitting *before* the
/// program (`<./evil.py python3`, `< ./evil.py python3`) — the shell
/// resolves stdin from it the same way regardless of which side of the
/// program name it sits on, but only the trailing form is visible to
/// [`walk_interpreter_argv`], which only ever sees `rest` (the tokens
/// *after* the program). Scans left to right and returns the first match,
/// spaced or glued (issue #384).
fn leading_stdin_redirect_target<'a>(prefix: &[&'a str]) -> Option<&'a str> {
    let mut pos = 0;
    while pos < prefix.len() {
        let tok = prefix[pos];
        if aegis_parser::is_redirection_operator(tok) {
            if is_plain_input_redirect(tok)
                && let Some(target) = prefix.get(pos + 1)
                && is_literal_path(target)
            {
                return Some(target);
            }
            pos += 2;
            continue;
        }
        if let Some(target) = glued_plain_input_redirect_target(tok)
            && is_literal_path(target)
        {
            return Some(target);
        }
        pos += 1;
    }
    None
}
