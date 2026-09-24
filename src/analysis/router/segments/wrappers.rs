//! Wrapper-body peeling for `case`/`function`/`coproc` and grammar-wrapper
//! constructs (issue #384, issue #430), split out of [`super`]
//! (`segments.rs`) to keep that file under `CONVENTION.md`'s file-size gate.
//! A descendant of [`super::super`] (`router.rs`) as much as `segments` is:
//! every private item there is visible here via `use super::*`, the same
//! pattern `segments.rs`'s own doc comment describes for `router::tests`.

use super::*;

/// `Some(inner)` when `raw`, trimmed, is wholly wrapped in a `{ ...; }` group
/// or a `(...)` subshell. Used only as [`wrapper_bodies`]'s brace-group
/// fallback ([`aegis_parser::unwrap_subshell_group`] already covers `(...)`).
fn strip_group_or_subshell_wrapper(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .or_else(|| trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')')))
}

/// Every `case` arm body in `rest` (already past the leading `case ` keyword),
/// one entry per arm, raw-substring only — a tokenize/rejoin pass here would
/// corrode an inline body's quoting the same way `logical_segments` does.
/// Splits on each arm terminator (`;;`, `;;&`, `;&`, issue #384: a
/// fallthrough arm's body must route exactly like a normal arm's) and strips
/// the trailing `esac` off the last arm. Imprecise on a case word containing
/// the literal substring `" in "`, or a body whose own text happens to embed
/// one of these terminator sequences outside any arm boundary; that only
/// means routing misses or misdraws an arm, the same fail-open gap
/// `route_direct_stage` already has for any command it cannot parse.
fn case_arm_bodies(rest: &str) -> Vec<String> {
    let Some((_word, mut remaining)) = rest.split_once(" in ") else {
        return Vec::new();
    };
    let mut bodies = Vec::new();

    loop {
        let trimmed = remaining.trim_start();
        let Some((label, after_label)) = trimmed.split_once(')') else {
            break;
        };
        if label.is_empty() || label.chars().any(char::is_whitespace) {
            break;
        }
        match find_case_arm_terminator(after_label) {
            Some((pos, len)) => {
                bodies.push(after_label[..pos].trim().to_owned());
                remaining = &after_label[pos + len..];
            }
            None => {
                bodies.push(strip_trailing_esac(after_label).trim().to_owned());
                break;
            }
        }
    }
    bodies
}

/// The earliest `case` arm terminator (`;;&`, `;;`, or `;&`, longest match
/// first so `;;&` is never misread as `;;` followed by a stray `&`) in `s`,
/// as a `(byte_index, terminator_len)` pair. Tracks single/double-quote and
/// backslash-escape state the same way `aegis_parser`'s own segment scanners
/// do, so a terminator-lookalike inside a quoted argument (`'x;;y'`) cannot
/// end the arm early and hide the rest of its body.
fn find_case_arm_terminator(s: &str) -> Option<(usize, usize)> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = s.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            ';' if !in_single_quote && !in_double_quote => {
                if s[idx..].starts_with(";;&") {
                    return Some((idx, 3));
                }
                if s[idx..].starts_with(";;") || s[idx..].starts_with(";&") {
                    return Some((idx, 2));
                }
            }
            _ => {}
        }
    }
    None
}

/// Strip a case statement's trailing `esac` keyword (and the whitespace
/// before it) off its last arm's body.
fn strip_trailing_esac(s: &str) -> &str {
    let trimmed = s.trim_end();
    trimmed.strip_suffix("esac").map_or(trimmed, str::trim_end)
}

