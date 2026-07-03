//! Forwards filtered OTLP payloads to the in-cluster OTel collector (R-2).
//!
//! The forwarder owns the per-signal URL suffix: it is constructed with the
//! **bare base** `otel_collector_endpoint` (scheme + host + port, no path) and
//! appends the fixed OTLP/HTTP spec path `/v1/metrics` or `/v1/traces` itself.
//! No part of the request path or body reaches the forward URL (no SSRF).
//!
//! Mirrors the `reqwest` client construction in [`crate::services::ac_client`],
//! but with an explicit, INJECTABLE total timeout so a slow/hung collector
//! cannot pile up GC request-handler tasks (and so the timeout path is testable
//! without a real wall-clock wait).

use crate::errors::GcError;
use crate::services::trace_headers;
use reqwest::Client;
use std::time::Duration;
use tracing::warn;

/// Default total request timeout for a collector forward, in seconds.
pub const TELEMETRY_FORWARD_TIMEOUT_SECS: u64 = 5;

/// Default connect timeout, in seconds.
const TELEMETRY_FORWARD_CONNECT_TIMEOUT_SECS: u64 = 2;

/// The OTLP signal being forwarded. Selects the fixed URL suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Metrics,
    Traces,
}

impl Signal {
    /// The fixed OTLP/HTTP spec path suffix for this signal.
    fn suffix(self) -> &'static str {
        match self {
            Signal::Metrics => "/v1/metrics",
            Signal::Traces => "/v1/traces",
        }
    }
}

/// HTTP client that forwards re-encoded OTLP payloads to the collector.
#[derive(Clone)]
pub struct TelemetryForwarder {
    client: Client,
    /// Bare base collector endpoint (no path, no trailing slash).
    base_url: String,
}

impl TelemetryForwarder {
    /// Create a forwarder with the default timeout.
    ///
    /// `base_url` is the bare-base `otel_collector_endpoint` from config.
    pub fn new(base_url: String) -> Result<Self, GcError> {
        Self::with_timeout(
            base_url,
            Duration::from_secs(TELEMETRY_FORWARD_TIMEOUT_SECS),
        )
    }

    /// Create a forwarder with an explicit total timeout (tests inject a short
    /// timeout to exercise the 502-on-timeout path without a real wait).
    pub fn with_timeout(base_url: String, timeout: Duration) -> Result<Self, GcError> {
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(TELEMETRY_FORWARD_CONNECT_TIMEOUT_SECS))
            .build()
            .map_err(|e| {
                GcError::Internal(format!("Failed to build telemetry HTTP client: {e}"))
            })?;

        // Normalize: strip a single trailing slash so the suffix join never
        // double-slashes (cheap insurance against `…:4318/`).
        let base_url = base_url.trim_end_matches('/').to_string();

        Ok(Self { client, base_url })
    }

    /// Whether forwarding is enabled. An empty configured endpoint disables the
    /// proxy (the handler returns 503 rather than forwarding).
    pub fn is_enabled(&self) -> bool {
        !self.base_url.is_empty()
    }

    /// Forward an already-filtered, re-encoded OTLP payload for `signal`.
    ///
    /// On the collector returning any 4xx/5xx, on a connection failure, or on
    /// timeout, returns [`GcError::BadGateway`]. The collector status/URL is
    /// logged server-side; nothing about the collector is echoed to the client.
    pub async fn forward(&self, signal: Signal, body: Vec<u8>) -> Result<(), GcError> {
        let url = format!("{}{}", self.base_url, signal.suffix());

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/x-protobuf")
            // R-56: inject the active span's W3C trace context so the
            // "telemetry-proxy preserves trace context" property holds — the
            // client's traceparent (extracted from the inbound proxy request
            // at surface (a)) propagates through to the collector. Same
            // shared helper as `ac_client.rs`'s surface (b); this call site
            // carries no other sensitive header, so applying it is
            // mechanical.
            .headers(trace_headers())
            .body(body)
            .send()
            .await
            .map_err(|e| {
                // Connection failure or timeout — details server-side only.
                warn!(
                    target: "gc.telemetry.forwarder",
                    error = %e,
                    url = %url,
                    "Telemetry forward to collector failed"
                );
                GcError::BadGateway("collector request failed".to_string())
            })?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            // Any 4xx OR 5xx from the collector → 502 to the client (we do not
            // pass collector status through). Logged server-side.
            warn!(
                target: "gc.telemetry.forwarder",
                status = %status,
                url = %url,
                "Telemetry collector returned non-success status"
            );
            Err(GcError::BadGateway(format!(
                "collector returned status {status}"
            )))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn trailing_slash_is_trimmed() {
        let f = TelemetryForwarder::new("http://collector:4318/".to_string()).unwrap();
        assert_eq!(f.base_url, "http://collector:4318");
    }

    #[test]
    fn empty_endpoint_is_disabled() {
        let f = TelemetryForwarder::new(String::new()).unwrap();
        assert!(!f.is_enabled());
    }

    #[test]
    fn non_empty_endpoint_is_enabled() {
        let f = TelemetryForwarder::new("http://collector:4318".to_string()).unwrap();
        assert!(f.is_enabled());
    }

    #[tokio::test]
    async fn forward_metrics_appends_v1_metrics_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/metrics"))
            .and(header("Content-Type", "application/x-protobuf"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let f = TelemetryForwarder::new(server.uri()).unwrap();
        let result = f.forward(Signal::Metrics, vec![1, 2, 3]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn forward_traces_appends_v1_traces() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/traces"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let f = TelemetryForwarder::new(server.uri()).unwrap();
        assert!(f.forward(Signal::Traces, vec![0u8]).await.is_ok());
    }

    #[tokio::test]
    async fn collector_5xx_maps_to_bad_gateway() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let f = TelemetryForwarder::new(server.uri()).unwrap();
        let err = f.forward(Signal::Metrics, vec![1]).await.unwrap_err();
        assert!(matches!(err, GcError::BadGateway(_)));
    }

    #[tokio::test]
    async fn collector_4xx_maps_to_bad_gateway() {
        // 4xx must ALSO become 502 — not passed through to the client.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;

        let f = TelemetryForwarder::new(server.uri()).unwrap();
        let err = f.forward(Signal::Metrics, vec![1]).await.unwrap_err();
        assert!(matches!(err, GcError::BadGateway(_)));
    }

    #[tokio::test]
    async fn collector_timeout_maps_to_bad_gateway() {
        // Inject a short timeout against a delayed response — no real wall-clock wait.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;

        let f = TelemetryForwarder::with_timeout(server.uri(), Duration::from_millis(100)).unwrap();
        let err = f.forward(Signal::Metrics, vec![1]).await.unwrap_err();
        assert!(matches!(err, GcError::BadGateway(_)));
    }

    #[tokio::test]
    async fn connection_refused_maps_to_bad_gateway() {
        // Nothing listening on this port.
        let f = TelemetryForwarder::with_timeout(
            "http://127.0.0.1:1".to_string(),
            Duration::from_millis(200),
        )
        .unwrap();
        let err = f.forward(Signal::Traces, vec![1]).await.unwrap_err();
        assert!(matches!(err, GcError::BadGateway(_)));
    }
}
