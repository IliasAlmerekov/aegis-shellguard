pub(super) fn quote_open_at_end(text: &str) -> bool {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut backtick = false;
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !single_quote => {
                chars.next();
            }
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '`' if !single_quote => backtick = !backtick,
            _ => {}
        }
    }

    single_quote || double_quote || backtick
}
