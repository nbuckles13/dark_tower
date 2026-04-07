//! Cluster connection and health check utilities.
//!
//! This module provides the `ClusterConnection` type for validating that the local
//! kind cluster and port-forwards are available before running tests.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use thiserror::Error;

/// Cluster connection errors.
#[derive(Debug, Error)]
pub enum ClusterError {
    #[error("Service health check failed: {message}")]
    HealthCheckFailed { message: String },

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Invalid URL '{url}': {reason}")]
    UrlParseError { url: String, reason: String },
}

/// Service URL configuration for local development environment.
///
/// Fields store full URLs (e.g., `http://localhost:8080`) rather than bare ports.
/// Use `from_env()` to read URLs from environment variables with fallback to defaults.
#[derive(Debug, Clone)]
pub struct ClusterPorts {
    pub ac_url: String,
    pub gc_url: String,
    pub mc_webtransport_url: String,
    pub prometheus_url: String,
    pub grafana_url: String,
    pub loki_url: Option<String>,
}

impl Default for ClusterPorts {
    fn default() -> Self {
        Self {
            ac_url: "http://localhost:8082".to_string(),
            gc_url: "http://localhost:8080".to_string(),
            mc_webtransport_url: "https://localhost:4433".to_string(),
            prometheus_url: "http://localhost:9090".to_string(),
            grafana_url: "http://localhost:3000".to_string(),
            loki_url: Some("http://localhost:3100".to_string()),
        }
    }
}

impl ClusterPorts {
    /// Create `ClusterPorts` from environment variables, falling back to defaults.
    ///
    /// Reads the following env vars as full URLs:
    /// - `ENV_TEST_AC_URL` (default: `http://localhost:8082`)
    /// - `ENV_TEST_GC_URL` (default: `http://localhost:8080`)
    /// - `ENV_TEST_PROMETHEUS_URL` (default: `http://localhost:9090`)
    /// - `ENV_TEST_GRAFANA_URL` (default: `http://localhost:3000`)
    /// - `ENV_TEST_LOKI_URL` (default: `http://localhost:3100`)
    ///
    /// MC/MH endpoints come from GC join response, not configuration.
    pub fn from_env() -> Result<Self, ClusterError> {
        let defaults = Self::default();

        let ac_url = read_env_url("ENV_TEST_AC_URL", &defaults.ac_url)?;
        let gc_url = read_env_url("ENV_TEST_GC_URL", &defaults.gc_url)?;
        let prometheus_url = read_env_url("ENV_TEST_PROMETHEUS_URL", &defaults.prometheus_url)?;
        let grafana_url = read_env_url("ENV_TEST_GRAFANA_URL", &defaults.grafana_url)?;
        let loki_url = match std::env::var("ENV_TEST_LOKI_URL") {
            Ok(url) if !url.is_empty() => {
                let url = url.trim_end_matches('/').to_string();
                parse_host_port(&url)?;
                eprintln!("[env-tests] ENV_TEST_LOKI_URL = {url} (from env)");
                Some(url)
            }
            _ => {
                eprintln!(
                    "[env-tests] ENV_TEST_LOKI_URL = {} (default)",
                    defaults.loki_url.as_deref().unwrap_or("<unset>")
                );
                defaults.loki_url
            }
        };

        Ok(Self {
            ac_url,
            gc_url,
            mc_webtransport_url: defaults.mc_webtransport_url,
            prometheus_url,
            grafana_url,
            loki_url,
        })
    }
}

/// Read a URL from an environment variable, falling back to a default.
///
/// Validates the URL via `parse_host_port` and logs whether the value came from env or default.
fn read_env_url(var_name: &str, default: &str) -> Result<String, ClusterError> {
    match std::env::var(var_name) {
        Ok(url) if !url.is_empty() => {
            let url = url.trim_end_matches('/').to_string();
            parse_host_port(&url)?;
            eprintln!("[env-tests] {var_name} = {url} (from env)");
            Ok(url)
        }
        _ => {
            eprintln!("[env-tests] {var_name} = {default} (default)");
            Ok(default.to_string())
        }
    }
}

