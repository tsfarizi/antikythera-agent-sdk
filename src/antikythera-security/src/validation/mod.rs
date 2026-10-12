//! Input validation and sanitization.
//!
//! Concrete implementation of `antikythera_ports::InputValidator` plus
//! richer API for size, keyword, HTML, JSON, and tool-input validation.

pub mod json;
pub mod types;
pub mod url;

pub use types::{ValidationError, ValidationResult};

use json::JSONValidator;
use url::URLValidator;

use antikythera_domain::security::ValidationConfig;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use thiserror::Error;

/// Errors raised during validator construction or reconfiguration.
#[derive(Debug, Clone, Error)]
pub enum InputValidatorError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Input rejected: {0}")]
    Rejected(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}

/// Configurable input validator.
pub struct InputValidator {
    config: ValidationConfig,
    url_validator: URLValidator,
    json_validator: JSONValidator,
    blocked_keywords_set: HashSet<String>,
}

impl InputValidator {
    /// Create a validator from the given config.
    ///
    /// Compiles all URL regex patterns up front; returns an error if any
    /// pattern is invalid.
    pub fn new(config: ValidationConfig) -> Result<Self, InputValidatorError> {
        let allowed_url_regexes = config
            .allowed_url_patterns
            .iter()
            .map(|pattern| {
                Regex::new(pattern).map_err(|e| {
                    InputValidatorError::Configuration(format!("Invalid allowed URL pattern: {e}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let blocked_url_regexes = config
            .blocked_url_patterns
            .iter()
            .map(|pattern| {
                Regex::new(pattern).map_err(|e| {
                    InputValidatorError::Configuration(format!("Invalid blocked URL pattern: {e}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let blocked_keywords_set = config
            .blocked_keywords
            .iter()
            .map(|k| k.to_lowercase())
            .collect();

        Ok(Self {
            url_validator: URLValidator::new(allowed_url_regexes, blocked_url_regexes),
            json_validator: JSONValidator::new(
                config.max_json_nesting_depth,
                config.max_json_array_length,
            ),
            config,
            blocked_keywords_set,
        })
    }

    /// Create a validator with default config.
    pub fn from_config() -> Result<Self, InputValidatorError> {
        Self::new(ValidationConfig::default())
    }

    /// Validate raw byte-size against the configured maximum.
    pub fn validate_size(&self, input: &str) -> ValidationResult {
        let size = input.len() as u64;
        if size > self.config.max_input_size_bytes {
            return ValidationResult::Invalid(format!(
                "Input size {size} bytes exceeds maximum {} bytes",
                self.config.max_input_size_bytes
            ));
        }
        ValidationResult::Valid
    }

    /// Validate character-count against the configured maximum.
    pub fn validate_message_length(&self, message: &str) -> ValidationResult {
        let length = message.chars().count();
        if length > self.config.max_message_length {
            return ValidationResult::Invalid(format!(
                "Message length {length} exceeds maximum {}",
                self.config.max_message_length
            ));
        }
        ValidationResult::Valid
    }

    /// Validate a URL against allowed/blocked patterns.
    pub fn validate_url(&self, url: &str) -> ValidationResult {
        self.url_validator.validate(url)
    }

    /// Check whether input contains any blocked keyword.
    pub fn check_blocked_keywords(&self, input: &str) -> ValidationResult {
        let lower_input = input.to_lowercase();
        for keyword in &self.blocked_keywords_set {
            if lower_input.contains(keyword) {
                return ValidationResult::Invalid(format!(
                    "Input contains blocked keyword: {keyword}"
                ));
            }
        }
        ValidationResult::Valid
    }

    /// Basic HTML sanitization — strips dangerous tags and handlers.
    pub fn sanitize_html(&self, html: &str) -> String {
        if !self.config.sanitize_html {
            return html.to_string();
        }

        html.replace("<script", "")
            .replace("</script>", "")
            .replace("javascript:", "")
            .replace("onerror=", "")
            .replace("onload=", "")
            .replace("onclick=", "")
    }

    /// Parse and validate JSON structure against depth/array limits.
    pub fn validate_json(&self, json_str: &str) -> Result<Value, InputValidatorError> {
        if !self.config.validate_json_schema {
            return serde_json::from_str(json_str)
                .map_err(|e| InputValidatorError::InvalidInput(e.to_string()));
        }

        let value: Value = serde_json::from_str(json_str)
            .map_err(|e| InputValidatorError::InvalidInput(e.to_string()))?;

        self.json_validator
            .validate_structure(&value, 0)
            .map_err(InputValidatorError::Rejected)?;

        Ok(value)
    }

    /// Validate tool call input: size, JSON, keywords, URLs.
    pub fn validate_tool_input(&self, _tool_name: &str, input: &str) -> ValidationResult {
        if let ValidationResult::Invalid(msg) = self.validate_size(input) {
            return ValidationResult::Invalid(msg);
        }

        if let Err(msg) = self.validate_json(input) {
            return ValidationResult::Invalid(format!("Invalid JSON in tool input: {msg}"));
        }

        if let ValidationResult::Invalid(msg) = self.check_blocked_keywords(input) {
            return ValidationResult::Invalid(msg);
        }

        if let Ok(json) = self.validate_json(input) {
            let res = self.validate_urls_in_json(&json);
            if let ValidationResult::Invalid(_) = res {
                return res;
            }
        }

        ValidationResult::Valid
    }

    /// Validate concurrent tool call count against the limit.
    pub fn validate_concurrent_calls(&self, current_calls: u32) -> ValidationResult {
        if current_calls >= self.config.max_concurrent_tool_calls {
            return ValidationResult::Invalid(format!(
                "Concurrent tool calls {current_calls} exceeds maximum {}",
                self.config.max_concurrent_tool_calls
            ));
        }
        ValidationResult::Valid
    }

    /// Run all enabled validations (size, message length, keywords).
    pub fn validate(&self, input: &str) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if let ValidationResult::Invalid(msg) = self.validate_size(input) {
            errors.push(ValidationError {
                field: "size".to_string(),
                message: msg,
            });
        }

        if let ValidationResult::Invalid(msg) = self.validate_message_length(input) {
            errors.push(ValidationError {
                field: "message_length".to_string(),
                message: msg,
            });
        }

        if let ValidationResult::Invalid(msg) = self.check_blocked_keywords(input) {
            errors.push(ValidationError {
                field: "keywords".to_string(),
                message: msg,
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get current configuration reference.
    pub fn config(&self) -> &ValidationConfig {
        &self.config
    }

    /// Replace config and rebuild internal validators.
    pub fn update_config(&mut self, config: ValidationConfig) -> Result<(), InputValidatorError> {
        let allowed_url_patterns = config.allowed_url_patterns.clone();
        let blocked_url_patterns = config.blocked_url_patterns.clone();

        self.config = config;

        let allowed_url_regexes = allowed_url_patterns
            .iter()
            .map(|pattern| {
                Regex::new(pattern).map_err(|e| {
                    InputValidatorError::Configuration(format!("Invalid allowed URL pattern: {e}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let blocked_url_regexes = blocked_url_patterns
            .iter()
            .map(|pattern| {
                Regex::new(pattern).map_err(|e| {
                    InputValidatorError::Configuration(format!("Invalid blocked URL pattern: {e}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.url_validator = URLValidator::new(allowed_url_regexes, blocked_url_regexes);
        self.json_validator = JSONValidator::new(
            self.config.max_json_nesting_depth,
            self.config.max_json_array_length,
        );

        self.blocked_keywords_set = self
            .config
            .blocked_keywords
            .iter()
            .map(|k| k.to_lowercase())
            .collect();

        Ok(())
    }

    fn validate_urls_in_json(&self, value: &Value) -> ValidationResult {
        match value {
            Value::String(s) if s.starts_with("http://") || s.starts_with("https://") => {
                self.validate_url(s)
            }
            Value::Array(arr) => {
                for item in arr {
                    let res = self.validate_urls_in_json(item);
                    if let ValidationResult::Invalid(_) = res {
                        return res;
                    }
                }
                ValidationResult::Valid
            }
            Value::Object(obj) => {
                for v in obj.values() {
                    let res = self.validate_urls_in_json(v);
                    if let ValidationResult::Invalid(_) = res {
                        return res;
                    }
                }
                ValidationResult::Valid
            }
            _ => ValidationResult::Valid,
        }
    }
}

#[async_trait::async_trait]
impl antikythera_ports::InputValidator for InputValidator {
    fn validate_input(&self, input: &str, max_size: usize) -> Result<String, String> {
        if input.len() > max_size {
            return Err(format!(
                "Input size {} bytes exceeds maximum {max_size} bytes",
                input.len()
            ));
        }

        if let ValidationResult::Invalid(msg) = self.validate_message_length(input) {
            return Err(msg);
        }

        if let ValidationResult::Invalid(msg) = self.check_blocked_keywords(input) {
            return Err(msg);
        }

        let sanitized = self.sanitize_html(input);
        Ok(sanitized)
    }

    fn validate_url(&self, url: &str) -> Result<(), String> {
        match self.url_validator.validate(url) {
            ValidationResult::Valid => Ok(()),
            ValidationResult::Invalid(msg) => Err(msg),
            ValidationResult::Sanitized(_) => Ok(()),
        }
    }
}
