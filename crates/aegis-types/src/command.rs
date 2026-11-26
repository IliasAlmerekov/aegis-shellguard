use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct InlineScript {
    pub interpreter: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCommand {
    pub program: Option<String>,
    pub argv: Vec<String>,
    pub normalized: String,
    pub inline_scripts: Vec<InlineScript>,
    pub raw: String,
}

impl fmt::Display for ParsedCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.program {
            Some(prog) if !self.argv.is_empty() => {
                write!(f, "{} {}", prog, self.argv.join(" "))
            }
            Some(prog) => write!(f, "{}", prog),
            None => write!(f, "{}", self.raw),
        }
    }
}
