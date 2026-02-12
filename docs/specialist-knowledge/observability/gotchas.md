# Observability Gotchas

*This file accumulates observability pitfalls to avoid. Updated from experience.*

---

## Gotcha: Health Endpoint Changes Break Tests in Multiple Locations
**Added**: 2026-02-06
**Related files**: `crates/global-controller/tests/auth_tests.rs`, `crates/global-controller/tests/health_tests.rs`, `crates/gc-test-utils/src/server_harness.rs`

When `/health` endpoint behavior changes (e.g., from JSON to plain text "OK" for Kubernetes liveness probes), tests in multiple locations may break:
1. Test utilities (`*-test-utils/src/server_harness.rs`)
2. Integration tests (`crates/*/tests/*.rs`)
3. Any test that calls the endpoint

Search for all usages: `grep -r "/health" crates/*/tests/`. The `/ready` endpoint returns JSON with detailed health status; use that for tests requiring structured responses.

---

## Gotcha: Grafana Datasource UID Must Match Provisioned Configuration
**Added**: 2026-02-06
**Related files**: `infra/grafana/dashboards/*.json`, `infra/grafana/provisioning/datasources/prometheus.yaml`

Dashboard panels reference datasource by UID, not name:
```json
"datasource": { "type": "prometheus", "uid": "prometheus" }
```
The UID must match the datasource provisioning configuration. The `grafana-datasources.sh` guard validates this. If adding new datasources, update both the provisioning config and reference it correctly in dashboards.

---

## Gotcha: Alert Duration Tuning Affects False Positive Rate
**Added**: 2026-02-06
**Related files**: `infra/docker/prometheus/rules/gc-alerts.yaml`

The `for:` duration in alerts controls how long the condition must be true before firing:
- Too short (1m): False positives from transient spikes
- Too long (15m): Delayed detection of real issues

Guidelines:
- Critical alerts: 1-5 minutes (fast detection, accept some noise)
- Warning alerts: 5-10 minutes (reduce noise for non-urgent issues)

Tune based on production data after deployment. Initial values are estimates.

---
