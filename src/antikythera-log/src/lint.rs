//! CLI output macros live in `macros.rs` so they are always available.
//!
//! # Compile-Time Logging Lint
//!
//! This module provides compile-time enforcement that ensures all logging
//! in the Antikythera framework uses the official `antikythera-log` system.
//!
//! ## How It Works
//!
//! When the `lint` feature is enabled, this module shadows common logging
//! macros (`println!`, `eprintln!`, `dbg!`) and tracing macros with
//! versions that produce **compile errors**, forcing developers to use
//! the `antikythera-log` Logger API or the `alog_*!` macros instead.
//!
//! ## Usage
//!
//! In your crate's `Cargo.toml`:
//! ```toml
//! [dependencies]
//! antikythera-log = { path = "../antikythera-log", features = ["lint"] }
//! ```
//!
//! In your crate's `lib.rs` or `main.rs`:
//! ```rust,ignore
//! // This MUST be at the crate root to shadow the built-in macros
//! #[cfg(feature = "antikythera-log/lint")]
//! use antikythera_log::lint::*;
//! ```
//!
//! ## Exceptions
//!
//! For legitimate CLI output (e.g., `--help`, config wizard), use the
//! `cli_print!` and `cli_eprintln!` macros which are explicitly allowed:
//!
//! ```rust,ignore
//! use antikythera_log::{cli_print, cli_eprint};
//!
//! cli_print!("Configuration saved to: {}", path);
//! cli_eprint!("Error: invalid argument");
//! ```

/// **BLOCKED** — Use `alog_debug!`, `alog_info!`, `alog_warn!`, or `alog_error!` instead.
///
/// Direct `println!` bypasses the structured logging system.
/// For legitimate CLI output, use `cli_print!` instead.
#[macro_export]
/// Shadow macros that produce compile errors.
///
macro_rules! println {
    ($($arg:tt)*) => {
        compile_error!(
            "Direct `println!` is forbidden in Antikythera crates. \
             Use `alog_info!(logger, ...)` from antikythera-log, or \
             `cli_print!(...)` for legitimate CLI output."
        )
    };
}

/// **BLOCKED** — Use `alog_error!` or `alog_warn!` instead.
///
/// Direct `eprintln!` bypasses the structured logging system.
/// For legitimate CLI error output, use `cli_eprint!` instead.
#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => {
        compile_error!(
            "Direct `eprintln!` is forbidden in Antikythera crates. \
             Use `alog_error!(logger, ...)` from antikythera-log, or \
             `cli_eprint!(...)` for legitimate CLI output."
        )
    };
}

/// **BLOCKED** — Use `alog_debug!` instead.
///
/// `dbg!` bypasses the structured logging system and writes to stderr.
#[macro_export]
macro_rules! dbg {
    ($($arg:tt)*) => {
        compile_error!(
            "Direct `dbg!` is forbidden in Antikythera crates. \
             Use `alog_debug!(logger, ...)` from antikythera-log instead."
        )
    };
}

/// **BLOCKED** — Use `alog_error!` instead.
#[macro_export]
macro_rules! tracing_error {
    ($($arg:tt)*) => {
        compile_error!(
            "`tracing::error!` / `error!` is forbidden. \
             Use `alog_error!(logger, ...)` from antikythera-log instead."
        )
    };
}

/// **BLOCKED** — Use `alog_warn!` instead.
#[macro_export]
macro_rules! tracing_warn {
    ($($arg:tt)*) => {
        compile_error!(
            "`tracing::warn!` / `warn!` is forbidden. \
             Use `alog_warn!(logger, ...)` from antikythera-log instead."
        )
    };
}

/// **BLOCKED** — Use `alog_info!` instead.
#[macro_export]
macro_rules! tracing_info {
    ($($arg:tt)*) => {
        compile_error!(
            "`tracing::info!` / `info!` is forbidden. \
             Use `alog_info!(logger, ...)` from antikythera-log instead."
        )
    };
}

/// **BLOCKED** — Use `alog_debug!` instead.
#[macro_export]
macro_rules! tracing_debug {
    ($($arg:tt)*) => {
        compile_error!(
            "`tracing::debug!` / `debug!` is forbidden. \
             Use `alog_debug!(logger, ...)` from antikythera-log instead."
        )
    };
}

/// **BLOCKED** — Use `alog_debug!` instead.
#[macro_export]
macro_rules! tracing_trace {
    ($($arg:tt)*) => {
        compile_error!(
            "`tracing::trace!` / `trace!` is forbidden. \
             Use `alog_debug!(logger, ...)` from antikythera-log instead."
        )
    };
}
