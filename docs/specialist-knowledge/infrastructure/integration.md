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
cargo test -p env-tests --features resilience -- network_policy
```

---

## Integration: Test Specialist - CanaryPod Design
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`
**Coordination with**: Test Specialist

CanaryPod API designed for Test Specialist to validate NetworkPolicy without deep Kubernetes knowledge:

```rust
// Simple case
let canary = CanaryPod::deploy("namespace").await?;
let ok = canary.can_reach("http://service:port").await;
canary.cleanup().await?;

// With custom labels for NetworkPolicy matching
let config = CanaryConfig::builder()
    .label("app", "global-controller")
    .build();
let canary = CanaryPod::deploy_with_config("namespace", config).await?;
```

All errors are `CanaryError` variants with descriptive messages.

---

## Integration: Security Specialist - NetworkPolicy Validation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`
**Coordination with**: Security Specialist

NetworkPolicy tests validate security boundaries at the network layer:

**What Infrastructure validates**:
- Same-namespace pods CAN reach services (positive test)
- Cross-namespace pods CANNOT reach services (negative test)

**What Security Specialist validates**:
- JWT validation, mTLS, rate limiting, authentication bypass

**Defense-in-depth relationship**:
- NetworkPolicy = Network layer (Layer 3/4) - Infrastructure owns
- Service authentication = Application layer (Layer 7) - Security owns

---

## Integration: Operations Specialist - Test Pod Cleanup
**Added**: 2026-01-13
**Coordination with**: Operations Specialist

Test pods (canary-*) may be left behind if tests crash or timeout.

**Identification labels**:
```
app=canary
test=network-policy
```

**Manual cleanup**:
```bash
kubectl delete pods -A -l app=canary
kubectl delete namespace -l canary-test --ignore-not-found
```

---

## Integration: Observability Specialist - Prometheus Metrics
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/30_observability.rs`
**Coordination with**: Observability Specialist

**Infrastructure requirements**:
- Prometheus port-forward on 9090
- ServiceMonitor configured for AC service
- Metrics endpoint exposed on AC service (:8082/metrics)

**Infrastructure validates**: Prometheus reachable, scrape targets up, basic metric existence
**Observability validates**: Metric semantics, label correctness, dashboard queries

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
