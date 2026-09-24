# Devloop Output: MH egress-budget admission chain

**Date**: 2026-09-24
**Task**: MH egress-budget admission chain — required budget/cost/threshold keys, derived stream ceiling (boot floor 2, advisory min 30), admission check + gauges, §8 keys to required, `MH_MAX_STREAMS` code retirement (story 2026-09-21-hear-each-other task 8; R-19, R-22, R-23)
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: ~4h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f16be87cd503f7c358146c5b342a17f8cf706b04` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired | `paired-global-controller` |

---

## Task Overview

### Objective
See task prompt: `/home/dev/.cache/devloop/story-runs/story-runner/2026-09-21-hear-each-other/task-8.prompt` (story `docs/user-stories/2026-09-21-hear-each-other.md`, task 8).

### Scope
- **Service(s)**: mh-service (GC is a consumer of `max_streams`; no GC code change expected)
- **Schema**: No
- **Cross-cutting**: Yes (metric catalog, env-tests)

### Debate Decision
NOT NEEDED - design settled in story + ADR-0036 §11.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/session/admission.rs` (new) | Mine | — |
| `crates/mh-service/src/grpc/gc_client.rs` | Mine | — |
| `crates/mh-service/src/grpc/mh_service.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/src/lib.rs` | Mine | — |
| `crates/mh-service/src/routing/mod.rs` (test fixture `PolicyLimits::for_tests` only) | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` (test constructor calls only) | Mine | — |
| `crates/mh-service/tests/**` (constructor updates + new `tests/stream_admission_integration.rs`) | Mine | — |
| `crates/mh-test-utils/src/**` (admission fixture) | Mine | — |
| `docs/observability/metrics/mh-service.md` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` (one clause the regex widening made false) | Not mine, Minor-judgment | dry-reviewer |
| `crates/gc-service/src/grpc/mh_service.rs` (`max_streams` `i32::try_from` + unit test) | Not mine, Minor-judgment | global-controller (paired) |
| `crates/dt-guard/src/env_config.rs` (widen the discovery matcher to tolerate a wrapped call site; no policy content) | Not mine, Minor-judgment | infrastructure |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (new, S9 MH half) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` (append `LOGGED_POLICY_BOUNDS`; ceiling + recommended-min gauge parity) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` (presence anchor for the new series) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/metrics.rs` (`gauge_by_instance_present` helper) | Not mine, Minor-judgment | test |
| `infra/grafana/dashboards/mh-media.json` (minimal panels, guard-required) | Not mine, Minor-judgment | observability |
| `docs/observability/dashboard-conventions.md` (flip forward references to live) | Not mine, Minor-judgment | observability |
| `docs/observability/dashboards.md` (§MH Media Path panel rows) | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mh-alerts.yaml` (description clause + rationale comment, TODO:482) | Not mine, Minor-judgment | operations |
| `docs/observability/alerts.md` (same falsified premise, one sentence) | Not mine, Minor-judgment | observability |
| `docs/runbooks/mh-deployment.md` (required-key count, `MH_MAX_STREAMS` tense, bake-gate regex note) | Not mine, Minor-judgment | operations |
| `docs/runbooks/gc-incident-response.md` (Scenario 3: second cause of new-meeting 503) | Not mine, Minor-judgment | global-controller (paired) |
| `infra/services/mh-service/configmap.yaml` (comment tense only; no key/value change) | Not mine, Minor-judgment | operations (infrastructure not on panel) |
| `infra/services/mh-service/mh-0-deployment.yaml` (comment tense only) | Not mine, Minor-judgment | operations (infrastructure not on panel) |
| `infra/services/mh-service/mh-1-deployment.yaml` (comment tense only) | Not mine, Minor-judgment | operations (infrastructure not on panel) |
| `docs/DEVELOPMENT.md` (anchor comment: "ten keys" -> the eight this task makes required) | Not mine, Minor-judgment | operations |
| `docs/TODO.md` (close :480 and :482; update :1373 hand-off) | Mine | — |

---

## Planning

**Mechanism restatement.** (1) *One configured number, one home, one reader-set*: every key THIS task makes required is one for which no code default can be correct, so the ConfigMap becomes its single home. Every published config gauge reads the exact field its enforcer reads. The GENERAL rule on required-vs-defaulted keys is NOT restated here. It lives in `infra/services/mh-service/mh-0-deployment.yaml` (the "optional-with-default in code and hard-referenced anyway … DO NOT flip them" block), which is cited rather than forked. Bind addresses, region, `GC_GRPC_URL` and the latency ratio stay defaulted by that rule (DRY Gate-1 A). (2) *A capacity figure advertised to GC is the same value MH enforces*: the derived stream ceiling is computed once at load and that one field feeds admission, the `max_streams` register field, the gauge and the startup log. (3) *A configuration that boots must be able to serve*: the derived ceiling below the smallest real meeting (2) is refused at boot, as is a ceiling the resource guard behind it can never let MH reach.

Wider-class instances checked: `MH_MEDIA_LATENCY_SAMPLE_RATIO` is also in the ConfigMap with an equal code default, but it is a **reasoned carve-out** (both deployments declare it the one deliberately `optional:` ref, so the image may roll back alone). It stays optional, and a one-line note at its read in `config.rs` says so. `MH_MAX_REGISTERED_MEETINGS` / `MH_MAX_MUTED_SOURCES_PER_MEETING` have no Rust reader at all and are assigned to tasks 11 / 10. They stay OUT of scope, and I correct the prose sites that say this task makes them required (`configmap.yaml` "reads all six", `DEVELOPMENT.md` "ten story-2 keys").

### Config (`config.rs`)
- New `EgressAdmission` struct (capacity, deliberately NOT in `PolicyLimits`, whose contract is "resource exhaustion, never capacity"). Fields: `budget_bytes_per_second`, `stream_cost_audio_bytes_per_second`, `stream_cost_video_bytes_per_second`, `stream_ceiling: u32`, `rejection_ratio_threshold: f64`, plus the raw `*_bps` bit values, kept for the provenance log line only.
- Four new REQUIRED reads. Each keeps its own literal `.ok_or_else(|| ConfigError::MissingEnvVar("MH_..."))` at the call site, because the `dt-guard env-config` rule-1 regex keys on those literals, and no example construction appears in prose.
  - Integers go through `parse_required_number` + `reject_zero`. Zero costs are refused, so the divisor is non-zero by construction (no `checked_div` downstream).
  - The threshold goes through ONE extracted ratio parser, `parse_unit_ratio(key, raw)`. The existing `MH_MEDIA_LATENCY_SAMPLE_RATIO` block is rerouted through it, so the operator message has one copy. The range check is `!(0.0..=1.0).contains(&parsed)`, which rejects NaN and ±inf. The presence check stays at the call site.
- **Bits → bytes exactly once**, in `from_vars`, before anything else reads the values. Budget uses `bits / 8` (floor); costs use `bits.div_ceil(8)` (ceil). Rounding goes toward FEWER streams in both cases, so the result fails closed. Only the startup log still sees the bit values.
- **Derivation.** `ceiling = budget_bytes / max(cost_audio_bytes, cost_video_bytes)`, computed in `u64`. Using the worst-case cost keeps MH type-blind (§7).
- **Refuse-boot rules.** Each is a new fielded `ConfigError` variant naming the value, the bound and the remediation.
  - `EgressStreamCeilingBelowFloor`: fires when `ceiling < MIN_EGRESS_STREAM_CEILING` (2). The remediation is "raise `MH_EGRESS_BUDGET_BPS`; NEVER lower the floor". The comparison is `<`, so 0 and 1 refuse and 2 boots.
  - `EgressStreamCeilingExceedsEdgeBound`: fires when `ceiling > MH_MAX_TOTAL_EGRESS_EDGES`. The resource guard must sit at or above the capacity it backstops; otherwise GC is advertised a capacity MH refuses at the resource guard, which is the black hole in the other direction. This also gives the ceiling a code-level upper bound for free: `MAX_TOTAL_EGRESS_EDGES_CEILING` = 1,048,576. A `const _: () = assert!(MAX_TOTAL_EGRESS_EDGES_CEILING <= i32::MAX)` covers GC's `i32` column (security #3, GC gap 2). `u32::try_from` is used, never `as`.
- **Constants.** A `const fn all_hear_all_streams(peers: u64) = (peers + 1) * peers`. This is exactly the shape of the env-test's `required_edges(n)`, with `n` = the peers each participant hears, so both sites use ONE parameter convention (DRY Gate-1 C). The helper is MH-local in `config.rs`, never in `common`.
  - `MIN_EGRESS_STREAM_CEILING = all_hear_all_streams(1)` = 2: two participants each hearing the other, the smallest meeting this story defines.
  - `EGRESS_STREAM_CEILING_RECOMMENDED_MIN = all_hear_all_streams(RECOMMENDED_MIN_DEMO_N = 5)` = 30: N+1 = 6 participants each hearing N = 5 (test Gate-1 #3: N counts peers heard, not participants).
  - Doc comments state the two different consequences (refuse vs nudge). An ANCHOR (DRY) names `01_mh_deployment_config.rs::{DEMO_N, required_edges}` and the Kind patch as sibling encodings that cannot share a home (env-tests links no service crate).
  - A `const` assert plus a unit test: `FLOOR < RECOMMENDED_MIN` (they must never merge).
- **§8 keys flipped to required.** `parse_bounded` loses its `default` parameter, takes the already-presence-checked `&str`, and becomes a thin wrapper `parse_required_number → reject_zero → reject_above(ceiling)`. All eight integer keys therefore explain a zero or over-ceiling value with ONE message family and ONE remediation sentence. The paragraph in `reject_zero`'s docstring explaining why the §8 keys were kept apart ("in no manifest today") expired with this diff and is rewritten (DRY Gate-1 B). Each of the four call sites writes its own `MissingEnvVar` literal. `DEFAULT_MAX_{EGRESS_STREAMS_PER_MEETING,CANDIDATE_SOURCES_PER_EGRESS,TOTAL_EGRESS_EDGES}` and `DEFAULT_POLICY_APPLY_TIMEOUT_MS` are deleted, along with `impl Default for PolicyLimits`.
  - The `DEFAULT_MAX_TOTAL_EGRESS_EDGES` docstring MOVES intact to `MAX_TOTAL_EGRESS_EDGES_CEILING` (TODO:1373 hand-off).
  - The module header prose ("each bound is optional-with-default … `Config::max_streams` is a capacity figure") is rewritten.
  - The `reject_zero` docstring's restated count ("all three") is reworded to name no count.
- **`MH_MAX_STREAMS` retirement.** Delete the read, `DEFAULT_MAX_STREAMS` and `Config::max_streams`. Add a module note: `media_handlers.max_streams DEFAULT 1000` (`migrations/20260124000001_mh_registry.sql`) is an intentionally retained, unreachable default, because register always binds the derived ceiling and GC's upsert overwrites it.

### Admission (`session/mod.rs` + new `session/admission.rs`)
- The actor is constructed with a `StreamAdmission { stream_ceiling, rejection_ratio_threshold }`. `SessionManagerHandle::new(admission)` / `new_with_parts(admission)` take it; a handle without a ceiling is not representable. Test sites use an `mh-test-utils` fixture.
- Branch order in `handle_config_apply`: `stale → equal (no-op, never counted, never able to trip capacity) → STREAM CEILING → aggregate edge cap → install`.
  - The ceiling check comes first because the boot invariant (`ceiling <= edge cap`) makes every over-edge-cap projection also an over-ceiling one. Every generation-advancing apply is therefore exactly one admission decision, and the edge-cap branch stays as a defense-in-depth backstop, documented as unreachable while the boot invariant holds. **Its tests are RETAINED, not retargeted** (security Gate-1 item 2). The existing `EdgeCapExceeded` cases build the actor with a `StreamAdmission` whose ceiling sits ABOVE the edge cap. `from_vars` refuses that state, so these are deliberately labelled backstop tests. A comment at the branch names both them and the `EgressStreamCeilingExceedsEdgeBound` boot test as the coverage that keeps it honest. A line at the check also says that `edges.len()` counts egress streams (one per `egress_stream_id`), not candidate fan-out.
  - **Quantity checked**: `snapshot.projected_total_edges(&policy)`, the same function the edge cap and `with_policy` use, so there is no second accounting. Egress streams are aggregated across all meetings on this handler, the same unit as `max_streams`. It is all-or-none: no swap, prior generation stays live.
  - The WARN on rejection carries `installed_total_streams`, `installed_meeting_count`, `projected_total_streams` and `stream_ceiling`. These are identity-free and answer "one fat policy vs accumulated dead meetings" (security #4). The ratchet (no teardown until task 11) is stated at the check.
- New `ApplyFailure::StreamCeilingExceeded` (reason `egress_stream_ceiling_exceeded`) maps to a NEW `PolicyApplyOutcome::RejectedStreamCeiling` (`rejected_stream_ceiling`), so `ALL` becomes 6 (operations #10). It gets its own label because the remedy differs: capacity, not an MC bug or an MH fault.
- `mh_media_stream_admission_total{outcome=admitted|rejected_stream_ceiling, key_custody=operator}` uses a `StreamAdmissionOutcome` enum with `ALL` and `as_label`, and is zero-initialised in `zero_initialize_counters()`.
- **Rejection ratio.**
  - `AdmissionWindow` in `session/admission.rs`: a pure, unit-testable, fixed-size ring of `WINDOW_BUCKETS = 30` × `BUCKET = 10 s`, which gives a **5-minute sliding window**, never a lifetime ratio.
  - The actor `select!` gains a `tokio::time::interval(BUCKET)` arm that rotates a bucket and republishes, so the gauge CLEARS when decisions stop. The loop still exits when both mailboxes close: explicit open-flags replace the `else => break`.
  - 0 decisions publishes **0.0** (never NaN, never 1.0). Zero admitted with any rejected publishes 1.0.
  - The actor publishes 0.0 at start, so the series exists from boot (operations #4).
- **MH-side reader of the threshold** (the same field the gauge publishes).
  - The actor logs one WARN when the windowed ratio exceeds the threshold, but only once the window holds at least `MIN_DECISIONS_FOR_THRESHOLD_LOG` (10) decisions. That evidence floor is a different rule from the 0/0 → 0.0 gauge convention.
  - It re-arms and logs INFO only when the ratio falls to at most threshold/2 or the window empties (hysteresis).
  - The log site and the catalog both say this line is a debugging breadcrumb, and that the Prometheus alert is authoritative for paging (observability O-4).
- Nothing is wired into readiness (operations #5). Nothing new goes under `src/media/`.

### Advertisement (`gc_client.rs`)
- `max_streams` = `config.egress_admission.stream_ceiling` at both `register()` and `attempt_reregistration()`. It is the same field the gauge and admission read.
- **GC gap 1 (paired-GC):** the load report's `current_streams` becomes `routing_table.load().total_edges()` (admitted egress streams, same unit), via `u32::try_from` saturating. `GcClient::new` takes the `Arc<RoutingTable>`. Without this, GC's soft filter `current_streams < max_streams` is always true.
- GC (`crates/gc-service/src/grpc/mh_service.rs`): BOTH truncating casts become `i32::try_from` → `InvalidArgument`, with unit tests for both beside the zero case. The first is `max_streams` at register. The second is `current_streams` in the load report, which is the sibling field that GC gap 1 makes live (security Gate-1 item 1). A wrapped-negative `current_streams` would make an exhausted handler sort as least-loaded, so it FAILS OPEN. Rejecting it is the decision, rather than saturating: MH's own value is bounded by the ceiling (<= 1,048,576), so an overflow means a malformed or foreign report, and refusing it loudly beats placing against it.

### Observability (`metrics.rs`, `main.rs`)
- Series (all `key_custody=operator` via `common::observability::labels`, names are string literals at the registration site):
  - `mh_media_egress_budget_bytes_per_second{basis}`: `basis` is a `&'static str` const `EGRESS_BUDGET_BASIS_UNMEASURED` whose doc names the ADR-0036 open item.
  - `mh_media_egress_stream_ceiling`
  - `mh_media_egress_stream_ceiling_recommended_min`
  - `mh_media_stream_admission_rejection_ratio_threshold`
- The static config gauges are published by `publish_egress_admission(&EgressAdmission)` AFTER `init_metrics_recorder()`, following the `publish_sample_ratio` precedent (observability ordering note). The ratio gauge is owned by the actor, which resolves its handles once at construction; the session manager is built after the recorder.
- **Startup log.** `"Configuration loaded successfully"` gains `egress_budget_bps`, `egress_budget_bytes_per_second`, `stream_cost_audio_bps`, `stream_cost_video_bps` (plus bytes), `egress_stream_ceiling`, `egress_stream_ceiling_floor`, `egress_stream_ceiling_recommended_min`, `egress_rejection_ratio_threshold`, `media_latency_sample_ratio` and its `_source` (env|default).
  - Provenance: every required key is necessarily `env`, and the comment says so. The one optional ConfigMap key logs its source.
  - `max_streams` is removed from the line.
  - The emission site is marked TEST-LOAD-BEARING. The §8-fields justification is rewritten (provenance + env-test contract).
  - A separate loud INFO "Egress stream ceiling derived …" line, and, when `ceiling < recommended_min`, a distinct INFO "configure me" nudge naming `MH_EGRESS_BUDGET_BPS` (never WARN).
- **Catalog.** `docs/observability/metrics/mh-service.md` gets a new `## Egress Admission Metrics` section, plus the `rejected_stream_ceiling` row in `mh_media_policy_applies_total`. It records:
  - the ratio window;
  - the 0/0 convention;
  - that the gauge is authoritative over the PromQL `rate` form, and why the two differ;
  - "no panel or alert may select on `basis`";
  - that `_recommended_min` is advisory and is never an alert input;
  - bits (key) vs bytes (gauge) as a deliberate single conversion at load.
- **Dashboards.** Minimal panels in `mh-media.json` (the guard requires them now; units `Bps` / `percentunit` / `short`). The admission counter panel uses `increase($__rate_interval)` with unit `short`, never `rate()`: a single rejection must be visible, which is the board's own recorded reason (O-2). `docs/observability/dashboards.md` §MH Media Path gains one row per new panel, filled in with the question each answers, and states that the new series preserve the board's "empty = healthy, eager registration" invariant (O-1). Observability's task 16 lays the panels out properly.
- **Counter identity (O-3), stated in both catalog entries.**
  - `mh_media_policy_applies_total` counts every post-boundary registration. Stale applies count `rejected_stale` only; equal-generation re-asserts count `applied` only.
  - `mh_media_stream_admission_total` counts one decision per generation-advancing apply the ACTOR processed.
  - So `sum(stream_admission)` equals the generation-advancing, actor-processed subset of `sum(policy_applies)`. The difference is stale, equal-generation, `no_generation`, `rejected_invalid` and pre-actor failures (mailbox full, actor gone). A timeout can count both an admission decision and `apply_failed`.
  - A ceiling rejection increments `rejected_stream_ceiling` on both counters: same event, different denominators. Do not sum them.
- **Scrape vs bucket (O-5, acknowledged).** The deployed scrape is 15 s and the bucket is 10 s. A sub-30 s excursion can be invisible on the ratio gauge. The S9 env-test therefore asserts rejection on the COUNTER, never on a transient ratio value; for the ratio it asserts presence and range only. Neither interval is tuned to the other.

### Tests
- **Unit (`config.rs`):**
  - derivation;
  - ceiling 0 / 1 refuse and 2 boots (built by choosing budget = k × cost);
  - above-edge-bound refuses;
  - bits converted once (budget bytes = bits/8, ceil on costs, ceiling from bytes);
  - absent + malformed refusal for ALL EIGHT flipped/new keys;
  - NaN / inf / >1 / <0 threshold refused;
  - `FLOOR < RECOMMENDED_MIN`;
  - deployed ConfigMap values (base 10M/90k/2.5M, which gives 4, and Kind 100M, which gives 40) boot, and base nudges.
- **Unit (`admission.rs`):** window 0/0 = 0.0, all-rejected = 1.0, rotation clears, threshold crossing edges.
- **Integration (`tests/stream_admission_integration.rs`, `MetricAssertion`):**
  - admitted and rejected counted by outcome, with the positive control;
  - the prior generation stays live on rejection;
  - an equal-generation re-assert at a full handler is not counted and not rejected;
  - the ratio gauge value;
  - each static gauge == its `Config` field;
  - `policy_applies{rejected_stream_ceiling}`;
  - gc_integration: `max_streams == stream_ceiling`, and `current_streams == installed streams`.
- **Env-test `28_mh_egress_admission.rs` (S9 MH half, paired with test).**
  - No service credential or MH gRPC port is available to env-tests, so it drives past the ceiling through REAL GC/MC joins.
  - It reads `mh_media_egress_stream_ceiling` per instance from Prometheus (fail-loud, empty vector = FAIL) and picks `P_h` = the smallest h with `h × (h−1) > ceiling`, then joins `P = 2·P_h − 1` distinct users. This exceeds the ceiling on at least one handler under round-robin placement (today) AND under task 20's co-location. Each participant declares `S = min(P−1, MC_MAX_RECEIVE_SLOTS)` slots, with the cap read from the deployed MC ConfigMap, never a literal. P is the smallest value that satisfies BOTH of these conditions, so the test does not depend on GC's weighted-random draw or on how MC spreads streams over a set of at most two handlers:
  - (i) Participant pigeonhole, for today's co-handler-only visibility (task 6): `P ≥ 2·P_h−1`, which puts at least P_h participants on one handler, and `P_h·min(S, P_h−1) > ceiling`.
  - (ii) Stream pigeonhole, for task 20's any-shared-handler routing: every subscriber fills S slots, so the meeting totals `P·S` streams; with `P·S > 2·ceiling`, one handler receives more than the ceiling under any split.

  Kind check (ceiling 40, S 8): P = 13 gives 7·6 = 42 > 40 for (i) and 13·8 = 104 > 80 for (ii).
  - Every participant opens an MH connection to every handler URL it was given, so task 20's "visibility follows observed connectivity" also sees full connectivity.
  - ENV-SANITY assertions, read live from the ConfigMap: `MH_MAX_EGRESS_STREAMS_PER_MEETING >= P·S` and `MH_MAX_TOTAL_EGRESS_EDGES >= ceiling`. Together with the branch order (the ceiling check precedes the edge cap), these guarantee the STREAM-CEILING guard is the one that trips, and the positive control is keyed on the exact `rejected_stream_ceiling` label.
  - Failures are phase-labelled (PRECONDITION / JOIN-FANOUT / ADMISSION), so a flaky join is never triaged as admission logic.
  - 13 AC registrations sit inside the suite's 100/min AC budget.
  - It asserts:
    - `admitted` rose (positive control) AND `rejected_stream_ceiling` rose on some instance, via `poll_until_any_instance_above`, a rig bound and never a sleep;
    - the ratio and threshold gauges are present on every instance;
    - the threshold is in [0,1] and equals the deployed ConfigMap value.
  - Participants then leave in order, so MC re-pushes shrinking policies and the handler's streams are released (no teardown until task 11).
  - Kind: ceiling 40 → P_h = 7 → 13 users.
- **Env-test `01`:**
  - append the budget/costs/threshold rows to `LOGGED_POLICY_BOUNDS` (one table);
  - add a gauge-parity test: `mh_media_egress_stream_ceiling` == the derivation, which closes TODO:480 (a); `_recommended_min` == `required_edges(DEMO_N)`.
- **Env-test `32`:** presence anchor for the new series.

### Docs / TODO
- `mh-alerts.yaml`: the trailing clause becomes a pointer (TODO:482).
- `mh-deployment.md`:
  - required-key count claim de-numbered;
  - the `MH_MAX_STREAMS` row tense flipped to "nothing reads it";
  - a bake-gate note on why `rejected_stream_ceiling` is excluded, in the `no_generation` shape.
- `configmap.yaml`: comment tense only, no key or value change. Sites: the `MH_MAX_STREAMS` "still live on the wire" block; the egress block's "reads all six as REQUIRED" (this task makes four required; the two capacity keys are tasks 11/10); the §8 block's "the running image still reads these optional-with-default" and its four ANCHOR (DRY) "until that flip" clauses; the "lands with the MH code task" pointers on the budget, costs and threshold; and the `MH_MAX_TOTAL_EGRESS_EDGES` block's "There is no saturation signal yet: the `mh_media_egress_edges` / `mh_media_egress_edges_limit` gauges and alert … land later in story 2", which splits — `mh_media_egress_edges` lands here, `_limit` and the alert do not. The `MH_MAX_REGISTERED_MEETINGS` block's "land with the MH code task" phrasing is re-checked so it does not read as a claim about this commit.
- `dashboard-conventions.md`: forward references flipped to live.
- `DEVELOPMENT.md`: anchor count.
- TODO:480 and TODO:482 closed; TODO:1373 hand-off marked done.
- **Pushed back:** the new incident-response scenario and the CrashLoop partition rows are story task 18 (operations, which depends on this task). No runbook scenario is authored here.

**Ratchet, quantified (operations Gate-1 blocker, resolved with option (b)).** Normal turnover does NOT ratchet. Every roster removal goes through MC's single choke point `remove_and_broadcast_left` (`crates/mc-service/src/actors/meeting.rs`), whether the cause is an explicit leave, a clean close or grace expiry. That function calls `media.reconcile(Affected::All)`, which re-renders and re-pushes EVERY handler. Once a handler holds at most one participant its policy is 0 streams, so a meeting that ends by its participants leaving returns its streams to 0 on MH. What ratchets is an ABNORMAL end: an MC crash or lost push, a meeting actor cancelled before its last push confirms, or a rejected push whose prior generation stays live. So time-to-exhaustion in Kind is not "four meetings". It is the number of abnormal endings per pod lifetime, and an MC restart during the suite is the realistic trigger. Because that path is real and silent, this task lands the saturation signal with the wiring:
  - `mh_media_egress_edges` (installed egress streams, the same `total_edges()` value that `current_streams` reports) is published by the actor at construction as 0 and then on every install. It uses task 11's already-chosen name (fixed by the story's "Names fixed by this plan" line), landed early; task 11 adds `_limit` and `registered_meetings`, and its hand-off is noted.
    - **N-2:** the 0-at-construction publish is what keeps the `mh-media.json` "empty panel = healthy, eager registration" invariant true on the same board this diff documents. After boot it updates ONLY on install: normal turnover re-pushes and drives it down, an abnormal end leaves it legitimately high, and no decay or freshening tick is added — the gauge telling the truth about the ratchet is why it lands early.
    - **N-1, in the catalog:** `mh_media_egress_edges`, `mh_media_egress_stream_ceiling` and (from task 11) `mh_media_egress_edges_limit` share ONE unit, the aggregate installed egress streams on this pod. The two bounds are different controls with different remedies — capacity (raise `MH_EGRESS_BUDGET_BPS`) versus resource guard (`MH_MAX_TOTAL_EGRESS_EDGES`) — not a duplicate. The boot invariant `stream_ceiling <= edges_limit` is what makes the capacity bound the one that binds, so a saturation panel compares against the CEILING. `_limit` lands in task 11, so no saturation alert may be written against it yet.
    - **N-3:** a component test asserts the published gauge equals the value placed in the load report. This covers runtime-state forking two ways (gauge and wire), which the config-gauge tests do not.
    - **N-4:** its `dashboards.md` row's "question it answers" column carries the ratchet caveat and the rollout-restart recovery.
  - The rejection WARN and the catalog state the interim recovery: `kubectl rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower`, until task 11's teardown.
  - `ConfigError` refusal text is the runbook until task 18 lands.

**Risk flagged:** until task 11 teardown, admission counts streams of ended meetings (the ratchet). Kind's ceiling of 40 per handler means a suite that leaves many never-shrunk meetings on one pod can reach it; the S9 test cleans up after itself by leaving. **GC consequence (paired-GC):** once `current_streams` is real, GC's soft filter engages. A handler that has ratcheted to its ceiling drops out of `get_candidate_mhs`, and when every handler in the region is in that state, the FIRST join to a NEW meeting fails at GC with ServiceUnavailable (503). Joins into existing meetings are unaffected (R-6 stickiness). This is a behaviour change: before, GC always placed and only media failed. It is the designed R-19 shape, and it is reachable on Kind during a long env-test run until task 11 lands. The S9 test fails with a distinct PRECONDITION message ("all MHs at stream ceiling before S9 started; ratchet until task 11") if its first join returns 503. **Placement assumption:** GC's weighted-random selection (`mh_selection.rs:weighted_random_select`, up to 2 handlers) chooses WHICH handlers form the meeting's set. The split of participants across that set is MC's round-robin, so with a 2-handler set the larger half holds P_h of 2·P_h−1, and with a 1-handler set it holds all of them. Either way it exceeds the ceiling.

### Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired global-controller | confirmed |

---

## Implementation Summary

- **Config** (`config.rs`):
  - `EgressAdmission::derive`: four REQUIRED keys, bits converted to bytes once (the budget floored, each cost ceiled), producing one `stream_ceiling: u32`.
  - Two fielded refusals: `EgressStreamCeilingBelowFloor` (ceiling < 2) and `EgressStreamCeilingExceedsEdgeBound`.
  - `all_hear_all_streams`, plus the separate constants `MIN_EGRESS_STREAM_CEILING` (2) and `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` (30). A const assert keeps floor < min, and another keeps the edge ceiling ≤ `i32::MAX`.
  - The four §8 keys are now required. Their defaults are deleted, and `parse_bounded` is a thin wrapper over `parse_required_number` → `reject_zero` → `reject_above`.
  - One shared `parse_unit_ratio` (the `contains` form, which rejects NaN).
  - `MH_MAX_STREAMS` / `DEFAULT_MAX_STREAMS` / `Config::max_streams` deleted, with the retirement and GC `DEFAULT 1000` note.
  - `media_latency_sample_ratio_source`.
- **Admission** (`session/admission.rs`, `session/mod.rs`):
  - `StreamAdmission` is required by `SessionManagerHandle::new`; the `Default` impl is removed.
  - Branch order: stale → equal → stream ceiling → edge cap (a labelled backstop) → install. Exactly one admission decision is counted per generation-advancing apply.
  - A 5-minute bucketed window rotated by a tick arm; the `select!` loop exits via open-flags.
  - A hysteretic threshold breadcrumb.
  - The `mh_media_egress_edges` gauge is published at construction and on every install.
- **Outcome/metrics** (`metrics.rs`):
  - `PolicyApplyOutcome::RejectedStreamCeiling` and `StreamAdmissionOutcome`, both zero-initialised.
  - `publish_egress_admission` (called in `main` after the recorder) and `resolve_admission_handles`.
- **GC**:
  - `gc_client` advertises `stream_ceiling` as `max_streams` and reports installed streams as `current_streams`.
  - `gc-service` uses `i32::try_from` for both fields.
- **Startup log**: the budget, both costs (bits and bytes), ceiling, floor, recommended min, threshold and latency-ratio source. The emission is marked TEST-LOAD-BEARING. Separate INFO lines carry the ceiling and the configure-me nudge.
- **Tests**:
  - Config unit tests: ceiling 0 / 1 / 2, all eight keys absent or malformed, NaN / inf, bits-once, the deployed base and Kind values.
  - `admission.rs` unit tests.
  - `mh_service` ceiling tests, with the edge-cap tests kept as labelled backstops.
  - `tests/stream_admission_integration.rs`.
  - A `gc_integration` `current_streams` == gauge test.
  - Env-tests 01 (log rows, ceiling / recommended-min gauge parity), 28 (S9), and 32 (presence).
- **Docs**:
  - Catalog section.
  - `mh-media.json` panels plus the `dashboards.md` rows.
  - `dashboard-conventions` flips.
  - `mh-alerts.yaml` description and rationale comment, and `alerts.md`.
  - `mh-deployment.md` (counts, retired row, bake-gate note, required-keys paragraph).
  - Comment-tense fixes in `configmap.yaml` and the deployments.
  - `DEVELOPMENT.md` anchor.
  - TODO: 480 and 482 closed, the 1373 hand-off marked done, and a note on the reclamation entry.
  - INDEX.

- **`dt-guard env-config` blindness, found by @operations at implementation and fixed in-loop.** Nesting the four §8 reads pushed two call sites past rustfmt's width; the formatter wrapped them between `MissingEnvVar(` and the string literal, and the guard's regex — which requires the two to be contiguous — silently stopped discovering `MH_MAX_EGRESS_STREAMS_PER_MEETING` and `MH_MAX_CANDIDATE_SOURCES_PER_EGRESS` while still printing `STATUS=OK`. Replicated, then fixed two ways:
  - each raw value is bound at STATEMENT level so the `MissingEnvVar` construction and its string literal stay on the SAME LINE — which is the whole requirement; the surrounding `.ok_or_else(...)` chain may wrap freely, and does (no `#[rustfmt::skip]`, which would leave the fragility in place). The reason is recorded at the site, with the rule stated no more strictly than the guard actually needs (@operations nit: an instruction stricter than the requirement, and unmet by the code beneath it, teaches the next author to distrust the comment);
  - `the_env_config_guard_can_see_every_required_key_in_this_file` replicates the guard's contiguity requirement over `include_str!("config.rs")` for all eight keys, with a positive control and a "do not delete the key from the list" instruction. Verified anti-false-green: re-wrapping one call site makes it fail naming that key; restoring it passes.
  - The durable fix landed in-loop on the Lead's ruling: `MISSING_ENV_VAR_RE` in `crates/dt-guard/src/env_config.rs` now tolerates whitespace after the paren (`MissingEnvVar\(\s*"…`), so no service can be silently dropped by a formatter. It stays anchored on the literal, because a `require(vars, key)` helper would erase every literal and no regex could see through it. `extract_required_env_vars_finds_missing_env_var` was EXTENDED (not duplicated) with the real wrapped shape plus the contiguous positive control.
  - **Verified, not assumed.** The wrapped fixture yields nothing under the OLD pattern and the key under the new one, so the test defends the fix rather than documenting it. The discovered set per service is byte-identical before and after widening (ac 2, gc 3, mc 15, mh 21), so the wider matcher invents no findings — the standard this guard sets for itself.
  - One self-inflicted instance of the same trap, caught by running the guard rather than by reading: the new test's doc comment originally wrote an illustrative `MissingEnvVar` construction, and the guard — which reads comments — reported a required variable named `KEY` that no manifest declares. The prose now describes the construction without instantiating it, which is the rule `parse_required_number` already records.

Self-check: `./scripts/layer-fast.sh` passes (L1–3 and L5 OK; L4 and L6 N/A in aggregate; the cargo-test, clippy and audit lanes are OK).

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | kubectl restart command removed from MH code (point, don't restate); debug_assert on admission window cursor |
| Test | CLEAR | 0 | 0 | 0 | 0/1/2 floor boundary, all 8 required keys, S9 env-test deterministic |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | MissedTickBehavior::Skip removed; latch log fields; catalog `admitted` row |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | env_config.rs classified Minor-judgment; table reconciled; lint rationale; test rename |
| DRY | RESOLVED-DEFERRED | 4 | 3 | 1 | 14th kubectl clone in env-test 28 — deferred to the combined helper per TODO §Cross-Service Duplication rule |
| Operations | RESOLVED-FIXED | 7 | 7 | 0 | env-config guard blindness (rustfmt wrap) fixed 3 ways; runbook/alert prose |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |
| Paired global-controller | RESOLVED-FIXED | 2 | 2 | 0 | saturate_for_gc clamps to i32::MAX; gc-incident-response Scenario 3 |

Lead notes: the task prompt's `docs/observability/metric-catalog.md` does not exist (used `docs/observability/metrics/mh-service.md`; story lines ~427/~559 still name the phantom path). Dashboard panels landed here because `dt-guard application-metrics` check 5 has no exemption. The prompt's "ceiling-at-least-1" test wording was superseded by the floor of 2. Residual risk carried to task 11: ratcheted streams from abnormally-ended meetings now gate GC placement (503 on new meeting when all MHs full).

---

## Accepted Deferrals

- `docs/TODO.md` §Cross-Service Duplication — env-test kubectl helper: trigger fired (14th clone, env-test 28)
- `docs/TODO.md` (dt-guard env-config rustfmt entry) — fail-closed predicate still owed by infrastructure

---

## Gate 2 — Validation (Lead)

Attempt 1 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`): L1 OK, L2 OK, L3 OK, L4 N/A (proto has no test verb; cargo-test + nx-test OK), L5 OK, L6 N/A (audit dep gate; cargo/pnpm audit + buf breaking OK), L7 OK (env-tests-passed incl. `28_mh_egress_admission::test_mh_refuses_admission_past_the_deployed_stream_ceiling`, `01_mh_deployment_config` new gauge-parity tests; browser-e2e-passed). TOTAL_RESULT=N/A (self-justified N/A only), exit 0. Duration 1578s.

Attempt 2 (post-Gate-3 fixes, same command): L1 OK, L2 OK, L3 OK, L4 N/A, L5 OK, L6 N/A, L7 OK (env-test 28 S9 passed again; browser E2E passed). TOTAL_RESULT=N/A (self-justified), exit 0. Duration 1351s.

## Final Summary

Gate 3 complete; all reviewers CLEAR / RESOLVED-FIXED except DRY RESOLVED-DEFERRED (one accepted deferral, see §Accepted Deferrals). Phase = complete.
