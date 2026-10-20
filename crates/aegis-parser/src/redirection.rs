//! Shell redirection tokens: operators, glued forms and fd markers.

/// `true` when `tok` *starts* with a redirection glyph (`<`, `>`, or one
/// prefixed by a leading fd — a bare digit (`2>`) or a bash named fd
/// (`{fd}>`) — optionally followed by `&` for the combined-stream form
/// (`&>`)), whether standalone (`>`, `2>`) or with a glued target/fd
/// (`>out`, `2>&1`, `<file`, `{fd}>out`, `1>|out`). Broader than
/// [`is_redirection_operator`], which only matches the pure-operator form.
///
/// The single source of truth for this decision, reused by both effective-
/// program resolution here and the router's own argv walk (issue #384).
#[must_use]
pub fn starts_with_redirection_glyph(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    after_amp.starts_with('<') || after_amp.starts_with('>')
}

/// A standalone shell redirection operator token (`>`, `>>`, `<`, `2>`, …) —
/// an fd marker (see [`starts_with_redirection_glyph`]) followed by nothing
/// but `<`/`>` characters. A glued form (`>out.txt`, `2>&1`, `>|out`) is not
/// standalone — it carries its own target in the same token and needs no
/// extra token skipped, so it is deliberately excluded here.
#[must_use]
pub fn is_redirection_operator(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    !after_amp.is_empty() && after_amp.chars().all(|c| c == '<' || c == '>')
}

/// Strip a leading redirection fd marker: a run of ASCII digits (`2>`), or a
/// bash named fd in braces (`{fd}>`, `{myfd}>`). Neither shape is valid
/// shell syntax as-is (a `{name}` fd requires the following `<`/`>` to make
/// it one), so this only strips the marker text itself and leaves that
/// check to the caller.
fn strip_leading_fd_marker(tok: &str) -> &str {
    if let Some(rest) = tok.strip_prefix('{')
        && let Some(close) = rest.find('}')
        && let name = &rest[..close]
        && !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return &rest[close + 1..];
    }
    tok.trim_start_matches(|c: char| c.is_ascii_digit())
}

/// Number of tokens a leading redirection at `tok` consumes: `2` for a
/// standalone operator (`>`, `2>`, …), whose target is the *next* token, or
/// `1` for a glued form (`>out`, `2>&1`, `{fd}>out`), which carries its
/// target/fd in the same token.
pub(crate) fn redirection_token_len(tok: &str) -> usize {
    if is_redirection_operator(tok) { 2 } else { 1 }
}
