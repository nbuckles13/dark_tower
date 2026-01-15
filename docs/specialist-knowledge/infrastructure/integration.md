# Infrastructure Specialist - Integration Notes

Notes on working with other services and specialists in Dark Tower.

---

## Integration: env-tests Cluster Requirements
**Added**: 2026-01-13
**Related services**: env-tests, ac-service
**Coordination with**: Test Specialist, Security Specialist

The env-tests crate requires a running Kubernetes cluster with specific infrastructure:

**Prerequisites**:
1. Kind cluster: `./infra/kind/scripts/setup.sh`
2. Services deployed: AC service running in `dark-tower` namespace
3. Port-forwards: AC (8082), Prometheus (9090), Grafana (3000), Loki (3100)
4. kubectl configured for target cluster

**NetworkPolicy tests specifically require**:
- AC service's NetworkPolicy deployed (allows only `app=global-controller` ingress)
- Ability to create pods in arbitrary namespaces
- Ability to delete namespaces (for cleanup)

**Feature gates**:
- `smoke`: Fast cluster health (30s)
- `flows`: Service flows including auth (2-3min)
- `observability`: Metrics/logs validation
- `resilience`: NetworkPolicy, pod restart tests (5min+)

**Running tests**:
```bash
# NetworkPolicy tests require resilience feature
cargo test -p env-tests --features resilience -- network_policy
```

---

## Integration: Test Specialist - CanaryPod Design
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`
**Coordination with**: Test Specialist

CanaryPod was designed for Test Specialist to validate NetworkPolicy enforcement without requiring deep Kubernetes knowledge:

**API surface**:
```rust
// Simple case - default labels
let canary = CanaryPod::deploy("namespace").await?;
let ok = canary.can_reach("http://service:port").await;
canary.cleanup().await?;

// With custom labels (for NetworkPolicy matching)
let config = CanaryConfig::builder()
    .label("app", "global-controller")
    .build();
let canary = CanaryPod::deploy_with_config("namespace", config).await?;
```

**Error handling**: All errors are `CanaryError` variants with descriptive messages.

**Cleanup**: Automatic via Drop, but explicit `cleanup()` recommended for clearer test flow.

**Test patterns**: See Test Specialist's `patterns.md` for NetworkPolicy test structure.

---

## Integration: Security Specialist - NetworkPolicy Validation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`
**Coordination with**: Security Specialist

NetworkPolicy tests validate security boundaries at the network layer:

**What we test**:
1. Same-namespace pods CAN reach services (positive test)
2. Cross-namespace pods CANNOT reach services (negative test)
3. Service-to-service authentication is still required (handled by Security tests)

**What we DON'T test** (Security Specialist's domain):
- JWT validation
- mTLS certificate validation
- Rate limiting
- Authentication bypass attempts

**Relationship to defense-in-depth**:
- NetworkPolicy = Network layer (Layer 3/4) - Infrastructure owns
- Service authentication = Application layer (Layer 7) - Security owns
- Both must pass for legitimate traffic to succeed

**Gap analysis**: If positive test fails but service works via port-forward, NetworkPolicy is too restrictive. If negative test passes, NetworkPolicy is not enforced (security gap).

---

## Integration: Operations Specialist - Test Pod Cleanup
**Added**: 2026-01-13
**Coordination with**: Operations Specialist

Test pods (canary-*) may be left behind if:
- Test crashes before cleanup
- kubectl delete fails
- CI job times out

**Identification**: All test pods have labels:
```
app=canary
test=network-policy
```

**Manual cleanup** (if needed):
```bash
# Delete all canary pods across namespaces
kubectl delete pods -A -l app=canary

# Delete test namespaces
kubectl delete namespace -l test-namespace=true
```

**Runbook suggestion**: Add periodic cleanup job or include in CI post-step:
```bash
kubectl delete pods -A -l app=canary --ignore-not-found
kubectl delete namespace -l canary-test --ignore-not-found
```

---

## Integration: Database Specialist - N/A
**Added**: 2026-01-13

No direct integration with Database Specialist for env-tests infrastructure. Database connectivity is tested via AC service health endpoints, not direct database access.

---

## Integration: Observability Specialist - Prometheus Metrics
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/30_observability.rs`
**Coordination with**: Observability Specialist

env-tests validates Prometheus metrics are exposed and queryable:

**Infrastructure requirements**:
- Prometheus port-forward on 9090
- ServiceMonitor configured for AC service
- Metrics endpoint exposed on AC service (:8082/metrics)

**What Infrastructure validates**:
- Prometheus is reachable
- Scrape targets are up
- Basic metric existence

**What Observability validates**:
- Metric semantics (counters vs gauges)
- Label correctness
- Dashboard queries work

---

## Integration: Protocol Specialist - N/A
**Added**: 2026-01-13

No direct integration. Protocol buffers are compiled and used by services; env-tests interact via HTTP/JSON APIs.

---

## Future Integration Notes

### mTLS Testing (Planned)
When mTLS is implemented, will need:
- Certificate generation for test pods
- Trust bundle configuration
- CanaryPod enhancement to support client certificates

### Service Mesh Testing (Planned)
If Linkerd/Istio adopted:
- Sidecar injection for test pods
- Traffic policy validation
- Mutual TLS via mesh

---

## Quick Reference: Which Specialist for What

| Concern | Primary Specialist | Infrastructure Role |
|---------|-------------------|---------------------|
| NetworkPolicy rules | Infrastructure | Owner |
| NetworkPolicy test code | Test | Owner |
| Security validation | Security | Owner |
| Prometheus infra | Infrastructure | Owner |
| Metric semantics | Observability | Owner |
| Pod cleanup runbook | Operations | Owner |
| kubectl access | Infrastructure | Owner |
