# ADR-0011: Dark Tower Observability Framework

**Status**: Accepted
**Date**: 2025-12-07
**Deciders**: Multi-agent debate (AC, GC, MC, MH, Infrastructure, Operations, Test, Security specialists)
**Debate Rounds**: 3
**Final Consensus**: 93% average satisfaction (all specialists ≥90%)

## Context

Dark Tower requires a comprehensive observability framework to operate reliably at scale. The AC operational readiness review identified critical gaps:

- No metrics for token issuance rate, validation latency, error rates
- No `#[instrument]` attributes on handlers
- No SLO definitions
- No dashboards or runbooks
- Privacy concerns around logging PII

This ADR establishes the observability framework design reached through multi-agent debate.

## Decision

We will implement a privacy-by-default observability framework using OpenTelemetry for traces, Prometheus for metrics, and structured JSON logging with strict PII controls.

### Core Principles

1. **Privacy by Default**: Use `#[instrument(skip_all)]`, explicitly allow-list safe fields
2. **Specify, Don't Assume**: Every metric must specify required dimensions/labels
3. **SLO-Driven**: Histogram buckets aligned with SLO thresholds
4. **Local-Cloud Parity**: Same dashboards work in local dev and cloud
5. **Security First**: All logging/metrics designs reviewed for PII leakage

## Documentation Ownership

This ADR establishes **requirements and patterns**. Living documentation is maintained separately:

| Document | Owner | Location | Purpose |
|----------|-------|----------|---------|
| Service Metrics Catalog | Observability + Service specialists | `docs/observability/metrics/` | Definitive list of all metrics |
| SLO Definitions | Observability + Operations | `docs/observability/slos.md` | Current SLO targets |
| Alert Definitions | Operations | `docs/runbooks/alerts/` | Alert configs + runbooks together |
| Dashboard Specs | Observability | `infra/grafana/dashboards/` | JSON dashboard definitions |
| Span Catalog | Observability + Service specialists | `docs/observability/spans/` | Per-service span documentation |

**Observability Specialist Responsibility**: During code reviews, ensure any changes to metrics, spans, or logging update the corresponding documentation in `docs/observability/`.

## Detailed Design

### 1. Metrics Requirements

#### Naming Convention (REQUIRED)

**Format**: `<service>_<subsystem>_<metric>_<unit>`

**Standard Labels** (use consistently across all services):
- `service`: ac, gc, mc, mh
- `region`: Deployment region
- `status`: success, error, timeout, invalid
- `error_type`: Specific error classification (no PII)

**Cardinality Limits** (REQUIRED):
- Maximum unique label combinations per metric: 1,000
- Maximum label value length: 64 characters
- Total cardinality budget: 5,000,000 time series
- Use indexed values (e.g., `mc_index: 1-20`) instead of UUIDs for high-cardinality identifiers

#### Infrastructure-Provided Metrics

The following metrics are provided by Kubernetes/infrastructure and should NOT be duplicated by services:

- `container_cpu_usage_seconds_total` - CPU usage per container
- `container_memory_usage_bytes` - Memory usage per container
- `container_network_*` - Network I/O per container
- `kube_pod_*` - Pod lifecycle and status
- `up{job="..."}` - Service availability

Services should focus on **application-level metrics** that infrastructure cannot observe.

#### Example Service Metrics

> **Note**: The following are initial examples, not a comprehensive list. The definitive metrics catalog is maintained in `docs/observability/metrics/{service}.md` by service specialists.

**AC Examples** (see `docs/observability/metrics/ac-service.md` for full list):
```
ac_token_issuance_duration_seconds{grant_type,status}
ac_token_validations_total{status,error_type}
ac_key_rotation_total{status}
ac_jwks_cache_operations_total{operation,status}
```

**Cross-Service Patterns** (required for all services):
```
{service}_db_queries_total{operation,status}
{service}_db_query_duration_seconds{operation}
service_load_shedding_total{service,reason}
```

### 2. SLO Requirements

