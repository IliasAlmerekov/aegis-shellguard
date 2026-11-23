//! Detect unclosed quotes or backticks at the end of a heredoc marker prefix (#396).
//!
//! `open_frames` only tracks bracket frames; a quote left open on an earlier
//! line makes bash read the marker line as quoted text. ANSI-C `$'` is
//! rejected upstream in `heredoc_marker_context` before this runs.

/// `true` when a single quote, double quote, or backtick command
/// substitution opened somewhere in `text` is still open at the end of it.
pub(super) fn quote_open_at_end(text: &str) -> bool {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut backtick = false;
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !single_quote => {
                chars.next();
            }
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '`' if !single_quote => backtick = !backtick,
            _ => {}
        }
    }

    single_quote || double_quote || backtick
}
