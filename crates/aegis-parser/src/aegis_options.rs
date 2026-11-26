const LONG_FLAGS: &[&str] = &["quiet", "verbose"];

const LONG_VALUE_OPTIONS: &[&str] = &["command", "output", "verbosity"];

const SHORT_FLAGS: &[char] = &['v'];

const SHORT_VALUE_OPTIONS: &[char] = &['c'];

pub fn aegis_option_subcommand_start(tokens: &[&str]) -> Option<usize> {
    let mut index = 1;
    while let Some(&token) = tokens.get(index) {
        if token == "--" || !token.starts_with('-') || token == "-" {
            break;
        }
        index += option_len(token, tokens.get(index + 1).is_some())?;
    }
    (index > 1 && index < tokens.len() && tokens[index] != "--").then_some(index)
}

fn option_len(token: &str, has_next: bool) -> Option<usize> {
    if let Some(long) = token.strip_prefix("--") {
        let (name, glued) = match long.split_once('=') {
            Some((name, _)) => (name, true),
            None => (long, false),
        };
        if LONG_FLAGS.contains(&name) && !glued {
            return Some(1);
        }
        if LONG_VALUE_OPTIONS.contains(&name) {
            return if glued {
                Some(1)
            } else {
                has_next.then_some(2)
            };
        }
        return None;
    }

    let cluster = token.strip_prefix('-')?;
    for (offset, flag) in cluster.char_indices() {
        if SHORT_FLAGS.contains(&flag) {
            continue;
        }
        if SHORT_VALUE_OPTIONS.contains(&flag) {
            let glued = offset + flag.len_utf8() < cluster.len();
            return if glued {
                Some(1)
            } else {
                has_next.then_some(2)
            };
        }
        return None;
    }
    Some(1)
}

#[cfg(test)]
mod tests {
    use super::aegis_option_subcommand_start;

    fn start(command: &str) -> Option<usize> {
        let tokens: Vec<&str> = command.split_whitespace().collect();
        aegis_option_subcommand_start(&tokens)
    }

    #[test]
    fn a_subcommand_in_position_one_needs_no_rescan() {
        assert_eq!(start("aegis off"), None);
    }

    #[test]
    fn flags_and_values_before_the_subcommand_are_skipped() {
        assert_eq!(start("aegis --quiet off"), Some(2));
        assert_eq!(start("aegis -v --output json off"), Some(4));
        assert_eq!(start("aegis --verbosity=quiet uninstall"), Some(2));
        assert_eq!(start("aegis -vc true off"), Some(3));
        assert_eq!(start("aegis -ctrue off"), Some(2));
    }

    #[test]
    fn an_unknown_option_stops_parsing() {
        assert_eq!(start("aegis --bogus off"), None);
        assert_eq!(start("aegis -x off"), None);
        assert_eq!(start("aegis --quiet=yes off"), None);
    }

    #[test]
    fn options_without_a_subcommand_yield_nothing() {
        assert_eq!(start("aegis --quiet"), None);
        assert_eq!(start("aegis --output"), None);
        assert_eq!(start("aegis --quiet -- off"), None);
    }
}
