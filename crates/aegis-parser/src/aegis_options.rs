//! Aegis global-option skipping.
//!
//! Aegis' own options between `aegis` and its subcommand (`--quiet`,
//! `--output json`, `-c <command>`, ...) shift the subcommand off position 1,
//! so an `AEG-*` rule keyed on `["aegis", "<subcommand>"]` never sees it.
//! This module finds where the subcommand starts so the scanner can re-run
//! those rules there. Like [`crate::git_option_subcommand_starts`], it is
//! detection-only and never changes the parsed or executed command.
//!
//! Unlike git, Aegis' option set is closed and known: it is the global
//! argument list of the `aegis` binary itself. An option outside that list
//! makes the CLI exit with a usage error before any subcommand runs, so
//! parsing stops there.

/// Long options that take no value, without their `--`. Case-sensitive, as
/// clap is.
const LONG_FLAGS: &[&str] = &["quiet", "verbose"];

/// Long options whose value is glued in with `=` or is the next token,
/// without their `--`.
const LONG_VALUE_OPTIONS: &[&str] = &["command", "output", "verbosity"];

/// Short options that take no value.
const SHORT_FLAGS: &[char] = &['v'];

/// Short options whose value is the rest of the cluster or the next token.
const SHORT_VALUE_OPTIONS: &[char] = &['c'];

/// Returns the index of the subcommand in an `aegis` token slice when global
/// options precede it.
///
/// `tokens[0]` is the `aegis` program. Returns `None` when no option precedes
/// the first non-option token, when the slice ends inside the options, or
/// when an option outside the known set appears first.
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

/// Number of tokens the option at `token` consumes, or `None` for an option
/// outside the known set.
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