/// Extract host and port from a URL string for TCP health checks.
///
/// Handles `http://` and `https://` schemes, strips trailing path.
/// Defaults to port 80 for http, 443 for https when no port is specified.
pub(crate) fn parse_host_port(url: &str) -> Result<(String, u16), ClusterError> {
    let (default_port, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (443u16, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (80u16, rest)
    } else {
        return Err(ClusterError::UrlParseError {
            url: url.to_string(),
            reason: "URL must start with http:// or https://".to_string(),
        });
    };

    if rest.contains('@') {
        return Err(ClusterError::UrlParseError {
            url: url.to_string(),
            reason: "URL must not contain credentials (@ in authority)".to_string(),
        });
    }

    // Strip path component
    let authority = rest.split('/').next().unwrap_or(rest);

    if authority.is_empty() {
        return Err(ClusterError::UrlParseError {
            url: url.to_string(),
            reason: "URL has no host".to_string(),
        });
    }

    // Handle IPv6 addresses like [::1]:8080
    if let Some(bracket_end) = authority.find(']') {
        let host = &authority[..=bracket_end];
        let port =
            if authority.len() > bracket_end + 1 && authority.as_bytes()[bracket_end + 1] == b':' {
                authority[bracket_end + 2..].parse::<u16>().map_err(|e| {
                    ClusterError::UrlParseError {
                        url: url.to_string(),
                        reason: format!("invalid port: {e}"),
                    }
                })?
            } else {
                default_port
            };
        return Ok((host.to_string(), port));
    }

    // host:port or just host
    match authority.rsplit_once(':') {
        Some((host, port_str)) => {
            let port = port_str
                .parse::<u16>()
                .map_err(|e| ClusterError::UrlParseError {
                    url: url.to_string(),
                    reason: format!("invalid port: {e}"),
                })?;
            Ok((host.to_string(), port))
        }
        None => Ok((authority.to_string(), default_port)),
    }
}

/// Connection to the local kind cluster.
///
/// Provides health check utilities and base URLs for service access.
pub struct ClusterConnection {
    pub ac_base_url: String,
    pub gc_base_url: String,
    pub mc_webtransport_url: String,
    pub prometheus_base_url: String,
    pub grafana_base_url: String,
    pub loki_base_url: Option<String>,
    http_client: reqwest::Client,
}

impl ClusterConnection {
    /// Create a new cluster connection using URLs from environment variables (or defaults).
    ///
    /// Reads `ENV_TEST_*_URL` env vars via `ClusterPorts::from_env()`.
    /// Performs TCP health checks on all required services with a 5s timeout.
    /// Returns actionable error messages if port-forwards are not detected.
    pub async fn new() -> Result<Self, ClusterError> {
        Self::new_with_ports(ClusterPorts::from_env()?).await
    }

    /// Create a new cluster connection with custom ports.
    pub async fn new_with_ports(ports: ClusterPorts) -> Result<Self, ClusterError> {
        // Check AC service connectivity
        let (ac_host, ac_port) = parse_host_port(&ports.ac_url)?;
        Self::check_tcp_port(&ac_host, ac_port)?;

        // Check GC service connectivity (optional - may not be deployed yet)
        let (gc_host, gc_port) = parse_host_port(&ports.gc_url)?;
        let _gc_available = Self::check_tcp_port(&gc_host, gc_port).is_ok();

        // Check Prometheus connectivity
        let (prom_host, prom_port) = parse_host_port(&ports.prometheus_url)?;
        Self::check_tcp_port(&prom_host, prom_port)?;

        // Check Grafana connectivity
        let (grafana_host, grafana_port) = parse_host_port(&ports.grafana_url)?;
        Self::check_tcp_port(&grafana_host, grafana_port)?;

        // Check optional Loki connectivity
        let loki_base_url = if let Some(ref loki_url) = ports.loki_url {
            let (loki_host, loki_port) = parse_host_port(loki_url)?;
            if Self::check_tcp_port(&loki_host, loki_port).is_ok() {
                Some(loki_url.clone())
            } else {
                // Loki is optional - just mark as unavailable
                None
            }
        } else {
            None
        };

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ClusterError::HealthCheckFailed {
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        Ok(Self {
            ac_base_url: ports.ac_url,
            gc_base_url: ports.gc_url,
            mc_webtransport_url: ports.mc_webtransport_url,
            prometheus_base_url: ports.prometheus_url,
            grafana_base_url: ports.grafana_url,
            loki_base_url,
            http_client,
        })
    }

    /// Check if a TCP port is reachable on the given host.
    ///
    /// Resolves hostnames via DNS (supports both IP addresses and names like
    /// `host.containers.internal`). Uses a 5 second timeout for the connection attempt.
    fn check_tcp_port(host: &str, port: u16) -> Result<(), ClusterError> {
        let addr_str = format!("{}:{}", host, port);
        let addr = addr_str
            .to_socket_addrs()
            .map_err(|e| ClusterError::HealthCheckFailed {
                message: format!("Cannot resolve '{}': {}", addr_str, e),
            })?
            .next()
            .ok_or_else(|| ClusterError::HealthCheckFailed {
                message: format!("No addresses found for '{}'", addr_str),
            })?;

        TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| {
            ClusterError::HealthCheckFailed {
                message: format!(
                    "Port-forward not detected on {}:{}. Run './infra/kind/scripts/setup.sh' to start port-forwards. TCP error: {}",
                    host, port, e
                ),
            }
        })?;

        Ok(())
    }

    /// Get the HTTP client for making requests.
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Check if the AC service health endpoint is responding.
    pub async fn check_ac_health(&self) -> Result<(), ClusterError> {
        let health_url = format!("{}/health", self.ac_base_url);

        let response = self.http_client.get(&health_url).send().await?;

        if !response.status().is_success() {
            return Err(ClusterError::HealthCheckFailed {
                message: format!("AC health endpoint returned status {}", response.status()),
            });
        }

        Ok(())
    }

    /// Check if the GC service health endpoint is responding.
    ///
    /// GC health endpoint is at `/health` (not versioned).
    /// Source of truth: `crates/gc-service/src/routes/mod.rs`
    pub async fn check_gc_health(&self) -> Result<(), ClusterError> {
        let health_url = format!("{}/health", self.gc_base_url);

        let response = self.http_client.get(&health_url).send().await?;

        if !response.status().is_success() {
            return Err(ClusterError::HealthCheckFailed {
                message: format!("GC health endpoint returned status {}", response.status()),
            });
        }

        Ok(())
    }

    /// Check if the GC service is available.
    ///
    /// Returns true if GC port-forward is detected and health check passes.
    pub async fn is_gc_available(&self) -> bool {
        self.check_gc_health().await.is_ok()
    }

    /// Get the MC WebTransport URL for pod-0 (`mc-service-0`).
    ///
    /// This returns a hardcoded URL for pod-0 (port 4433). For positive join
    /// flow tests, use the pod-specific `webtransport_endpoint` from the GC
    /// join response instead, since GC may assign the meeting to any MC pod.
    ///
    /// This method is appropriate for negative tests (e.g., invalid token
    /// rejection) that don't go through GC join and just need any MC pod.
    ///
    /// MC availability cannot be probed at initialization time because MC
    /// uses QUIC (UDP), not TCP. Tests that require MC should attempt to
    /// connect and handle connection failures gracefully.
    pub fn mc_webtransport_url(&self) -> &str {
        &self.mc_webtransport_url
    }

    /// Check if the AC service ready endpoint is responding.
    pub async fn check_ac_ready(&self) -> Result<(), ClusterError> {
        let ready_url = format!("{}/ready", self.ac_base_url);

        let response = self.http_client.get(&ready_url).send().await?;

        if !response.status().is_success() {
            return Err(ClusterError::HealthCheckFailed {
                message: format!("AC ready endpoint returned status {}", response.status()),
            });
        }

        Ok(())
    }

    /// Check if Prometheus is reachable.
    pub async fn check_prometheus(&self) -> Result<(), ClusterError> {
        let prometheus_url = format!("{}/api/v1/status/config", self.prometheus_base_url);

        let response = self.http_client.get(&prometheus_url).send().await?;

        if !response.status().is_success() {
            return Err(ClusterError::HealthCheckFailed {
                message: format!("Prometheus returned status {}", response.status()),
            });
        }

        Ok(())
    }

    /// Check if Grafana is reachable.
    pub async fn check_grafana(&self) -> Result<(), ClusterError> {
        let grafana_url = format!("{}/api/health", self.grafana_base_url);

        let response = self.http_client.get(&grafana_url).send().await?;

        if !response.status().is_success() {
            return Err(ClusterError::HealthCheckFailed {
                message: format!("Grafana returned status {}", response.status()),
            });
        }

        Ok(())
    }

    /// Check if Loki is available.
    ///
    /// Returns true if Loki was detected during initialization and is responding.
    pub async fn is_loki_available(&self) -> bool {
        if let Some(loki_url) = &self.loki_base_url {
            let ready_url = format!("{}/ready", loki_url);

            self.http_client
                .get(&ready_url)
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_default_ports() {
        let ports = ClusterPorts::default();
        assert_eq!(ports.ac_url, "http://localhost:8082");
        assert_eq!(ports.gc_url, "http://localhost:8080");
        assert_eq!(ports.mc_webtransport_url, "https://localhost:4433");
        assert_eq!(ports.prometheus_url, "http://localhost:9090");
        assert_eq!(ports.grafana_url, "http://localhost:3000");
        assert_eq!(ports.loki_url, Some("http://localhost:3100".to_string()));
    }

    #[test]
    #[serial]
    fn test_from_env_defaults() {
        // Clear any env vars that might be set
        std::env::remove_var("ENV_TEST_AC_URL");
        std::env::remove_var("ENV_TEST_GC_URL");
        std::env::remove_var("ENV_TEST_PROMETHEUS_URL");
        std::env::remove_var("ENV_TEST_GRAFANA_URL");
        std::env::remove_var("ENV_TEST_LOKI_URL");

        let ports = ClusterPorts::from_env().expect("from_env should succeed with defaults");
        let defaults = ClusterPorts::default();
        assert_eq!(ports.ac_url, defaults.ac_url);
        assert_eq!(ports.gc_url, defaults.gc_url);
        assert_eq!(ports.mc_webtransport_url, defaults.mc_webtransport_url);
        assert_eq!(ports.prometheus_url, defaults.prometheus_url);
        assert_eq!(ports.grafana_url, defaults.grafana_url);
        assert_eq!(ports.loki_url, defaults.loki_url);
    }

    #[test]
    #[serial]
    fn test_from_env_custom() {
        std::env::set_var("ENV_TEST_AC_URL", "http://host.containers.internal:24200");
        std::env::set_var("ENV_TEST_GC_URL", "http://host.containers.internal:24201");
        std::env::set_var(
            "ENV_TEST_PROMETHEUS_URL",
            "http://host.containers.internal:24300",
        );
        std::env::set_var(
            "ENV_TEST_GRAFANA_URL",
            "http://host.containers.internal:24301",
        );
        std::env::set_var("ENV_TEST_LOKI_URL", "http://host.containers.internal:24302");

        let ports = ClusterPorts::from_env().expect("from_env should succeed with env vars");
        assert_eq!(ports.ac_url, "http://host.containers.internal:24200");
        assert_eq!(ports.gc_url, "http://host.containers.internal:24201");
        // MC should remain at default — not configurable via env
        assert_eq!(ports.mc_webtransport_url, "https://localhost:4433");
        assert_eq!(
            ports.prometheus_url,
            "http://host.containers.internal:24300"
        );
        assert_eq!(ports.grafana_url, "http://host.containers.internal:24301");
        assert_eq!(
            ports.loki_url,
            Some("http://host.containers.internal:24302".to_string())
        );

        // Clean up
        std::env::remove_var("ENV_TEST_AC_URL");
        std::env::remove_var("ENV_TEST_GC_URL");
        std::env::remove_var("ENV_TEST_PROMETHEUS_URL");
        std::env::remove_var("ENV_TEST_GRAFANA_URL");
        std::env::remove_var("ENV_TEST_LOKI_URL");
    }

    #[test]
    #[serial]
    fn test_from_env_rejects_non_http_scheme() {
        std::env::set_var("ENV_TEST_AC_URL", "ftp://evil.example.com:8080");

        let result = ClusterPorts::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must start with http:// or https://"),
            "error: {err}"
        );

        std::env::remove_var("ENV_TEST_AC_URL");
    }

    #[test]
    #[serial]
    fn test_from_env_rejects_credentials_in_url() {
        std::env::set_var("ENV_TEST_GC_URL", "http://user:pass@evil.example.com:8080");

        let result = ClusterPorts::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must not contain credentials"), "error: {err}");

        std::env::remove_var("ENV_TEST_GC_URL");
    }

    #[test]
    #[serial]
    fn test_from_env_partial() {
        std::env::set_var("ENV_TEST_AC_URL", "http://host.containers.internal:24200");
        std::env::set_var("ENV_TEST_GC_URL", "http://host.containers.internal:24201");
        std::env::remove_var("ENV_TEST_PROMETHEUS_URL");
        std::env::remove_var("ENV_TEST_GRAFANA_URL");
        std::env::remove_var("ENV_TEST_LOKI_URL");

        let ports =
            ClusterPorts::from_env().expect("from_env should succeed with partial env vars");
        let defaults = ClusterPorts::default();
        assert_eq!(ports.ac_url, "http://host.containers.internal:24200");
        assert_eq!(ports.gc_url, "http://host.containers.internal:24201");
        assert_eq!(ports.mc_webtransport_url, defaults.mc_webtransport_url);
        assert_eq!(ports.prometheus_url, defaults.prometheus_url);
        assert_eq!(ports.grafana_url, defaults.grafana_url);
        assert_eq!(ports.loki_url, defaults.loki_url);

        std::env::remove_var("ENV_TEST_AC_URL");
        std::env::remove_var("ENV_TEST_GC_URL");
    }

    #[test]
    #[serial]
    fn test_from_env_strips_trailing_slash() {
        std::env::set_var("ENV_TEST_AC_URL", "http://host.containers.internal:24200/");

        let ports = ClusterPorts::from_env().expect("from_env should succeed");
        assert_eq!(ports.ac_url, "http://host.containers.internal:24200");

        std::env::remove_var("ENV_TEST_AC_URL");
    }

    #[test]
    fn test_parse_host_port_http() {
        let (host, port) = parse_host_port("http://localhost:8080").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_parse_host_port_https() {
        let (host, port) = parse_host_port("https://localhost:4433").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 4433);
    }

    #[test]
    fn test_parse_host_port_default_http() {
        let (host, port) = parse_host_port("http://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
    }

    #[test]
    fn test_parse_host_port_default_https() {
        let (host, port) = parse_host_port("https://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_with_path() {
        let (host, port) = parse_host_port("http://localhost:9090/api/v1").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 9090);
    }

    #[test]
    fn test_parse_host_port_container_internal() {
        let (host, port) = parse_host_port("http://host.containers.internal:24200").unwrap();
        assert_eq!(host, "host.containers.internal");
        assert_eq!(port, 24200);
    }

    #[test]
    fn test_parse_host_port_ipv6_with_port() {
        let (host, port) = parse_host_port("http://[::1]:8080").unwrap();
        assert_eq!(host, "[::1]");
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_parse_host_port_ipv6_default_port() {
        let (host, port) = parse_host_port("http://[::1]").unwrap();
        assert_eq!(host, "[::1]");
        assert_eq!(port, 80);
    }

    #[test]
    fn test_parse_host_port_rejects_non_http() {
        let result = parse_host_port("ftp://localhost:21");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must start with http:// or https://"),
            "error: {err}"
        );
    }

    #[test]
    fn test_parse_host_port_rejects_credentials() {
        let result = parse_host_port("http://admin:secret@localhost:8080");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must not contain credentials"), "error: {err}");
    }

    #[test]
    fn test_parse_host_port_rejects_empty_host() {
        let result = parse_host_port("http://");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no host"), "error: {err}");
    }

    #[test]
    fn test_parse_host_port_rejects_invalid_port() {
        let result = parse_host_port("http://localhost:notaport");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid port"), "error: {err}");
    }

    #[test]
    fn test_parse_host_port_rejects_file_scheme() {
        assert!(parse_host_port("file:///etc/passwd").is_err());
    }
}
