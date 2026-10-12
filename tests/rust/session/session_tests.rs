//! Split into 5 parts for consistent test organization.
//!
//! Antikythera Session Module Tests
//!
//! Tests for session management including:
//! - Session creation and deletion
//! - Message handling
//! - Session export/import
//! - Batch operations

use antikythera_session::*;

include!("session_tests/create_add_message.rs");
include!("session_tests/list_delete.rs");
include!("session_tests/clear_export_import.rs");
include!("session_tests/batch_search.rs");
include!("session_tests/user_query_serialization.rs");
