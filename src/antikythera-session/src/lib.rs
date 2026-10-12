//! # Antikythera Session
//!
//! Session management for Antikythera Agent SDK with persistent chat history.
//!
//! ## Features
//!
//! - **Concurrent sessions** - Multiple sessions managed simultaneously
//! - **Parallel operations** - Thread-safe session operations
//! - **Persistent state** - Export/import sessions with consistent format
//! - **Chat history** - Full tracking of user, assistant, and tool interactions
//! - **Typed consistency** - Rust type system ensures valid session data
//! - **JSON serialization** - Human-readable format for persistence
//!
//! ## Architecture
//!
//! ```text
//! antikythera-session/
//! ├── session.rs       # Session entity with chat history
//! ├── manager.rs       # Session manager (concurrent access)
//! └── export.rs        # Export/import with JSON serialization
//! ```
//!
//! ## Usage
//!
//! ### Basic Session Management
//!
//! ```rust
//! use antikythera_session::{SessionManager, Message};
//!
//! let manager = SessionManager::new();
//!
//! // Create session
//! let session_id = manager.create_session("user-123", "gpt-4").unwrap();
//!
//! // Add messages
//! manager.add_message(&session_id, Message::user("What's the weather?")).unwrap();
//! manager.add_message(&session_id, Message::assistant("It's 72°F and sunny.")).unwrap();
//!
//! // Get chat history
//! let history = manager.get_chat_history(&session_id).unwrap();
//! ```
//!
//! ### Export/Import Sessions
//!
//! ```rust,ignore
//! use antikythera_session::{SessionManager, SessionExport};
//!
//! let manager = SessionManager::new();
//! let session_id = manager.create_session("user-123", "gpt-4").unwrap();
//!
//! // Export session to JSON
//! let session = manager.get_session(&session_id).unwrap().unwrap();
//! let export = SessionExport::from_session(session);
//! let data = export.to_json().unwrap();
//!
//! // Import session later
//! let export = SessionExport::from_json(&data).unwrap();
//! manager.import_session(export.into_session()).unwrap();
//! ```

pub mod export;
pub mod manager;
pub mod session;

pub use export::*;
pub use manager::*;
pub use session::*;

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-session/lib.tests.rs"]
mod tests;
