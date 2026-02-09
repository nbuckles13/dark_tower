# Operational Runbooks Index

This document indexes all operational runbooks for Dark Tower services.

## Runbook Purpose

Runbooks are step-by-step guides for diagnosing and resolving incidents. Each service has two comprehensive mega-runbooks following the AC service pattern (per ADR-0011):

1. **Deployment Runbook**: Pre-deployment checklist, deployment steps, rollback procedure, configuration reference, common deployment issues, smoke tests, monitoring
2. **Incident Response Runbook**: Severity classification, escalation paths, common failure scenarios (7+ scenarios), diagnostic commands, recovery procedures, postmortem template

All runbooks are stored in `docs/runbooks/`.

---

## Runbook Organization

Following ADR-0011 pattern (established by AC service reference implementation):

| Service | Deployment Runbook | Incident Response Runbook |
|---------|-------------------|---------------------------|
| AC (Auth Controller) | `ac-service-deployment.md` | `ac-service-incident-response.md` |
| GC (Global Controller) | `gc-deployment.md` | `gc-incident-response.md` |
| MC (Meeting Controller) | `mc-deployment.md` (planned) | `mc-incident-response.md` (planned) |
| MH (Media Handler) | `mh-deployment.md` (planned) | `mh-incident-response.md` (planned) |

Prometheus alerts link directly to specific sections within the incident response runbook using anchor links (e.g., `gc-incident-response.md#scenario-1-database-connection-failures`).

---

## Global Controller Runbooks

### GC Deployment Runbook

**File**: [gc-deployment.md](../runbooks/gc-deployment.md)
**Purpose**: Complete deployment, rollback, and troubleshooting procedures

**Sections**:
1. **Pre-Deployment Checklist** - Code quality, infrastructure, coordination requirements
2. **Deployment Steps** - 9-step deployment process with verification at each stage
3. **Rollback Procedure** - When and how to rollback, database considerations
4. **Configuration Reference** - Environment variables, Kubernetes secrets/ConfigMaps, resource limits
5. **Common Deployment Issues** - 5 scenarios (DB connection, JWKS fetch, TokenManager, pod startup, MC assignment)
6. **Smoke Tests** - 5 tests (health, readiness, metrics, authenticated endpoint, meeting join)
7. **Monitoring and Verification** - Key metrics, dashboards, alerting rules

**Last Updated**: 2026-02-05

---

### GC Incident Response Runbook

**File**: [gc-incident-response.md](../runbooks/gc-incident-response.md)
**Purpose**: Comprehensive incident response for all GC failure scenarios