> **Note**: Specific SLO targets are maintained in `docs/observability/slos.md`. The following establishes requirements and initial targets that may be adjusted based on production data.

#### SLO Structure (REQUIRED)

Every SLO must define:
- **SLI** (Service Level Indicator): The metric being measured
- **Target**: The threshold (e.g., p99 < 350ms)
- **Error Budget**: Acceptable failure rate
- **Measurement Window**: Time period (typically 30 days)
- **Burn Rate Alerts**: When to alert based on consumption rate

#### Initial SLO Targets

| Service | Operation | SLI | Initial Target | Rationale |
|---------|-----------|-----|----------------|-----------|
| AC | Token issuance | p99 latency | < 350ms | Bcrypt ~250ms + DB ~50ms |
| AC | Token validation | p99 latency | < 50ms | Signature verification only |
| GC | Request (regional) | p95 latency | < 200ms | DB query + routing |
| MC | Session join | p99 latency | < 500ms | WebTransport handshake ~200ms |
| MH | Audio forwarding | p99 latency | < 30ms | Real-time constraint |
| MH | Audio jitter | p99 | < 20ms | Human perception threshold |

#### Error Budget Burn Rate Alerts (REQUIRED)

- **Critical**: >10x burn rate for 1h (budget exhausted in <3 days)
- **Warning**: >5x burn rate for 6h (budget exhausted in <6 days)

### 3. Privacy-by-Default Logging

#### Requirements (REQUIRED)

1. **Default instrumentation**: `#[instrument(skip_all)]` - opt-in to fields, not opt-out
2. **PII Review**: Security specialist must review any field added to logging/tracing
3. **Logging Tiers**: Different verbosity levels have different PII rules

#### Logging Tier Policy (REQUIRED)

- **DEBUG**: Full payloads allowed (DEV ONLY, never in staging/prod)
- **INFO**: SAFE fields only, no user-identifiable information
- **WARN**: Error classification, hashed identifiers only
- **ERROR**: Sanitized error messages, correlation IDs, no stack traces with user input

#### Field Classification

**SAFE Fields** - Always logged in plaintext (any environment):
- System identifiers: `service`, `region`, `environment`
- Correlation IDs: `trace_id`, `span_id`, `request_id`
- Operation metadata: `method`, `status_code`, `error_type`, `operation`
- Timing: `duration_ms`, `timestamp`
- Enums/bounded values: `grant_type`, `codec`, `media_type`

**UNSAFE Fields** - Require visibility level selection:

> **Note**: This is not a comprehensive list. Any field that could identify a user, session, or contain secrets is UNSAFE. When in doubt, mask it.

- Credentials: `password`, `secret`, `api_key`, `bearer_token`, `private_key`
- Tokens: `jwt` (full token), `session_cookie`, `refresh_token`
- PII: `email`, `phone_number`, `display_name`, `full_name`
- Identifiers that may contain PII: `participant_id`, `user_id` (if derived from email)
- Network: `ip_address`, `user_agent` (fingerprinting risk)
- Location: `geolocation` (exact coordinates)
- Session data: `meeting_id`, `request_body`, `error_message`

#### Three-Level Visibility Model for UNSAFE Fields (REQUIRED)

To balance privacy, debuggability, and performance, UNSAFE fields use a three-level model:

| Level | Output | Performance | Use Case |
|-------|--------|-------------|----------|
| **Masked** (default) | `****` | Zero overhead | Default for all UNSAFE fields |
| **Hashed** | `h:a1b2c3d4` | ~1μs per hash | When correlation across logs is needed |
| **Plaintext** | Full value | Zero overhead | DEBUG level only, dev environments |

**Level Selection Guidelines**:

1. **Masked (default)**: Use when you just need to know a field was present but don't need to correlate across log entries. Most UNSAFE fields should use this.
   - Examples: `ip_address`, `user_agent`, `email`, `password`

2. **Hashed**: Explicitly enable when you need to correlate the same value across multiple log entries (e.g., "find all logs for this meeting"). Use sparingly due to CPU cost.
   - Examples: `meeting_id` (correlate meeting events), `client_id` (correlate client requests)

