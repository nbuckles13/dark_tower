//! NetworkPolicy canary pod utilities.
//!
//! This module provides utilities for deploying canary pods to test NetworkPolicy
//! enforcement in the cluster. Canary pods are minimal containers that can be used
//! to verify network connectivity rules.
//!
//! # Usage
//!
//! ```ignore
//! use env_tests::canary::CanaryPod;
//!
//! // Deploy a canary pod in a specific namespace
//! let canary = CanaryPod::deploy("dark-tower").await?;
//!
//! // Test if the canary can reach a target URL
//! let can_reach = canary.can_reach("http://ac-service:8082/health").await;
//!
//! // Cleanup (also happens automatically on drop)
//! canary.cleanup().await?;
//! ```
//!
//! # Prerequisites
//!
//! - kubectl must be in PATH and configured for the target cluster
//! - RBAC: The calling context must have pods.create/delete permissions

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

/// Errors that can occur during canary pod operations.
#[derive(Debug, Error)]
pub enum CanaryError {
    #[error("Failed to execute kubectl: {0}")]
    KubectlExec(String),

    #[error("Failed to deploy canary pod: {0}")]
    DeployFailed(String),

    #[error("Pod did not become ready in time: {0}")]
    PodNotReady(String),

    #[error("Failed to execute command in pod: {0}")]
    ExecFailed(String),

    #[error("Failed to cleanup canary pod: {0}")]
    CleanupFailed(String),

    #[error("Namespace creation failed: {0}")]
    NamespaceFailed(String),
}

/// A canary pod for testing NetworkPolicy enforcement.
///
/// The canary pod is a minimal busybox container that can be used to test
/// network connectivity from within the cluster. It supports HTTP probes
/// via wget to verify if NetworkPolicies are blocking or allowing traffic.
pub struct CanaryPod {
    name: String,
    namespace: String,
    cleaned_up: AtomicBool,
}

/// Configuration for canary pod deployment.
#[derive(Clone, Debug)]
pub struct CanaryConfig {
    /// Labels to apply to the pod (format: "key=value,key2=value2")
    pub labels: String,
}

impl Default for CanaryConfig {
    fn default() -> Self {
        Self {
            labels: "app=canary,test=network-policy".to_string(),
        }
    }
}

impl CanaryPod {
    /// Deploy a canary pod for NetworkPolicy testing in the specified namespace.
    ///
    /// The pod will be named `canary-{uuid}` and runs a busybox container
    /// with `sleep infinity` to keep it alive for testing.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The Kubernetes namespace to deploy the pod in.
    ///   If the namespace doesn't exist, it will be created.
    ///
    /// # Returns
    ///
    /// A `CanaryPod` instance that can be used to test connectivity.
    /// The pod will be automatically cleaned up when dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if kubectl commands fail or the pod doesn't become ready.
    pub async fn deploy(namespace: &str) -> Result<Self, CanaryError> {
        Self::deploy_with_config(namespace, CanaryConfig::default()).await
    }

    /// Deploy a canary pod with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The Kubernetes namespace to deploy the pod in.
    /// * `config` - Configuration including custom labels.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Deploy with custom labels to match NetworkPolicy ingress rules
    /// let config = CanaryConfig {
    ///     labels: "app=global-controller,test=network-policy".to_string(),
    /// };
    /// let canary = CanaryPod::deploy_with_config("dark-tower", config).await?;
    /// ```
    pub async fn deploy_with_config(
        namespace: &str,
        config: CanaryConfig,
    ) -> Result<Self, CanaryError> {
        // Generate a unique pod name
        let uuid = uuid::Uuid::new_v4().to_string();
        let short_uuid = &uuid[..8];
        let name = format!("canary-{}", short_uuid);

        // Ensure the namespace exists (create if it doesn't)
        Self::ensure_namespace(namespace)?;

        // Deploy the canary pod using kubectl run
        // Using --restart=Never creates a bare pod (not a Deployment)
        let labels_arg = format!("--labels={}", config.labels);
        let deploy_result = Command::new("kubectl")
            .args([
                "run",
                &name,
                "--image=busybox:1.36",
                &format!("--namespace={}", namespace),
                "--restart=Never",
                &labels_arg,
                "--",
                "sleep",
                "3600", // Sleep for 1 hour (test should complete well before)
            ])
            .output()
            .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;

        if !deploy_result.status.success() {
            let stderr = String::from_utf8_lossy(&deploy_result.stderr);
            return Err(CanaryError::DeployFailed(format!(
                "kubectl run failed: {}",
                stderr
            )));
        }

        let canary = Self {
            name: name.clone(),
            namespace: namespace.to_string(),
            cleaned_up: AtomicBool::new(false),
        };

        // Wait for the pod to be running
        canary.wait_for_ready(30).await?;

        Ok(canary)
    }

