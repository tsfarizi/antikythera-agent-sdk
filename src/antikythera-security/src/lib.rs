//! Security implementations for the Antikythera Agent SDK.
//!
//! Concrete implementations of the security port traits defined in
//! `antikythera_ports::security`, plus richer APIs for each subsystem.

pub mod error;
pub mod facade;

#[cfg(feature = "validation")]
pub mod validation;

#[cfg(feature = "rate-limit")]
pub mod rate_limit;

#[cfg(feature = "memory")]
pub mod secrets;

pub use error::SecurityError;
pub use facade::SecurityFacade;

#[cfg(feature = "validation")]
pub use validation::{InputValidator, InputValidatorError, ValidationError, ValidationResult};

#[cfg(feature = "rate-limit")]
pub use rate_limit::{RateLimitError, RateLimiter, SessionUsage};

#[cfg(feature = "memory")]
pub use secrets::{SecretManager, SecretManagerError};
