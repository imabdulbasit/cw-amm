#![deny(
    missing_debug_implementations,
    clippy::missing_panics_doc,
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::print_stdout
)]

pub mod contract;
pub mod error;
pub mod msg;
pub mod state;
