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
//! // Explicit, asserted cleanup (Drop is only a backstop)
//! canary.cleanup().await?;
//! ```
//!
//! # Deletion is ALWAYS graceful
//!
//! Every canary is deleted with a plain `kubectl delete pod --wait` and then
//! asserted gone. Never `--force --grace-period=0`: a zero-grace force delete
//! removes the API object while the container may still run, which is exactly
//! the way to defeat a hostPort lock (ADR-0039 §9). Canaries instead carry
//! [`CANARY_TERMINATION_GRACE_SECONDS`] so a graceful delete is quick even
//! though `sh`/`sleep` as PID 1 ignores SIGTERM.
//!
//! # Process model
//!
//! Every async method runs kubectl through `tokio::process`. [`Drop`] is the
//! one synchronous exception (it cannot await): it prints the
//! graceful delete command and spawns a non-blocking, non-forced delete whose
//! outcome is not observed.
//!
//! # Prerequisites
//!
//! - kubectl must be in PATH and configured for the target cluster
//! - RBAC: The calling context must have pods.create/delete permissions

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// The ONE canary image. Already preloaded/pulled by the existing NetworkPolicy
/// canaries; adding another image means checking `preload_third_party_images`
/// in `infra/kind/scripts/deploy.sh` first.
pub const BUSYBOX_IMAGE: &str = "busybox:1.36";

/// `terminationGracePeriodSeconds` for every canary. `sh`/`sleep` as PID 1
/// ignores SIGTERM, so the default 30s grace would make each graceful delete
/// block for 30s and race [`DELETE_TIMEOUT_SECONDS`].
pub const CANARY_TERMINATION_GRACE_SECONDS: u64 = 1;

/// Bound on `kubectl delete pod --wait`. Comfortably above
/// [`CANARY_TERMINATION_GRACE_SECONDS`].
pub const DELETE_TIMEOUT_SECONDS: u64 = 30;

// The graceful delete bound must comfortably exceed the grace period.
const _: () = assert!(DELETE_TIMEOUT_SECONDS > CANARY_TERMINATION_GRACE_SECONDS * 5);

/// Lifetime of a generic canary (`sleep`), and its `activeDeadlineSeconds`
/// backstop: a leaked canary terminates itself.
const GENERIC_CANARY_LIFETIME_SECONDS: u64 = 3600;

/// Bound on reaching phase Running for the generic canaries.
const GENERIC_RUNNING_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// Pod name prefix; the name is `<prefix>-<8 hex>`.
    pub name_prefix: String,
}

impl Default for CanaryConfig {
    fn default() -> Self {
        Self {
            labels: "app=canary,test=network-policy".to_string(),
            name_prefix: "canary".to_string(),
        }
    }
}

/// The ONE canary name generator: `<prefix>-<8 hex>`.
#[must_use]
pub fn canary_name(prefix: &str) -> String {
    format!("{prefix}-{}", short_nonce())
}

/// 8 random hex chars: the canary-name suffix and per-run nonces.
#[must_use]
pub fn short_nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// Is `name` a canary name produced by [`canary_name`] for `prefix`?
#[must_use]
pub fn is_canary_name(prefix: &str, name: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|s| s.len() == 8 && s.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The ONE canary Pod skeleton: owns the invariants every canary carries
/// (`restartPolicy: Never`, [`CANARY_TERMINATION_GRACE_SECONDS`], an
/// `activeDeadlineSeconds` backstop, [`BUSYBOX_IMAGE`]). `container` supplies
/// name/command/ports/probes; its `image` is filled in here.
#[must_use]
pub fn canary_pod_manifest(
    name: &str,
    namespace: &str,
    labels: &Value,
    mut container: Value,
    active_deadline_seconds: u64,
) -> Value {
    container["image"] = Value::String(BUSYBOX_IMAGE.to_string());
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": name,
            "namespace": namespace,
            "labels": labels,
        },
        "spec": {
            "restartPolicy": "Never",
            "terminationGracePeriodSeconds": CANARY_TERMINATION_GRACE_SECONDS,
            "activeDeadlineSeconds": active_deadline_seconds,
            "automountServiceAccountToken": false,
            "containers": [container],
        },
    })
}

/// The graceful delete command for a canary — printed by the Drop backstop and
/// by every leftover/cleanup failure message. Never `--force`.
#[must_use]
pub fn graceful_delete_command(name: &str, namespace: &str) -> String {
    format!(
        "kubectl delete pod {name} -n {namespace} --wait=true --timeout={DELETE_TIMEOUT_SECONDS}s"
    )
}

