//! Limits for regexes that come from user config rather than the built-in set.

use regex::{Regex, RegexBuilder};

/// Longest accepted untrusted pattern, in UTF-8 bytes.
///
/// Why 256: regex compile time is roughly quadratic in literal length, and it
/// is paid on every command. A 4096-byte literal costs about 1.7 s per command.
pub const MAX_PATTERN_LEN: usize = 256;

/// Compiled-regex size budget; equals the `regex` 1.13 default (10 MiB).
pub const REGEX_SIZE_LIMIT: usize = 10 * (1 << 20);

/// Lazy-DFA cache budget; equals the `regex` 1.13 default (2 MiB).
pub const REGEX_DFA_SIZE_LIMIT: usize = 2 * (1 << 20);

/// Reject `pattern` when it is longer than [`MAX_PATTERN_LEN`] bytes.
///
/// # Errors
///
/// Returns a message naming the byte length and the limit.
pub fn check_pattern_length(pattern: &str) -> Result<(), String> {
    let len = pattern.len();
    if len > MAX_PATTERN_LEN {
        return Err(format!(
            "pattern is {len} bytes, over the {MAX_PATTERN_LEN}-byte pattern length limit"
        ));
    }
    Ok(())
}

/// Compile an untrusted regex with the explicit size and DFA budgets.
///
/// # Errors
///
/// Returns the `regex` error when the pattern is invalid or over budget.
pub fn compile_untrusted_regex(regex: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(regex)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_DFA_SIZE_LIMIT)
        .build()
}
