//! gRPC services for Global Controller.
//!
//! Provides gRPC endpoints for Meeting Controller and Media Handler registration.
//! All gRPC requests require JWT authentication via the auth layer.

pub mod auth_layer;
pub mod mc_service;
pub mod mh_service;

pub use mc_service::McService;
pub use mh_service::MhService;

use tonic::Status;

/// Maximum allowed registration endpoint length (MC and MH registration;
/// the `VARCHAR(255)` endpoint columns).
pub(crate) const MAX_ENDPOINT_LENGTH: usize = 255;

/// The shape check every registration endpoint shares: non-empty and at most
/// [`MAX_ENDPOINT_LENGTH`]. Scheme/host/port rules are per registrant (MH:
/// `crate::mh_endpoint_policy`; MC: `McService::validate_endpoint`).
pub(crate) fn validate_endpoint_bounds(endpoint: &str, field_name: &str) -> Result<(), Status> {
    if endpoint.is_empty() {
        return Err(Status::invalid_argument(format!(
            "{field_name} is required"
        )));
    }
    if endpoint.len() > MAX_ENDPOINT_LENGTH {
        return Err(Status::invalid_argument(format!(
            "{field_name} is too long"
        )));
    }
    Ok(())
}

/// Maximum allowed handler id length (the `VARCHAR(255)` `handler_id` column).
pub(crate) const MAX_HANDLER_ID_LENGTH: usize = 255;

/// The ONE handler-id rule: non-empty, at most [`MAX_HANDLER_ID_LENGTH`],
/// ASCII alphanumeric plus `-` and `_` (ASCII only: these ids reach the
/// operator-facing `gc.mh.replacement_report` evidence log, where a Unicode
/// homoglyph would read identically to a real id). Used by RegisterMH / SendLoadReport
/// (`handler_id`) and AcquireMediaHandler (`exclude_handler_ids`,
/// `replaced_handler_id`). Messages name the field, never echo the value.
pub(crate) fn validate_handler_id(id: &str, field_name: &str) -> Result<(), Status> {
    if id.is_empty() {
        return Err(Status::invalid_argument(format!(
            "{field_name} is required"
        )));
    }
    if id.len() > MAX_HANDLER_ID_LENGTH {
        return Err(Status::invalid_argument(format!(
            "{field_name} is too long"
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Status::invalid_argument(format!(
            "{field_name} contains invalid characters"
        )));
    }
    Ok(())
}

/// Maximum allowed region length (MC and MH registration).
pub(crate) const MAX_REGION_LENGTH: usize = 50;

/// The ONE region rule for MC and MH registration: non-empty and at most
/// [`MAX_REGION_LENGTH`] bytes.
pub(crate) fn validate_region(region: &str) -> Result<(), Status> {
    if region.is_empty() {
        return Err(Status::invalid_argument("region is required"));
    }
    if region.len() > MAX_REGION_LENGTH {
        return Err(Status::invalid_argument("region is too long"));
    }
    Ok(())
}

/// GC's inbound gRPC layer stack: request span → trace-context extraction →
/// `auth`. The ONE place the ordering is decided (see
/// [`common::observability::otel_grpc::inbound_layers`]); `main.rs` and the
/// test rigs both use it.
#[must_use]
pub fn server_layers(
    auth: auth_layer::GrpcAuthLayer,
) -> common::observability::otel_grpc::InboundLayers<
    common::observability::otel_grpc::GrpcTraceLayer,
    auth_layer::GrpcAuthLayer,
> {
    common::observability::otel_grpc::inbound_layers(
        common::observability::otel_grpc::grpc_trace_layer(),
        auth,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Non-ASCII letters and digits (homoglyphs) are rejected by every id rule.
    #[test]
    fn id_rules_are_ascii_only() {
        for id in ["mh-\u{0430}", "\u{FF11}", "mh-\u{03BF}"] {
            let err = validate_handler_id(id, "handler_id").unwrap_err();
            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{id:?}");
            assert!(err.message().contains("invalid characters"), "{id:?}");
        }
        assert!(validate_handler_id("mh-us-east-1-001", "handler_id").is_ok());
    }

    #[test]
    fn test_validate_handler_id_valid() {
        assert!(validate_handler_id("mh-us-east-1-001", "handler_id").is_ok());
        assert!(validate_handler_id("mh_123", "handler_id").is_ok());
        assert!(validate_handler_id("MH123", "handler_id").is_ok());
    }

    #[test]
    fn test_validate_handler_id_empty() {
        let err = validate_handler_id("", "handler_id").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("required"));
    }

    #[test]
    fn test_validate_handler_id_too_long() {
        let long_id = "a".repeat(256);
        let err = validate_handler_id(&long_id, "handler_id").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_handler_id_at_255_chars() {
        let id_255 = "a".repeat(255);
        assert!(
            validate_handler_id(&id_255, "handler_id").is_ok(),
            "Handler ID at 255 chars should pass"
        );
    }

    #[test]
    fn test_validate_handler_id_invalid_chars() {
        let err = validate_handler_id("mh/invalid", "handler_id").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("invalid characters"));

        let err = validate_handler_id("mh with space", "handler_id").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_region_valid() {
        assert!(validate_region("us-east-1").is_ok());
        assert!(validate_region("eu-west-1").is_ok());
    }

    #[test]
    fn test_validate_region_empty() {
        let err = validate_region("").unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_validate_region_too_long() {
        let long_region = "r".repeat(51);
        let err = validate_region(&long_region).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_region_at_50_chars() {
        let region_50 = "r".repeat(50);
        assert!(
            validate_region(&region_50).is_ok(),
            "Region at 50 chars should pass"
        );
    }

    #[test]
    fn test_validate_region_at_51_chars() {
        // One over the limit (51 chars)
        let region_51 = "r".repeat(51);
        let err = validate_region(&region_51).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("too long"));
    }

    #[test]
    fn test_validate_region_at_1_char() {
        // Minimum valid length (1 char)
        assert!(
            validate_region("r").is_ok(),
            "Region with 1 char should pass"
        );
    }
}
