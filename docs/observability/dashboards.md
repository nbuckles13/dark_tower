# Grafana Dashboards Catalog

This document catalogs all Grafana dashboards for Dark Tower services.

## Dashboard Organization

Dashboards are organized by:
- **Service**: Per-service operational dashboards (AC, GC, MC, MH)
- **Function**: Cross-service functional dashboards (SLOs, Security, Platform)
- **Environment**: Same dashboards used in dev, staging, and production

All dashboard JSON files are stored in `infra/grafana/dashboards/` and auto-loaded via Grafana provisioning.

---

## Global Controller Dashboards

### GC Overview

**File**: `infra/grafana/dashboards/gc-overview.json`
**UID**: `gc-overview`
**Tags**: `gc-service`, `service-overview`

**Purpose**: Primary operational dashboard for Global Controller service.

**Panels**:
1. **HTTP Request Rate by Endpoint** - Request rate (req/sec) across all endpoints
2. **HTTP Request Latency (P50/P95/P99)** - Latency percentiles with 200ms SLO line
3. **HTTP Error Rate (%)** - Gauge showing error rate with thresholds (green <0.1%, yellow 0.1-1%, red >1%)
4. **HTTP Status Codes** - Breakdown by 2xx/4xx/5xx status codes
5. **MC Assignment Rate by Status** - Assignment rate (ops/sec) by success/rejected/error status
6. **MC Assignment Latency (P50/P95/P99)** - Assignment latency with 20ms SLO line
7. **MC Assignment Success Rate (%)** - Gauge showing success rate (green >99%, yellow 95-99%, red <95%)
8. **Database Query Rate by Operation** - Query rate by operation type
9. **Database Query Latency (P50/P95/P99)** - DB query latency with 50ms SLO line
10. **Service Status** - Up/down status gauge
11. **Pod Count** - Number of running GC pods
12. **Memory Usage** - Memory consumption per pod
13. **CPU Usage** - CPU utilization per pod
14. **Token Refresh Rate by Status** - Token refresh attempts by success/error status
15. **Token Refresh Latency (P50/P95/P99)** - Token refresh latency percentiles
16. **Token Refresh Failures by Type** - Token refresh failures by error type
17. **AC Request Rate by Operation** - Requests to Auth Controller by operation and status
18. **AC Request Latency (P50/P95/P99)** - AC client request latency percentiles
19. **gRPC MC Call Rate** - gRPC calls to Meeting Controllers by method and status
20. **gRPC MC Call Latency (P50/P95/P99)** - gRPC call latency to Meeting Controllers
21. **MH Selection Rate by Status** - Media Handler selection attempts by status
22. **MH Selection Latency (P50/P95/P99)** - MH selection latency percentiles
23. **Meeting Creation Rate by Status** - Meeting creation attempts by success/error status
24. **Meeting Creation Latency (P50/P95/P99)** - Meeting creation latency percentiles (p50, p95, p99)
25. **Meeting Creation Failures by Type** - Meeting creation failures by error type (bad_request, forbidden, db_error, etc.)
26. **Registered Controllers by Type & Status** - Fleet health by controller type and status
27. **Errors by Operation & Type** - Error rate by operation and error type

**Metrics Used**:
- `gc_http_requests_total`
- `gc_http_request_duration_seconds`
- `gc_mc_assignments_total`
- `gc_mc_assignment_duration_seconds`
- `gc_db_queries_total`
- `gc_db_query_duration_seconds`
- `up{job="gc-service"}`
- `container_memory_usage_bytes`
- `container_cpu_usage_seconds_total`
- `gc_token_refresh_total`
- `gc_token_refresh_duration_seconds`
- `gc_token_refresh_failures_total`
- `gc_ac_requests_total`
- `gc_ac_request_duration_seconds`
- `gc_grpc_mc_calls_total`
- `gc_grpc_mc_call_duration_seconds`
- `gc_mh_selections_total`
- `gc_mh_selection_duration_seconds`
- `gc_meeting_creation_total`
- `gc_meeting_creation_duration_seconds`
- `gc_meeting_creation_failures_total`
- `gc_registered_controllers`
- `gc_errors_total`

**Default Time Range**: Last 1 hour
**Refresh**: 10 seconds

