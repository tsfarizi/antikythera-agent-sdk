//! Cooperative task cancellation for the multi-agent orchestrator.
//!
//! [`CancellationToken`] is a lightweight, cloneable handle backed by a
//! shared `Arc<AtomicBool>`.  Multiple tasks can hold child tokens derived
//! from the same root; cancelling the root cancels all children simultaneously.
//!
//! # Example
//!
//! ```rust
//! use antikythera_core::application::agent::multi_agent::cancellation::CancellationToken;
//!
//! let token = CancellationToken::new();
//! let child = token.child_token();
//!
//! assert!(!token.is_cancelled());
//! token.cancel();
//! assert!(token.is_cancelled());
//! assert!(child.is_cancelled(), "child shares the same flag");
//! ```

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

/// A cloneable cancellation signal.
///
/// All tokens created via [`child_token`] share the same underlying flag, so
/// cancelling one cancels all.  This is intentional — the orchestrator owns
/// the root token and distributes child tokens to individual task executors.
///
/// [`child_token`]: CancellationToken::child_token
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Create a new, non-cancelled token.
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Signal cancellation.  All tokens sharing this flag will observe
    /// [`is_cancelled`] returning `true` after this call.
    ///
    /// [`is_cancelled`]: CancellationToken::is_cancelled
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if [`cancel`] has been called.
    ///
    /// [`cancel`]: CancellationToken::cancel
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Returns a new token that shares the same underlying cancellation flag.
    pub fn child_token(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// A serialisable snapshot of the cancellation state, suitable for embedding
/// in [`TaskExecutionMetadata`].
///
/// [`TaskExecutionMetadata`]: super::task::TaskExecutionMetadata
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
/// CancellationHandle (serialisable snapshot for introspection).
///
pub struct CancellationSnapshot {
    /// Whether the task was cancelled externally before or during execution.
    pub was_cancelled: bool,
}

impl From<&CancellationToken> for CancellationSnapshot {
    fn from(t: &CancellationToken) -> Self {
        Self {
            was_cancelled: t.is_cancelled(),
        }
    }
}