/// `Some(body)` when `trimmed` (a whole stage, already trimmed of leading
/// whitespace and any trailing redirection — see [`strip_trailing_redirection`])
/// is a POSIX function definition — `NAME() BODY`, `NAME(){ BODY; }`, `NAME
/// () BODY`, ... — with `NAME` a bare identifier-like token with no shell
/// metacharacter, so this cannot misfire on a subshell or command
/// substitution. No whitespace is required, or disallowed, anywhere around
/// `()` (issue #384). `BODY` is returned exactly as written, whatever
/// compound command it is (`{...}`, `(...)`, `case...esac`, an `if`/`while`
/// keyword body, ...): routing the body at definition time is the accepted
/// conservative choice (a later call site of `NAME` is not tracked), and the
/// caller's own wrapper-body recursion already knows how to peel any
/// compound-command shape, so this stops at "found the body's start" rather
/// than duplicating that per-shape knowledge here. Raw-substring only, same
/// standard as this module's other wrapper-body extraction.
pub(super) fn posix_function_definition_body(trimmed: &str) -> Option<&str> {
    let paren_pos = trimmed.find('(')?;
    let name = trimmed[..paren_pos].trim_end();
    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '$' | '`' | '/' | '{' | '}' | ';' | '|'))
    {
        return None;
    }
    let after_close = trimmed[paren_pos + 1..].strip_prefix(')')?;
    let body = after_close.trim_start();
    (!body.is_empty()).then_some(body)
}

/// `Some(body)` when `rest` (already past the leading `function ` keyword) is
/// a `function NAME BODY` definition, with or without the optional empty
/// `()` between `NAME` and `BODY` (issue #384; same conservative
/// routed-at-definition choice, and the same "any compound command" contract,
/// as [`posix_function_definition_body`]). `NAME` must be a bare
/// identifier-like token with no shell metacharacter.
fn function_keyword_body(rest: &str) -> Option<&str> {
    let name_end = rest.find(|c: char| c.is_whitespace() || c == '(')?;
    let name = &rest[..name_end];
    if name.is_empty()
        || name
            .chars()
            .any(|c| matches!(c, '$' | '`' | '{' | '}' | ';' | '|'))
    {
        return None;
    }
    let tail = rest[name_end..].trim_start();
    let body = match tail.strip_prefix('(') {
        // `NAME ( ) BODY`: an empty parameter-list pair, discarded, before
        // the compound-command body.
        Some(after_open) if after_open.trim_start().starts_with(')') => after_open
            .trim_start()
            .strip_prefix(')')
            .map_or("", str::trim_start),
        // `NAME ( BODY` with no matching empty pair right after — the `(`
        // itself opens the compound command (a subshell body), so it stays
        // part of the body rather than being consumed as the empty pair.
        _ => tail,
    };
    (!body.is_empty()).then_some(body)
}

/// `Some(body)` when `rest` (already past the leading `coproc ` keyword) is a
/// bash coprocess with a brace-group body — `NAME { BODY; }` or `{ BODY; }`
/// (issue #384). A bare `coproc <cmd> <args>` with no brace group is
/// already resolved by `aegis_parser`'s launcher-prefix stripping before
/// routing ever reaches wrapper detection, so this only needs to cover the
/// brace-group shape.
fn coproc_body(rest: &str) -> Option<&str> {
    if let Some(body) = strip_group_or_subshell_wrapper(rest) {
        return Some(body);
    }
    let (name, tail) = rest.split_once(char::is_whitespace)?;
    if name.is_empty() || name.chars().any(|c| matches!(c, '{' | '}' | '(' | ')')) {
        return None;
    }
    strip_group_or_subshell_wrapper(tail.trim_start())
}

/// Strip every trailing shell redirection off `s`, glued (`>log`,
/// `2>/dev/null`, `{fd}>out`) or spaced (`> log`, `2> /dev/null`) alike,
/// repeated until none remain (`>out 2>&1` is two redirections). Reuses
/// [`aegis_parser::starts_with_redirection_glyph`] and
/// [`aegis_parser::is_redirection_operator`] — the same glyph primitives
/// `aegis_parser`'s own effective-program walk uses for a *leading*
/// redirection (issue #384) — so a wrapper stage's trailing redirect and
/// argv's leading one are recognized by one rule, not two. Whitespace-
/// boundary word splitting only, the same raw-substring standard as this
/// module's other wrapper-body extraction: a quoted redirect target is not
/// unquoted or re-tokenized.
pub(super) fn strip_trailing_redirection(s: &str) -> &str {
    let mut rest = s.trim_end();
    loop {
        let (before, last_word) = match rest.rfind(char::is_whitespace) {
            Some(idx) => (&rest[..idx], &rest[idx + 1..]),
            None => ("", rest),
        };
        if last_word.is_empty() {
            return rest;
        }
        if aegis_parser::starts_with_redirection_glyph(last_word) {
            rest = before.trim_end();
            continue;
        }
        // Not glued to its target — check whether the word before it is a
        // standalone redirection operator (`>`, `2>`, ...) whose target
        // `last_word` is, so the pair strips together.
        let before = before.trim_end();
        let (before_before, prior_word) = match before.rfind(char::is_whitespace) {
            Some(idx) => (&before[..idx], &before[idx + 1..]),
            None => ("", before),
        };
        if aegis_parser::is_redirection_operator(prior_word) {
            rest = before_before.trim_end();
            continue;
        }
        return rest;
    }
}

