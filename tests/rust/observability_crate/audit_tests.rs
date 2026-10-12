//! Observability crate integration tests — audit subsystem.

use antikythera_observability::{AuditCategory, AuditRecord, AuditTrail};

include!("audit_tests/category_filter.rs");
include!("audit_tests/record_details.rs");
include!("audit_tests/port_trait.rs");
