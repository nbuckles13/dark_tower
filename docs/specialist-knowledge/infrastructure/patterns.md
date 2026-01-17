# Infrastructure Specialist - Patterns

Infrastructure patterns worth documenting for Dark Tower codebase.

---

## Pattern: CanaryPod for NetworkPolicy Testing
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Deploy minimal test pods to validate NetworkPolicy enforcement from within the cluster:

```rust
pub struct CanaryPod {
    name: String,
    namespace: String,
    cleaned_up: AtomicBool,
}

impl CanaryPod {
    pub async fn deploy(namespace: &str) -> Result<Self, CanaryError> {
        // Generate unique name: canary-{uuid8}
        // kubectl run with busybox:1.36, sleep 3600
        // Wait for pod Running status
    }

    pub async fn can_reach(&self, target_url: &str) -> bool {
        // kubectl exec -- wget --spider -T 5 <url>
    }
}
```

Key design decisions:
- **busybox:1.36**: Minimal image with wget for HTTP probes
- **sleep 3600**: Keep pod alive for testing duration (1hr max)
- **--restart=Never**: Create bare pod, not Deployment
- **AtomicBool cleanup tracking**: Prevent double-delete on Drop + explicit cleanup()

---

## Pattern: Idempotent Namespace Creation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Handle namespace creation race conditions in parallel test runs:

```rust
fn ensure_namespace(namespace: &str) -> Result<(), Error> {
    let check = Command::new("kubectl")
        .args(["get", "namespace", namespace])
        .output()?;

    if check.status.success() {
        return Ok(());
    }

    let create = Command::new("kubectl")
        .args(["create", "namespace", namespace])
        .output()?;

    if !create.status.success() {
        let stderr = String::from_utf8_lossy(&create.stderr);
        if !stderr.contains("already exists") {
            return Err(Error::NamespaceFailed(stderr.to_string()));
        }
    }
    Ok(())
}
```

This handles the race condition when parallel tests create the same namespace simultaneously.

---

## Pattern: Test Namespace Cleanup Guard
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Only cleanup namespaces that were created for testing:

```rust
fn cleanup_test_namespace(namespace: &str) {
    // Safety check: only delete test namespaces
    if !namespace.starts_with("canary-test") {
        return;
    }
    let _ = Command::new("kubectl")
        .args(["delete", "namespace", namespace, "--ignore-not-found=true"])
        .output();
}
```

Prevents accidental deletion of `dark-tower` or `kube-system` namespaces. Use distinctive prefix like `canary-test-` for all test-created namespaces.
