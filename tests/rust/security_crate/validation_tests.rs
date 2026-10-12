//! Security crate integration tests — validation subsystem.

use antikythera_domain::security::ValidationConfig;
use antikythera_security::validation::{InputValidator, ValidationResult};

include!("validation_tests/size_and_length.rs");
include!("validation_tests/url_patterns.rs");
include!("validation_tests/html_sanitization.rs");
include!("validation_tests/json_structure.rs");
include!("validation_tests/keyword_blocking.rs");
include!("validation_tests/tool_input.rs");
include!("validation_tests/comprehensive.rs");
include!("validation_tests/config_update.rs");
