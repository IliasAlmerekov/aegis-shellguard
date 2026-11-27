#[must_use]
pub fn starts_with_redirection_glyph(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    after_amp.starts_with('<') || after_amp.starts_with('>')
}

#[must_use]
pub fn is_redirection_operator(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    !after_amp.is_empty() && after_amp.chars().all(|c| c == '<' || c == '>')
}

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

pub(crate) fn redirection_token_len(tok: &str) -> usize {
    if is_redirection_operator(tok) { 2 } else { 1 }
}

pub(crate) fn ends_with_redirect_target(current: &str) -> bool {
    let mut rev = current.trim_end().chars().rev();
    match rev.next() {
        Some('>') | Some('<') => rev.take_while(|&c| c == '\\').count() % 2 == 0,
        _ => false,
    }
}