    /// Ensure the namespace exists, creating it if necessary.
    fn ensure_namespace(namespace: &str) -> Result<(), CanaryError> {
        // Check if namespace exists
        let check_result = Command::new("kubectl")
            .args(["get", "namespace", namespace])
            .output()
            .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;

        if check_result.status.success() {
            return Ok(()); // Namespace already exists
        }

        // Create the namespace
        let create_result = Command::new("kubectl")
            .args(["create", "namespace", namespace])
            .output()
            .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;

        if !create_result.status.success() {
            let stderr = String::from_utf8_lossy(&create_result.stderr);
            // Ignore "already exists" error (race condition)
            if !stderr.contains("already exists") {
                return Err(CanaryError::NamespaceFailed(stderr.to_string()));
            }
        }

        Ok(())
    }

    /// Wait for the pod to be in Running state.
    async fn wait_for_ready(&self, timeout_seconds: u32) -> Result<(), CanaryError> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_seconds as u64);

        loop {
            if start.elapsed() > timeout {
                return Err(CanaryError::PodNotReady(format!(
                    "Pod {} in namespace {} did not become ready within {}s",
                    self.name, self.namespace, timeout_seconds
                )));
            }

            // Check pod status
            let status_result = Command::new("kubectl")
                .args([
                    "get",
                    "pod",
                    &self.name,
                    &format!("--namespace={}", self.namespace),
                    "-o",
                    "jsonpath={.status.phase}",
                ])
                .output()
                .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;

            if status_result.status.success() {
                let phase = String::from_utf8_lossy(&status_result.stdout);
                if phase == "Running" {
                    return Ok(());
                }
                if phase == "Failed" || phase == "Error" {
                    return Err(CanaryError::PodNotReady(format!(
                        "Pod {} entered {} state",
                        self.name, phase
                    )));
                }
            }

            // Wait before next check
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    /// Test if the canary pod can reach a target URL.
    ///
    /// This executes wget inside the canary pod to test HTTP connectivity.
    /// The method returns true if the target is reachable, false if blocked/timeout.
    ///
    /// # Arguments
    ///
    /// * `target_url` - The URL to test connectivity to (e.g., `http://ac-service:8082/health`)
    ///
    /// # Returns
    ///
    /// `true` if the target is reachable (HTTP request succeeded),
    /// `false` if the connection is blocked by NetworkPolicy or times out.
    pub async fn can_reach(&self, target_url: &str) -> bool {
        // Use wget with a short timeout to test connectivity
        // -q: quiet mode
        // -O-: output to stdout
        // -T 5: 5 second timeout
        // --spider: don't download, just check if accessible
        let exec_result = Command::new("kubectl")
            .args([
                "exec",
                &self.name,
                &format!("--namespace={}", self.namespace),
                "--",
                "wget",
                "-q",
                "-O-",
                "-T",
                "5",
                "--spider",
                target_url,
            ])
            .output();

        match exec_result {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Test connectivity with a full HTTP GET and return the response.
    ///
    /// Unlike `can_reach`, this method returns the actual response body
    /// if the request succeeds.
    ///
    /// # Arguments
    ///
    /// * `target_url` - The URL to fetch
    ///
    /// # Returns
    ///
    /// `Some(body)` if the request succeeded, `None` if blocked/failed.
    pub async fn fetch(&self, target_url: &str) -> Option<String> {
        let exec_result = Command::new("kubectl")
            .args([
                "exec",
                &self.name,
                &format!("--namespace={}", self.namespace),
                "--",
                "wget",
                "-q",
                "-O-",
                "-T",
                "5",
                target_url,
            ])
            .output();

        match exec_result {
            Ok(output) if output.status.success() => {
                Some(String::from_utf8_lossy(&output.stdout).to_string())
            }
            _ => None,
        }
    }

    /// Get the pod name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the namespace the pod is running in.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Explicitly cleanup the canary pod.
    ///
    /// This deletes the pod from the cluster. The method is idempotent
    /// and safe to call multiple times.
    ///
    /// # Errors
    ///
    /// Returns an error if kubectl delete fails (but ignores "not found" errors).
    pub async fn cleanup(&self) -> Result<(), CanaryError> {
        self.do_cleanup()
    }

    /// Internal cleanup implementation.
    fn do_cleanup(&self) -> Result<(), CanaryError> {
        // Check if already cleaned up
        if self.cleaned_up.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already cleaned up
        }

        let delete_result = Command::new("kubectl")
            .args([
                "delete",
                "pod",
                &self.name,
                &format!("--namespace={}", self.namespace),
                "--grace-period=0",
                "--force",
                "--ignore-not-found=true",
            ])
            .output()
            .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;

        if !delete_result.status.success() {
            let stderr = String::from_utf8_lossy(&delete_result.stderr);
            // Ignore "not found" errors - pod may already be gone
            if !stderr.contains("not found") && !stderr.contains("NotFound") {
                return Err(CanaryError::CleanupFailed(stderr.to_string()));
            }
        }

        Ok(())
    }
}

impl Drop for CanaryPod {
    fn drop(&mut self) {
        // Best-effort cleanup on drop
        // We can't await in drop, so we spawn a blocking task
        if !self.cleaned_up.load(Ordering::SeqCst) {
            // Perform synchronous cleanup
            let _ = self.do_cleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canary_error_display() {
        let err = CanaryError::DeployFailed("test error".to_string());
        assert!(err.to_string().contains("test error"));
    }
}
