use aegis_parser::{PipelineChain, effective_program, logical_segments, split_tokens};
use aegis_types::ParsedCommand;

const SHELL_INTERPRETERS: &[&str] = &["sh", "bash", "zsh"];
const LANG_INTERPRETERS: &[&str] = &["python", "python3", "node", "ruby", "perl"];
const SOURCE_BUILTINS: &[&str] = &["source", "."];
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
const SCRIPT_EXTENSIONS: &[&str] = &[
    ".sh", ".bash", ".zsh", ".py", ".js", ".mjs", ".cjs", ".rb", ".pl",
];
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
];

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

    if !has_potential_shape(&command.normalized) {
        return false;
    }

    logical_segments(cmd)
        .iter()
        .any(|seg| segment_is_effect_opaque(seg))
}

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

fn interpreter_invocation_is_effect_opaque(program: &str, tokens: &[String]) -> bool {
    let inline_flag = INLINE_FLAGS
        .iter()
        .find(|(interp, _)| *interp == program)
        .map(|(_, flag)| *flag);
    let value_consuming = VALUE_CONSUMING_OPTIONS
        .iter()
        .find(|(interp, _)| *interp == program)
        .map(|(_, options)| *options);

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
            idx += 2;
            continue;
        } else if !token.starts_with('-') && first_positional_idx.is_none() {
            first_positional_idx = Some(idx);
        }
        idx += 1;
    }

    if let Some(idx) = inline_idx
        && first_positional_idx.is_none_or(|positional| idx < positional)
    {
        return false;
    }

    if let Some(positional) = first_positional_idx
        && is_script_file_token(&tokens[positional])
    {
        return true;
    }

    is_shell_interpreter(program) && tokens.iter().any(|token| token == "-s")
}

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
