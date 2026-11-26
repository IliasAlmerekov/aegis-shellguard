use std::cell::Cell;
use std::path::{Path, PathBuf};

use aegis_language::{SourceLanguage, SourceTarget};
use aegis_types::DegradationReason;

use super::AnalysisCwd;
use super::heredoc::{self, StdinRoute};
use super::source_reader::{self, SourceReadError};

mod segments;
use segments::{CwdState, HomeState, route_list_segment};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutedTarget {
    Inline {
        language: SourceLanguage,
        source: String,
    },
    ScriptFile {
        language: SourceLanguage,
        path: PathBuf,
    },
    Dynamic {
        language: SourceLanguage,
        reason: DegradationReason,
    },
    Unresolved {
        reason: DegradationReason,
    },
    DirectExec {
        path: PathBuf,
    },
    LauncherOperand {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedTarget {
    pub language: Option<SourceLanguage>,
    pub reason: DegradationReason,
}

pub(super) enum Resolution {
    #[cfg_attr(
        not(feature = "language-analysis"),
        expect(
            dead_code,
            reason = "fields are read only when language-analysis feeds the worker"
        )
    )]
    Resolved {
        language: SourceLanguage,
        source: String,
        source_hash: Option<String>,
        source_byte_offset: usize,
    },
    Degraded(DegradationReason),
    NotApplicable,
}

pub(super) async fn resolve_for_analysis(
    target: RoutedTarget,
    command_cwd: AnalysisCwd<'_>,
    script_file_limit_bytes: u64,
) -> Resolution {
    match target {
        RoutedTarget::Inline { language, source } => Resolution::Resolved {
            language,
            source,
            source_hash: None,
            source_byte_offset: 0,
        },
        RoutedTarget::ScriptFile { language, path } => {
            let path = match resolve_command_path(&path, command_cwd) {
                Ok(path) => path,
                Err(reason) => return Resolution::Degraded(reason),
            };
            match source_reader::read_script_file(&path, script_file_limit_bytes).await {
                Ok(read) => Resolution::Resolved {
                    language,
                    source: read.source,
                    source_hash: Some(read.source_hash),
                    source_byte_offset: read.source_byte_offset,
                },
                Err(err) => Resolution::Degraded(degradation_reason(&err)),
            }
        }
        RoutedTarget::Dynamic { reason, .. } => Resolution::Degraded(reason),
        RoutedTarget::Unresolved { reason } => Resolution::Degraded(reason),
        RoutedTarget::DirectExec { path } => {
            let path = match resolve_command_path(&path, command_cwd) {
                Ok(path) => path,
                Err(reason) => return Resolution::Degraded(reason),
            };
            resolve_verified_shebang_read(&path, script_file_limit_bytes).await
        }
        RoutedTarget::LauncherOperand { path } => {
            let path = match resolve_command_path(&path, command_cwd) {
                Ok(path) => path,
                Err(reason) => return Resolution::Degraded(reason),
            };
            if launcher_operand_is_missing_or_directory(&path).await {
                return Resolution::NotApplicable;
            }
            resolve_verified_shebang_read(&path, script_file_limit_bytes).await
        }
    }
}

async fn resolve_verified_shebang_read(path: &Path, script_file_limit_bytes: u64) -> Resolution {
    match source_reader::read_script_file(path, script_file_limit_bytes).await {
        Ok(read) => {
            let Some(language) = read
                .source
                .lines()
                .next()
                .and_then(verified_shebang_language)
            else {
                return Resolution::NotApplicable;
            };
            Resolution::Resolved {
                language,
                source: read.source,
                source_hash: Some(read.source_hash),
                source_byte_offset: read.source_byte_offset,
            }
        }
        Err(SourceReadError::TooLarge { .. } | SourceReadError::InvalidUtf8) => {
            Resolution::NotApplicable
        }
        Err(err) => Resolution::Degraded(degradation_reason(&err)),
    }
}

async fn launcher_operand_is_missing_or_directory(path: &Path) -> bool {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata.is_dir(),
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    }
}

fn resolve_command_path(
    path: &Path,
    command_cwd: AnalysisCwd<'_>,
) -> Result<PathBuf, DegradationReason> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    match command_cwd {
        AnalysisCwd::Resolved(cwd) => Ok(cwd.join(path)),
        AnalysisCwd::Unavailable => Err(DegradationReason::DynamicSource),
    }
}

struct Interpreter {
    program: &'static str,
    inline_flag: &'static str,
    language: SourceLanguage,
}