3. **Plaintext**: Only in DEBUG level, only in dev environments. Never in staging/prod.

**Implementation Pattern**:
```rust
enum FieldVisibility {
    Masked,      // "****" - zero overhead
    Hashed,      // "h:a1b2c3d4" - enables correlation
    Plaintext,   // Full value (DEBUG only)
}

// Usage in instrumentation
#[instrument(skip_all, fields(
    meeting_id = display_field(&meeting_id, FieldVisibility::Hashed),  // Need correlation
    ip_address = display_field(&ip, FieldVisibility::Masked),          // Default
))]
```

#### Hash Generation (when hashing is enabled)

```rust
// HMAC-SHA256 with per-service key, truncated to 64 bits
fn hash_field(value: &str, hmac_key: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(hmac_key).unwrap();
    mac.update(value.as_bytes());
    format!("h:{}", hex::encode(&mac.finalize().into_bytes()[..8]))
}
```

- Salt source: Kubernetes Secret `observability-hmac-key-{service}`
- Rotation: Every 30 days
- Prefix `h:` distinguishes hashed values from masked `****`

### 4. Span Requirements

#### Span Naming Convention (REQUIRED)

**Format**: `{service}.{subsystem}.{operation}`

Examples: `ac.token.issue`, `gc.http.request`, `mc.session.join`, `mh.packet.forward`

#### Required Span Attributes

All spans must include:
- `service.name`: Service identifier
- `service.version`: From Cargo.toml
- `deployment.environment`: dev, staging, prod

#### Span Attribute Cardinality Limits (REQUIRED)

- `participant_id`: 200 max (use hash)
- `stream_id`: 1000 max
- `meeting_id`: 10000 max (use prefix)

#### Example Span Hierarchies

> **Note**: These are examples demonstrating the pattern. Full span catalogs are maintained in `docs/observability/spans/{service}.md`.

**AC Example**:
```
ac.token.issue
├─ ac.client.validate
├─ ac.db.query.client
├─ ac.token.sign
└─ ac.db.insert.token
```

**GC Example**:
```
gc.http.request
├─ gc.auth.verify
├─ gc.route.resolve
└─ gc.db.query
```

**MC Example**:
```
mc.session.join
├─ mc.participant.validate
├─ mc.state.transition
├─ mc.layout.compute
│   ├─ mc.layout.detect_speakers
│   └─ mc.layout.notify_clients
└─ mc.signaling.broadcast
```

**MH Example**:
```
mh.packet.receive
├─ mh.packet.parse
├─ mh.quality.assess
├─ mh.congestion.update
└─ mh.packet.forward
```

### 5. Alert Requirements

> **Note**: Alert definitions and runbooks are maintained together in `docs/runbooks/alerts/`. Each alert file contains the alert configuration and its runbook.

#### Alert Structure (REQUIRED)

Every alert must include:
- **Alert name**: Following pattern `{SERVICE}-{NUMBER}` (e.g., AC-001)
- **Severity**: Critical, Warning, or Info
- **Condition**: PromQL expression
- **Runbook**: Investigation steps and remediation actions (in same file)

#### Alert Routing Matrix (REQUIRED)

| Severity | Channel | Escalation |
|----------|---------|------------|
| Critical | PagerDuty immediate, Slack #incidents | 15min → on-call lead |
| Warning | Slack #alerts | 1h → service owner |
| Info | Slack #observability | None |

#### Alert Fatigue Prevention (REQUIRED)

- Deduplication window: 5 minutes
- Severity bumping: Warning → Critical if firing > 30 minutes
- Volume limit: Max 20 alerts/hour, then suppress and page SRE lead

#### Initial Alerts

> **Note**: This is the initial set. Operations specialist maintains the full list in `docs/runbooks/alerts/`.

