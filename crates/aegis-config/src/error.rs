#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config error: {0}")]
    Config(String),

    #[error("config error: failed to parse {path}: {source}")]
    ParseFailed {
        path: String,
        #[source]
        source: Box<toml::de::Error>,
    },

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
