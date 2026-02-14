# Operations Specialist Checkpoint: Complete Grafana Dashboard Infrastructure

**Date**: 2026-02-10
**Agent**: Operations Specialist
**Task**: Complete Grafana dashboard infrastructure with AC updates, log dashboards, error dashboard, and SLO dashboards

---

## Patterns Discovered

### 1. Dashboard Naming Convention
- Overview dashboards: `{service}-overview.json` (e.g., `ac-overview.json`)
- Log dashboards: `{service}-logs.json` (e.g., `ac-logs.json`)
- SLO dashboards: `{service}-slos.json` (e.g., `ac-slos.json`)
- Cross-service dashboards: `{domain}-overview.json` (e.g., `errors-overview.json`)

### 2. Infrastructure Panel Pattern
All service overview dashboards should have infrastructure panels at the bottom:
- Memory Usage (timeseries) - `container_memory_working_set_bytes{job="service-name"}`
- CPU Usage (timeseries) - `rate(container_cpu_usage_seconds_total{job="service-name"}[5m])`
- Pod Count (gauge) - `count(up{job="service-name"} == 1)`

Grid positioning:
- Infrastructure row typically starts after last business metric panel
- Each panel: width=8, height=6
- Panels positioned horizontally: x=0,8,16

### 3. Log Dashboard Structure
Consistent structure across all services:
1. Log Volume Over Time (stacked bar chart) - Shows distribution by log level
2. Recent Logs (logs panel) - Filterable by pod and log level using variables
3. Error Logs (logs panel) - `{job="service"} | level="error"`
4. Warning Logs (logs panel) - `{job="service"} | level="warn"`

Variables:
- Pod selector: Label values query from Loki
- Log level filter: Custom variable (all, error, warn, info, debug, trace)

### 4. SLO Dashboard Pattern (from gc-slos.json)
Consistent SLO tracking across services:
1. Error Budget Remaining (gauge) - 30-day window calculation
2. Error Budget Burn Rate (timeseries) - 1h and 6h burn rates with thresholds
3. Availability Trend (timeseries) - 7-day and 28-day windows
4. Service-specific latency SLOs (gauge + compliance %)
5. Error Rate SLO (gauge)
6. Availability/Uptime SLO (gauge)

Thresholds:
- Error budget: Red <5%, Yellow 5-20%, Green >20%
- Burn rate: Sustainable=1.0, Critical=5.0
- SLO compliance: Red <95%, Yellow 95-99%, Green >99%

### 5. Cross-Service Error Dashboard
Aggregates error metrics across all services using service-specific queries:
- AC: Error rate based on token issuance/JWKS failures
- GC: HTTP 4xx/5xx status codes
- MC: Message drop rate

Table panel transformation:
- Use `organize` transformation to rename fields
- Exclude `Time` column for cleaner presentation

### 6. Datasource UID References
Dashboards must reference datasource UIDs from `infra/grafana/provisioning/datasources/datasources.yaml`:
- Prometheus: `prometheus`
- Loki: `loki`

Never use variable placeholders like `${DS_LOKI}` - use direct UID values.

---

## Gotchas Encountered

### 1. Datasource UID Placeholder Issue
**Problem**: Initially used `${DS_LOKI}` placeholder in log dashboards, causing guard validation failure.
**Root Cause**: The `grafana-datasources` guard validates that all UIDs match configured datasources. Variable placeholders are not supported.
**Fix**: Replace all `${DS_LOKI}` with direct UID `loki` from datasources.yaml.
**Lesson**: Always reference datasource UIDs directly from the configuration file, not placeholders.

### 2. Panel ID Uniqueness
**Problem**: Panel IDs must be unique within each dashboard.
**Solution**: When adding new panels to existing dashboards (like AC infrastructure panels), increment IDs from the last used ID.
**AC Dashboard**: Last ID was 17, new infrastructure panels got IDs 18-21.

### 3. Grid Position Y-Coordinate Calculation
**Problem**: Panels must not overlap. Y-coordinate calculation must account for previous panel heights.
**AC Dashboard Example**:
- Last panel ended at y=43 with height=8
- New infrastructure row starts at y=51 (43 + 8)
- Infrastructure panels at y=52 (51 + 1 for row header)