/// Parse `"k=v,k2=v2"` into a JSON label map.
fn parse_labels(labels: &str) -> Result<serde_json::Map<String, Value>, CanaryError> {
    let mut map = serde_json::Map::new();
    for pair in labels.split(',').filter(|p| !p.is_empty()) {
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| CanaryError::DeployFailed(format!("malformed label '{pair}'")))?;
        map.insert(k.to_string(), Value::String(v.to_string()));
    }
    Ok(map)
}

async fn kubectl(args: &[&str]) -> Result<std::process::Output, CanaryError> {
    Command::new("kubectl")
        .args(args)
        .output()
        .await
        .map_err(|e| CanaryError::KubectlExec(e.to_string()))
}

impl CanaryPod {
    /// Deploy a canary pod for NetworkPolicy testing in the specified namespace.
    ///
    /// The pod will be named `canary-{uuid}` and runs a busybox container
    /// with `sleep` to keep it alive for testing.
    ///
    /// # Errors
    ///
    /// Returns an error if kubectl commands fail or the pod doesn't become ready.
    pub async fn deploy(namespace: &str) -> Result<Self, CanaryError> {
        Self::deploy_with_config(namespace, CanaryConfig::default()).await
    }

    /// Deploy a generic `sleep` canary with custom labels/prefix.
    ///
    /// ```ignore
    /// let config = CanaryConfig {
    ///     labels: "app=gc-service,test=network-policy".to_string(),
    ///     ..CanaryConfig::default()
    /// };
    /// let canary = CanaryPod::deploy_with_config("dark-tower", config).await?;
    /// ```
    ///
    /// # Errors
    ///
    /// As [`CanaryPod::deploy_manifest`].
    pub async fn deploy_with_config(
        namespace: &str,
        config: CanaryConfig,
    ) -> Result<Self, CanaryError> {
        let name = canary_name(&config.name_prefix);
        let manifest = canary_pod_manifest(
            &name,
            namespace,
            &Value::Object(parse_labels(&config.labels)?),
            json!({
                "name": "canary",
                "command": ["sleep", GENERIC_CANARY_LIFETIME_SECONDS.to_string()],
            }),
            GENERIC_CANARY_LIFETIME_SECONDS,
        );
        Self::deploy_manifest(namespace, &name, &manifest, GENERIC_RUNNING_TIMEOUT).await
    }

