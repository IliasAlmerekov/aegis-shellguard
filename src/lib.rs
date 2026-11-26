#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

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
