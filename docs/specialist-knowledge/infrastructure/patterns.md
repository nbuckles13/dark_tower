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

## Pattern: Synchronous kubectl in Test Code
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

For test utilities that shell out to kubectl, use `std::process::Command` (synchronous) rather than async alternatives:

```rust
let result = Command::new("kubectl")
    .args(["get", "pod", &name, "-o", "jsonpath={.status.phase}"])
    .output()
    .map_err(|e| CanaryError::KubectlExec(e.to_string()))?;
```

Why synchronous is acceptable in tests:
- Tests run sequentially (often with `#[serial]`)
- Simplifies error handling (no async error propagation)
- kubectl operations are I/O-bound anyway (not CPU-bound)
- Test code doesn't need high concurrency

When to use async (`tokio::process::Command`):
- Production code with multiple concurrent operations
- When blocking would starve the async executor
- When kubectl operations must interleave with other async work

---

## Pattern: Idempotent Namespace Creation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Handle namespace creation race conditions gracefully:

```rust
fn ensure_namespace(namespace: &str) -> Result<(), Error> {
    // Check if exists
    let check = Command::new("kubectl")
        .args(["get", "namespace", namespace])
        .output()?;

    if check.status.success() {
        return Ok(());  // Already exists
    }

    // Create namespace
    let create = Command::new("kubectl")
        .args(["create", "namespace", namespace])
        .output()?;

    if !create.status.success() {
        let stderr = String::from_utf8_lossy(&create.stderr);
        // Race condition: another process created it
        if !stderr.contains("already exists") {
            return Err(Error::NamespaceFailed(stderr.to_string()));
        }
    }
    Ok(())
}
```

This pattern handles:
- Namespace already exists (common case)
- Race condition when parallel tests create same namespace
- Actual creation failures (permission denied, etc.)

---

## Pattern: Force Delete for Test Pods
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Test pods don't need graceful shutdown. Use force deletion for faster cleanup:

```bash
kubectl delete pod $name --namespace=$ns --grace-period=0 --force --ignore-not-found=true
```

Flags explained:
- `--grace-period=0`: Don't wait for graceful termination
- `--force`: Delete immediately even if apiserver thinks pod is stuck
- `--ignore-not-found=true`: No error if already deleted (idempotent)

When NOT to force delete:
- Production workloads (need graceful drain)
- Stateful pods (need orderly shutdown)
- Pods with preStop hooks (hooks won't run)

---

## Pattern: Drop-based Resource Cleanup
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Implement `Drop` to clean up external resources when test struct goes out of scope:

```rust
impl Drop for CanaryPod {
    fn drop(&mut self) {
        if !self.cleaned_up.load(Ordering::SeqCst) {
            // Synchronous cleanup in drop (can't await)
            let _ = self.do_cleanup();
        }
    }
}
```

Key considerations:
- **Cannot await in Drop**: Use synchronous implementation
- **AtomicBool tracking**: Prevent cleanup if already done via explicit cleanup()
- **Ignore errors in Drop**: Log but don't panic (causes double-panic)
- **Best-effort**: Resource may leak if kubectl fails, but prevents zombie pods on panic

---

## Pattern: Pod Labels for Test Identification
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Label test pods for easy identification and cleanup:

```bash
kubectl run canary-abc123 \
    --labels=app=canary,test=network-policy \
    ...
```

Recommended labels:
- `app=canary` or `app=test-pod`: Identifies as test infrastructure
- `test=<test-name>`: Links to specific test suite
- `created-by=env-tests`: Identifies automation source

Useful for:
- Manual cleanup: `kubectl delete pods -l app=canary`
- Debugging: `kubectl get pods -l test=network-policy`
- Preventing accidental deletion of non-test pods

---

## Pattern: Service DNS in Kubernetes Tests
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Use appropriate DNS names based on test scenario:

**Same namespace**: Short name works
```rust
let url = "http://ac-service:8082/health";  // Resolves via search domain
```

**Cross namespace**: Requires FQDN
```rust
let url = "http://ac-service.dark-tower.svc.cluster.local:8082/health";
```

DNS resolution rules:
- Pod in namespace X trying to reach service in namespace X: short name works
- Pod in namespace X trying to reach service in namespace Y: FQDN required
- The search domain is based on the pod's namespace, not the target

This matters for NetworkPolicy tests where canary pod is in different namespace.

---

## Pattern: Pod Readiness Wait Loop
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Poll pod status until ready or timeout:

```rust
async fn wait_for_ready(&self, timeout_seconds: u32) -> Result<(), Error> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_seconds as u64);

    loop {
        if start.elapsed() > timeout {
            return Err(Error::PodNotReady(format!(
                "Pod {} did not become ready within {}s",
                self.name, timeout_seconds
            )));
        }

        let phase = get_pod_phase(&self.name)?;
        match phase.as_str() {
            "Running" => return Ok(()),
            "Failed" | "Error" => return Err(Error::PodNotReady(...)),
            _ => {}  // Pending, ContainerCreating - keep waiting
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
```

Key considerations:
- Check for terminal states (Failed, Error) to fail fast
- 1-second poll interval is reasonable for test scenarios
- 30 seconds default timeout is usually sufficient for busybox

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

This prevents accidental deletion of:
- `dark-tower` namespace (production services)
- `kube-system` namespace (cluster infrastructure)
- Any other non-test namespace

Use a distinctive prefix like `canary-test-` for all test-created namespaces.
