# Devloop Output: MC join dashboard panels + alert rules

**Date**: 2026-03-28
**Task**: Add MC join dashboard panels + alert rules + update metrics catalog
**Specialist**: observability
**Mode**: full
**Branch**: `feature/meeting-join-user-story-devloop-task7`
**Duration**: ~45 minutes

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `bce73a15d39974fccedb4ed3702fe25956be241f` |
| Branch | `feature/meeting-join-user-story-devloop-task7` |
| End Commit | `8b94604` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementing Specialist | `observability` |
| Iteration | `1` |

---

## Task Overview

### Objective
Add MC join flow alert rules + update metrics catalog and docs. Dashboard panels already exist from task 11.

### Scope
- **Service(s)**: mc-service (alerts, docs only — no code changes)
- **Schema**: No
- **Cross-cutting**: No

### Requirements Covered
- R-15: MC join dashboard panels in mc-overview.json (already done in task 11) + alert rules for WebTransport failures, token validation failures, session join failures

---

## Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

---

## Planning

Implementer proposed 4 new alert rules following GC task 12 pattern:
1. MCHighJoinFailureRate (warning, >5% for 5m) — mc_session_joins_total
2. MCHighJoinLatency (info, p95 >2s for 5m, filter status="success") — mc_session_join_duration_seconds
3. MCHighWebTransportRejections (warning, >10% for 5m) — mc_webtransport_connections_total
4. MCHighJwtValidationFailures (warning, >10% for 5m) — mc_jwt_validations_total

All with div-by-zero guards, runbook_url to mc-incident-response.md scenarios 8-10.

Observability reviewer identified 2 critical pre-existing bugs: `mc_message_processing_duration_seconds` (wrong, should be `mc_message_latency_seconds`) and `mc_gc_heartbeat_total` (wrong, should be `mc_gc_heartbeats_total`) — 5 dead alerts including 2 critical-severity. Mandatory fix in this task.

Test reviewer independently confirmed the same bugs.

---

## Implementation Summary

### Bug Fixes (pre-existing)
- Fixed `mc_message_processing_duration_seconds` → `mc_message_latency_seconds` in MCHighLatency, MCHighMessageDropRate, MCMeetingStale alerts (3 occurrences)
- Fixed `mc_gc_heartbeat_total` → `mc_gc_heartbeats_total` in MCGCHeartbeatFailure, MCGCHeartbeatWarning (4 occurrences)
- Fixed `mc_gc_heartbeat_total` → `mc_gc_heartbeats_total` in mc-deployment.md (1 occurrence)
- Fixed 3 additional stale `mc_message_processing_duration_seconds` in mc-deployment.md (test reviewer finding)

### New Alert Rules
- 3 warnings in mc-service-warning group + 1 info in new mc-service-info group
- Header comment updated with info severity level

### Documentation
- mc-service.md: alert/dashboard cross-references
- alerts.md: MC join alert documentation with response steps
- dashboards.md: MC overview panel listing with Related Alerts section

### Files Changed
15 files changed (incl. INDEX updates), +387/-58 lines.

---

## Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | No PII in annotations, bounded labels |
| Test | CLEAR | 1 | 1 | 0 | Stale metric names in mc-deployment.md |
| Observability | CLEAR | 0 | 0 | 0 | Bug fixes revived 5 dead alerts |
| Code Quality | CLEAR | 1 obs | 0 | 0 | Info alert `for` duration matches GC |
| DRY | CLEAR | 0 | 0 | 0 | No duplication |
| Operations | CLEAR | 1 | 1 | 0 | mc-deployment.md stale names fixed |