**When to Use**: Day-to-day operations, investigating performance issues, capacity planning

---

### GC SLOs

**File**: `infra/grafana/dashboards/gc-slos.json`
**UID**: `gc-slos`
**Tags**: `gc-service`, `slo`

**Purpose**: SLO compliance tracking and error budget monitoring for Global Controller.

**Panels**:
1. **Availability SLO - Error Budget Remaining** - Gauge showing remaining error budget for 30-day window (99.9% target)
2. **Error Budget Burn Rate** - Burn rate over time (1h and 6h windows) with sustainable rate line
3. **Availability Trend (7d / 28d)** - 7-day and 28-day availability percentage with 99.9% SLO line
4. **HTTP Latency SLO - Current p95** - Gauge showing current p95 latency vs 200ms target
5. **HTTP Latency SLO - Compliance %** - Percentage of requests meeting p95 <200ms
6. **HTTP Latency Distribution (Histogram)** - Request distribution across latency buckets
7. **MC Assignment SLO - Current p95** - Gauge showing current assignment p95 vs 20ms target
8. **MC Assignment SLO - Compliance %** - Percentage of assignments meeting p95 <20ms

**Metrics Used**:
- `gc_http_requests_total`
- `gc_http_request_duration_seconds_bucket`
- `gc_mc_assignment_duration_seconds_bucket`

**Default Time Range**: Last 1 hour
**Refresh**: 10 seconds

**When to Use**: Weekly SLO reviews, incident post-mortems, performance trend analysis

**Related Alerts**:
- `GCHighErrorRate` (fires when availability SLO violated)
- `GCHighLatency` (fires when latency SLO violated)
- `GCMCAssignmentSlow` (fires when assignment SLO violated)
- `GCErrorBudgetBurnRateCritical` (fires when error budget burning >10x)

---

## Authentication Controller Dashboards

### AC Overview

**Status**: 🚧 To be created
**File**: `infra/grafana/dashboards/ac-overview.json` (planned)

**Planned Panels**:
- Token issuance rate
- Token validation rate
- Token issuance latency (p95 <350ms SLO)
- Token validation latency (p95 <50ms SLO)
- Key rotation status
- JWKS cache hit rate

---

## Meeting Controller Dashboards

### MC Overview

**Status**: 🚧 To be created
**File**: `infra/grafana/dashboards/mc-overview.json` (planned)

**Planned Panels**:
- Active sessions
- Session join rate
- Session join latency (p99 <500ms SLO)
- Participant count distribution
- WebTransport connection status
- Signaling message rate

---

## Media Handler Dashboards

### MH Overview

**Status**: 🚧 To be created
**File**: `infra/grafana/dashboards/mh-overview.json` (planned)

**Planned Panels**:
- Packet forwarding rate
- Audio forwarding latency (p99 <30ms SLO)
- Video forwarding latency
- Jitter (p99 <20ms SLO)
- Packet loss rate
- Codec usage distribution

---

## Platform Dashboards

### Service Health Overview

**Status**: 🚧 To be created
**File**: `infra/grafana/dashboards/platform-overview.json` (planned)

**Purpose**: Single-pane-of-glass view of all Dark Tower services.

**Planned Panels**:
- Service status (up/down) for AC, GC, MC, MH
- Request rate across all services
- Error rate across all services
- Latency percentiles across all services
- Kubernetes pod health

---

### Database Performance

**Status**: 🚧 To be created
**File**: `infra/grafana/dashboards/database-performance.json` (planned)

**Purpose**: PostgreSQL performance monitoring across all services.

**Planned Panels**:
- Query latency by service
- Connection pool utilization
- Database CPU/memory usage
- Slow query count
- Replication lag

---

## Dashboard Standards

All dashboards must follow these standards (per ADR-0011):

### 1. PromQL Query Requirements

- ✅ Use cardinality-safe labels only (no unbounded values like UUIDs)
- ✅ Aggregate with `sum by(label)` to control cardinality
- ✅ Use rate() for counters, histogram_quantile() for histograms
- ✅ Validate queries against actual metrics (cross-reference `crates/*/src/observability/metrics.rs`)

### 2. Panel Configuration

