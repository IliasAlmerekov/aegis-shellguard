use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::RiskLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternSource {
    Builtin,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(description = "Which class of operation the pattern guards against.")]
pub enum Category {
    #[schemars(description = "File system operations (delete, overwrite, chmod, etc.).")]
    Filesystem,
    #[schemars(description = "Git history rewrite operations.")]
    Git,
    #[schemars(description = "Database DDL/DML operations.")]
    Database,
    #[schemars(description = "Cloud-provider resource mutations.")]
    Cloud,
    #[schemars(description = "Docker container and image management.")]
    Docker,
    #[schemars(description = "Process and system-level operations.")]
    Process,
    #[schemars(description = "Package manager operations.")]
    Package,
    #[schemars(description = "Aegis' own enforcement state and installed integration points.")]
    Aegis,
}

pub type PrefixPattern = Vec<PatternToken>;

#[derive(Debug, Clone, PartialEq)]
pub enum PatternToken {
    Single(Cow<'static, str>),
    Alts(Vec<Cow<'static, str>>),
    Any,
    AnyStar,
    ShortFlag {
        short: char,
        long: &'static [&'static str],
    },
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub id: Cow<'static, str>,
    pub category: Category,
    pub risk: RiskLevel,
    pub pattern: Cow<'static, str>,
    pub description: Cow<'static, str>,
    pub safe_alt: Option<Cow<'static, str>>,
    pub justification: Option<Cow<'static, str>>,
    pub source: PatternSource,
}
