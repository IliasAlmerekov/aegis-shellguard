//! Bounded v1 shape detection for `Effect-opaque execution` (ADR-016).
//!
//! Detects three shapes without filesystem `stat()`: pipe-to-shell (`... | sh`),
//! interpreter or `source`/`.` script-file execution (`python3 ./x.py`), and
//! shell stdin execution (`sh -s`). Inline bodies (`-c`/`-e`) are scanned
//! recursively and are not effect-opaque.
//!
//! Unlisted value-consuming interpreter options may spoof the script-file slot;
//! this fail-safe v1 limit (ADR-016) only adds a recovery snapshot and never
//! blocks or raises `RiskLevel`.

use aegis_parser::{PipelineChain, effective_program, logical_segments, split_tokens};
use aegis_types::ParsedCommand;

/// Shell interpreters that run a script file named in argv (v1 bounded set).
const SHELL_INTERPRETERS: &[&str] = &["sh", "bash", "zsh"];
/// Language interpreters that run a script file named in argv (v1 bounded set).
const LANG_INTERPRETERS: &[&str] = &["python", "python3", "node", "ruby", "perl"];
/// Shell builtins that execute a file into the current shell.
const SOURCE_BUILTINS: &[&str] = &["source", "."];
/// The inline-script flag that makes an interpreter invocation *not*
/// effect-opaque: the body is extracted and scanned recursively, so the
/// effect is visible to the scanner. Maps interpreter → inline flag.
const INLINE_FLAGS: &[(&str, &str)] = &[
    ("sh", "-c"),
    ("bash", "-c"),
    ("zsh", "-c"),
    ("python", "-c"),
    ("python3", "-c"),
    ("node", "-e"),
    ("ruby", "-e"),
    ("perl", "-e"),
];
/// File extensions that mark an argv token as a script file even without a `/`.
const SCRIPT_EXTENSIONS: &[&str] = &[
    ".sh", ".bash", ".zsh", ".py", ".js", ".mjs", ".cjs", ".rb", ".pl",
];
/// Interpreter options that consume the next argv token as their value (ADR-016).
///
/// Bounded v1 table; unlisted options fail safe (extra recovery snapshot).
/// `=`-form options are single tokens and need no entry.
const VALUE_CONSUMING_OPTIONS: &[(&str, &[&str])] = &[
    (
        "node",
        &[
            "-r",
            "--require",
            "--loader",
            "--experimental-loader",
            "--import",
            "--watch-path",
        ],
    ),
    ("python", &["-m", "-W", "-X"]),
    ("python3", &["-m", "-W", "-X"]),
    ("ruby", &["-r", "-I", "-C"]),
    ("perl", &["-m", "-M", "-I"]),
    // sh/bash/zsh: `-c` (inline) and `-s` (stdin) are the only argv-shaping
    // options; neither takes a path-like separate argument that could spoof
    // the script-file slot, so no value-consuming set is needed.
];

/// Whether `command` is an `Effect-opaque execution` shape (ADR-016).
///
/// `pipelines` is the optional top-level pipeline decomposition of `cmd` (only
/// computed when `cmd` contains `|`); it is reused for pipe-to-shell detection
/// rather than re-parsing.
pub(super) fn detect(
    cmd: &str,
    command: &ParsedCommand,
    pipelines: Option<&[PipelineChain]>,
) -> bool {
    if let Some(chains) = pipelines
        && pipe_to_shell(chains)
    {
        return true;
    }

    // Cheap pre-filter: skip the segment walk when no interpreter/source token
    // appears anywhere in the normalized command. This keeps the safe hot path
    // (the common case) allocation-free beyond the single token scan.
    if !has_potential_shape(&command.normalized) {
        return false;
    }

    logical_segments(cmd)
        .iter()
        .any(|seg| segment_is_effect_opaque(seg))
}

/// Pipe-to-shell: any pipeline window whose right-hand segment is a shell sink.
fn pipe_to_shell(chains: &[PipelineChain]) -> bool {
    for chain in chains {
        for window in chain.segments.windows(2) {
            if super::pipeline_semantics::is_shell_sink(&window[1].normalized) {
                return true;
            }
        }
    }
    false
}

