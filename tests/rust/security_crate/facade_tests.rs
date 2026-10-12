//! Security crate integration tests — facade.

use antikythera_domain::security::SecurityConfig;
use antikythera_security::SecurityFacade;

#[test]
fn facade_creates_with_default_config() {
    let facade = SecurityFacade::from_config(SecurityConfig::default()).unwrap();
    let _ = &facade.validator;
    let _ = &facade.rate_limiter;
    let _ = &facade.secret_store;
}

#[test]
fn facade_validator_works() {
    let facade = SecurityFacade::from_config(SecurityConfig::default()).unwrap();
    let result = facade.validator.validate_size("hello");
    assert!(matches!(
        result,
        antikythera_security::ValidationResult::Valid
    ));
}

#[tokio::test]
async fn facade_rate_limiter_works() {
    let facade = SecurityFacade::from_config(SecurityConfig::default()).unwrap();
    let result = facade.rate_limiter.check("test");
    assert!(result.is_ok());
}
