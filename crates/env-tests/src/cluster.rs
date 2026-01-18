//! Cluster connection and health check utilities.
//!
//! This module provides the `ClusterConnection` type for validating that the local
//! kind cluster and port-forwards are available before running tests.

use std::net::TcpStream;
use std::time::Duration;
use thiserror::Error;

/// Cluster connection errors.
#[derive(Debug, Error)]
pub enum ClusterError {
    #[error("Port-forward not detected on localhost:{port}. Run './infra/kind/scripts/setup.sh' to start port-forwards")]
    PortForwardNotFound { port: u16 },

    #[error("Service health check failed: {message}")]
    HealthCheckFailed { message: String },

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
}

/// Default port configuration for local development environment.
#[derive(Debug, Clone)]
pub struct ClusterPorts {
    pub ac_service: u16,
    pub gc_service: u16,
    pub prometheus: u16,
    pub grafana: u16,
    pub loki: Option<u16>,
}

impl Default for ClusterPorts {
    fn default() -> Self {
        Self {
            ac_service: 8082,
            gc_service: 8080,
            prometheus: 9090,
            grafana: 3000,
            loki: Some(3100),
        }
    }
}

/// Connection to the local kind cluster.
///
/// Provides health check utilities and base URLs for service access.
pub struct ClusterConnection {
    pub ac_base_url: String,
    pub gc_base_url: String,
    pub prometheus_base_url: String,
    pub grafana_base_url: String,
    pub loki_base_url: Option<String>,
    http_client: reqwest::Client,
}

impl ClusterConnection {
    /// Create a new cluster connection with default ports.
    ///
    /// Performs TCP health checks on all required services with a 5s timeout.
    /// Returns actionable error messages if port-forwards are not detected.
    pub async fn new() -> Result<Self, ClusterError> {
        Self::new_with_ports(ClusterPorts::default()).await
    }

    /// Create a new cluster connection with custom ports.
    pub async fn new_with_ports(ports: ClusterPorts) -> Result<Self, ClusterError> {
        // Check AC service port-forward
        Self::check_tcp_port(ports.ac_service)?;

        // Check GC service port-forward (optional - may not be deployed yet)
        let gc_available = Self::check_tcp_port(ports.gc_service).is_ok();

        // Check Prometheus port-forward
        Self::check_tcp_port(ports.prometheus)?;

        // Check Grafana port-forward
        Self::check_tcp_port(ports.grafana)?;

        // Check optional Loki port-forward
        let loki_base_url = if let Some(loki_port) = ports.loki {
            if Self::check_tcp_port(loki_port).is_ok() {
                Some(format!("http://localhost:{}", loki_port))
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

        // GC base URL - may be unavailable if GC not deployed
        let gc_base_url = if gc_available {
            format!("http://localhost:{}", ports.gc_service)
        } else {
            // Return URL anyway, tests will fail with connection error if GC not running
            format!("http://localhost:{}", ports.gc_service)
        };

        Ok(Self {
            ac_base_url: format!("http://localhost:{}", ports.ac_service),
            gc_base_url,
            prometheus_base_url: format!("http://localhost:{}", ports.prometheus),
            grafana_base_url: format!("http://localhost:{}", ports.grafana),
            loki_base_url,
            http_client,
        })
    }

    /// Check if a TCP port is reachable on localhost.
    ///
    /// Uses a 5 second timeout for the connection attempt.
    fn check_tcp_port(port: u16) -> Result<(), ClusterError> {
        let addr = format!("127.0.0.1:{}", port);

        TcpStream::connect_timeout(
            &addr.parse().map_err(|_| ClusterError::HealthCheckFailed {
                message: format!("Invalid address: {}", addr),
            })?,
            Duration::from_secs(5),
        )
        .map_err(|_| ClusterError::PortForwardNotFound { port })?;

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
    pub async fn check_gc_health(&self) -> Result<(), ClusterError> {
        let health_url = format!("{}/v1/health", self.gc_base_url);

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

    #[test]
    fn test_default_ports() {
        let ports = ClusterPorts::default();
        assert_eq!(ports.ac_service, 8082);
        assert_eq!(ports.gc_service, 8080);
        assert_eq!(ports.prometheus, 9090);
        assert_eq!(ports.grafana, 3000);
        assert_eq!(ports.loki, Some(3100));
    }
}