const INTERPRETERS: &[Interpreter] = &[
    Interpreter {
        program: "python3",
        inline_flag: "-c",
        language: SourceLanguage::Python,
    },
    Interpreter {
        program: "python",
        inline_flag: "-c",
        language: SourceLanguage::Python,
    },
    Interpreter {
        program: "bash",
        inline_flag: "-c",
        language: SourceLanguage::Bash,
    },
    Interpreter {
        program: "sh",
        inline_flag: "-c",
        language: SourceLanguage::Bash,
    },
    Interpreter {
        program: "node",
        inline_flag: "-e",
        language: SourceLanguage::JavaScript,
    },
    Interpreter {
        program: "nodejs",
        inline_flag: "-e",
        language: SourceLanguage::JavaScript,
    },
];

struct RouteContext<'a> {
    command: &'a str,
    trusted_aliases: &'a [(&'a str, &'a str)],
    home: Option<&'a Path>,
    alias_scope_end: Cell<usize>,
}

#[must_use]
pub fn route(command: &str, trusted_aliases: &[(&str, &str)]) -> Vec<RoutedTarget> {
    route_with_home(command, trusted_aliases, None)
}

#[must_use]
pub fn route_with_home(
    command: &str,
    trusted_aliases: &[(&str, &str)],
    home: Option<&Path>,
) -> Vec<RoutedTarget> {
    let canonical = aegis_parser::canonical_ansi_c_quoting_marked(command);
    let command = canonical.as_ref();
    let ctx = RouteContext {
        command,
        trusted_aliases,
        home,
        alias_scope_end: Cell::new(0),
    };
    let mut cwd = CwdState::Unset;
    let mut home = HomeState::Trusted;
    let mut targets = Vec::new();
    let mut search_from = 0;
    for segment in aegis_parser::list_segments(command) {
        search_from = segment_text_end(command, search_from, &segment.pipeline.raw);
        ctx.alias_scope_end.set(search_from);
        route_list_segment(&segment, &ctx, &mut cwd, &mut home, &mut targets, 0);
    }
    targets
}

fn segment_text_end(command: &str, search_from: usize, raw: &str) -> usize {
    command
        .get(search_from..)
        .and_then(|rest| rest.find(raw))
        .map_or(command.len(), |idx| search_from + idx + raw.len())
}

fn is_literal_path(path: &str) -> bool {
    !path.is_empty() && !path.contains(['$', '`', '*', '~', '?', '[', ']', '{', '}'])
}

fn direct_exec_route(program_token: &str) -> Option<RoutedTarget> {
    program_token
        .contains('/')
        .then(|| RoutedTarget::DirectExec {
            path: PathBuf::from(program_token),
        })
}

fn stdin_target(language: SourceLanguage, route: StdinRoute) -> RoutedTarget {
    match route {
        StdinRoute::Literal(source) => RoutedTarget::Inline { language, source },
        StdinRoute::Dynamic(reason) => RoutedTarget::Dynamic { language, reason },
    }
}

fn heredoc_write_then_exec_reuse(
    command: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<Vec<RoutedTarget>> {
    let first_line = command.lines().next()?;
    let (before_marker, after_marker) = aegis_parser::split_at_heredoc_marker(first_line)?;
    let write_path = heredoc_write_target(before_marker)?;

    let exec_part = after_marker.trim_start().strip_prefix("&&")?.trim_start();
    if exec_part.is_empty() {
        return None;
    }
    let exec_tokens = aegis_parser::split_tokens(exec_part);
    if exec_tokens
        .iter()
        .any(|tok| matches!(tok.as_str(), ";" | "&&" | "||" | "|"))
    {
        return None;
    }
    let exec_refs: Vec<&str> = exec_tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&exec_refs)
        .into_iter()
        .next()?;
    if slice.tokens.len() != 2 || Path::new(slice.tokens[1]) != write_path {
        return None;
    }
    let interp = resolve_interpreter(slice.program, trusted_aliases)?;

    let heredoc_body = aegis_parser::extract_heredoc_bodies(command)
        .into_iter()
        .next()?;
    let route = heredoc::classify(heredoc_body.body, heredoc_body.is_nowdoc);
    Some(vec![stdin_target(interp.language, route)])
}

fn heredoc_write_target(before_marker: &str) -> Option<PathBuf> {
    let tokens = aegis_parser::split_tokens(before_marker.trim());
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    match refs.as_slice() {
        ["cat", ">", path] | ["tee", path] => Some(PathBuf::from(*path)),
        _ => None,
    }
}

