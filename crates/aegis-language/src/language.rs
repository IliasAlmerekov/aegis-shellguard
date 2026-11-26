#[cfg(feature = "grammars")]
use tree_sitter::{Language, Parser, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    Python,
    JavaScript,
    TypeScript,
    Bash,
}

impl SourceLanguage {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            SourceLanguage::Python => "python",
            SourceLanguage::JavaScript => "javascript",
            SourceLanguage::TypeScript => "typescript",
            SourceLanguage::Bash => "bash",
        }
    }

    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "python" => Some(SourceLanguage::Python),
            "javascript" => Some(SourceLanguage::JavaScript),
            "typescript" => Some(SourceLanguage::TypeScript),
            "bash" => Some(SourceLanguage::Bash),
            _ => None,
        }
    }

    #[cfg(feature = "grammars")]
    #[must_use]
    pub fn tree_sitter_language(self) -> Language {
        match self {
            SourceLanguage::Python => tree_sitter_python::LANGUAGE.into(),
            SourceLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            SourceLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            SourceLanguage::Bash => tree_sitter_bash::LANGUAGE.into(),
        }
    }
}

#[cfg(feature = "grammars")]
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Tree-sitter rejected the grammar for `{language}`: {reason}")]
    IncompatibleLanguage {
        language: &'static str,
        reason: String,
    },
    #[error("Tree-sitter produced no tree for `{language}`")]
    NoTree { language: &'static str },
}

#[cfg(feature = "grammars")]
pub fn parse(language: SourceLanguage, source: &str) -> Result<Tree, ParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&language.tree_sitter_language())
        .map_err(|err| ParseError::IncompatibleLanguage {
            language: language.id(),
            reason: err.to_string(),
        })?;
    parser
        .parse(source.as_bytes(), None)
        .ok_or(ParseError::NoTree {
            language: language.id(),
        })
}
