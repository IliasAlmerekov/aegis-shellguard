use std::iter::Peekable;
use std::str::CharIndices;

#[derive(Default)]
pub(crate) struct QuoteState {
    in_single_quote: bool,
    in_double_quote: bool,
    in_backticks: bool,
    paren_depth: usize,
    command_subst_depth: usize,
}

impl QuoteState {
    pub(crate) fn copy_quoting_syntax(
        &mut self,
        ch: char,
        chars: &mut Peekable<CharIndices<'_>>,
        out: &mut String,
    ) -> bool {
        let next = chars.peek().map(|&(_, c)| c);
        match ch {
            '\\' if !self.in_single_quote => {
                out.push(ch);
                out.extend(chars.next().map(|(_, c)| c));
            }
            '\'' if !self.in_double_quote && !self.in_backticks => {
                self.in_single_quote = !self.in_single_quote;
                out.push(ch);
            }
            '"' if !self.in_single_quote && !self.in_backticks => {
                self.in_double_quote = !self.in_double_quote;
                out.push(ch);
            }
            '`' if !self.in_single_quote => {
                self.in_backticks = !self.in_backticks;
                out.push(ch);
            }
            '$' if !self.in_single_quote && !self.in_backticks && next == Some('(') => {
                self.command_subst_depth += 1;
                out.push(ch);
                out.extend(chars.next().map(|(_, c)| c));
            }
            '(' if self.is_unquoted() && self.command_subst_depth == 0 => {
                self.paren_depth += 1;
                out.push(ch);
            }
            ')' if !self.in_single_quote
                && !self.in_backticks
                && (self.command_subst_depth > 0 || self.paren_depth > 0) =>
            {
                if self.command_subst_depth > 0 {
                    self.command_subst_depth -= 1;
                } else {
                    self.paren_depth -= 1;
                }
                out.push(ch);
            }
            _ => return false,
        }
        true
    }

    fn is_unquoted(&self) -> bool {
        !self.in_single_quote && !self.in_double_quote && !self.in_backticks
    }

    pub(crate) fn is_top_level(&self) -> bool {
        self.is_unquoted() && self.paren_depth == 0 && self.command_subst_depth == 0
    }

    pub(crate) fn allows_brace_open(&self) -> bool {
        self.is_unquoted() && self.command_subst_depth == 0
    }

    pub(crate) fn allows_brace_close(&self) -> bool {
        !self.in_single_quote && !self.in_backticks
    }
}

struct Frame {
    command_sub_depth: Option<usize>,
    saved_quotes: (bool, bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scan {
    Plain,
    Quoted,
    Consumed,
    CommandStart(usize),
}

#[derive(Default)]
pub(crate) struct SubstitutionScan {
    single_quote: bool,
    double_quote: bool,
    frames: Vec<Frame>,
}

impl SubstitutionScan {
    pub(crate) fn step(
        &mut self,
        idx: usize,
        ch: char,
        chars: &mut Peekable<CharIndices<'_>>,
    ) -> Scan {
        match ch {
            '\\' if !self.single_quote => {
                chars.next();
            }
            '\'' if !self.double_quote => self.single_quote = !self.single_quote,
            '"' if !self.single_quote => self.double_quote = !self.double_quote,
            '`' if !self.single_quote => {
                if matches!(self.frames.last(), Some(f) if f.command_sub_depth.is_none()) {
                    self.close_frame();
                } else {
                    self.open_frame(None);
                    return Scan::CommandStart(idx + 1);
                }
            }
            '$' if !self.single_quote && chars.peek().map(|&(_, c)| c) == Some('(') => {
                chars.next();
                self.open_frame(Some(1));
                return Scan::CommandStart(idx + 2);
            }
            '(' if self.is_unquoted() => match self.innermost_depth() {
                Some(depth) => *depth += 1,
                None => return Scan::CommandStart(idx + 1),
            },
            ')' if self.is_unquoted() => {
                if let Some(depth) = self.innermost_depth() {
                    *depth -= 1;
                    if *depth == 0 {
                        self.close_frame();
                    }
                }
            }
            _ if self.is_unquoted() => return Scan::Plain,
            _ => return Scan::Quoted,
        }
        Scan::Consumed
    }

    fn is_unquoted(&self) -> bool {
        !self.single_quote && !self.double_quote
    }

    fn innermost_depth(&mut self) -> Option<&mut usize> {
        self.frames
            .last_mut()
            .and_then(|f| f.command_sub_depth.as_mut())
    }

    fn open_frame(&mut self, command_sub_depth: Option<usize>) {
        self.frames.push(Frame {
            command_sub_depth,
            saved_quotes: (self.single_quote, self.double_quote),
        });
        self.single_quote = false;
        self.double_quote = false;
    }

    fn close_frame(&mut self) {
        if let Some(frame) = self.frames.pop() {
            (self.single_quote, self.double_quote) = frame.saved_quotes;
        }
    }
}
