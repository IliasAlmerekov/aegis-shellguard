//! Wiring between [`SourceLanguage`] and the pinned Tree-sitter grammar crates (ADR-022 §8, §9).
//!
//! Owns the pinned grammar crates (`tree_sitter_{python,javascript,typescript,bash}`)
//! and the [`parse`] entry point, keeping the native C toolchain boundary inside
//! `aegis-language` (ADR-022 §8).

use tree_sitter::{Language, Parser, Tree};

/// The supported source languages for the L1 foundation (ADR-022 §9).
///
/// Go, PHP, Ruby, PowerShell, Perl, and Lua are staged 1.x adapters and have no
/// variant here until each passes its independent qualification gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    Python,
    JavaScript,
    TypeScript,
    Bash,
}

impl SourceLanguage {
    /// The canonical manifest identifier matching
    /// [`crate::manifest::RELEASE_GRAMMARS`].
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            SourceLanguage::Python => "python",
            SourceLanguage::JavaScript => "javascript",
            SourceLanguage::TypeScript => "typescript",
            SourceLanguage::Bash => "bash",
        }
    }

    /// Resolve a manifest language id to a [`SourceLanguage`].
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

    /// The pinned Tree-sitter [`Language`] for this source language.
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

/// A Tree-sitter parse failure.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The grammar's Tree-sitter ABI is incompatible with the pinned runtime.
    #[error("Tree-sitter rejected the grammar for `{language}`: {reason}")]
    IncompatibleLanguage {
        language: &'static str,
        reason: String,
    },
    /// The parser returned no tree (for example, a NULL result from the C API).
    #[error("Tree-sitter produced no tree for `{language}`")]
    NoTree { language: &'static str },
}

/// Parse `source` as `language` using the pinned Tree-sitter runtime.
///
/// Pure and in-process: performs no filesystem access and spawns no worker.
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
