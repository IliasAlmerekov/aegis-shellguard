//! Whether a bare quote is still open at the end of a heredoc marker's
//! prefix (issue #396, PR #463 review): `open_frames` only tracks the
//! bracket-shaped frames a `$(`, backtick, `(`, `{` or `#` comment opens, so
//! a `'`, `"` or backtick left open on an earlier physical line and never
//! closed before the marker's own `<<` reads as an empty frame list, and
//! `heredoc_marker_context` took that for `TopLevel`. `echo 'start` on one
//! line, then `cat <<'EOF'` on the next, actually closes the pending quote at
//! the delimiter's own opening `'` and opens a fresh one at its closing `'`,
//! so bash never sees a heredoc operator there at all — the text this walk
//! reads as a marker line is live shell text to bash.
//!
//! This tracker deliberately does not follow `$(`/backtick nesting the way
//! `open_frames` does: for well-formed bash, every quote character still
//! pairs up across the flat text regardless of which `$(...)` it sits in, so
//! ignoring that structure costs nothing for balanced input and still flags
//! the unbalanced case this predicate exists to catch. It also does not
//! special-case `$'...'` ANSI-C quoting — `heredoc_marker_context` bails to
//! `Untrusted` on any `$'` in the prefix before this tracker ever runs, so an
//! escaped `\'` inside one never reaches here.

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