/// Every raw wrapper body found directly in `stage_raw`: a reserved-word
/// prefix's remainder, a `(...)`/`{...}` group wrapping the whole stage, and
/// every top-level `$(...)`/backtick command-substitution body — the exact
/// source bytes of each, not a dequoted/normalized copy. Reuses
/// [`aegis_parser::unwrap_subshell_group`] and
/// [`aegis_parser::extract_command_substitution_bodies`], the same raw
/// extraction the scanner's `logical_segments` composes (GHSA-mgwj-4828-3mrg),
/// instead of a parallel implementation; `logical_segments` itself is not
/// reused here because its output is already dequoted, which would corrupt
/// an inline `-c`/`-e` body's quoting on the second tokenizer pass
/// [`super::route_direct_stage`] performs.
pub(super) fn wrapper_bodies(stage_raw: &str, trusted_aliases: &[(&str, &str)]) -> Vec<String> {
    let mut bodies = Vec::new();
    // Stripped once, generally, for every wrapper-shape check below (issue
    // #384) rather than per wrapper kind — a trailing `2>/dev/null` (or
    // any other redirect) on a brace group, coprocess, or function/case body
    // is not part of that body's own closing punctuation, whichever wrapper
    // it turns out to be.
    let trimmed = strip_trailing_redirection(stage_raw.trim_start());

    for kw in aegis_parser::COMMAND_STARTING_KEYWORDS
        .iter()
        .chain(WRAPPER_ONLY_RESERVED_WORDS)
    {
        if let Some(rest) = trimmed.strip_prefix(kw)
            && rest.starts_with(char::is_whitespace)
        {
            let rest = rest.trim_start();
            match *kw {
                "case" => bodies.extend(case_arm_bodies(rest)),
                "function" => {
                    if let Some(body) = function_keyword_body(rest) {
                        bodies.push(body.to_owned());
                    }
                }
                "coproc" => match coproc_body(rest) {
                    Some(body) => bodies.push(body.to_owned()),
                    None => bodies.push(rest.to_owned()),
                },
                _ => bodies.push(rest.to_owned()),
            }
        }
    }
    if let Some(body) = posix_function_definition_body(trimmed) {
        bodies.push(body.to_owned());
    }
    if let Some(inner) = aegis_parser::unwrap_subshell_group(trimmed) {
        bodies.push(inner);
    } else if let Some(inner) = strip_group_or_subshell_wrapper(trimmed) {
        // A `{ ...; }` group (or a `(...)` with no trailing text —
        // `unwrap_subshell_group` above already covers `(...)` including a
        // trailing redirect, so this only adds the brace case).
        bodies.push(inner.to_owned());
    }
    bodies.extend(aegis_parser::extract_command_substitution_bodies(stage_raw));
    bodies.extend(aegis_parser::extract_process_substitution_bodies(stage_raw));
    if command_has_heredoc(stage_raw)
        && heredoc_write_then_exec_reuse(stage_raw, trusted_aliases).is_none()
        && let Some(tail) = heredoc_marker_line_tail(stage_raw)
    {
        // Guarded against `heredoc_write_then_exec_reuse` above: that narrow
        // shape already reads the heredoc body directly for its own single
        // route, so re-walking the same tail here would add a redundant
        // second route for the identical exec.
        bodies.push(tail.to_owned());
    }
    bodies
}