fn printf_percent_s_literal(segment: &str) -> Option<String> {
    let tokens = aegis_parser::split_tokens(segment);
    let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let slice = aegis_parser::effective_token_slices(&refs)
        .into_iter()
        .next()?;
    if slice.program != "printf" {
        return None;
    }
    let rest = &slice.tokens[1..];
    if rest.len() != 2 || rest[0] != "%s" {
        return None;
    }
    Some(rest[1].to_owned())
}

fn resolve_interpreter(
    program: &str,
    trusted_aliases: &[(&str, &str)],
) -> Option<&'static Interpreter> {
    if let Some(interp) = INTERPRETERS.iter().find(|i| i.program == program) {
        return Some(interp);
    }

    let base = strip_version_suffix(program);
    if base != program
        && let Some((_, canonical)) = BASENAME_FAMILIES.iter().find(|(family, _)| *family == base)
        && let Some(interp) = INTERPRETERS.iter().find(|i| i.program == *canonical)
    {
        return Some(interp);
    }

    let (_, canonical) = trusted_aliases
        .iter()
        .find(|(alias, _)| *alias == program)?;
    INTERPRETERS.iter().find(|i| i.program == *canonical)
}

const BASENAME_FAMILIES: &[(&str, &str)] = &[("python", "python3"), ("node", "node")];

fn strip_version_suffix(name: &str) -> &str {
    let mut end = name.len();
    for (idx, ch) in name.char_indices().rev() {
        if ch.is_ascii_digit() || ch == '.' {
            end = idx;
        } else {
            break;
        }
    }
    &name[..end]
}

fn inline_body(tok: &str, flag: &str, rest: &[&str], pos: usize) -> Option<String> {
    if tok == flag {
        return rest.get(pos + 1).map(|s| (*s).to_owned());
    }
    let stripped = tok.strip_prefix(flag)?;
    if stripped.is_empty() || stripped.starts_with('-') {
        return None;
    }
    Some(stripped.to_owned())
}

pub async fn resolve(
    routed: Vec<RoutedTarget>,
    script_file_limit_bytes: u64,
) -> Vec<Result<SourceTarget, UnresolvedTarget>> {
    let mut results = Vec::with_capacity(routed.len());
    for target in routed {
        if let Some(result) = resolve_one(target, script_file_limit_bytes).await {
            results.push(result);
        }
    }
    results
}

async fn resolve_one(
    target: RoutedTarget,
    script_file_limit_bytes: u64,
) -> Option<Result<SourceTarget, UnresolvedTarget>> {
    match target {
        RoutedTarget::Inline { language, source } => Some(Ok(SourceTarget { language, source })),
        RoutedTarget::ScriptFile { language, path } => {
            Some(
                match source_reader::read_script_file(&path, script_file_limit_bytes).await {
                    Ok(read) => Ok(SourceTarget {
                        language,
                        source: read.source,
                    }),
                    Err(err) => Err(UnresolvedTarget {
                        language: Some(language),
                        reason: degradation_reason(&err),
                    }),
                },
            )
        }
        RoutedTarget::Dynamic { language, reason } => Some(Err(UnresolvedTarget {
            language: Some(language),
            reason,
        })),
        RoutedTarget::Unresolved { reason } => Some(Err(UnresolvedTarget {
            language: None,
            reason,
        })),
        RoutedTarget::DirectExec { path } | RoutedTarget::LauncherOperand { path } => {
            let read = source_reader::read_script_file(&path, script_file_limit_bytes)
                .await
                .ok()?;
            let first_line = read.source.lines().next().unwrap_or("");
            let language = verified_shebang_language(first_line)?;
            Some(Ok(SourceTarget {
                language,
                source: read.source,
            }))
        }
    }
}

fn degradation_reason(err: &SourceReadError) -> DegradationReason {
    match err {
        SourceReadError::NotFound
        | SourceReadError::NotRegularFile
        | SourceReadError::PermissionDenied
        | SourceReadError::Io(_) => DegradationReason::UnsafeSource,
        SourceReadError::TooLarge { .. } => DegradationReason::LimitExceeded,
        SourceReadError::InvalidUtf8 => DegradationReason::UnsupportedEncoding,
    }
}

#[must_use]
pub fn verified_shebang_language(first_line: &str) -> Option<SourceLanguage> {
    let rest = first_line.strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    let mut program = words.next()?;
    if program.ends_with("/env") || program == "env" {
        program = words.next()?;
    }
    let program = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    resolve_interpreter(program, &[]).map(|interp| interp.language)
}

#[cfg(test)]
mod tests;
