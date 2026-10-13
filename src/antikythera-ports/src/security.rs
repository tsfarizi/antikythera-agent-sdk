//! Port: Security
//!
//! Application defines these traits. Infrastructure (CLI security module) implements them.

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use async_trait::async_trait;

/// Rate limiting abstraction.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if a request is allowed under the rate limit.
    /// Returns Ok(()) if allowed, Err(message) if rate limited.
    async fn check_rate_limit(&self, key: &str) -> Result<(), String>;

    /// Record a request against the rate limit.
    async fn record_request(&self, key: &str);
}

/// Input validation abstraction.
pub trait InputValidator: Send + Sync {
    /// Validate input text, returning Ok(sanitized) or Err(reason).
    fn validate_input(&self, input: &str, max_size: usize) -> Result<String, String>;

    /// Validate a URL format.
    fn validate_url(&self, url: &str) -> Result<(), String>;
}

/// Secret storage abstraction.
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Store a secret.
    async fn store_secret(&self, id: &str, secret: &[u8]) -> Result<(), String>;

    /// Retrieve a secret.
    async fn get_secret(&self, id: &str) -> Result<Vec<u8>, String>;

    /// Delete a secret.
    async fn delete_secret(&self, id: &str) -> Result<(), String>;
}
