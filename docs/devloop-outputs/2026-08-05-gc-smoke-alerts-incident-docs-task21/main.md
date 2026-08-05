# Devloop Output: GC Smoke Tests + Alert Catalog + Incident Scenarios (task #21, R-50)

**Date**: 2026-08-05
**Task**: Add six GC smoke-test curl stanzas (CORS preflight allowed/denied, telemetry proxy authed/unauthed/oversize/rate-limit), reconcile the two `gc_telemetry_*` alert-catalog entries, and add incident scenarios (CORS misconfig post-deploy; telemetry proxy unreachable / rate-limit storm) — camelCase wire keys per R-53/task #51.
**Specialist**: operations
**Mode**: Agent Teams (full)
**Branch**: `feature/user-story-run-test`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `06e57cc3486c4a8131a3d0e56b693730d2164ef6` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (operations) |
| Implementing Specialist | `operations` |
| Iteration | `2` (Gate-2 attempt 1 failed: cross-boundary table heading suffix + zero-edit alerts.md row broke scope-drift guard; routed back) |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `implementer (operations is the implementing specialist — reviewer slot held by implementer)` |
| Semantic Guard | `semantic-guard` |

---

## Task Overview

### Objective
Story requirement R-50 (task #21): documentation deliverables for the GC CORS layer (task #5) and telemetry proxy (task #10).

1. `docs/runbooks/gc-deployment.md` §"Run Smoke Tests" — six new curl stanzas.
2. camelCase wire keys (R-53 / task #51 single rule) across all curl bodies/response-key refs touching AC/GC HTTP APIs.
3. `docs/observability/alerts.md` — catalog entries for `GCTelemetryProxyHighRejectionRate` (warn) + `GCTelemetryProxySilent` (page) matching `infra/docker/prometheus/rules/gc-alerts.yaml`.
4. `docs/runbooks/gc-incident-response.md` — two scenarios (Telemetry proxy unreachable / rate-limit storm; CORS misconfig post-deploy).
5. Fixture `infra/smoke/empty-otlp-metrics.bin` — create if task #16 has not (it has not; `infra/smoke/` does not exist).

### Scope
- **Service(s)**: GC (docs only + one binary test fixture)
- **Schema**: No
- **Cross-cutting**: Yes (observability owns alerts.md + the OTLP fixture)

### Prior-art / overlap (Single Source of Truth)
- **Task #16** (observability, `docs/devloop-outputs/2026-08-03-gc-telemetry-obs-artifacts/`) ALREADY landed:
  - Both alerts in `infra/docker/prometheus/rules/gc-alerts.yaml` (`GCTelemetryProxySilent`, `GCTelemetryProxyHighRejectionRate`).
  - **Complete** `docs/observability/alerts.md` entries for both (PromQL, thresholds, runbook links).
  - `gc-incident-response.md` Scenario 10 (Telemetry Proxy High Rejection Rate) + Scenario 11 (Telemetry Ingest Silent).
- Implication: requirement (3) is likely already satisfied — the operations task is to **verify exact-name/threshold consistency** against `gc-alerts.yaml`, NOT to duplicate. Requirement (4)'s "CORS misconfig post-deploy" is genuinely new; "Telemetry proxy unreachable / rate-limit storm" overlaps Scenarios 10/11 and must be reconciled (cross-reference / add the missing angle), not duplicated.

---

## Planning

### Code verification (ground truth — every stanza claim cited against real code)

**CORS** (`crates/gc-service/src/middleware/cors_observer.rs`, `crates/gc-service/src/routes/mod.rs::build_cors_layer`, `crates/gc-service/src/config.rs`):
- Allowlist source: `CORS_ALLOWED_ORIGINS` env (comma-separated) → `Config.cors_allowed_origins`. Explicit origins only, never `*`; empty vec = fail-closed (no origin allowed).
- `tower_http` `CorsLayer` short-circuits a genuine preflight (`OPTIONS` + `Origin` + `Access-Control-Request-Method`) with **200 OK** — with `Access-Control-Allow-Origin` (ACAO) iff the origin is allowed, and **no ACAO** if denied.
- `cors_preflight_observer` sits just outside `CorsLayer`: an allowed preflight passes through as **200 + ACAO**; a denied preflight (200-no-ACAO) is rewritten to a **fresh 403** with generic empty body and NO CORS headers.
- **FAIL-LOUDLY / TASK DISCREPANCY:** the task text says stanza (a) allowed preflight returns **204**. The code returns **200**, proven by the unit test `allowed_preflight_passes_through_200_with_acao` (asserts `StatusCode::OK`) and `record_cors_preflight("allowed", 200)`. NO GC path returns 204. **The stanza will document 200, not 204**, and I flag the task-text error here. (Independently confirmed by @test and @code-reviewer.)

**Telemetry proxy** (`crates/gc-service/src/handlers/telemetry.rs`, `crates/gc-service/src/services/telemetry_filter.rs`, `crates/gc-service/src/routes/mod.rs`, `crates/gc-service/src/config.rs`, `crates/gc-service/src/errors.rs`):
- Route: `POST /api/v1/telemetry/v1/metrics` (+ `/v1/traces`), behind `require_user_auth` (user JWT → `UserClaims`). Missing/invalid Bearer → **401** via `GcError::InvalidToken` (adds `WWW-Authenticate: Bearer realm="dark-tower-api", ...`).
- Auth mechanism gating the proxy: `Authorization: Bearer <user-access-token>` validated as a **user** token (`validate_user`). This is the user JWT obtained from AC login/register, NOT the service token.
- Handler check order (matters for stanza rationale): 0. disabled (503) → 1. size `body.len() > max_bytes` (**413**, emits `rejected_size`) → 2. Content-Type must be `application/x-protobuf` (else 415) → 3. per-`sub` GCRA rate limit (**429**) → 4. decode (400) → 5. forward to collector (502) → 6. **202**.
- Size limit: `DEFAULT_TELEMETRY_PROXY_MAX_BYTES = 256 * 1024` = **262144 bytes (256 KiB)** (`telemetry_proxy_max_bytes`). Handler 413 fires for bodies in (256 KiB, 512 KiB]; the route `DefaultBodyLimit` (2× = 512 KiB, via `body_limit_ceiling`) rejects > 512 KiB (also 413, no metric). Stanza (e) uses ~300000 bytes → lands in the handler gate.
- Rate limit: `DEFAULT_TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE = 60`. `governor` GCRA `Quota::per_minute(60)` keyed on JWT `sub` → burst capacity 60, replenish ~1 cell/sec. 70 rapid calls with the **same sub** → ~60× **202** then remainder **429**; exact split depends on elapsed time (replenishment). 429 (step 3) fires before decode/forward.
- 202 requires the OTel collector reachable (forward must return 2xx); unreachable collector → 502 (`GcError::BadGateway`, "Telemetry collector is unavailable"). Empty endpoint → 503 disabled. Stanzas note this prerequisite.

### Deliverable (1): six smoke stanzas — `docs/runbooks/gc-deployment.md` §"### 6. Run Smoke Tests"
Add a new sub-block "CORS + Telemetry Proxy Smoke Tests (R-1 / R-2)" inside §6, with a shared preamble reusing the existing port-forward + AC-login token-acquisition pattern (DRY with Test 4/5), then six stanzas:
- (a) allowed-origin preflight → **200** + `Access-Control-Allow-Origin: <origin>` (documents 200, flags the 204 task-text error inline as a NOTE).
- (b) disallowed-origin preflight → **403**, no ACAO, empty body.
- (c) authed telemetry POST of `infra/smoke/empty-otlp-metrics.bin` (`Content-Type: application/x-protobuf`) → **202** (prereq: collector reachable, else 502).
- (d) unauthenticated telemetry POST → **401** (+ `WWW-Authenticate`).
- (e) oversize (~300000 B > 256 KiB) → **413** (`PAYLOAD_TOO_LARGE`).
- (f) burst of 70 same-`sub` rapid POSTs → mix of **202** then **429** (dedicated smoke sub so the 60-cell GCRA budget is fresh; note replenishment).
Security/semantic-guard: all tokens/passwords are env-var placeholders (`$USER_TOKEN`, `$SMOKE_PASSWORD`), no realistic JWT/secret literals; allowed stanza echoes the exact configured origin, never `*`; no `ACAO: *` + credentials.

### Deliverable (2): camelCase wire keys (R-53 / task #51)
New stanzas use camelCase only: token extraction via `jq -r '.accessToken'` (matching existing Tests 4/5). Verified existing smoke section already uses `accessToken` throughout — zero snake_case token keys present (grep clean). No rewrite of existing stanzas (SSoT); only my new stanzas add camelCase refs.

### Deliverable (3): alerts.md — VERIFY-CONSISTENT, ZERO edits (unless drift found)
Verified both catalog entries against the SSoT `infra/docker/prometheus/rules/gc-alerts.yaml`:
- `GCTelemetryProxySilent`: catalog = Critical/page, `for: 5m`, flat-OR-absent shape gated by 1h baseline, runbook scenario-11 → **matches** the `GCTelemetryProxySilent` rule in the yaml (severity page, component telemetry-proxy, identical PromQL, `for: 5m`).
- `GCTelemetryProxyHighRejectionRate`: catalog = Warning, `>0.10` + non-zero-traffic gate, `for: 10m`, runbook scenario-10 → **matches** the `GCTelemetryProxyHighRejectionRate` rule in the yaml (severity warning, component telemetry-proxy, identical PromQL, `for: 10m`).
- Names + thresholds match exactly. **No drift. Requirement (3) is already satisfied by task #16 — expected diff to `docs/observability/alerts.md` = zero.** Recorded here per Single Source of Truth.

### Deliverable (4): two incident scenarios — `docs/runbooks/gc-incident-response.md`
Structure decision (justified): add two new numbered scenarios following the sibling Scenario shape (Symptoms / Diagnosis / Root Causes / Remediation / Escalation, detection→diagnosis→resolution), plus TOC entries. Both **cross-reference**, never restate, existing Scenarios 10/11 PromQL/thresholds (SSoT lives in `gc-alerts.yaml`; alerts.md is the catalog).
- **Scenario 12: CORS Misconfiguration Post-Deploy** — genuinely new *entry point* (deploy-time). Detection: `gc_cors_preflight_total{origin_class="denied"}` spike right after a gc-service config/image rollout + browser console CORS errors. Diagnosis: cross-reference Scenario 11 step 2 for the `gc_cors_preflight_total` query and the `CORS_ALLOWED_ORIGINS` fail-closed knob (do NOT re-derive); add the deploy-correlation angle (diff the ConfigMap/overlay vs last-good, denied-preflight warn logs carry `requested_origin`). Resolution (security-safe): add the *specific* missing origin to `CORS_ALLOWED_ORIGINS` (never `*`, never allow-all), rollout restart, verify with the deny→allow smoke stanzas (a)/(b); rollback the offending deploy if that is faster.
- **Scenario 13: Telemetry Proxy Unreachable / Rate-Limit Storm** — reconciling scenario. Explicitly points to Scenario 10 for the rejection-rate taxonomy (`rejected_rate` + `gc_telemetry_rate_limited_total{reason="per_user"}`; `error`+502/503 collector path) and Scenario 11 for the silent variant. Adds ONLY the genuinely additive deltas: (i) the **GC→collector network-path** distinction (netpol/DNS between GC and the collector, vs collector-pod-down — both surface as 502 `BadGateway` but have different fixes), and (ii) framing the **rate-limit storm** as a burst-shaped event (sudden `rejected_rate` spike from a client retry loop hitting the per-`sub` 60/min GCRA) with the config knob `TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE`. No threshold/PromQL hand-copy.

### Deliverable (5): fixture `infra/smoke/empty-otlp-metrics.bin`
Create `infra/smoke/` and the fixture. Chosen shape: **(b) one empty `resource_metrics` entry — 2 bytes `0x0a 0x00`** = `ExportMetricsServiceRequest { resource_metrics: [ ResourceMetrics::default() ] }`, zero data points. Rationale: unambiguously an OTLP metrics message on byte inspection (vs a 0-byte file), still minimal + zero-data-point. Verified acceptance path against `handlers/telemetry.rs::ingest` + `telemetry_filter::filter_metrics`: decodes via prost to one default `ResourceMetrics`, filter no-ops (resource None, empty scope_metrics), re-encodes to `0x0a 0x00`, forwards to collector → 2xx → **202**. Generation (reproducible, no magic blob): `printf '\x0a\x00' > infra/smoke/empty-otlp-metrics.bin`. Verification command recorded in main.md for @observability/@semantic-guard byte decode.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. The table lists exactly the files in this change's diff; column 1 is the changed path, column 2 the classification keyword, column 3 the owner when not operations.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/gc-deployment.md` | Mine | — |
| `docs/runbooks/gc-incident-response.md` | Mine | — |
| `infra/smoke/empty-otlp-metrics.bin` | Not mine, Minor-judgment | observability |

Per-file detail (folded out of the table so it lists only diff files):
- `docs/runbooks/gc-deployment.md` — add six CORS/telemetry smoke stanzas in §6. Operations-owned runbook.
- `docs/runbooks/gc-incident-response.md` — add Scenario 12 (CORS misconfig) + Scenario 13 (telemetry unreachable/rate-limit storm) + TOC. Operations-owned runbook.
- `infra/smoke/empty-otlp-metrics.bin` — create minimal zero-data-point OTLP fixture. Observability-owned; Minor-judgment (requires OTLP + proxy-accept knowledge; small bounded artifact). Observability co-signs the hunk; commit carries `Approved-Cross-Boundary: observability`.

Files deliberately NOT in the table (not in the diff, so listing them would be scope drift):
- `docs/observability/alerts.md` — **verify-only, zero edits** (observability-owned). Both catalog entries already match the SSoT `infra/docker/prometheus/rules/gc-alerts.yaml`; no drift found, so there is no hunk to classify. Recorded here in prose per Single Source of Truth.
- `infra/docker/prometheus/rules/gc-alerts.yaml` — read-only SSoT; cited, never edited.
- This working doc (`docs/devloop-outputs/.../main.md`) — the devloop-output note itself, excluded from its own classification table.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed (Ownership Lens co-signed: alerts.md verify-only, .bin fixture Minor-judgment) |
| Code Quality | confirmed |
| DRY | confirmed |
| Semantic Guard | confirmed |

**Gate 1 classification-sanity guard**: `STATUS=OK REASON=cross-boundary-classification-clean-1-files` (no GSA paths; observability-owned rows carry Owner + owner co-sign).

**Approved plan note**: task text's stanza (a) "204" is a factual error — GC returns **200** for an allowed CORS preflight (unit test `allowed_preflight_passes_through_200_with_acao`). Implementer documents the real behavior (200) with an inline note per "Fail loudly; never mask". Approved.

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | Load-bearing: Sc 12 said `*` is "silently ignored" — actually panics `AllowOrigin::list` (tower-http 0.5.2) → gc-service CrashLoopBackOff at startup; corrected all 3 passages |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | `$BURST_TOKEN` undefined in stanza (f) → would 70×401 (auth gate) not exercise GCRA; fixed in-diff (inline token from separate smoke account) |
| Observability | CLEAR | 0 | 0 | 0 | Ownership Lens confirmed: alerts.md zero-diff; `.bin` valid `ExportMetricsServiceRequest` (`0a 00`) co-signed; needs `Approved-Cross-Boundary: observability` trailer |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | Sc 12/13 section headers aligned to file's `Symptoms`/`Diagnosis`/`Remediation`/`Escalation` set + sibling self-anchor; cite guards clean |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | Three verbatim PromQL/command copies → pointers into Scenarios 10/11; each aggregation query now appears once; no extraction opportunities |
| Semantic Guard | CLEAR | 0 | 0 | 0 | Placeholder creds only; camelCase sweep additive; fixture holds no sensitive data |

**Gate 3 outcome**: all six CLEAR / RESOLVED-FIXED. **Zero accepted deferrals, zero spin-outs, zero escalations** — every finding fixed in-diff. Re-validation after review fixes: `scripts/layer3.sh` on the final tree → 35/35 guards pass, doc-citation cite guards PASSED. Close-time condition satisfied at commit: `Approved-Cross-Boundary: observability` trailer on the `.bin` fixture.

---

## Implementation Notes (iteration 1)

**Deliverable (1) — six smoke stanzas:** added to `docs/runbooks/gc-deployment.md` §"### 6. Run Smoke Tests" as a new sub-block "CORS + Telemetry Proxy Smoke Tests (R-1 / R-2)" + six checklist items. Stanza (a) documents **200** (not 204) with the inline fail-loudly NOTE citing `allowed_preflight_passes_through_200_with_acao`. (b) 403/no-ACAO. (c) authed→202. (d) unauth→401. (e) 300000-byte body→413. (f) 70-burst→202/429.

**Deliverable (2) — camelCase:** all token references use `jq -r '.accessToken'`; passwords/tokens are env placeholders (`$SMOKE_PASSWORD`, `$USER_TOKEN`, `$BURST_TOKEN`). No snake_case wire keys.

**Deliverable (3) — alerts.md reconcile (VERIFY-ONLY, ZERO EDITS):** confirmed both catalog entries in `docs/observability/alerts.md` match the SSoT `infra/docker/prometheus/rules/gc-alerts.yaml` exactly:
- `GCTelemetryProxySilent` — catalog Critical / yaml `severity: page`; both `for: 5m`; identical flat-OR-absent PromQL; runbook `#scenario-11-telemetry-ingest-silent`. MATCH.
- `GCTelemetryProxyHighRejectionRate` — catalog Warning / yaml `severity: warning`; both `for: 10m`; identical `>0.10` + non-zero-traffic-gate PromQL; runbook `#scenario-10-telemetry-proxy-high-rejection-rate`. MATCH.
- Names + thresholds + `for` clauses match. **No drift. Zero edits to alerts.md.** Requirement (3) satisfied by task #16.

**Deliverable (4) — incident scenarios:** added to `docs/runbooks/gc-incident-response.md` + TOC:
- Scenario 12 "CORS Misconfiguration Post-Deploy" — new; cross-references Scenario 11 step 2 for the shared `gc_cors_preflight_total` allowlist diagnosis, adds the deploy-time entry point + security-safe remediation (specific origin, never `*`).
- Scenario 13 "Telemetry Proxy Unreachable / Rate-Limit Storm" — reconciling; explicitly defers the taxonomy/thresholds to Scenarios 10/11, adds only 13a (GC→collector network-path vs collector-pod-down 502 split) and 13b (burst-shaped per-user rate-limit storm). No PromQL/threshold restatement.

**Deliverable (5) — fixture:** created `infra/smoke/empty-otlp-metrics.bin`, 2 bytes `0x0a 0x00` = `ExportMetricsServiceRequest { resource_metrics: [ ResourceMetrics{} ] }`, zero data points. Generated via `printf '\x0a\x00'`. Verified with `protoc --decode_raw` → `1: ""` (field 1 = resource_metrics, empty submessage), i.e. a valid metrics request with zero data points that the proxy accepts (202 path). Observability co-signs; commit carries `Approved-Cross-Boundary: observability`.

---

## Gate 2 — Validation Record

Attempt 1 (`layer-all.sh`, buggy main.md heading): `LAYER=3 RESULT=FAIL` (`validate-cross-boundary-scope` → `scope_drift_inbound` ×3). Root cause: H2 heading `## Cross-Boundary Classification (ADR-0024 §6.2)` did not exactly match the parser's `## Cross-Boundary Classification`, so zero plan rows parsed. All other layers green in the same run: `LAYER=1 OK, LAYER=2 OK, LAYER=4 OK (cargo+nx test), LAYER=5 OK (clippy+nx lint), LAYER=6 OK (cargo-audit + pnpm-audit + buf-breaking), LAYER=7 OK (env-tests-passed + browser-e2e-passed, 868s cluster run)`.

Fix (doc-only, `main.md` classification section — no runbook/fixture change): bare heading + canonical 3-column table listing exactly the three diff files; zero-edit alerts.md recorded in prose.

Attempt 2 (targeted re-run of the only affected layer, `scripts/layer3.sh`, fixed tree): `STATUS=OK REASON=guards-passed`, **35/35 guards pass** incl. `validate-cross-boundary-scope`. Layers 1,2,4,5,6,7 unaffected by a devloop-output doc change (they passed in the attempt-1 full run on the code-identical tree). **All seven layers green → Gate 2 clear.**

---

## Gate 3 review fixes (iteration 2)

- **@test (RESOLVED-FIXED):** smoke stanza (f) referenced an undefined `$BURST_TOKEN` → would have exercised the auth gate (70× 401), not the GCRA limiter. Now acquires `$BURST_TOKEN` inline from a separate smoke account.
- **@code-reviewer (section-header consistency):** aligned Scenarios 12/13 to the file's established header set — `Detection`→`Symptoms`, Sc13 `Resolution`→`Remediation`; added the sibling `**Runbook Section**` self-anchor to both.
- **@security (fix-now, code-verified):** corrected the Scenario 12 `*` behavior. Prior text said the layer "silently ignores" `*`; verified against code that `CORS_ALLOWED_ORIGINS="*"` is kept verbatim by `config.rs`, parses as a valid `HeaderValue` in `routes/mod.rs::build_cors_layer`, and reaches `AllowOrigin::list`, which **panics** on a wildcard in tower-http 0.5.2 (workspace pins `tower-http = "0.5"`; `WILDCARD = HeaderValue::from_static("*")`; `panic!("Wildcard origin (\`*\`) cannot be passed to \`AllowOrigin::list\`...")`). Since `build_cors_layer` runs at router build, a `*` entry crashes gc-service at startup (CrashLoopBackOff / rollout never Ready) — a different, more severe incident shape than browser-only breakage. Fixed the Diagnosis step-2 comment, Root Cause #3 (Check now includes "pod crashlooping"), and the Remediation comment.
- **@dry-reviewer (must-fix 1 & 2 + accepted 3):** converted the two verbatim PromQL copies into pointers — Sc12 CORS detection query → "run the query from Scenario 11 step 2"; Sc13b rate-limit query → "run the query from Scenario 10 step 4" (kept the additive cardinality + quota notes). Also de-duped Sc13a collector-outage: collector-health check now points to Scenario 10 step 5 and the collector restart to Scenario 10 Remediation Option 2, keeping only the genuinely-additive network-path (DNS/netpol/endpoint) commands. Verified each aggregation query now appears exactly once in the file.

Post-fix guard re-run: `validate-cross-boundary-scope` = OK; no line-number cites; alerts.md still zero-diff.

---

## Accepted Deferrals

**None.** Every reviewer finding (6 total across security/test/code-reviewer/dry-reviewer) was fixed in-diff. DRY reviewer confirmed no cross-service extraction opportunities to append to `docs/TODO.md`. No scope was deferred.

---

## Final Summary

All five R-50 deliverables landed:
1. Six CORS/telemetry smoke stanzas in `docs/runbooks/gc-deployment.md` §"Run Smoke Tests" (allowed preflight **200**+ACAO — task-text 204 corrected & cited; denied 403; telemetry 202/401/413/429).
2. camelCase wire keys throughout (`.accessToken`, no snake_case).
3. `docs/observability/alerts.md` verified consistent with `gc-alerts.yaml` — **zero edit** (task #16 already landed both entries; SSoT honored).
4. Two incident scenarios in `docs/runbooks/gc-incident-response.md`: Scenario 12 (CORS misconfig post-deploy, new) + Scenario 13 (telemetry unreachable / rate-limit storm, reconciling — cross-references Scenarios 10/11).
5. Fixture `infra/smoke/empty-otlp-metrics.bin` (2-byte valid zero-data-point `ExportMetricsServiceRequest`).

Gates: Gate 1 confirmed (6/6 reviewers + classification guard). Gate 2 cleared (all 7 layers green; one attempt-1 scope-drift failure from a main.md heading suffix, fixed). Gate 3 cleared (6/6 CLEAR/RESOLVED-FIXED, zero deferrals). Two review findings were load-bearing correctness catches (CORS `*` panic behavior; undefined burst token).
