//! Split by concern to keep file size manageable and improve readability.
//!
//! Comprehensive Logging Module Tests
//!
//! Extensive test suite for antikythera-log with focus on:
//! - Edge cases and boundary conditions
//! - Concurrency safety and race conditions
//! - Security: input validation, injection prevention
//! - Performance: memory leaks, bounds
//! - Panic safety: no unwrap/expect in hot paths
//! - Data integrity: serialization, ordering

use antikythera_log::*;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

include!("comprehensive_logging_tests/edge_cases_boundaries.rs");
include!("comprehensive_logging_tests/concurrency_thread_safety.rs");
include!("comprehensive_logging_tests/input_validation_security.rs");
include!("comprehensive_logging_tests/data_integrity_serialization.rs");
include!("comprehensive_logging_tests/performance_resource.rs");
include!("comprehensive_logging_tests/panic_safety_error.rs");
include!("comprehensive_logging_tests/filtering_edge_cases.rs");
include!("comprehensive_logging_tests/clone_share_behavior.rs");