- ✅ Add descriptive panel descriptions (what metric measures, why it matters)
- ✅ Set appropriate units (seconds, bytes, percent, requests/sec)
- ✅ Configure color thresholds (green=good, yellow=warning, red=critical)
- ✅ Include SLO threshold lines where applicable

### 3. Time Range and Refresh

- ✅ Default time range: Last 1 hour
- ✅ Auto-refresh: 10 seconds (adjustable)
- ✅ Time range selector visible

### 4. Privacy

- ❌ No PII in panel titles, queries, or annotations
- ❌ No unbounded labels (user_id, meeting_id, email, etc.)
- ✅ Use hashed or aggregated identifiers only

### 5. Legends and Tooltips

- ✅ Use meaningful legend labels (template with `{{label}}`)
- ✅ Show mean and last values in legend (calcs: ["mean", "lastNotNull"])
- ✅ Enable multi-series tooltips

---

## Dashboard Deployment

### Local Development

Dashboards are automatically loaded via Docker Compose:

```yaml
# docker-compose.yml
services:
  grafana:
    volumes:
      - ./infra/grafana/dashboards:/etc/grafana/provisioning/dashboards
```

### Kubernetes (Staging/Production)

Dashboards are loaded via **dynamic ConfigMap discovery** using `kiwigrid/k8s-sidecar`:

**How it works**:
1. The setup script dynamically discovers dashboard JSON files from `infra/grafana/dashboards/`
2. Files are grouped by service prefix (e.g., `ac-*.json` -> `grafana-dashboards-ac`)
3. Files without a `{prefix}-*` pattern go into `grafana-dashboards-common`
4. Each ConfigMap is labeled with `grafana_dashboard=1` and applied with `--server-side` (avoids the 262KB annotation limit)
5. A `kiwigrid/k8s-sidecar` init container discovers labeled ConfigMaps and writes them to a shared `emptyDir` volume at `/var/lib/grafana/dashboards`

**Adding a new dashboard**: Simply place the JSON file in `infra/grafana/dashboards/` following the `{service}-{name}.json` naming convention. Re-run `setup.sh` and the file will be automatically picked up -- no script edits required.

```bash
# Example: adding a new mc-latency.json dashboard
cp my-dashboard.json infra/grafana/dashboards/mc-latency.json
# Re-run setup.sh -- it auto-discovers the new file and adds it to grafana-dashboards-mc
```

---

## Dashboard Validation

Before deploying dashboards, validate:

1. **JSON Syntax**: Use `jq` to validate JSON
   ```bash
   jq empty infra/grafana/dashboards/gc-overview.json
   ```

2. **Datasource References**: Ensure datasource UIDs match environment
   ```bash
   grep -r "prometheus" infra/grafana/dashboards/
   ```

3. **Metric Existence**: Verify all metrics exist in codebase
   ```bash
   # Cross-reference dashboard queries against metrics.rs files
   grep "gc_http_requests_total" crates/gc-service/src/observability/metrics.rs
   ```

4. **Cardinality**: Check for unbounded labels
   ```bash
   # Search for user_id, meeting_id, etc. in dashboard queries
   grep -E "(user_id|meeting_id|participant_id)" infra/grafana/dashboards/*.json
   ```

---

## Requesting New Dashboards

To request a new dashboard:

1. Create Jira ticket with label `dashboard-request`
2. Specify:
   - Service or function to monitor
   - Key metrics to display
   - SLO thresholds (if applicable)
   - Target users (SRE, developers, leadership)
3. Tag **Observability Specialist** for review

---

## Dashboard Ownership

| Dashboard | Owner | Reviewer | Last Updated |
|-----------|-------|----------|--------------|
| GC Overview | Observability | GC Team | 2026-02-28 |
| GC SLOs | Observability | Operations | 2026-02-05 |
| AC Overview | Observability | AC Team | TBD |
| MC Overview | Observability | MC Team | TBD |
| MH Overview | Observability | MH Team | TBD |

**Update Frequency**: Review quarterly or after major service changes.

---

**Last Updated**: 2026-02-28
**Maintained By**: Observability Specialist
**Related Documents**:
- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md)
- [Metrics Catalog](./metrics/)
- [Alert Catalog](./alerts.md)