/// Cheap, allocation-free pre-filter: does the normalized command contain any
/// interpreter or source-builtin token?
///
/// Scans whitespace tokens (the normalized form is already free of quoting
/// noise) and compares each case-insensitively against the interpreter/source
/// sets via `eq_ignore_ascii_case` — no `String` allocation, no `to_lowercase`.
/// Token-level (not substring) so `wash` ≠ `sh`. This is a superset of the real
/// per-segment check for the v1 forms, so it never under-reports; it only gates
/// the heavier `logical_segments` walk, keeping the safe hot path cheap.
fn has_potential_shape(normalized: &str) -> bool {
    normalized.split_whitespace().any(|token| {
        SHELL_INTERPRETERS
            .iter()
            .any(|i| token.eq_ignore_ascii_case(i))
            || LANG_INTERPRETERS
                .iter()
                .any(|i| token.eq_ignore_ascii_case(i))
            || SOURCE_BUILTINS
                .iter()
                .any(|i| token.eq_ignore_ascii_case(i))
    })
}

/// Per-segment shape check: resolve the effective program (launcher-stripped,
/// basename-normalized) and decide whether the segment is effect-opaque.
fn segment_is_effect_opaque(segment: &str) -> bool {
    let tokens = split_tokens(segment);
    let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let Some(program) = effective_program(&token_refs).map(str::to_ascii_lowercase) else {
        return false;
    };

    if is_source_builtin(&program) {
        return has_script_file_token(&tokens);
    }

    if is_interpreter(&program) {
        return interpreter_invocation_is_effect_opaque(&program, &tokens);
    }

    false
}

fn is_shell_interpreter(program: &str) -> bool {
    SHELL_INTERPRETERS.contains(&program)
}

fn is_lang_interpreter(program: &str) -> bool {
    LANG_INTERPRETERS.contains(&program)
}

fn is_source_builtin(program: &str) -> bool {
    SOURCE_BUILTINS.contains(&program)
}

fn is_interpreter(program: &str) -> bool {
    is_shell_interpreter(program) || is_lang_interpreter(program)
}

/// Decide effect-opacity for an interpreter invocation by locating its
/// execution mode from argv position (ADR-016 / Standards #2).
///
/// The inline flag (`-c` / `-e`) is the interpreter's execution-mode flag only
/// when it precedes the first positional (non-option) argument: then the body
/// is the next token, already extracted and scanned recursively, so the effect
/// is visible and the command is NOT effect-opaque. When a positional argument
/// that looks like a script file (`./x.py`, `cleanup.sh`) precedes the inline
/// flag, the interpreter runs that file and the flag is a script argument — so
/// the command IS effect-opaque. `sh -s` reads and executes stdin (no body in
/// argv) and is effect-opaque.
fn interpreter_invocation_is_effect_opaque(program: &str, tokens: &[String]) -> bool {
    let inline_flag = INLINE_FLAGS
        .iter()
        .find(|(interp, _)| *interp == program)
        .map(|(_, flag)| *flag);
    let value_consuming = VALUE_CONSUMING_OPTIONS
        .iter()
        .find(|(interp, _)| *interp == program)
        .map(|(_, options)| *options);

    // Walk argv after the program, locating the first inline flag and the first
    // positional argument by index. A value-consuming option eats its next
    // token (that token is the option's argument, not a positional), so skip
    // past it — otherwise a path-like value such as `--require ./preload.js`
    // spoofs the script-file slot.
    let mut inline_idx = None;
    let mut first_positional_idx = None;
    let mut idx = 1;
    while idx < tokens.len() {
        let token = tokens[idx].as_str();
        if inline_flag.is_some_and(|flag| token == flag) {
            if inline_idx.is_none() {
                inline_idx = Some(idx);
            }
        } else if value_consuming.is_some_and(|options| options.contains(&token)) {
            // Skip the option and its separate-argument value.
            idx += 2;
            continue;
        } else if !token.starts_with('-') && first_positional_idx.is_none() {
            first_positional_idx = Some(idx);
        }
        idx += 1;
    }

    // Inline mode wins only when the flag precedes the first positional — the
    // body is then the next token, extracted and scanned recursively.
    if let Some(idx) = inline_idx
        && first_positional_idx.is_none_or(|positional| idx < positional)
    {
        return false;
    }

    // Otherwise a leading positional that looks like a script file is the
    // executed payload — effect-opaque.
    if let Some(positional) = first_positional_idx
        && is_script_file_token(&tokens[positional])
    {
        return true;
    }

    // Shell stdin form: `sh -s` reads and executes stdin, whose body is not in
    // argv — effect-opaque.
    is_shell_interpreter(program) && tokens.iter().any(|token| token == "-s")
}

/// Whether any non-option argv token looks like a script file: contains a path
/// separator or has a known script extension. Interpreter/source program tokens
/// never match this predicate, so the program itself is never mistaken for the
/// script file.
fn has_script_file_token(tokens: &[String]) -> bool {
    tokens
        .iter()
        .filter(|token| !token.starts_with('-'))
        .any(|token| is_script_file_token(token))
}

fn is_script_file_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.contains('/') || SCRIPT_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}