    /// Create a canary from a full Pod manifest (`kubectl create -f -`, so an
    /// existing pod of the same name is an error, never adopted), then wait for
    /// it to be Running with every container started. Readiness is NOT
    /// required — a canary may carry a never-passing readiness probe.
    ///
    /// The returned value owns the pod: explicit [`CanaryPod::cleanup`] is the
    /// path; Drop is only a backstop.
    ///
    /// # Errors
    ///
    /// kubectl cannot run, the create fails, or the pod is not Running with all
    /// containers started within `running_timeout` (the pod is then deleted
    /// with the asserted [`CanaryPod::cleanup`] and the error reports its
    /// outcome).
    pub async fn deploy_manifest(
        namespace: &str,
        name: &str,
        manifest: &Value,
        running_timeout: Duration,
    ) -> Result<Self, CanaryError> {
        Self::ensure_namespace(namespace).await?;

        let mut child = Command::new("kubectl")
            .args(["create", "-f", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;
        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| CanaryError::KubectlExec("kubectl stdin unavailable".into()))?;
            stdin
                .write_all(manifest.to_string().as_bytes())
                .await
                .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;
        }
        let out = child
            .wait_with_output()
            .await
            .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;
        if !out.status.success() {
            return Err(CanaryError::DeployFailed(format!(
                "kubectl create pod {name} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }

        let canary = Self {
            name: name.to_string(),
            namespace: namespace.to_string(),
            cleaned_up: AtomicBool::new(false),
        };
        if let Err(wait_err) = canary.wait_for_running(running_timeout).await {
            // Explicit, asserted cleanup — never left to the Drop backstop.
            let cleanup = match canary.cleanup().await {
                Ok(()) => "canary deleted (asserted NotFound)".to_string(),
                Err(e) => e.to_string(),
            };
            return Err(CanaryError::PodNotReady(format!(
                "{wait_err}; cleanup: {cleanup}"
            )));
        }
        Ok(canary)
    }

    /// Ensure the namespace exists, creating it if necessary.
    async fn ensure_namespace(namespace: &str) -> Result<(), CanaryError> {
        if kubectl(&["get", "namespace", namespace])
            .await?
            .status
            .success()
        {
            return Ok(());
        }
        let create = kubectl(&["create", "namespace", namespace]).await?;
        if !create.status.success() {
            let stderr = String::from_utf8_lossy(&create.stderr);
            // Ignore "already exists" error (race condition)
            if !stderr.contains("already exists") {
                return Err(CanaryError::NamespaceFailed(stderr.to_string()));
            }
        }
        Ok(())
    }

    /// Wait for phase Running AND every container `started=true` (NOT Ready).
    async fn wait_for_running(&self, timeout: Duration) -> Result<(), CanaryError> {
        let start = std::time::Instant::now();
        loop {
            let pod = self.pod_json().await;
            if let Ok(pod) = &pod {
                let phase = pod["status"]["phase"].as_str().unwrap_or("");
                if phase == "Failed" || phase == "Succeeded" {
                    return Err(CanaryError::PodNotReady(format!(
                        "Pod {} entered phase {phase}",
                        self.name
                    )));
                }
                let statuses = pod["status"]["containerStatuses"].as_array();
                let all_started = statuses.is_some_and(|s| {
                    !s.is_empty() && s.iter().all(|c| c["started"].as_bool() == Some(true))
                });
                if phase == "Running" && all_started {
                    return Ok(());
                }
            }
            if start.elapsed() > timeout {
                let detail = match pod {
                    Ok(p) => format!(
                        "phase={} conditions={}",
                        p["status"]["phase"], p["status"]["conditions"]
                    ),
                    Err(e) => e.to_string(),
                };
                return Err(CanaryError::PodNotReady(format!(
                    "Pod {} in namespace {} was not Running with all containers started \
                     within {}s ({detail})",
                    self.name,
                    self.namespace,
                    timeout.as_secs()
                )));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// The live Pod object (`kubectl get pod -o json`).
    ///
    /// # Errors
    ///
    /// kubectl cannot run, fails, or prints unparseable output.
    pub async fn pod_json(&self) -> Result<Value, CanaryError> {
        let out = kubectl(&[
            "get",
            "pod",
            &self.name,
            "-n",
            &self.namespace,
            "-o",
            "json",
        ])
        .await?;
        if !out.status.success() {
            return Err(CanaryError::KubectlExec(format!(
                "kubectl get pod {} failed: {}",
                self.name,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|e| CanaryError::KubectlExec(format!("unparseable pod JSON: {e}")))
    }

    /// Run `sh -c <script>` inside the canary; returns stdout.
    ///
    /// # Errors
    ///
    /// kubectl cannot run, or the command exits non-zero.
    pub async fn exec_sh(&self, script: &str) -> Result<String, CanaryError> {
        let out = kubectl(&[
            "exec",
            &self.name,
            "-n",
            &self.namespace,
            "--",
            "sh",
            "-c",
            script,
        ])
        .await?;
        if !out.status.success() {
            return Err(CanaryError::ExecFailed(format!(
                "exec in {} exited {}: {}",
                self.name,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    /// The canary's container log.
    ///
    /// # Errors
    ///
    /// kubectl cannot run or fails.
    pub async fn logs(&self) -> Result<String, CanaryError> {
        let out = kubectl(&["logs", &self.name, "-n", &self.namespace]).await?;
        if !out.status.success() {
            return Err(CanaryError::KubectlExec(format!(
                "kubectl logs {} failed: {}",
                self.name,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    /// Test if the canary pod can reach a target URL (wget `--spider`, 5s).
    ///
    /// `true` if the target is reachable, `false` if blocked/timeout.
    pub async fn can_reach(&self, target_url: &str) -> bool {
        self.wget(target_url, true).await.is_some()
    }

    /// Full HTTP GET from inside the canary; `Some(body)` on success.
    pub async fn fetch(&self, target_url: &str) -> Option<String> {
        self.wget(target_url, false).await
    }

    /// `wget -q -O- -T 5 [--spider] <url>` inside the canary; `Some(stdout)`
    /// when it succeeds, `None` when blocked, timed out, or kubectl failed.
    async fn wget(&self, url: &str, spider: bool) -> Option<String> {
        let mut args = vec![
            "exec",
            &self.name,
            "-n",
            &self.namespace,
            "--",
            "wget",
            "-q",
            "-O-",
            "-T",
            "5",
        ];
        if spider {
            args.push("--spider");
        }
        args.push(url);
        kubectl(&args)
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    }

    /// Get the pod name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the namespace the pod is running in.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Explicit, ASSERTED cleanup: graceful `kubectl delete pod --wait`, then
    /// the pod must be NotFound. Idempotent. Never `--force`.
    ///
    /// # Errors
    ///
    /// The delete fails or times out, or the pod still exists afterwards.
    pub async fn cleanup(&self) -> Result<(), CanaryError> {
        if self.cleaned_up.load(Ordering::SeqCst) {
            return Ok(());
        }
        let timeout = format!("--timeout={DELETE_TIMEOUT_SECONDS}s");
        let delete = kubectl(&[
            "delete",
            "pod",
            &self.name,
            "-n",
            &self.namespace,
            "--ignore-not-found=true",
            "--wait=true",
            &timeout,
        ])
        .await?;
        if !delete.status.success() {
            return Err(CanaryError::CleanupFailed(format!(
                "{}: {} — run: {}",
                self.name,
                String::from_utf8_lossy(&delete.stderr).trim(),
                graceful_delete_command(&self.name, &self.namespace)
            )));
        }
        // Gone = a SUCCESSFUL `--ignore-not-found` get that prints nothing
        // (no dependence on kubectl's error wording).
        let get = kubectl(&[
            "get",
            "pod",
            &self.name,
            "-n",
            &self.namespace,
            "--ignore-not-found",
            "-o",
            "name",
        ])
        .await?;
        let stdout = String::from_utf8_lossy(&get.stdout);
        if !get.status.success() || !stdout.trim().is_empty() {
            return Err(CanaryError::CleanupFailed(format!(
                "{} not confirmed gone after delete (get exit {}, stdout {:?}, stderr {:?}) — run: {}",
                self.name,
                get.status,
                stdout.trim(),
                String::from_utf8_lossy(&get.stderr).trim(),
                graceful_delete_command(&self.name, &self.namespace)
            )));
        }
        self.cleaned_up.store(true, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for CanaryPod {
    /// Backstop only. Prints the graceful delete command and fires a
    /// NON-BLOCKING, NON-FORCED delete (`--wait=false`). Its outcome is NOT
    /// observed (only a spawn failure is reported); the printed command is the
    /// verification path. Synchronous `std::process` because Drop cannot await.
    fn drop(&mut self) {
        if self.cleaned_up.load(Ordering::SeqCst) {
            return;
        }
        eprintln!(
            "[canary] {} in {} was not explicitly cleaned up; requesting a graceful delete. \
             Verify with: {}",
            self.name,
            self.namespace,
            graceful_delete_command(&self.name, &self.namespace)
        );
        let spawned = std::process::Command::new("kubectl")
            .args([
                "delete",
                "pod",
                &self.name,
                "-n",
                &self.namespace,
                "--ignore-not-found=true",
                "--wait=false",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Err(e) = spawned {
            eprintln!(
                "[canary] backstop delete of {} could not start: {e}",
                self.name
            );
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

    #[test]
    fn names_carry_the_prefix_and_eight_hex() {
        let n = canary_name("gate-zero-canary");
        let suffix = n.strip_prefix("gate-zero-canary-").expect("prefix");
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn canary_names_are_recognised_by_shape() {
        assert!(is_canary_name(
            "gate-zero-canary",
            &canary_name("gate-zero-canary")
        ));
        assert!(!is_canary_name("gate-zero-canary", "gate-zero-canary-xyz"));
        assert!(!is_canary_name("gate-zero-canary", "mh-0-abcdef12"));
        assert!(!is_canary_name("canary", "canaryx-abcdef12"));
    }

    #[test]
    fn every_canary_carries_the_skeleton_invariants() {
        let m = canary_pod_manifest("n", "ns", &json!({"a": "b"}), json!({"name": "c"}), 7);
        assert_eq!(m["spec"]["containers"][0]["image"], BUSYBOX_IMAGE);
        assert_eq!(
            m["spec"]["terminationGracePeriodSeconds"],
            CANARY_TERMINATION_GRACE_SECONDS
        );
        assert_eq!(m["spec"]["activeDeadlineSeconds"], 7);
        assert_eq!(m["spec"]["restartPolicy"], "Never");
    }

    #[test]
    fn the_delete_command_is_graceful() {
        let cmd = graceful_delete_command("p", "ns");
        assert!(!cmd.contains("--force"));
        assert!(!cmd.contains("--grace-period"));
    }

    #[test]
    fn labels_parse() {
        let m = parse_labels("a=b,c=d").expect("parse");
        assert_eq!(m["a"], "b");
        assert_eq!(m["c"], "d");
        assert!(parse_labels("nope").is_err());
    }
}
