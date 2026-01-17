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
