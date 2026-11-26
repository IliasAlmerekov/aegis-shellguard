use regex::{Regex, RegexBuilder};

pub const MAX_PATTERN_LEN: usize = 256;

pub const REGEX_SIZE_LIMIT: usize = 10 * (1 << 20);

pub const REGEX_DFA_SIZE_LIMIT: usize = 2 * (1 << 20);

pub fn check_pattern_length(pattern: &str) -> Result<(), String> {
    let len = pattern.len();
    if len > MAX_PATTERN_LEN {
        return Err(format!(
            "pattern is {len} bytes, over the {MAX_PATTERN_LEN}-byte pattern length limit"
        ));
    }
    Ok(())
}

pub fn compile_untrusted_regex(regex: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(regex)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_DFA_SIZE_LIMIT)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_equal_the_regex_crate_defaults() {
        assert_eq!(REGEX_SIZE_LIMIT, 10_485_760);
        assert_eq!(REGEX_DFA_SIZE_LIMIT, 2_097_152);
    }
}
