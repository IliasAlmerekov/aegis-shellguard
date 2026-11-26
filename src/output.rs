use std::fmt::Arguments;

#[expect(
    clippy::print_stdout,
    reason = "single sanctioned stdout sink; println! keeps closed-pipe behavior"
)]
pub fn out_line(args: Arguments<'_>) {
    println!("{args}");
}

#[expect(
    clippy::print_stdout,
    reason = "single sanctioned stdout sink; print! keeps closed-pipe behavior"
)]
pub fn out_raw(args: Arguments<'_>) {
    print!("{args}");
}

#[expect(
    clippy::print_stderr,
    reason = "single sanctioned stderr sink; eprintln! keeps its behavior"
)]
pub fn err_line(args: Arguments<'_>) {
    eprintln!("{args}");
}

#[expect(
    clippy::print_stderr,
    reason = "single sanctioned stderr sink; eprint! keeps its behavior"
)]
pub fn err_raw(args: Arguments<'_>) {
    eprint!("{args}");
}

#[macro_export]
macro_rules! out {
    ($($arg:tt)*) => { $crate::out_line(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! out_raw {
    ($($arg:tt)*) => { $crate::out_raw(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => { $crate::err_line(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! err_raw {
    ($($arg:tt)*) => { $crate::err_raw(format_args!($($arg)*)) };
}
