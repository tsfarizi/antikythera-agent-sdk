//! Execution mode configuration for the multi-agent orchestrator.
//!
//! Defines how tasks are scheduled across available compute resources.
//! The three modes cover the full spectrum from strictly sequential
//! execution to explicit multi-thread parallelism.

use serde::{Deserialize, Serialize};
use std::thread;

/// Controls how the [`super::scheduler::TaskScheduler`] dispatches tasks to agents.
///
/// # Choosing a mode
///
/// | Scenario | Recommended mode |
/// |---|---|
/// | General-purpose (default) | [`ExecutionMode::Auto`] |
/// | WASM or deterministic ordering | [`ExecutionMode::Sequential`] |
/// | Single-thread async interleaving | [`ExecutionMode::Concurrent`] |
/// | Explicit CPU-bound parallelism | [`ExecutionMode::Parallel`] |
///
/// # FFI / WASM host usage
///
/// When calling from a host language over FFI, specify the mode as a string
/// via [`ExecutionMode::from_spec`].  The host does **not** manage threads
/// directly – it only declares its preference.
///
/// ```text
/// "auto"          → ExecutionMode::Auto
/// "sequential"    → ExecutionMode::Sequential
/// "concurrent"    → ExecutionMode::Concurrent
/// "parallel:4"    → ExecutionMode::Parallel { workers: 4 }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ExecutionMode {
    /// **Default.** Tasks are spawned as independent tokio tasks and run
    /// concurrently.  On a multi-thread runtime every available CPU can be
    /// used; on a single-thread runtime tasks are interleaved cooperatively.
    ///
    /// This is the right choice for most workloads.
    #[default]
    Auto,

    /// Tasks are awaited one at a time in the order they were submitted.
    ///
    /// Useful when task B depends on the output of task A, or when strict
    /// ordering matters for debugging and testing.
    Sequential,

    /// Tasks are driven concurrently without spawning new tokio tasks.
    ///
    /// All futures are polled on the *calling* task's thread using a
    /// [`FuturesUnordered`] collector.  This is safe in single-threaded WASM
    /// environments and avoids the overhead of `tokio::spawn`.
    ///
    /// [`FuturesUnordered`]: futures::stream::FuturesUnordered
    Concurrent,

    /// Tasks are spawned as tokio tasks, but at most `workers` are allowed
    /// to run simultaneously via an async semaphore.
    ///
    /// On a `rt-multi-thread` runtime each spawned task may execute on a
    /// different OS thread, providing true parallelism.  On a
    /// `current_thread` runtime tasks are still interleaved cooperatively but
    /// the concurrency limit still applies.
    Parallel {
        /// Maximum number of tasks that may be in-flight at the same time.
        /// A value of 0 is treated as 1 (one task at a time).
        workers: usize,
    },
}

impl ExecutionMode {
    /// Construct an [`ExecutionMode::Auto`] mode value.
    pub fn auto() -> Self {
        Self::Auto
    }

    /// Construct a [`ExecutionMode::Sequential`] mode value.
    pub fn sequential() -> Self {
        Self::Sequential
    }

    /// Construct a [`ExecutionMode::Concurrent`] mode value.
    pub fn concurrent() -> Self {
        Self::Concurrent
    }

    /// Construct a [`ExecutionMode::Parallel`] mode value with the given worker limit.
    pub fn parallel(workers: usize) -> Self {
        Self::Parallel {
            workers: workers.max(1),
        }
    }

    /// Detect a reasonable mode from the current environment.
    ///
    /// - If more than one CPU is available: `Parallel { workers: min(cpus, 8) }`
    /// - Otherwise: `Concurrent`
    ///
    /// This is intended as a convenience for native binaries that want to
    /// pick sensible defaults at runtime.
    pub fn detect() -> Self {
        let cpus = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        if cpus > 1 {
            Self::Parallel {
                workers: cpus.min(8),
            }
        } else {
            Self::Concurrent
        }
    }

    /// Parse from a human-readable spec string.
    ///
    /// Accepted formats:
    /// - `"auto"` → [`ExecutionMode::Auto`]
    /// - `"sequential"` → [`ExecutionMode::Sequential`]
    /// - `"concurrent"` → [`ExecutionMode::Concurrent`]
    /// - `"parallel:N"` where N is a positive integer → [`ExecutionMode::Parallel`]
    ///
    /// Returns `None` for unrecognised strings.
    pub fn from_spec(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "sequential" => Some(Self::Sequential),
            "concurrent" => Some(Self::Concurrent),
            s if s.starts_with("parallel:") => {
                let n: usize = s[9..].parse().ok()?;
                Some(Self::Parallel { workers: n.max(1) })
            }
            _ => None,
        }
    }

    /// Serialise to the spec string accepted by [`Self::from_spec`].
    pub fn to_spec(self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Sequential => "sequential".to_string(),
            Self::Concurrent => "concurrent".to_string(),
            Self::Parallel { workers } => format!("parallel:{workers}"),
        }
    }

    /// Return `true` if this mode may spawn multiple concurrent tasks.
    pub fn is_concurrent(&self) -> bool {
        !matches!(self, Self::Sequential)
    }

    /// Return `true` if this mode spawns tokio tasks (and may use multiple
    /// OS threads on a multi-thread runtime).
    pub fn spawns_tasks(&self) -> bool {
        matches!(self, Self::Auto | Self::Parallel { .. })
    }
}

impl std::fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_spec())
    }
}
