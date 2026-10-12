//! Split into 5 parts for consistent test organization.
//!
//! Antikythera Log Module Tests
//!
//! Tests for the unified logging system including:
//! - Basic logging operations
//! - Log filtering and pagination
//! - Log serialization
//! - Session-based logging

use antikythera_log::*;

include!("logger_tests/basic_logging_levels.rs");
include!("logger_tests/log_filter_json.rs");
include!("logger_tests/log_source_context.rs");
include!("logger_tests/clear_session_id.rs");
include!("logger_tests/serialization_capacity.rs");
