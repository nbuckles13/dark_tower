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

---

## Gotcha: Promtail Exports `app` Label, Not `job`
**Added**: 2026-02-12
**Related files**: `infra/kubernetes/observability/promtail-config.yaml`, `infra/grafana/dashboards/*-logs.json`

Prometheus examples commonly use `job="service-name"` in queries. However, Promtail's relabel_configs export `app` (from pod labels), not `job`. Dashboard LogQL queries using `job=` will return no data.

**Promtail relabel_configs output:**
- `namespace` - Kubernetes namespace
- `pod` - Pod name
- `container` - Container name
- `app` - From `__meta_kubernetes_pod_label_app`
- `component` - From `__meta_kubernetes_pod_label_component`

**No `job` label exists!**

**Symptom**: Grafana dashboard shows "No data" for all Loki log panels, even when logs are visible in Explore.

**Solution**: Use `app="service-name"` instead of `job="service-name"` in LogQL queries:
```diff
- {job="ac-service"} | level="error"
+ {app="ac-service"} | level="error"
```

**Lesson**: Before writing LogQL queries, verify which labels Promtail actually exports by checking the relabel_configs in the Promtail ConfigMap.

---

## Gotcha: Python Heredocs Don't Accept Command-Line Arguments
**Added**: 2026-02-12
**Related files**: `scripts/guards/simple/grafana-datasources.sh`

When embedding Python in bash scripts, heredocs (`<<EOF`) don't support passing arguments via `sys.argv`:

```bash
# This FAILS - sys.argv[1] is undefined
python3 << 'EOF'
import sys
with open(sys.argv[1], 'r') as f:  # Error: index out of range
    content = f.read()
EOF
"$CONFIG_FILE"  # This is NOT passed to Python
```

**Solution**: Use inline Python with variable interpolation:
```bash
# This WORKS
python3 -c "
config_file = '$CONFIG_FILE'
with open(config_file, 'r') as f:
    content = f.read()
"
```

**Lesson**: When bash variables need to reach Python, either:
1. Use `-c` with variable interpolation (simple scripts)
2. Use environment variables with `os.environ` (complex scripts)
3. Save to temp file and pass path via argument (very complex scripts)

---

## Gotcha: JSON Structured Logs Require Label Selector Syntax, Not Line Filters
**Added**: 2026-02-13
**Related files**: `infra/grafana/dashboards/*-logs.json`, `infra/kubernetes/observability/promtail-config.yaml`

When switching from text logs to JSON structured logging, log fields extracted by Promtail become indexed Loki labels. Dashboard queries must use label selector syntax, not line filter syntax.

**Wrong approach (line filter for indexed label)**:
```logql
{app="ac-service", pod=~"$pod"} |~ "${log_level}"
```

**Correct approach (label selector for indexed label)**:
```logql
{app="ac-service", pod=~"$pod", level=~"$level"}
```

**Why this matters**:
- **Text logs**: `level` is in message content → use line filter `|~ "ERROR"`
- **JSON logs**: `level` is extracted to label → use label selector `level="ERROR"`

**Symptom**: Dashboard queries return no data even when logs exist. Grafana Explore shows the label exists but queries don't match.

**How to identify**:
1. Check Promtail `pipeline_stages` for `json` stage
2. Check for `labels:` stage that promotes fields
3. If field is promoted to label, use label selector syntax

**Lesson**: When Promtail extracts fields to labels (via `json` or `regex` + `labels` stages), those fields are indexed metadata, not searchable text. Query syntax must match the field location.

---

## Gotcha: Grafana Variables Must Match Log Case Sensitivity
**Added**: 2026-02-13
**Related files**: `infra/grafana/dashboards/*-logs.json`

Hard-coded dashboard variable values must match the exact case of values in logs. tracing_subscriber's JSON formatter emits uppercase log levels (ERROR, WARN, INFO), not lowercase.

**Wrong approach (hard-coded lowercase)**:
```json
{
  "name": "log_level",
  "type": "custom",
  "options": [
    {"text": "error", "value": "error"},  // Won't match "ERROR"
    {"text": "warn", "value": "warn"}     // Won't match "WARN"
  ]
}
```

**Correct approach (dynamic query)**:
```json
{
  "name": "level",
  "type": "query",
  "datasource": {"type": "loki", "uid": "loki"},
  "query": {"label": "level", "stream": ""},
  "includeAll": true
}
```

**Benefits of dynamic query**:
- Auto-discovers actual values from Loki (no case mismatch)
- Resilient to log level changes (trace → verbose)
- Consistent with other dynamic variables (pod, namespace)

**Symptom**: Dashboard variable dropdown shows "error", but selecting it returns no logs. Loki has "ERROR" logs but query filters for "error".

**Lesson**: Use query-type variables to discover values dynamically from data sources. Only use custom-type when values are truly static (like "true"/"false" toggles).
