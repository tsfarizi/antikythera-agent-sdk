//! Security crate integration tests — rate limiting subsystem.

use antikythera_domain::security::RateLimitConfig;
use antikythera_security::rate_limit::{RateLimitError, RateLimiter};

include!("rate_limit_tests/sliding_window.rs");
include!("rate_limit_tests/session_lifecycle.rs");
include!("rate_limit_tests/concurrent_sessions.rs");
include!("rate_limit_tests/config_update.rs");
