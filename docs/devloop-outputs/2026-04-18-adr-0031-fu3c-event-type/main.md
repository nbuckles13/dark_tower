# Devloop Output: ADR-0031 FU#3c — MC + MH `event` → `event_type`

**Date**: 2026-04-18
**Task**: Rename `event` → `event_type` on `mc_mh_notifications_received_total` (MC) and `mh_mc_notifications_total` (MH). Aligns with AC's canonical `event_type` naming. Last of the ADR-0031 label-canonicalization follow-ups.
**Specialist**: meeting-controller (implementer; cross-boundary edit into MH source is mechanical and MH specialist reviews)
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `111ef265af551624f63533daac8cb5f5ee3386d6` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-fu3c-event-type` |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `APPROVE (security@devloop-fu3c-event-type) — 0 findings` |
| Media Handler | `APPROVE (media-handler@devloop-fu3c-event-type) — 0 findings, 1 non-blocking nit on frozen user-story doc` |

---

## Task Overview

### Objective

Close ADR-0031 label-canonicalization FU#3c. MC and MH both emit a bare `event` label on their notification metrics (`mc_mh_notifications_received_total` and `mh_mc_notifications_total`). AC already uses the canonical `event_type` on `ac_audit_log_failures_total`. Bring MC + MH into line.

### Scope

Surveyed up-front — scope is bounded:

1. **`crates/mc-service/src/observability/metrics.rs:398`** — 1 label invocation on `mc_mh_notifications_received_total`.
2. **`crates/mh-service/src/observability/metrics.rs:182`** — 1 label invocation on `mh_mc_notifications_total`.
3. **`infra/grafana/dashboards/mc-overview.json:3968-3969`** — 1 PromQL `sum by(event)` + 1 `{{event}}` legend on the MC notifications panel.
4. **`infra/grafana/dashboards/mh-overview.json:1794-1795`** — 1 PromQL `sum by(event, status)` + 1 `{{event}} / {{status}}` legend on the MH notifications panel.
5. **`docs/observability/metrics/mc-service.md`** — 2 refs (label list + cardinality-budget table row).
6. **`docs/observability/metrics/mh-service.md`** — 1 ref (label list).
7. **`docs/observability/metrics/mc.md`** (legacy) — verify no notification-metric refs (survey says clean; worth confirming).
8. **`docs/observability/label-taxonomy.md`** — update §Current Drift to mark FU#3c resolved, removing the last entry in that subsection.
9. **`TODO.md`** — remove the FU#3c entry; this closes out the entire `## ADR-0031 label-canonicalization follow-ups` section (all three sub-entries now complete).