### 4. Service-Specific Metric Naming
**Problem**: Each service has different metric names and error semantics:
- AC: `ac_errors_total`, token issuance success/failure
- GC: `gc_http_requests_total`, status code-based errors
- MC: `mc_messages_dropped_total`, message drop rate

**Solution**: Use service-specific queries in cross-service dashboards rather than trying to unify metric names.

### 5. Loki Query Syntax
**LogQL Filter vs. Parser**:
- Use `|=` for simple string matching in log lines
- Use `| level="error"` for structured label filtering
- `|~ "pattern"` for regex matching with variables

### 6. PromQL Division by Zero
**Problem**: Error rate calculations can divide by zero when there are no requests.
**Pattern Used**:
```promql
sum(rate(errors[5m])) / (sum(rate(errors[5m])) + sum(rate(requests[5m])))
```
This avoids division by zero since denominator includes error count.

---

## Key Decisions

### 1. Dashboard Organization
**Decision**: Group dashboards by service and concern (overview, logs, SLOs).
**Rationale**: Makes it easy to find relevant dashboards. Operators can quickly navigate from overview to logs to SLOs for a specific service.

### 2. Log Dashboard Separation
**Decision**: Create separate log dashboards for each service rather than one combined dashboard.
**Rationale**:
- Different services may have different log volume characteristics
- Easier to add service-specific log queries later
- Follows the pattern established for overview and SLO dashboards

### 3. Cross-Service Error Dashboard
**Decision**: Create a single error dashboard aggregating all services.
**Rationale**:
- Provides holistic view of platform health
- Easier to spot cross-service error patterns
- Links to Recent Error Logs panel for quick troubleshooting

### 4. Privacy-by-Default in Queries
**Decision**: No unbounded identifiers in any query (meeting_id, participant_id, session_id, user_id).
**Rationale**:
- Follows ADR-0011 cardinality safety principles
- Prevents metric explosion
- Protects user privacy in observability data

### 5. SLO Alignment Across Services
**Decision**: Use consistent SLO thresholds and panel structure across AC/MC SLO dashboards.
**Rationale**:
- Makes it easier to compare SLO compliance across services
- Operators become familiar with standard layout
- Service-specific SLOs (like "GC Heartbeat SLO" for MC) are added as additional panels

---

## Current Status

### Completed
- ✅ Renamed `ac-service.json` to `ac-overview.json`
- ✅ Added infrastructure panels to AC dashboard (Memory, CPU, Pod Count)
- ✅ Updated AC dashboard title and UID to match naming convention
- ✅ Created `ac-logs.json` with Loki log queries and variables
- ✅ Created `gc-logs.json` with Loki log queries and variables
- ✅ Created `mc-logs.json` with Loki log queries and variables
- ✅ Created `errors-overview.json` with cross-service error aggregation
- ✅ Created `ac-slos.json` following GC pattern
- ✅ Created `mc-slos.json` with MC-specific SLOs (heartbeat)
- ✅ Fixed datasource UID references (loki instead of ${DS_LOKI})
- ✅ All 7 verification layers passed

### Files Created
1. `/home/nathan/code/dark_tower/infra/grafana/dashboards/ac-logs.json`
2. `/home/nathan/code/dark_tower/infra/grafana/dashboards/gc-logs.json`
3. `/home/nathan/code/dark_tower/infra/grafana/dashboards/mc-logs.json`
4. `/home/nathan/code/dark_tower/infra/grafana/dashboards/errors-overview.json`
5. `/home/nathan/code/dark_tower/infra/grafana/dashboards/ac-slos.json`
6. `/home/nathan/code/dark_tower/infra/grafana/dashboards/mc-slos.json`

### Files Modified
1. `/home/nathan/code/dark_tower/infra/grafana/dashboards/ac-overview.json` (renamed from ac-service.json, infrastructure panels added)

### Verification Results
- Layer 1 (cargo check): ✅ PASSED
- Layer 2 (cargo fmt): ✅ PASSED
- Layer 3 (guards): ✅ PASSED (9/9 guards)
- Layer 4 (unit tests): ✅ PASSED (153 tests)
- Layer 5 (all tests): ✅ PASSED
- Layer 6 (clippy): ✅ PASSED
- Layer 7 (semantic guards): ✅ PASSED (10/10 guards)

---

## Next Steps

None - implementation complete. All dashboards created and validated.
