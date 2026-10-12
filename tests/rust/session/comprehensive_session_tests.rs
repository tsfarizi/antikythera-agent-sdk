//! Split by concern to keep file size manageable and improve readability.
//!
//! Comprehensive Session Module Tests
//!
//! Extensive test suite for antikythera-session with focus on:
//! - Session creation and lifecycle
//! - Concurrent session management
//! - Message integrity and ordering
//! - Serialization/deserialization roundtrips
//! - Data corruption recovery
//! - Performance under load

use antikythera_session::*;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

include!("comprehensive_session_tests/message_creation_types.rs");
include!("comprehensive_session_tests/message_serialization.rs");
include!("comprehensive_session_tests/session_lifecycle.rs");
include!("comprehensive_session_tests/session_manager_ops.rs");
include!("comprehensive_session_tests/concurrent_sessions.rs");
include!("comprehensive_session_tests/ordering_capacity.rs");
include!("comprehensive_session_tests/export_summary.rs");
include!("comprehensive_session_tests/clone_behavior.rs");
include!("comprehensive_session_tests/edge_case_ids.rs");
include!("comprehensive_session_tests/performance_stress.rs");
