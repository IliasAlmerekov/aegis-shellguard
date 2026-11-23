//! Shell redirection tokens: operators, glued forms and fd markers.

/// `true` when `tok` starts with a redirection glyph (`<`, `>`, or one
/// prefixed by a digit or `{fd}` marker and optional `&`), standalone or glued (#384).
#[must_use]
pub fn starts_with_redirection_glyph(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    after_amp.starts_with('<') || after_amp.starts_with('>')
}

/// `true` when `tok` is a standalone redirection operator (`>`, `>>`, `<`, `2>`, ...)
/// whose target is the following token. Glued forms (`>out.txt`, `2>&1`) return `false`.
#[must_use]
pub fn is_redirection_operator(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    !after_amp.is_empty() && after_amp.chars().all(|c| c == '<' || c == '>')
}

/// Strip a leading numeric (`2>`) or bash named (`{fd}>`) fd marker; callers check `<`/`>`.
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

/// Number of tokens a leading redirection consumes: `2` for standalone operators, `1` for glued forms.
pub(crate) fn redirection_token_len(tok: &str) -> usize {
    if is_redirection_operator(tok) { 2 } else { 1 }
}
