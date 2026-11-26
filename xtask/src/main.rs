use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use ra_ap_rustc_lexer::{FrontmatterAllowed, TokenKind, strip_shebang, tokenize};

#[derive(Debug, PartialEq, Eq)]
struct Comment {
    start: usize,
    end: usize,
    line: usize,
}

fn forbidden_comments(src: &str) -> Vec<Comment> {
    let mut found: Vec<(Comment, bool, bool)> = Vec::new();
    let mut pos = strip_shebang(src).unwrap_or(0);
    let mut line = 1 + src[..pos].matches('\n').count();
    for token in tokenize(&src[pos..], FrontmatterAllowed::No) {
        let end = pos + token.len as usize;
        let text = &src[pos..end];
        let is_line = matches!(token.kind, TokenKind::LineComment { .. });
        if is_line || matches!(token.kind, TokenKind::BlockComment { .. }) {
            let alone = src[..pos]
                .rsplit('\n')
                .next()
                .is_some_and(|before| before.trim().is_empty());
            found.push((
                Comment {
                    start: pos,
                    end,
                    line,
                },
                is_line && alone,
                is_line && text.starts_with("// SAFETY:"),
            ));
        }
        line += text.matches('\n').count();
        pos = end;
    }

    let mut kept_group = false;
    let mut previous_line = 0;
    let mut forbidden = Vec::new();
    for (comment, alone_line, safety_start) in found {
        let continues = alone_line && kept_group && comment.line == previous_line + 1;
        kept_group = (safety_start && alone_line) || continues;
        previous_line = comment.line;
        if !kept_group {
            forbidden.push(comment);
        }
    }
    forbidden
}

fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut cursor = 0;
    for comment in forbidden_comments(src) {
        let line_start = src[..comment.start].rfind('\n').map_or(0, |i| i + 1);
        let before_blank = src[line_start..comment.start].trim().is_empty();
        let rest_end = src[comment.end..]
            .find('\n')
            .map_or(src.len(), |i| comment.end + i);
        let after_blank = src[comment.end..rest_end].trim().is_empty();
        let (from, to) = if before_blank && after_blank {
            (line_start, (rest_end + 1).min(src.len()))
        } else if after_blank {
            let mut from = comment.start;
            while from > 0 && matches!(bytes[from - 1], b' ' | b'\t') {
                from -= 1;
            }
            (from, comment.end)
        } else {
            let mut to = comment.end;
            while matches!(bytes.get(to), Some(b' ' | b'\t')) {
                to += 1;
            }
            (comment.start, to)
        };
        out.push_str(&src[cursor..from]);
        cursor = to;
    }
    out.push_str(&src[cursor..]);
    out
}

fn tracked_rust_files() -> Result<Vec<PathBuf>, String> {
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("git rev-parse: {e}"))?;
    let top = String::from_utf8_lossy(&top.stdout).trim().to_string();
    let listing = Command::new("git")
        .current_dir(&top)
        .args(["ls-files", "*.rs"])
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !listing.status.success() {
        return Err("git ls-files failed".to_string());
    }
    Ok(String::from_utf8_lossy(&listing.stdout)
        .lines()
        .map(|path| PathBuf::from(&top).join(path))
        .collect())
}

fn run(command: &str) -> Result<bool, String> {
    let mut clean = true;
    for path in tracked_rust_files()? {
        let src = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let comments = forbidden_comments(&src);
        if comments.is_empty() {
            continue;
        }
        clean = false;
        match command {
            "check-comments" => {
                for comment in comments {
                    println!("{}:{}", path.display(), comment.line);
                }
            }
            _ => fs::write(&path, strip_comments(&src))
                .map_err(|e| format!("{}: {e}", path.display()))?,
        }
    }
    Ok(clean || command == "strip-comments")
}

fn main() -> ExitCode {
    let command = std::env::args().nth(1).unwrap_or_default();
    if command != "check-comments" && command != "strip-comments" {
        eprintln!("usage: cargo run -p xtask -- check-comments | strip-comments");
        return ExitCode::from(2);
    }
    match run(&command) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("Rust code carries comments; only `// SAFETY:` is allowed");
            ExitCode::FAILURE
        }
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(src: &str) -> Vec<usize> {
        forbidden_comments(src).iter().map(|c| c.line).collect()
    }

    #[test]
    fn slashes_inside_literals_are_data() {
        let src = "let a = \"http://x\";\nlet b = r#\"// no \"# ;\nlet c = '/';\nlet d = b\"//\";\nlet e: &'static str = \"/* */\";\n";
        assert!(lines(src).is_empty());
        assert_eq!(strip_comments(src), src);
    }

    #[test]
    fn nested_block_comment_is_one_comment() {
        let src = "a\n/* x /* y */ z */\nb\n";
        assert_eq!(lines(src), vec![2]);
        assert_eq!(strip_comments(src), "a\nb\n");
    }

    #[test]
    fn doc_comments_are_forbidden() {
        let src = "//! crate\n/// item\n/** block */\n/*! inner */\nfn f() {}\n";
        assert_eq!(lines(src), vec![1, 2, 3, 4]);
        assert_eq!(strip_comments(src), "fn f() {}\n");
    }

    #[test]
    fn safety_comment_is_kept() {
        let src = "// SAFETY: fd is open\n//  and owned\nunsafe { f() }\n// other\n";
        assert_eq!(lines(src), vec![4]);
        assert_eq!(
            strip_comments(src),
            "// SAFETY: fd is open\n//  and owned\nunsafe { f() }\n"
        );
    }

    #[test]
    fn trailing_comment_drops_preceding_whitespace() {
        let src = "let x = 1;   // why\nlet y = 2; /* z */\n";
        assert_eq!(strip_comments(src), "let x = 1;\nlet y = 2;\n");
    }

    #[test]
    fn inline_block_comment_keeps_code() {
        assert_eq!(strip_comments("f(/* a */ b)\n"), "f(b)\n");
    }

    #[test]
    fn indented_comment_line_is_removed_whole() {
        let src = "fn f() {\n    // note\n    g();\n}\n";
        assert_eq!(strip_comments(src), "fn f() {\n    g();\n}\n");
    }

    #[test]
    fn file_with_comment_fails_check() {
        assert!(!forbidden_comments("fn f() {} // x\n").is_empty());
    }
}
