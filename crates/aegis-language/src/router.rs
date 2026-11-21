//! Pure inline source-target detector (ADR-022 §1).
//!
//! Detects interpreter `-c`/`-e` inline source bodies from `&str` alone with
//! no filesystem access. No-source commands yield no targets so the language
//! worker does not start.

use crate::language::SourceLanguage;

/// An analyzable source target discovered without filesystem access.
///
/// `source` is an owned `String` because it is extracted from the runtime
/// command string (the quoted argument to an interpreter's `-c`/`-e` flag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTarget {
    /// The language the inline source declares (via its interpreter).
    pub language: SourceLanguage,
    /// The inline source body to be parsed.
    pub source: String,
}

/// A known interpreter invocation shape that exposes inline source.
struct Interpreter {
    /// The program basename (after any `sudo`/`time`/`nohup` prefix is stripped).
    program: &'static str,
    /// The inline-source flag, e.g. `-c` (Python/Bash) or `-e` (Node).
    flag: &'static str,
    /// The language the inline body should be parsed as.
    language: SourceLanguage,
}

/// The L1 foundation interpreters that expose inline source. Shell `sh` is mapped
/// onto the Bash grammar (the L1 Shell/Bash adapter, ADR-022 §9).
const INTERPRETERS: &[Interpreter] = &[
    Interpreter {
        program: "python3",
        flag: "-c",
        language: SourceLanguage::Python,
    },
    Interpreter {
        program: "python",
        flag: "-c",
        language: SourceLanguage::Python,
    },
    Interpreter {
        program: "bash",
        flag: "-c",
        language: SourceLanguage::Bash,
    },
    Interpreter {
        program: "sh",
        flag: "-c",
        language: SourceLanguage::Bash,
    },
    Interpreter {
        program: "node",
        flag: "-e",
        language: SourceLanguage::JavaScript,
    },
];

/// Command prefixes that wrap another command without being the program itself.
/// Stripped before interpreter matching so `sudo python3 -c …` is still detected.
/// `env` is intentionally excluded: it is followed by `VAR=value` assignments,
/// not the program, so skipping only the literal `env` token would misread the
/// assignment as the program.
const COMMAND_PREFIXES: &[&str] = &["sudo", "time", "nice", "nohup", "command"];

/// Detect analyzable inline source targets in `command` without any filesystem
/// access. Returns an empty vector for no-source commands.
#[must_use]
pub fn source_targets(command: &str) -> Vec<SourceTarget> {
    let words = shell_words(command);
    if words.is_empty() {
        return Vec::new();
    }

    // Skip wrapping prefixes to find the program token.
    let mut idx = 0;
    while idx + 1 < words.len() && COMMAND_PREFIXES.contains(&words[idx].as_str()) {
        idx += 1;
    }
    let program = basename(&words[idx]);

    let Some(interp) = INTERPRETERS.iter().find(|i| i.program == program) else {
        return Vec::new();
    };

    // Scan the remaining tokens for the inline-source flag and its body. The
    // flag may be a standalone token (`-c "code"`) or glued to its body
    // (`-c"code"` → one token `-ccode`).
    let rest = &words[idx + 1..];
    for (pos, tok) in rest.iter().enumerate() {
        let Some(body) = inline_body(tok, interp.flag, rest, pos) else {
            continue;
        };
        if body.is_empty() {
            // Flag present but no inline body to analyze — not a source target.
            return Vec::new();
        }
        return vec![SourceTarget {
            language: interp.language,
            source: body,
        }];
    }

    Vec::new()
}

/// Extract the inline body for `flag` from token `tok`, returning `None` if
/// `tok` is not the flag. Handles both the standalone (`-c "code"`) and glued
/// (`-ccode`) forms.
fn inline_body(tok: &str, flag: &str, rest: &[String], pos: usize) -> Option<String> {
    if tok == flag {
        // Standalone flag: the body is the next token, if any.
        return rest.get(pos + 1).cloned();
    }
    // Glued form: `-c<code>` (flag immediately followed by its body, no `-`
    // continuation, so `--eval` is not misread as `-e` + `val`).
    let stripped = tok.strip_prefix(flag)?;
    if stripped.is_empty() || stripped.starts_with('-') {
        return None;
    }
    Some(stripped.to_owned())
}

/// Strip the directory portion of a program path, mirroring how a shell locates
/// the interpreter: `/usr/bin/python3` and `python3` are the same program.
fn basename(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, base)) => base,
        None => path,
    }
}

/// Split a command string into shell words, honoring single and double quotes
/// and backslash escapes inside double quotes.
fn shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut active = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == '\\' && q == '"' {
                if let Some(&next) = chars.peek()
                    && matches!(next, '\\' | '"' | '$' | '`')
                {
                    // Consume the peeked escape char and emit it unescaped.
                    let _ = chars.next();
                    cur.push(next);
                    continue;
                }
                cur.push(c);
            } else if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '"' | '\'' => {
                    quote = Some(c);
                    active = true;
                }
                ' ' | '\t' | '\n' => {
                    if active {
                        words.push(std::mem::take(&mut cur));
                        active = false;
                    }
                }
                _ => {
                    cur.push(c);
                    active = true;
                }
            }
        }
    }

    if active {
        words.push(cur);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_strips_directory_portion() {
        assert_eq!(basename("python3"), "python3");
        assert_eq!(basename("/usr/bin/python3"), "python3");
        assert_eq!(basename("./python3"), "python3");
    }

    #[test]
    fn shell_words_splits_and_unquotes() {
        assert_eq!(
            shell_words("python3 -c \"import os\""),
            vec![
                "python3".to_owned(),
                "-c".to_owned(),
                "import os".to_owned(),
            ]
        );
        assert_eq!(
            shell_words("echo 'a b' c"),
            vec!["echo".to_owned(), "a b".to_owned(), "c".to_owned(),]
        );
    }
}
