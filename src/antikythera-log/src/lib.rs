//! # Antikythera Log
//!
//! Unified logging system for the Antikythera Agent SDK.
//!
//! ## Features
//!
//! - **Structured logging** - JSON-encoded log entries with context
//! - **Multiple levels** - Debug, Info, Warn, Error
//! - **Subscription system** - Real-time log streaming via subscribers
//! - **Periodic polling** - Fetch logs on-demand without subscription
//! - **WASM compatible** - Works in both native and WASM environments
//! - **Session tracking** - Logs grouped by session ID for traceability
//! - **Convenience macros** - `alog_info!`, `alog_debug!`, `alog_warn!`, `alog_error!`
//! - **Compile-time lint** - Blocks `println!`, `eprintln!`, `dbg!`, and `tracing` macros
//!
//! ## Usage
//!
//! ### Basic Logging (No Subscription)
//!
//! ```rust
//! use antikythera_log::{Logger, LogLevel, LogFilter, alog_info, alog_debug};
//!
//! let logger = Logger::new("my-session");
//! alog_info!(logger, "Agent started on port {}", 8080);
//! alog_debug!(logger, "Processing LLM response");
//! logger.warn("Max steps approaching");
//! logger.error("Tool execution failed");
//!
//! // Fetch logs periodically
//! let logs = logger.get_logs(&LogFilter::new());
//! ```
//!
//! ### With Source Module
//!
//! ```rust
//! use antikythera_log::{Logger, alog_info_src};
//!
//! let logger = Logger::new("my-session");
//! alog_info_src!(logger, "transport", "Connected to {}", "mcp-server");
//! ```
//!
//! ### Subscription (Real-time Streaming)
//!
//! ```rust,ignore
//! use antikythera_log::{Logger, LogSubscriber};
//!
//! let logger = Logger::new("my-session");
//!
//! // Create subscriber
//! let subscriber = logger.subscribe();
//!
//! // In another thread/task:
//! // while let Ok(entry) = subscriber.recv() {
//! //     cli_print!("[{}] {}", entry.level, entry.message);
//! // }
//!
//! // Logs are automatically sent to all subscribers
//! logger.info("This will be sent to subscriber in real-time");
//! ```
//!
//! ### Compile-Time Lint (Feature: `lint`)
//!
//! Enable the `lint` feature to block non-standard logging at compile time:
//!
//! ```toml
//! antikythera-log = { path = "../antikythera-log", features = ["lint"] }
//! ```
//!
//! This will produce compile errors for `println!`, `eprintln!`, `dbg!`,
//! and bare `tracing` macros. Use `cli_print!` / `cli_eprint!` for
//! legitimate CLI output.

#[macro_use]
/// Macros must be declared before other modules so they are available to all downstream code.
///
pub mod macros;

pub mod entries;
pub mod logger;
pub mod wasm_compat;

#[cfg(feature = "subscriber")]
pub mod subscriber;

pub mod session_logger;
pub mod session_logs;

/// Compile-time lint module.
///
/// When the `lint` feature is enabled, importing this module's macros
/// will shadow `println!`, `eprintln!`, `dbg!`, and tracing macros
/// with versions that produce compile errors.
#[cfg(feature = "lint")]
pub mod lint;

pub use entries::*;
pub use logger::*;

#[cfg(feature = "subscriber")]
pub use subscriber::{LogSender, LogSubscriber};

pub use session_logger::{SessionLogger, clear_all_loggers, get_logger, logger_count};
pub use session_logs::{BatchLogExport, SessionLogExport};

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-log/lib.tests.rs"]
mod tests;
