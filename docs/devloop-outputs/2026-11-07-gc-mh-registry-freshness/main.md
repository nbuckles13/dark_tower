# Devloop Output: GC registry freshness and configuration (Story 4 task GC-1)

**Date**: 2026-11-07
**Task**: Story 4 task GC-1 — one configured MH load-report interval and derived staleness bound govern all GC freshness decisions; registry retention; GC MH config keys; telemetry
**Specialist**: global-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present (tier=light ESCALATED to full: plan reaches --light exclusion surfaces — instrumentation `metrics::` and deployment config `infra/services/gc-service/config.env`)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~4h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c6e569bf721663951afb0088bca8a8175a1503d7` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `global-controller` |
| Tier | `full — escalated from manifest tier=light: task adds metrics instrumentation and edits deployment config (--light exclusion surfaces), so Gate-1 runs per Step 5 escalation rule` |
| Iteration | `1` |
| Security | `security` |
| Test | `paired-test` (paired, replaces test slot) |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Database (conditional) | `database` |
| Semantic Guard | not spawned (no credential/Debug/secret surface in task) |

---

## Task Overview

### Objective
See task prompt (`task-7.prompt`, story 2026-10-31-survive-handler-loss): GC-1 registry freshness and configuration (R-24, R-26, GC series of R-34).

### Scope
- **Service(s)**: gc-service (+ env-tests, infra config.env, Grafana dashboard, docs)
- **Schema**: No (queries only; no migration)
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - governed by ADR-0039 §7, §10.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/gc-service/src/config.rs` (`MhRegistryConfig`, 4 REQUIRED keys, bounded parse helper, derived bound) | Mine | — |
| `crates/gc-service/src/grpc/mh_service.rs` (interval from config; load-report counter) | Mine | — |
| `crates/gc-service/src/repositories/media_handlers.rs` (bound param, ms binds, sweep drops draining skip, retention DELETE, availability count query) | Mine (database reviews every query) | — |
| `crates/gc-service/src/repositories/mod.rs` (re-export `MhAvailabilityRow`) | Mine | — |
| `crates/gc-service/src/tasks/generic_health_checker.rs` (closure captures its own threshold; loop carries no unit) | Mine | — |
| `crates/gc-service/src/tasks/health_checker.rs` (MC wrapper: capture seconds in closure; no behavioural change) | Mine | — |
| `crates/gc-service/src/tasks/mh_health_checker.rs` (MH tick = sweep, retention, gauge recompute; tests) | Mine | — |
| `crates/gc-service/src/services/mh_selection.rs` (initial count + bound as params) | Mine | — |
| `crates/gc-service/src/services/mc_assignment.rs` (thread `&MhRegistryConfig`) | Mine | — |
| `crates/gc-service/src/handlers/meetings.rs` (pass `&state.config.mh_registry`) | Mine | — |
| `crates/gc-service/src/main.rs` (derived MH bound to MH checker; config gauges; zero-init) | Mine | — |
| `crates/gc-service/src/observability/metrics.rs` (3 new series + zero-init, 2 DB labels, zero-init render test extended) | Mine (observability reviews) | — |
| `crates/gc-service/tests/**` (new reference keys in `from_vars` call sites; `mh_registry_tests.rs`, `db_metrics_integration.rs`, `mc_assignment_rpc_tests.rs`; new `mh_registry_metrics_integration.rs`, `mh_health_checker_tests.rs`, `common/mh_registry.rs`) | Mine | — |
| `crates/gc-test-utils/src/**` (new `reference_config.rs`: strict config.env parse + reference MH registry; harness + lib wiring) | Mine | — |
| `infra/services/gc-service/config.env` (4 keys) | Mine | — |
| `infra/services/gc-service/deployment.yaml` (4 per-key `configMapKeyRef` env entries, same shape as existing) | Not mine, Mechanical | infrastructure |
| `infra/grafana/dashboards/gc-overview.json` (3 new panels) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/gc-service.md` (3 new catalog entries) | Not mine, Minor-judgment | observability |
| `docs/DATABASE_SCHEMA.md` (§8 only) | Not mine, Minor-judgment | database |
| `docs/runbooks/gc-deployment.md` (Configuration Reference rows) | Not mine, Minor-judgment | operations |
| `crates/env-tests/tests/03_gc_deployment_config.rs` (new) | Not mine, Domain-judgment (paired; paired-test writes) | test |
| `crates/env-tests/src/fixtures/mh_config.rs` (doc wording: GC consumer of `Conversion`; no new module) | Not mine, Minor-judgment (paired) | test |
| `crates/env-tests/src/fixtures/deployment_config.rs` (header lists 03) | Not mine, Minor-judgment (paired) | test |
| `crates/env-tests/src/fixtures/kube.rs` (Gate 2 fix: shared `settled_serving_pods` settle loop, `max_termination_grace` from live pod specs; `live_mh_pods` routed through it) | Not mine, Domain-judgment (paired; paired-test writes) | test |
| `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md` (GC freshness gates subsection) | Not mine, Minor-judgment (paired) | test |
| `docs/TODO.md` (close the :971 MH-gauge asymmetry item; Observability Debt entry for `gc_registered_controllers` per Lead O9 ruling; MH ticker-rebuild follow-up, Lead-approved) | Mine | — |
| `docs/specialist-knowledge/global-controller/INDEX.md` (pointers: MH registry config/tick) | Mine | — |


---

## Planning

### Problem in mechanism language
GC makes three freshness decisions about an MH row: (1) is it a selection candidate, (2) does the sweep flip it to `unhealthy`, (3) (new) how is it counted in `gc_mh_available`; plus one lifecycle decision, (4) when is the row deleted. Today (1) uses a hardcoded 30 s string-concatenated interval, (2) borrows the MC threshold (`MC_STALENESS_THRESHOLD_SECONDS`) and skips `draining` forever, (3) does not exist, (4) never happens. The push to MH (`load_report_interval_ms`) is a separate hardcoded 10 000. Invariant to establish: **one Duration `B = k × I`, computed once in `Config::from_vars`, is the only MH staleness value in GC**, read by (1), (2), (3) and by the config gauge; `I` is the same value pushed in `RegisterMHResponse`; retention `R > B` is enforced at startup. GC-2 (registration "stale" check) and GC-3 (exclude-aware selection) consume `B` from the same field.

### Configuration (`config.rs`)
- New `pub struct MhRegistryConfig { load_report_interval: Duration, staleness_multiplier: u32, staleness_bound: Duration, retention: Duration, initial_handlers_per_meeting: usize }`, private fields + accessors, field `Config.mh_registry`. Constructed only via `MhRegistryConfig::new(interval_ms, k, retention_s, initial) -> Result<_, ConfigError>` (validation lives there; `from_vars` parses and delegates; tests ABOUT freshness call `new` with their own values and get identical validation).
- Four keys REQUIRED, **no Rust `DEFAULT_*`** (config.env is the SSoT for the reference value; a Rust default would be a second copy). Each presence check is `ConfigError::MissingEnvVar("KEY".to_string())` with the literal on ONE line (dt-guard env-config discovery). One local `parse_bounded::<u64>(key, raw, min, max)` helper (DRY #5) → `ConfigError::InvalidMhRegistryConfig(msg naming the key)`.
- Ranges: interval [1000, 60000] ms; k [2, 10]; initial [1, 4]; retention: integer seconds, `retention_s.checked_mul(1000)` then `> B` in ms (else error naming `GC_MH_REGISTRY_RETENTION_SECONDS`), upper bound 2 592 000 s (30 d) — **judgment call, flagged for ops/security**: the prompt gives no max, but an unbounded value can overflow i64 ms or PostgreSQL's interval range and would then fail every tick at runtime instead of at startup (S3). `B = interval × k` via `checked_mul` in u64 (cannot overflow in range, still checked; no `as`).
- Debug impl gets the new fields (none secret).
- Test support lives ONLY in `gc-test-utils` (no new gc-service public API): `reference_mh_registry_vars()` = the four `GC_MH_*` entries parsed from `include_str!("../../../infra/services/gc-service/config.env")` by a STRICT parser (non-KEY=VALUE line, empty key/value or duplicate key panics naming the line; the single GC home for both the file text and the parse), and `reference_mh_registry() -> MhRegistryConfig`; every integration-test `from_vars` site and the harness extend with it (no retyped 10000/3/3600/2 in integration tests). In-crate unit tests (config.rs, mh_service.rs, mh_health_checker.rs) test validation/behaviour and use explicit test-local values via `MhRegistryConfig::new` / `from_vars`.

### Registry (`media_handlers.rs`), all binds i64 ms as `$n * INTERVAL '1 millisecond'`
- One private `fn bind_ms(d: Duration) -> Result<i64, GcError>` (`i64::try_from(d.as_millis())`) used by every query; no `||`/`::text`/`format!` left in the file (S4, DB #1).
- Delete `DEFAULT_HEARTBEAT_STALENESS_SECONDS`. `get_candidate_mhs(pool, region, staleness_bound: Duration)`; predicate otherwise unchanged (`health_status = 'healthy'` keeps excluding draining/unhealthy/pending/degraded — S6, DB #6).
- `mark_stale_handlers_unhealthy(pool, staleness_bound: Duration)`: drops `AND health_status != 'draining'`, keeps `!= 'unhealthy'` (rows_affected counts transitions). Instrument/log field renamed `threshold_ms`.
- New `delete_expired_handlers(pool, retention: Duration) -> u64`: `DELETE FROM media_handlers WHERE last_heartbeat_at < NOW() - $1 * INTERVAL '1 millisecond'`, DB label `delete_expired_mh`. No FKs reference the table; **no migration, no new index** (bounded table).
- New `count_available_handlers(pool, staleness_bound, own_region) -> Vec<MhAvailabilityRow { own_region: bool, state: HealthStatus, count: u64 }>`: one aggregate `SELECT (region = $2) AS own_region, CASE WHEN last_heartbeat_at < NOW() - $1 * INTERVAL '1 millisecond' THEN 'unhealthy' ELSE health_status END AS state, COUNT(*) … GROUP BY 1, 2`, DB label `count_available_mh`. Row decode via `try_get` (ADR-0002); state via `HealthStatus::from_db_str` (closed domain).
- `update_load_report` stays UPDATE-only; zero rows → existing generic NOT_FOUND "Handler not registered" (no id echoed, S5); a retention-deleted row takes that same path and is never recreated.
- Both new labels added to `GC_DB_QUERY_OPERATIONS` (zero-init).

### Health-checker tick
- `generic_health_checker.rs`: loop no longer carries a threshold; signature becomes `start_generic_health_checker(pool, cancel, entity_name, F: Fn(PgPool) -> Fut)`; each wrapper's closure captures its own typed threshold. This removes the unit seam entirely (MC keeps seconds inside its closure, MH keeps Duration) — no ms smuggled through a `_seconds` param, no forked loop (CR #1, DRY #2). MC behaviour unchanged.
- `mh_health_checker.rs`: `start_mh_health_checker(pool, registry: MhRegistryConfig, own_region: String, cancel)`; closure calls `mh_registry_tick(&pool, &registry, &own_region)`, which runs in order (O1): sweep → retention DELETE → gauge recompute. Each step independent: a failed step is logged at error with a literal `step = "sweep"|"retention"|"availability"` field (DB metric already recorded) and does not skip the others; returns the sweep result for the generic loop. Retention deletes logged at info only when count > 0. Logs carry counts only, no endpoints (S7); the existing NOT_FOUND warn keeps handler_id (operator join key, P5).
- `main.rs`: deletes `let mh_staleness_threshold = mc_staleness_threshold`; passes `config.mh_registry` (copy) and `config.region`. After recorder install: `set_mh_registry_config_gauges(&config.mh_registry)` and zero-init of `gc_mh_available`. Startup log adds the four values.

### Selection
- `select_mhs_for_meeting(pool, region, staleness_bound, initial_handlers_per_meeting)`; the "first, then second from the remainder" becomes "repeat weighted pick from the remainder up to N" — same algorithm, N from config. `assign_meeting_with_mh` gains `mh_registry: &MhRegistryConfig`; handlers pass `&state.config.mh_registry`. `LOAD_BALANCING_CANDIDATE_COUNT` (5) ≥ max N (4) — kept, noted in a comment.

### Telemetry (no handler_id/meeting/connection labels; S1/O6)
- `gc_mh_load_reports_total{outcome=accepted|unknown_handler}`: `accepted` only after a successful UPDATE, `unknown_handler` only on the zero-row NOT_FOUND path; DB errors and invalid-argument rejections in neither (O5). Zero-init both.
- `gc_mh_available{region, state}`: **region label bounded per security S1(a)**: `region = <GC's own config.region>` for rows whose region equals it, else `"other"` — the MH-asserted string never becomes a label value. State domain from `CONTROLLER_STATUSES` (DRY #3). **Semantics (pinned in a test):** a row within `B` counts under its reported state; a row older than `B` (and not yet retention-deleted) counts as `unhealthy` regardless of stored status; so `unhealthy` ≤ retained rows. **Recompute-and-set over the full closed set** {own, other} × 5 states every tick, missing combinations set to 0 (S2/O2) — no tracked set needed because the label domain is closed. Zero-initialised at startup.
- `gc_mh_load_report_interval_seconds`, `gc_mh_staleness_threshold_seconds`: set once at startup from `MhRegistryConfig` accessors (`as_secs_f64()` of the same Durations the push and the bound use; no second multiplication, O4).
- O9: OUT of scope per Lead ruling (obligation b). `gc_registered_controllers` code/catalog/panel untouched; recorded as a docs/TODO.md §Observability Debt entry (observability's text).
- Catalog for `gc_mh_available` states the exact label domain `region ∈ {<GC_REGION>, "other"}`, that MH-asserted region strings never become label values, and that `gc_db_queries_total{operation="count_available_mh",status="error"}` rising means the gauge is stale (P3/P4). `zero_init_renders_counters_present_at_zero` extended to both load-report outcomes and both new DB operations (P6).
- Catalog entries (unit, `max by(region, state)` guidance because every GC replica emits identical values — O3/O7, semantics, distinction from `gc_registered_controllers`); dashboard panels: stacked timeseries `max by(region,state)(gc_mh_available)`, `sum by(outcome)(increase(gc_mh_load_reports_total[$__rate_interval]))`, stat panel `max(...)` for both config gauges.
- ADR-0032 test `tests/mh_registry_metrics_integration.rs` with `MetricAssertion`: accepted +1; unknown_handler +1 after deletion (accepted unobserved); gauge after a tick per state incl. stale→unhealthy, `other` bucket, and a second tick after a state change/deletion proving set-not-accumulate and vanished→0; config gauges equal config values.

### Tests (gates; paired with test)
- Sweep, one test: stale draining→unhealthy, fresh draining→draining, stale healthy→unhealthy, fresh healthy→healthy; replaces `test_mh_health_checker_skips_draining_handlers` (which encodes the opposite rule).
- Millisecond proof: bound 3750 ms (interval 1250, k 3); rows at NOW()−3200 ms (fresh; 550 ms slack, backdate UPDATE immediately before the query under test) and NOW()−3900 ms (stale); truncation to 3 s mis-sorts the first, rounding to 4 s the second.
- Selection, one test, set-by-id: fresh healthy in; stale healthy, fresh draining, unhealthy, pending out. Initial count: N=1 and N=3 with 3+ candidates → exactly N distinct.
- gc-test-utils strict parser: unit tests for comment/blank lines, empty value, value containing `=`, malformed line and duplicate key panic; `reference_mh_registry_vars()` panics naming any missing `GC_MH_*` key (never a partial map).
- Tick order, one test: in one tick a row older than R is deleted AND absent from `gc_mh_available`, a stale row younger than R is swept and counted `unhealthy`.
- Load-report negative arm: an invalid-argument report leaves both outcome series unobserved.
- Retention, one test: row older than R deleted, younger-but-stale kept; then load report for deleted id → NOT_FOUND + unknown_handler +1 + `SELECT count(*)` = 0.
- Config table test: each key missing (error names key), below min, above max, min and max accepted, non-numeric; retention == B fails (k=2, I=1000, R=2 s), R=3 s passes; retention above max fails.
- Interval push: `MhService` built with I=2500 → `RegisterMHResponse.load_report_interval_ms == 2500`.
- Every hardcoded 30/60/1 s MH staleness in `mh_registry_tests.rs`, `mh_health_checker.rs` tests, `db_metrics_integration.rs` replaced by a test-local bound passed in; backdating offsets derived from that bound. Tick tests drive the tick function synchronously.
- Env-test `crates/env-tests/tests/03_gc_deployment_config.rs` (feature `smoke`), **written by paired-test** per their plan: per-GC-pod pinned reads of both config gauges vs ConfigMap via `mh_config::gauge_verdict` + `Conversion::MsToSeconds` (staleness expectation = deployed k × deployed interval), and every GC instance's `sum by(instance)(gc_mh_available{state="healthy"}) ≥ n` where n = `kube::live_mh_pods()` count with `n ≥ 1` asserted; poll budget derived from config.
- test-gates.md gets a "GC freshness (GC-1)" subsection listing the above (paired-test drafts, I review).

### Docs
- `DATABASE_SCHEMA.md` §8 rewritten to mirror the migration exactly (types, NOT NULL/DEFAULTs, CHECK, both partial indexes with WHERE, updated_at trigger), handler_id per incarnation, staleness `B = k × I` and retention derivation, max_streams DEFAULT note re-cited to ADR-0039 R-25 ("no schema migration"); fictional indexes and "authoritative over the sketch" wording dropped.
- `gc-deployment.md` Configuration Reference: four rows (Required **Yes**, reference value, range, effect of change — ops O-C): I reaches an MH only at its registration (MH builds its ticker once, `mh-service/src/main.rs:707-710`), so changing I follows the cadence-below-every-replica's-B procedure (O-A: lower I → roll GC then roll MHs via `mh.sh`, or raise k first; raise I only while I_new stays well below k × I_old; the interval gauge shows GC's configured value, not MHs' live cadence); k takes effect on GC roll; raising k is always safe, lowering k shrinks B immediately and follows the same cadence-below-every-replica's-B rule as I; R > B, max 30 d, startup error otherwise; N affects only new meetings. Plus: missing/out-of-range key → new GC pods CrashLoop with a ConfigError naming the key while old pods keep serving; fix config.env and re-apply. Plus O-B: a GC/DB outage longer than R deletes every MH row on the first tick after recovery; MHs get NOT_FOUND, re-register within ~one interval; expect a brief burst of refusals and `unknown_handler` (= MH alive, row gone; self-heals). `config.env` comment carries the O-A warning. `deployment.yaml` entries are hard refs (no `optional: true`).
- docs/TODO.md: MH follow-up — MH should rebuild its load-report ticker on re-registration so an interval change reaches running MHs (pending Lead confirmation; MH's `gc_client.rs` is out of scope here).
- `docs/TODO.md` :971 item closed (served by `gc_mh_available`).
- INDEX.md: MH registry config + tick pointers.

### Deferred / not done
- MC staleness (`MC_STALENESS_THRESHOLD_SECONDS`, `meeting_assignments.rs`) untouched (out of scope per task).
- Hoisting a shared bounded-parse helper to `common` — DRY reviewer logs it (cross-service extraction, task-sized).


---

## Pre-Work

None

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Database | confirmed |

---

## Implementation Summary

- **One bound.** `MhRegistryConfig::new` (config.rs) validates the four REQUIRED keys (no Rust defaults; one-line `MissingEnvVar` per key) and computes `staleness_bound = k × interval` once (checked_mul). Retention: `checked_mul(1000)`, must be > bound, max 30 d. Every MH freshness decision reads `config.mh_registry`: `get_candidate_mhs`, `mark_stale_handlers_unhealthy`, `count_available_handlers`, the config gauges, and `MhService` (interval push). `DEFAULT_HEARTBEAT_STALENESS_SECONDS`, `DEFAULT_LOAD_REPORT_INTERVAL_MS` and main.rs's borrowed MC threshold are deleted. MC staleness untouched.
- **Queries.** Every registry interval is an i64 ms bind (`bind_ms`, checked) as `$n * INTERVAL '1 millisecond'`; no string concatenation remains. Sweep drops the draining skip. New `delete_expired_handlers` (`delete_expired_mh`) and `count_available_handlers` (`count_available_mh`, one GROUP BY; stale rows → `unhealthy`; region reduced to `region = GC_REGION`) with `try_get` decoding into `MhAvailabilityRow`. No migration.
- **Tick.** The generic loop no longer carries a threshold (`Fn(PgPool)`); MC's closure captures its seconds (same conversion as before, moved), MH's captures `MhRegistryConfig`. `mh_registry_tick`: sweep → retention → `set_mh_available`; independent steps, error logs with literal `step`.
- **Selection.** "first, then second" generalised to "weighted pick from the remainder until N" with N = `GC_MH_INITIAL_HANDLERS_PER_MEETING`, threaded via `assign_meeting_with_mh(.., &MhRegistryConfig)`.
- **Telemetry.** `gc_mh_load_reports_total{outcome}` (accepted after a matched UPDATE, unknown_handler on NOT_FOUND only; zero-init), `gc_mh_available{region∈{GC_REGION,"other"}, state}` (full 10-series set every tick, 0 at startup), `gc_mh_load_report_interval_seconds`, `gc_mh_staleness_threshold_seconds` (set once at startup). Catalog entries, 3 panels + row, ADR-0032 tests, zero-init render test extended.
- **Test config SSoT.** `gc-test-utils::reference_config` is the only reader of `infra/services/gc-service/config.env` (strict parser); integration tests and the harness extend their vars from it; in-crate unit tests use explicit test-local values.
- **Docs.** DATABASE_SCHEMA §8 mirrors the migration; runbook rows incl. the ops O-A change procedure, CrashLoop line and O-B recovery note; TODO: :971 closed, `gc_registered_controllers` Observability Debt entry (O9 ruling), MH ticker-rebuild follow-up.
- Paired-test authored `crates/env-tests/tests/03_gc_deployment_config.rs`, the `mh_config.rs` doc rows, the `deployment_config.rs` header line and the test-gates.md "GC freshness (GC-1)" subsection (two wording corrections applied by me: ms-proof slack, retention cap).

## Files Modified

See §Cross-Boundary Classification (every changed path is a row). New files: `crates/gc-test-utils/src/reference_config.rs`, `crates/gc-service/tests/mh_registry_metrics_integration.rs`, `crates/gc-service/tests/mh_health_checker_tests.rs`, `crates/gc-service/tests/common/mh_registry.rs`, `crates/env-tests/tests/03_gc_deployment_config.rs`.

`./scripts/layer-fast.sh`: green (layers 1–6 OK).

---

## Issues Encountered

- **Gate 2, Layer 7 attempt 1: all three `03_gc_deployment_config.rs` tests failed their precondition.** Right after the GC rollout one old-ReplicaSet GC pod was still Terminating, and `live_gc_pods()` refused a non-settled pod set at its first listing (correctly: a terminating predecessor could otherwise satisfy a pinned gauge read). Every other env-test passed. Cause: the env-test assumed Layer 7's rollout wait also waited for old pods to be gone; `kubectl rollout status` returns when the new ReplicaSet is Available, while old pods can still be inside their grace period. Fix (paired-test, `crates/env-tests/src/fixtures/kube.rs`, `crates/env-tests/tests/03_gc_deployment_config.rs`): one settle loop `settled_serving_pods` re-lists until every selected pod is serving, bounded by the live pods' max `spec.terminationGracePeriodSeconds` (35 s today, read from the cluster, not retyped) + a 15 s removal margin, failing loudly with the last verdict at the bound; the ConfigMap-generation check now runs on the same settled listing (removes a second-listing race). `live_mh_pods` shares the loop. No product code change.

## Code Review Results

All findings fixed (none deferred); `./scripts/layer-fast.sh` green after the fixes.

| Reviewer | Finding | Resolution |
|----------|---------|------------|
| operations | F1 missing-key symptom conflated (CreateContainerConfigError vs CrashLoopBackOff); deployment.yaml "admission" wording | Runbook triage table (two causes, `describe pod` vs `logs --previous`), link to mc-deployment.md §Config-failure triage; deployment.yaml comment → "fails container creation (CreateContainerConfigError)" |
| operations | F2 runbook cited `mh.sh upgrade`, not yet in tree | `kubectl rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower` (replaced by `mh.sh upgrade` with I-2); media-shedding warning + link to mh-deployment.md §Rollout With Media Flowing |
| code-reviewer | F1 `MhService::new -> Result<_, String>` | `MhRegistryConfig` stores validated `load_report_interval_ms: u64` (+ accessor); `MhService::new(pool, u64)` infallible; main.rs map_err removed |
| code-reviewer | F2 `as i64` in MC wrapper | `i64::try_from(..).unwrap_or(i64::MAX)` (saturate = never stale) |
| code-reviewer | F3 sweep failure logged twice | Tick no longer logs the sweep error; generic loop logs it once; module doc says so |
| code-reviewer | F4 new `#[allow]` lint attrs | Removed (unneeded in integration tests); new files carry none |
| code-reviewer | F5 idioms | `count_available_handlers` collects via `map(..).collect()`; selection `Vec::with_capacity`, redundant annotation dropped |
| dry-reviewer | F1 seed/ms/registry fixtures in 3 copies | `tests/common/mh_registry.rs` is the single home; DB-backed tick/sweep tests moved from `src/tasks/mh_health_checker.rs` to `tests/mh_health_checker_tests.rs` |
| dry-reviewer | F2 reference config re-parsed the keys | `MhRegistryConfig::from_vars` is the one parse path (Config::from_vars and gc-test-utils call it); `pub const MH_REGISTRY_KEYS` in config.rs, re-exported by gc-test-utils |
| paired-test | F1 `assert_delta(0)` for never-fires arms | `assert_unobserved()` |
| paired-test | F2 selection-size test used shipped N | Test-local N=2, asserts `== 2` |
| paired-test | F3 candidate-count invariant comment-only | Compile-time `const _: () = assert!(..)` against `config::MAX_MH_INITIAL_HANDLERS_PER_MEETING` (now `pub(crate)`); literal "(4)" dropped |
| paired-test | F4 bare `is_err()` | Matches `InvalidMhRegistryConfig` naming `GC_MH_REGISTRY_RETENTION_SECONDS` |

---

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | No GSA touched |
| Test (paired) | RESOLVED-FIXED | 4 | 4 | 0 | Env-test runs at Layer 7 |
| Observability | CLEAR | 0 | 0 | 0 | O9 out of scope by Lead ruling → TODO §Observability Debt |
| Code Quality | RESOLVED-FIXED | 5 | 5 | 0 | |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | 2 extraction opportunities → TODO §Cross-Service Duplication |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | |
| Database | CLEAR | 0 | 0 | 0 | |

**DRY extraction opportunities** (`docs/TODO.md` §Cross-Service Duplication, "From DRY Reviewer — story 4 GC-1"):
- bounded-integer config parse helper duplicated in MH and GC config.rs
- four KEY=VALUE env-file parsers with three strictness policies

---

## Accepted Deferrals

- (none surfaced in this devloop)

Scope holds (not findings in this diff): `gc_registered_controllers` sum→max / media arm (task obligation (b), Lead ruling) → `docs/TODO.md` §Observability Debt; MH ticker rebuild on re-registration (MH out of scope) → `docs/TODO.md` §MC / MH Resource and Capacity Reporting.

---

## Gate 2 — Full Validation

| Attempt | Result | Notes |
|---------|--------|-------|
| 1 | L1-6 OK; **L7 FAIL** (`env-tests-failed`) | All 3 tests in `03_gc_deployment_config.rs` hit PRECONDITION gc-pods-unserving: an old-ReplicaSet GC pod was still Terminating after rollout. Test-design defect, fixed by paired-test (bounded settle loop in `kube.rs`, bound = live max `terminationGracePeriodSeconds` + 15 s). Consumed L7 attempt 1/2. |
| 2 | **TOTAL_RESULT=OK** (L1-7 OK; L7 1373 s) | All three new GC env-tests pass. |

---

## Rollback Procedure

1. Start commit: `c6e569bf721663951afb0088bca8a8175a1503d7`
2. `git diff c6e569bf..HEAD`; revert the single commit.
3. No schema changes.
4. **Safe-revert unit**: the single commit (code, `config.env` keys, `deployment.yaml` refs, docs and tests together). The old binary ignores the extra ConfigMap keys; retention only ever deleted dead rows.
