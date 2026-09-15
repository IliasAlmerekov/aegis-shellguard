//! Typed errors for configuration loading, validation, and amendment.

/// Errors raised while loading, validating, or amending Aegis configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A configuration value was invalid (parse failure, bad field, validation
    /// rule, scope error, …). Carries a human-readable message.
    ///
    /// The `config error: ` prefix lives here, not in the binary's orchestration
    /// error type, so a transparent wrapper reproduces this message byte-for-byte.
    #[error("config error: {0}")]
    Config(String),

    /// An I/O error while reading or writing a config file.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::ConfigError;

    #[test]
    fn config_variant_display_carries_the_config_error_prefix() {
        let err = ConfigError::Config("missing field `mode`".to_string());
        assert_eq!(err.to_string(), "config error: missing field `mode`");
    }
}
