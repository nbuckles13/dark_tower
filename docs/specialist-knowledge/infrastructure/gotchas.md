# Infrastructure Specialist - Gotchas

Mistakes to avoid, learned from experience in Dark Tower infrastructure work.

---

## Gotcha: Synchronous kubectl Blocks Async Executor
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Using `std::process::Command` inside `async fn` blocks the current executor thread:

```rust
// This BLOCKS the async executor thread!
async fn get_pod_status() -> String {
    let output = Command::new("kubectl")  // std::process::Command
        .args(["get", "pod", "foo"])
        .output()
        .unwrap();
    // ...
}
```

**Impact**: Low for test code with sequential execution. High for production code with many concurrent tasks.

**Mitigation options**:
1. Accept it for test code (recommended if tests are sequential)
2. Use `tokio::process::Command` for true async
3. Use `tokio::task::spawn_blocking()` to move to blocking thread pool
4. Use kube-rs crate for native async Kubernetes API

**Recommendation**: For env-tests, synchronous is fine. Document as intentional choice.

---

## Gotcha: Missing Image Pull Policy on Test Pods
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Kubernetes default `imagePullPolicy` depends on tag:
- `:latest` tag → Always pull
- Specific tag (`:1.36`) → IfNotPresent

```bash
kubectl run canary --image=busybox:1.36  # Default: IfNotPresent
```

**Problem scenarios**:
- Air-gapped cluster without image pre-pulled
- Docker Hub rate limits in CI (100 pulls/6hr for anonymous)
- Slow network causing test timeout

**Mitigation**: Explicitly set `--image-pull-policy=IfNotPresent`:
```bash
kubectl run canary --image=busybox:1.36 --image-pull-policy=IfNotPresent
```

Or pre-pull required images in cluster setup script.

---

## Gotcha: NetworkPolicy Tests Require Matching Pod Labels
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

NetworkPolicy selects pods by label, not by namespace alone. A canary pod with `app=canary` label will be blocked by a NetworkPolicy that only allows `app=global-controller`.

```yaml
# AC service NetworkPolicy
spec:
  ingress:
    - from:
        - podSelector:
            matchLabels:
              app: global-controller  # Only allows this label!
```

**Symptom**: Positive test fails (same-namespace connectivity blocked).

**Solution**: Configure canary pod labels to match allowed ingress:
```rust
let config = CanaryConfig::builder()
    .label("app", "global-controller")  // Impersonate allowed caller
    .build();
let canary = CanaryPod::deploy_with_config("dark-tower", config).await?;
```

**Lesson**: Review actual NetworkPolicy rules before writing connectivity tests.

---

## Gotcha: Namespace Deletion is Async
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

`kubectl delete namespace` returns immediately, but namespace deletion is asynchronous:

```bash
kubectl delete namespace canary-test  # Returns quickly
kubectl get namespace canary-test     # May still exist!
```

**Problem**: Subsequent test creates namespace with same name, fails because deletion is in progress (namespace is "Terminating").

**Mitigation options**:
1. Use unique namespace names per test run (recommended)
2. Wait for namespace to fully delete: `kubectl wait --for=delete namespace/foo`
3. Add delay between test runs

**Current approach**: Use UUID in namespace name (`canary-test-{uuid}`) so each test gets fresh namespace.

---

## Gotcha: Drop Cannot Await
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Rust's `Drop::drop()` is synchronous - you cannot `.await` inside it:

```rust
impl Drop for CanaryPod {
    fn drop(&mut self) {
        // ERROR: Cannot await in non-async function
        self.cleanup().await;  // Won't compile!
    }
}
```

**Solution**: Implement synchronous cleanup for Drop, async cleanup for explicit calls:

```rust
impl CanaryPod {
    fn do_cleanup(&self) -> Result<(), Error> {
        // Synchronous kubectl call
        Command::new("kubectl").args(["delete", "pod", ...]).output()
    }

    pub async fn cleanup(&self) -> Result<(), Error> {
        self.do_cleanup()  // Reuse sync impl
    }
}

impl Drop for CanaryPod {
    fn drop(&mut self) {
        let _ = self.do_cleanup();  // Sync cleanup
    }
}
```

---

## Gotcha: kubectl exec Timeout vs Network Timeout
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

When testing NetworkPolicy blocking, there are two timeouts to consider:

1. **wget timeout** (`-T 5`): How long wget waits for network response
2. **kubectl exec timeout**: How long kubectl waits for exec to complete

```rust
// wget has 5s timeout, but kubectl exec might timeout first!
Command::new("kubectl")
    .args(["exec", pod, "--", "wget", "-T", "5", url])
    .output()  // No timeout - waits indefinitely!
```

**Problem**: If cluster is very slow, kubectl exec could hang forever.

**Mitigation options**:
1. Use `Command::new().timeout()` (if available in your Command wrapper)
2. Rely on wget's built-in timeout (usually sufficient)
3. Add explicit timeout in calling code

**Current approach**: Rely on wget's 5-second timeout. If that fails, test fails with timeout error.

---

## Gotcha: FQDN Required for Cross-Namespace Service Access
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Short service names only resolve within the same namespace:

```rust
// From pod in namespace "canary-test":
"http://ac-service:8082"           // FAILS - no such service in canary-test
"http://ac-service.dark-tower:8082" // Works (FQDN)
"http://ac-service.dark-tower.svc.cluster.local:8082"  // Also works (full FQDN)
```

**Symptom**: Negative test passes for wrong reason (DNS failure, not NetworkPolicy).

**Solution**: Always use FQDN for cross-namespace tests:
```rust
let url = "http://ac-service.dark-tower.svc.cluster.local:8082/health";
```

---

## Gotcha: Pod Phase vs Container Ready
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Pod `status.phase = Running` doesn't guarantee containers are ready:

```bash
kubectl get pod canary -o jsonpath='{.status.phase}'      # Running
kubectl get pod canary -o jsonpath='{.status.containerStatuses[0].ready}'  # false!
```

**For busybox with `sleep`**: Phase check is sufficient (no readiness probe).

**For real services**: Should check:
- `status.phase == Running` AND
- All containers have `ready == true`

Or use:
```bash
kubectl wait --for=condition=Ready pod/canary --timeout=30s
```

**Current approach**: Phase check only, acceptable for busybox canary pods.

---

## Gotcha: AtomicBool Ordering for Simple Flags
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Using `Ordering::SeqCst` for a simple "already cleaned up" flag is overkill:

```rust
// Overly conservative but correct:
self.cleaned_up.swap(true, Ordering::SeqCst)

// Would also work for single boolean flag:
self.cleaned_up.swap(true, Ordering::Relaxed)
```

**Why SeqCst is used**: Defensive programming, performance doesn't matter in test cleanup.

**When to optimize**: Only if profiling shows atomic operations are a bottleneck (unlikely).

**Recommendation**: Keep SeqCst for safety. Not worth the mental overhead of weaker orderings.
