//! TOML Configuration Tests

use antikythera_sdk::config::*;

include!("config_tests/config_serialization.rs");
include!("config_tests/config_custom_values.rs");
include!("config_tests/config_size.rs");
include!("config_tests/config_defaults.rs");