No alert files touched (MC and MH alerts don't filter on `event`).
No cross-service dashboards touched (errors-overview.json doesn't reference these metrics).

### Posture

Cross-boundary mechanical rename. MC specialist touches MH's `metrics.rs` + `mh-overview.json` + `mh-service.md`. MH specialist review is the ownership safeguard.

### Debate Decision

NOT NEEDED — canonical-name target documented; scope is a mechanical rename mirrored across two services.

---

## Reference

- TODO.md "ADR-0031 label-canonicalization follow-ups → MC + MH `event` / AC `event_type` → canonical `event_type`"
- Canonical-name source: `docs/observability/label-taxonomy.md` §Shared Label Names
- AC precedent (already canonical): `docs/observability/metrics/ac-service.md` entry for `ac_audit_log_failures_total`
- Prior FU#3b pattern (bare-label rename in MC, with filter discipline + legacy-catalog sweep): commit `111ef26`
- Dual-MC-catalog follow-up (separate TODO entry from FU#3b): `docs/observability/metrics/mc.md` stays authoritative-duplicate pending that separate cleanup.

---

## Implementation Summary

Closes ADR-0031 label-canonicalization FU#3c — last of three renames. MC and MH now emit canonical `event_type` on their notification metrics, matching AC's existing shape on `ac_audit_log_failures_total`. Also closes out the entire `## ADR-0031 label-canonicalization follow-ups` section in TODO.md (all three sub-entries done) and the `§Current Drift` subsection in label-taxonomy.md.

### Cross-service discipline
MC specialist touched MH's source. Per our "mechanical is mechanical" posture, this is a supported pattern for trivial renames. MH specialist cross-checked the MH hunks (metrics.rs, mh-overview.json, mh-service.md, mh-incident-response.md) and confirmed pure rename with semantics preserved.

### Changes

1. **`crates/mc-service/src/observability/metrics.rs:398`** — 1 label key rename on `mc_mh_notifications_received_total`.
2. **`crates/mh-service/src/observability/metrics.rs:179-186`** — 1 label key rename on `mh_mc_notifications_total` + local variable `event` → `event_type` for internal consistency.
3. **`infra/grafana/dashboards/mc-overview.json:3968-3969`** — PromQL `sum by(event)` → `by(event_type)`, legend `{{event}}` → `{{event_type}}`.
4. **`infra/grafana/dashboards/mh-overview.json:1794-1795`** — PromQL `sum by(event, status)` → `by(event_type, status)`, legend `{{event}} / {{status}}` → `{{event_type}} / {{status}}`.
5. **`docs/observability/metrics/mc-service.md`** — 3 refs: label list, PromQL example on line 345, cardinality-budget table row.
6. **`docs/observability/metrics/mh-service.md`** — 1 ref (label list).
7. **`docs/runbooks/mh-incident-response.md:588`** — 1 runbook PromQL sample (surveyed post-spawn; implementer caught this beyond the original enumerated scope).
8. **`docs/observability/label-taxonomy.md`** — removed §Current Drift subsection entirely (all three renames now done). Rewrote line-61–64 cross-reference prose that pointed to the deleted subsection (was pointing to both §Current Drift AND to the deleted TODO section header; replaced with reviewer-only-rationale plus a historical-note line referencing FU#3a/b/c closure).
9. **`TODO.md`** — removed the entire `## ADR-0031 label-canonicalization follow-ups` section (all three sub-entries complete).
10. **`docs/observability/metrics/mc.md`** (legacy catalog) — zero refs to notification metrics. Confirmed via grep; no changes.

### Iteration 2: stale drift cleanup
First-pass implementation left a stale `§Current Drift` entry referencing the AC path / GC status drift that FU#3a and FU#3a-status had already closed. Lead caught it during Gate 2 pre-review; implementer removed the entry plus rewrote the cross-reference on line 61–64 that pointed to both the deleted subsection and the deleted TODO section. Ripple-clean verification via grep confirmed zero other refs to `§Current Drift` or the deleted TODO header outside devloop-output archives (historical).

### Iteration lesson
FU#3a-status's commit had left this stale drift entry; FU#3c's first-pass implementer read it as "still outstanding" rather than "left over." Same class of iterative-review failure as FU#3b's 4-pass catalog sweep. **When renaming closes a follow-up, scan for all meta-references to the renamed item — not just the data itself.** Worth a note when the skill debate runs (goes alongside the other "mechanical cross-ownership" lessons).

---

## Files Modified

| File | Lines changed |
|------|-------|
| `crates/mc-service/src/observability/metrics.rs` | 1 label rename + docstring |
| `crates/mh-service/src/observability/metrics.rs` | 1 label rename + local var rename + docstring |
| `infra/grafana/dashboards/mc-overview.json` | PromQL + legend |
| `infra/grafana/dashboards/mh-overview.json` | PromQL + legend |
| `docs/observability/metrics/mc-service.md` | 3 refs |
| `docs/observability/metrics/mh-service.md` | 1 ref |
| `docs/runbooks/mh-incident-response.md` | 1 PromQL sample (survey-found) |
| `docs/observability/label-taxonomy.md` | §Current Drift removed, cross-ref rewritten |
| `TODO.md` | Label-canonicalization follow-ups section removed |

Net: 9 files, +22 / −76 lines.

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt): PASS
- L3 (guards): 18/18 PASS — validate-dashboard-panels + validate-application-metrics clean (source, dashboards, catalogs consistent across both services)
- L4 (tests — MC metrics 23/23, MH metrics 15/15): PASS
- L5 (clippy): trivial
- L6 (cargo audit): pre-existing vulnerabilities, not this devloop
- L7 (semantic): Lead-judgment SAFE — pure rename mirrored across two services
- L8 (env-tests): skipped — no service-behavior changes

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVE (0 findings)

Pure label-key rename. No credentials/PII/hostnames. PromQL semantic equivalence verified on both dashboards (same metric, same aggregation, same cardinality). Filter discipline verified via grep (no stray `"event"` string matches outside scope-local iterator/param names and English prose). Alert files correctly untouched — `mh_mc_notifications_total` is referenced by an alert but only filters on `status`, never `event`. Ancillary cleanup (TODO.md, label-taxonomy.md Current Drift) correctly reflects series closure.

### Media Handler Specialist (ownership cross-check)
**Verdict**: APPROVE (0 findings, 1 non-blocking nit)

MH hunks verified pure rename:
- `metrics.rs:179-186`: label key + param rename. Value set (`connected`/`disconnected`) preserved. Doc-comment + cardinality (2×2=4) correct. No retry / connection-state / emission-trigger changes.
- `mh-overview.json:1794-1795`: aggregation + cardinality identical post-rename.
- `mh-service.md` catalog + `mh-incident-response.md:588` runbook: consistent with emission.
- No other MH metrics affected.

**Non-blocking nit**: flagged a stray `event=...` ref in `docs/user-stories/2026-04-12-mh-quic-connection.md:385` — frozen design-time user-story snapshot, not drift. Did not affect verdict. Implementer's call on whether to touch the frozen doc (didn't, which is correct per "frozen means frozen" discipline).

---

## Rollback Procedure

1. Start commit: `111ef265af551624f63533daac8cb5f5ee3386d6`
2. Soft reset: `git reset --soft 111ef26`
3. Contained to MC + MH metrics.rs, two dashboards, two catalogs (three if mc.md has notification refs), label-taxonomy drift list, TODO.md.
