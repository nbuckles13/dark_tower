# Infrastructure Specialist - Gotchas

Mistakes to avoid, learned from experience in Dark Tower infrastructure work.

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

## Gotcha: Redis Probes Expose Password in Process List
**Added**: 2026-01-31
**Related files**: `infra/services/redis/statefulset.yaml`

Using `redis-cli -a $REDIS_PASSWORD ping` in liveness/readiness probes exposes the password in the process list (visible via `ps aux`). This is a security issue even in development clusters.

**Symptom**: Password visible in process listings, security scanners flag credential exposure.

**Solution**: Use the `REDISCLI_AUTH` environment variable instead. Redis-cli reads this automatically for authentication:
```yaml
env:
- name: REDISCLI_AUTH
  valueFrom:
    secretKeyRef:
      name: redis-secrets
      key: REDIS_PASSWORD
livenessProbe:
  exec:
    command: ["sh", "-c", "redis-cli ping | grep -q PONG"]
```

**Lesson**: Always prefer environment-based authentication over command-line flags for any probe or init container.

---

## Gotcha: UDP Services Require Explicit Protocol in K8s Service
**Added**: 2026-01-31
**Related files**: `infra/services/meeting-controller/service.yaml`

Kubernetes Services default to TCP protocol. For UDP-based services like WebTransport (QUIC), you must explicitly specify `protocol: UDP` in the Service port definition, or traffic will not route correctly.

**Symptom**: UDP clients cannot connect via ClusterIP; works with hostNetwork but not via Service.

**Solution**: Explicitly declare UDP protocol:
```yaml
ports:
- name: webtransport
  protocol: UDP  # Required! Defaults to TCP otherwise
  port: 4433
  targetPort: 4433
```

**Lesson**: When adding non-HTTP services, always verify the protocol field matches the actual transport.
