use crate::language::SourceLanguage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTarget {
    pub language: SourceLanguage,
    pub source: String,
}

struct Interpreter {
    program: &'static str,
    flag: &'static str,
    language: SourceLanguage,
}

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

const COMMAND_PREFIXES: &[&str] = &["sudo", "time", "nice", "nohup", "command"];

#[must_use]
pub fn source_targets(command: &str) -> Vec<SourceTarget> {
    let words = shell_words(command);
    if words.is_empty() {
        return Vec::new();
    }

    let mut idx = 0;
    while idx + 1 < words.len() && COMMAND_PREFIXES.contains(&words[idx].as_str()) {
        idx += 1;
    }
    let program = basename(&words[idx]);

    let Some(interp) = INTERPRETERS.iter().find(|i| i.program == program) else {
        return Vec::new();
    };

    let rest = &words[idx + 1..];
    for (pos, tok) in rest.iter().enumerate() {
        let Some(body) = inline_body(tok, interp.flag, rest, pos) else {
            continue;
        };
        if body.is_empty() {
            return Vec::new();
        }
        return vec![SourceTarget {
            language: interp.language,
            source: body,
        }];
    }

    Vec::new()
}

fn inline_body(tok: &str, flag: &str, rest: &[String], pos: usize) -> Option<String> {
    if tok == flag {
        return rest.get(pos + 1).cloned();
    }
    let stripped = tok.strip_prefix(flag)?;
    if stripped.is_empty() || stripped.starts_with('-') {
        return None;
    }
    Some(stripped.to_owned())
}

fn basename(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, base)) => base,
        None => path,
    }
}

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
