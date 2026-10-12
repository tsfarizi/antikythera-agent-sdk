//! Security crate integration tests — secrets management subsystem.

use antikythera_domain::security::SecretsConfig;
use antikythera_security::secrets::{SecretManager, SecretManagerError};

include!("secrets_tests/crud_operations.rs");
include!("secrets_tests/versioning.rs");
include!("secrets_tests/error_cases.rs");