**Sections**:
1. **Severity Classification** - P1-P4 definitions with response times and examples
2. **Escalation Paths** - Initial response, escalation chain, specialist contacts
3. **Common Failure Scenarios** (7 scenarios):
   - [Scenario 1: Database Connection Failures](#gc-scenario-1)
   - [Scenario 2: High Latency / Slow Responses](#gc-scenario-2)
   - [Scenario 3: MC Assignment Failures](#gc-scenario-3)
   - [Scenario 4: Complete Service Outage](#gc-scenario-4)
   - [Scenario 5: High Error Rate](#gc-scenario-5)
   - [Scenario 6: Resource Pressure](#gc-scenario-6)
   - [Scenario 7: Token Refresh Failures](#gc-scenario-7)
4. **Diagnostic Commands** - Quick health check, metrics analysis, database queries, log analysis, resource utilization, network debugging
5. **Recovery Procedures** - Service restart, database failover, load shedding
6. **Postmortem Template** - Complete template for P1/P2 incidents
7. **Maintenance and Updates** - Ownership, review schedule, change process

**Alert Mapping**:
| Alert | Scenario |
|-------|----------|
| `GCDown` | Scenario 4: Complete Service Outage |
| `GCHighErrorRate` | Scenario 5: High Error Rate |
| `GCHighLatency` | Scenario 2: High Latency / Slow Responses |
| `GCMCAssignmentSlow` | Scenario 3: MC Assignment Failures |
| `GCMCAssignmentFailures` | Scenario 3: MC Assignment Failures |
| `GCDatabaseDown` | Scenario 1: Database Connection Failures |
| `GCDatabaseSlow` | Scenario 1: Database Connection Failures |
| `GCHighMemory` | Scenario 6: Resource Pressure |
| `GCHighCPU` | Scenario 6: Resource Pressure |
| `GCTokenRefreshFailures` | Scenario 7: Token Refresh Failures |
| `GCErrorBudgetBurnRateCritical` | Scenario 5: High Error Rate |
| `GCErrorBudgetBurnRateWarning` | Scenario 5: High Error Rate |
| `GCPodRestartingFrequently` | Scenario 4: Complete Service Outage |

**Last Updated**: 2026-02-05

---

## Authentication Controller Runbooks

### AC Deployment Runbook

**File**: [ac-service-deployment.md](../runbooks/ac-service-deployment.md)
**Purpose**: Complete deployment, rollback, and troubleshooting procedures
**Status**: Production-ready (reference implementation)

**Sections**:
1. Pre-Deployment Checklist
2. Deployment Steps
3. Rollback Procedure
4. Configuration Reference
5. Common Deployment Issues (5 scenarios)
6. Smoke Tests (5 tests)
7. Monitoring and Verification

**Last Updated**: 2025-12-10

---

### AC Incident Response Runbook

**File**: [ac-service-incident-response.md](../runbooks/ac-service-incident-response.md)
**Purpose**: Comprehensive incident response for all AC failure scenarios
**Status**: Production-ready (reference implementation)

**Sections**:
1. Severity Classification
2. Escalation Paths
3. Common Failure Scenarios (7 scenarios)
4. Diagnostic Commands
5. Recovery Procedures
6. Postmortem Template
7. Maintenance and Updates

**Last Updated**: 2025-12-10

---

## Meeting Controller Runbooks

**Status**: Planned

**Planned Runbooks**:
- `mc-deployment.md` - Deployment runbook following AC/GC pattern
- `mc-incident-response.md` - Incident response with WebTransport-specific scenarios

---

## Media Handler Runbooks

**Status**: Planned

**Planned Runbooks**:
- `mh-deployment.md` - Deployment runbook following AC/GC pattern
- `mh-incident-response.md` - Incident response with real-time media-specific scenarios

---

## Runbook Standards

All runbooks follow the ADR-0011 two-runbook pattern:

### Deployment Runbook Structure

1. **Overview** - Service description, criticality statement
2. **Pre-Deployment Checklist** - Code quality, infrastructure, coordination
3. **Deployment Steps** - Numbered steps with verification at each stage
4. **Rollback Procedure** - When/how to rollback, database considerations
5. **Configuration Reference** - Environment variables, secrets, ConfigMaps, resource limits
6. **Common Deployment Issues** - At least 5 scenarios with symptoms, causes, resolution
7. **Smoke Tests** - At least 5 tests with expected results
8. **Monitoring and Verification** - Key metrics, dashboards, alerting rules

### Incident Response Runbook Structure

1. **Header** - Service, owner, on-call rotation, last updated, table of contents
2. **Severity Classification** - P1-P4 matrix with response times, examples, escalation
3. **Escalation Paths** - Initial response, escalation chain, specialist contacts, external dependencies
4. **Common Failure Scenarios** - At least 7 scenarios, each with:
   - Symptoms
   - Diagnosis (specific commands with expected output)
   - Common Root Causes (numbered list)
   - Remediation (multiple options with expected recovery time)
   - Escalation (when and to whom)
5. **Diagnostic Commands** - Quick health check, metrics analysis, database queries, log analysis, resource utilization, network debugging
6. **Recovery Procedures** - Service restart, database failover, emergency procedures
7. **Postmortem Template** - Complete template for P1/P2 incidents
8. **Maintenance and Updates** - Ownership, review schedule, change process, version history
9. **Additional Resources** - Links to ADRs, metrics catalog, SLO definitions, architecture docs

---

## Runbook Maintenance

### Review Frequency

- **After Incidents**: Update runbook within 24 hours based on learnings
- **Monthly**: Review during on-call handoff
- **Quarterly**: Comprehensive review by service owner and observability specialist

### Version Control

All runbooks are version-controlled in Git:
- Changes tracked in commit history
- Use PR review process for updates
- Include "Last Updated" date in each runbook

### Ownership

| Service | Runbook Owner | Backup Owner | Review Frequency |
|---------|---------------|--------------|------------------|
| GC | GC Team | Observability | Quarterly |
| AC | AC Team | Observability | Quarterly |
| MC | MC Team | Observability | Quarterly |
| MH | MH Team | Observability | Quarterly |

---

**Last Updated**: 2026-02-05
**Maintained By**: Observability Specialist + Service Teams
**Related Documents**:
- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md)
- [Alert Catalog](./alerts.md)
- [Dashboard Catalog](./dashboards.md)
