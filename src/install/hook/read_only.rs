pub(super) const PIPE_FILTERS: [&str; 6] = ["head", "tail", "grep", "wc", "jq", "cut"];

pub(super) fn is_read_only_aegis_command(command: &str) -> bool {
    let Some(commands) = split_simple_commands(command) else {
        return false;
    };
    if commands[0].words.first() != Some(&"aegis") {
        return false;
    }
    let last = commands.len() - 1;
    commands
        .iter()
        .enumerate()
        .all(|(index, simple)| match simple.words.split_first() {
            Some((&"aegis", args)) => is_read_only_aegis_args(args),
            Some((program, _)) => simple.reads_pipe && PIPE_FILTERS.contains(program),
            None => index == last && !simple.reads_pipe,
        })
}

fn is_read_only_aegis_args(args: &[&str]) -> bool {
    let is_help_flag = |arg: &str| arg == "--help" || arg == "-h";
    match args {
        ["--version" | "-V"] | ["status"] | ["snapshot", "list"] => true,
        ["config", "show" | "validate", ..] | ["audit", ..] | ["help", ..] => true,
        [words @ .., last] if is_help_flag(last) => words.iter().all(|word| !word.starts_with('-')),
        _ => false,
    }
}

struct SimpleCommand<'a> {
    words: Vec<&'a str>,
    reads_pipe: bool,
}

fn split_simple_commands(command: &str) -> Option<Vec<SimpleCommand<'_>>> {
    if !command
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " \t\n-_=.,:/+@|&;>".contains(c))
    {
        return None;
    }

    let mut commands = vec![SimpleCommand {
        words: Vec::new(),
        reads_pipe: false,
    }];
    let bytes = command.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let next = bytes.get(i + 1).copied();
        match bytes[i] {
            b' ' | b'\t' => i += 1,
            b'&' if next == Some(b'&') => {
                commands.push(new_command(false));
                i += 2;
            }
            b'|' if next == Some(b'|') => {
                commands.push(new_command(false));
                i += 2;
            }
            b'|' if next == Some(b'&') => return None,
            b'|' => {
                commands.push(new_command(true));
                i += 1;
            }
            b';' | b'\n' => {
                commands.push(new_command(false));
                i += 1;
            }
            b'&' if next == Some(b'>') => i = skip_redirection(command, i + 1)?,
            b'&' => return None,
            b'>' => i = skip_redirection(command, i)?,
            _ => {
                let start = i;
                while i < bytes.len() && !b" \t\n|&;>".contains(&bytes[i]) {
                    i += 1;
                }
                let word = &command[start..i];
                let is_descriptor = word.len() == 1 && word.as_bytes()[0].is_ascii_digit();
                if bytes.get(i) == Some(&b'>') && is_descriptor {
                    continue;
                }
                commands.last_mut()?.words.push(word);
            }
        }
    }
    Some(commands)
}

fn new_command(reads_pipe: bool) -> SimpleCommand<'static> {
    SimpleCommand {
        words: Vec::new(),
        reads_pipe,
    }
}

fn skip_redirection(command: &str, start: usize) -> Option<usize> {
    let bytes = command.as_bytes();
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'&') {
        let descriptor = bytes.get(i + 1)?;
        let after = bytes.get(i + 2);
        let ends = after.is_none_or(|b| b" \t\n|&;".contains(b));
        return (descriptor.is_ascii_digit() && ends).then_some(i + 2);
    }
    if bytes.get(i) == Some(&b'>') {
        i += 1;
    }
    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    let target_start = i;
    while i < bytes.len() && !b" \t\n|&;>".contains(&bytes[i]) {
        i += 1;
    }
    (&command[target_start..i] == "/dev/null").then_some(i)
}
