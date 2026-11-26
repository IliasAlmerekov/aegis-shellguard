#[macro_use]
mod output;
#[doc(hidden)]
pub use output::{err_line, err_raw, out_line, out_raw};

pub mod analysis;
pub mod error;
pub mod explanation;
pub mod planning;
pub mod runtime;
pub mod runtime_gate;
pub mod toggle;
pub mod update;
pub mod watch;

#[cfg(test)]
mod test_git;
