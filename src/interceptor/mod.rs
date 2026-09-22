/// Tokenizer and parser for shell commands.
pub mod parser;
/// Pattern definitions, categories, and built-in pattern loading.
pub mod patterns;
/// Scanner: keyword-based quick scan + regex full scan.
pub mod scanner;

pub use aegis_types::RiskLevel;
