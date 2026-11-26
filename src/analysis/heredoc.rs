use aegis_parser::extract_heredoc_bodies;
use aegis_types::DegradationReason;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdinRoute {
    Literal(String),
    Dynamic(DegradationReason),
}

#[must_use]
pub fn heredoc_stdin(command: &str) -> Option<StdinRoute> {
    let first_line = command.lines().next()?;
    if !first_line.contains("<<") {
        return None;
    }
    let heredoc = extract_heredoc_bodies(command).into_iter().next()?;
    Some(classify(heredoc.body, heredoc.is_nowdoc))
}

#[must_use]
pub fn here_string_stdin(rest_tokens: &[&str]) -> Option<StdinRoute> {
    let pos = rest_tokens.iter().position(|tok| tok.starts_with("<<<"))?;
    let glued = &rest_tokens[pos][3..];
    let body = if glued.is_empty() {
        (*rest_tokens.get(pos + 1)?).to_owned()
    } else {
        glued.to_owned()
    };
    Some(classify(body, false))
}

pub(crate) fn classify(body: String, is_nowdoc: bool) -> StdinRoute {
    if is_nowdoc || !contains_expansion(&body) {
        StdinRoute::Literal(body)
    } else {
        StdinRoute::Dynamic(DegradationReason::DynamicSource)
    }
}

fn contains_expansion(body: &str) -> bool {
    body.contains('$') || body.contains('`')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_heredoc_is_literal() {
        let command = "bash <<'EOF'\nrm -rf /tmp/x\nEOF";
        assert_eq!(
            heredoc_stdin(command),
            Some(StdinRoute::Literal("rm -rf /tmp/x".to_owned()))
        );
    }

    #[test]
    fn expanding_heredoc_without_expansion_syntax_is_literal() {
        let command = "bash <<EOF\necho hello\nEOF";
        assert_eq!(
            heredoc_stdin(command),
            Some(StdinRoute::Literal("echo hello".to_owned()))
        );
    }

    #[test]
    fn expanding_heredoc_with_variable_expansion_is_dynamic() {
        let command = "bash <<EOF\necho $HOME\nEOF";
        assert_eq!(
            heredoc_stdin(command),
            Some(StdinRoute::Dynamic(DegradationReason::DynamicSource))
        );
    }

    #[test]
    fn no_heredoc_marker_yields_none() {
        assert_eq!(heredoc_stdin("bash script.sh"), None);
    }

    #[test]
    fn literal_here_string_is_recognized() {
        assert_eq!(
            here_string_stdin(&["<<<", "print(1)"]),
            Some(StdinRoute::Literal("print(1)".to_owned()))
        );
    }

    #[test]
    fn here_string_with_expansion_is_dynamic() {
        assert_eq!(
            here_string_stdin(&["<<<", "print($HOME)"]),
            Some(StdinRoute::Dynamic(DegradationReason::DynamicSource))
        );
    }

    #[test]
    fn no_here_string_token_yields_none() {
        assert_eq!(here_string_stdin(&["script.py"]), None);
    }
}