- `AC-001`: Token issuance latency SLO breach
- `AC-002`: Token validation error rate high
- `AC-003`: Key rotation failed
- `SYS-001`: Error budget burn rate critical
- `SYS-002`: Load shedding active
- `SEC-001`: High rate limit denials

### 6. Dashboard Requirements

> **Note**: Dashboard JSON files are maintained in `infra/grafana/dashboards/`. The following establishes requirements, not the definitive list.

#### Required Dashboards

Each service must have:
1. **Service Overview Dashboard**: Request rate, error rate, latency percentiles
2. **SLO Dashboard**: Error budget remaining, burn rate trend

Platform-wide dashboards:
1. **Service Health Overview**: All services at a glance
2. **Database Performance**: Query latency, connection pools
3. **Security Monitoring**: Rate limiting, auth failures

#### Deployment Strategy (REQUIRED)

- **Local**: Docker Compose with Grafana, volume-mounted JSON dashboards
- **Cloud**: Kubernetes ConfigMaps with Grafana sidecar auto-discovery
- **Parity**: Same JSON files used in both environments
- **CI**: Validate JSON schema and datasource references

### 7. OpenTelemetry Collector Requirements

Deploy as Kubernetes DaemonSet with:
- **Receivers**: OTLP (gRPC/HTTP), Prometheus scrape, host metrics
- **Processors**: Batch (10s, 1024 items), tail sampling, PII attribute filtering
- **Exporters**: Prometheus remote write, Jaeger, Loki
- **Sampling**: 100% errors, 5% success (tail-based)

Configuration maintained in `infra/otel-collector/config.yaml`.

### 8. Security Controls

#### Inference Attack Mitigations (REQUIRED)

- Hash meeting_id/participant_id with HMAC (prevent enumeration)
- Aggregate participant counts into buckets (1-5, 6-10, etc.)
- Rate limit span queries (10/min per user)

#### Span Validation Middleware (REQUIRED)

- PII detection via regex (email, SSN, credit card patterns)
- Cardinality enforcement (drop attributes exceeding limits)
- Sensitive attribute filtering (password, api_key, secret)

#### Dashboard Access Control (REQUIRED)

- Security dashboards: Security + SRE teams only
- Service dashboards: Service team + SRE (read-only)
- Prometheus API: Role-based query filtering, rate limits

### 9. Testing Requirements

| Test Type | Coverage | CI Requirement |
|-----------|----------|----------------|
| Metrics unit tests | All metrics recorded | Mandatory |
| PII leakage tests | All log paths | Mandatory |
| Trace propagation | All service boundaries | Mandatory |
| Dashboard availability | All dashboards load | Mandatory |
| Chaos metrics verification | Critical paths | Nightly |

## Consequences

### Positive

- Comprehensive visibility into all service operations
- SLO-driven alerting reduces false positives
- Privacy-by-default prevents PII leakage
- Local-cloud parity enables development debugging
- Cardinality limits prevent Prometheus DoS

### Negative

- Implementation overhead for all services
- HMAC hashing adds ~1μs per operation
- Tail sampling may miss rare issues (5% success rate)
- Dashboard maintenance as services evolve

### Risks

- Hash salt compromise enables PII correlation (mitigated by rotation)
- Alert fatigue if thresholds misconfigured (mitigated by volume limits)
- Cardinality limits may drop legitimate metrics (mitigated by monitoring)

## Implementation Plan

1. **Phase 1**: Create documentation structure (`docs/observability/`)
2. **Phase 2**: Implement OTel collector and basic metrics (AC first)
3. **Phase 3**: Add service-specific metrics, update metric catalog
4. **Phase 4**: Create dashboards and alerts with runbooks
5. **Phase 5**: Implement span validation middleware

## References

- [Multi-agent debate transcript](../debates/2025-12-07-observability-framework.md)
- [AC Operational Readiness Review](../reviews/2025-12-07-ac-service-operational-readiness.md)
- [OpenTelemetry Specification](https://opentelemetry.io/docs/specs/)
- [Prometheus Naming Conventions](https://prometheus.io/docs/practices/naming/)
