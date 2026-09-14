# Devloop Output: Zero-init discrete-event counters + counter panel presentation + two guards

**Date**: 2026-09-10
**Task**: Fix counter-visibility defect (ADR-0036 story-1 demo): lazily-created `*_total` counters init directly to 1, so no 0→1 edge exists and `increase()` reads 0 forever. Two coupled fixes + two guards.
**Specialist**: observability
**Mode**: Agent Teams (v2) — full (escalated from `--light`: touches metrics instrumentation, guard crate, dashboards, ADR)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1bae02926d29f3425fec9705c5588706739047f4` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-4-8[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (Gate 3 cleared 2026-09-14 post-crash recovery; all 8 reviewers resolved; Gate 2 green on final tree) |
| Implementer | `implementer` |
| Implementing Specialist | `observability` |
| Iteration | `1` (+ Gate-3 fix pass during recovery) |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-DEFERRED` |
| Observability | `RESOLVED-FIXED` (reviewer slot; implementer is a separate observability agent) |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |
| Infrastructure | `RESOLVED-FIXED` (conditional reviewer — dt-guard machinery; paired co-implementer for the guard files) |

**FIX 2 noValue decision (user, 2026-09-10):** DROP `noValue:0`; keep empty-is-healthy descriptions (per observability's fail-loudly argument). GUARD 2 expected-empty rule = description-marker check only.

---

## Task Overview

### Objective
Discrete-event `*_total` counters are created lazily on first event and initialized directly to 1, so Prometheus never records a 0→1 edge and `increase()` reads 0 forever. Evidence: mc-overview Join Flow panels showed 0/No Data while `mc_session_joins_total=1` per pod; series first appeared already at 1, pods scraped 90+ min, `resets()=0`.

Four deliverables:
- **FIX 1**: Zero-initialize discrete-event counters at process start (describe_* + touch each enum-label combination via cached handle) for mc/gc/ac/mh. Genuinely unbounded label domains marked exempt-with-reason in catalog.
- **FIX 2** (CORRECTED by user 2026-09-10 — the original cumulative-sweep instruction was wrong and reviewers caught it): **Do NOT** sweep stat panels to cumulative `sum()` and **do NOT** pre-reject stat `increase()`. Once FIX 1 lands, range-scoped `increase($__range)` on stat panels is honest and is the correct presentation; cumulative `sum()` has a time-range blind spot and decreases on rolling restart. **Keep ADR-0029's existing `increase()` presentation as-is.** The kept part of FIX 2 (user ruled 2026-09-10 — see §FIX 2): **NO `noValue`**; the empty-is-healthy DESCRIPTION marker on catalog-declared expected-empty counters (after present-at-zero, `noValue` is dead code on zero-init'd counters and MASKS a fault on exempt ones), plus the two live window-violation fixes. ADR-0029 amendment = zero-init precondition + window-split enforcement + the empty-is-healthy DESCRIPTION convention, NOT a cumulative rule and NOT a noValue rule.
- **GUARD 1**: New dt-guard subcommand asserting every catalogued enumerable `*_total` has startup zero-init touching each label combo, unless catalog-exempt. Fixtures for fire + apply.
- **GUARD 2** (CORRECTED per FIX 2 — NOT cumulative): the existing `dashboard_panels.rs:592` counter_misuse arm (counter over *_total must be inside rate/increase/irate) stays UNCHANGED. Add two checks: (a) **window distinction** — a stat panel over *_total whose increase() window is not `$__range` fails; a timeseries over *_total whose window is not `$__rate_interval` fails; (b) **expected-empty** — a panel over a catalog-declared expected-empty counter must carry the empty-is-healthy description marker (NO `noValue` — user ruling; expected-empty read via the shared `common/metric_catalog` annotation reader both guards use, so they can't disagree). Rust unit-test fixtures per rule (wrong-window shape isolates the new rule from counter_misuse).

Constraint: **do not change metric semantics or add new metrics.**

### Scope
- **Service(s)**: mc, gc, ac, mh (metrics init); dt-guard crate
- **Schema**: No
- **Cross-cutting**: Yes — metric-presentation convention (observability-owned, ADR-0019 Pattern B) applied across four service crates + guard machinery + dashboards + ADR.

### Debate Decision
NOT NEEDED — ADR-0036 coverage doctrine + ADR-0029 presentation model already govern; this is an amendment + enforcement, not a new decision.

---

## Cross-Boundary Classification

<!-- Populated by implementer during planning; reviewers may upgrade. -->

Per launch note: the per-service zero-init touches are the observability-owned metric-presentation
convention (ADR-0019 Pattern B, observability = named convention author for metric taxonomy) — classified
Mine even though they live in each service's crate. Guard **machinery** (new module, clap arm, wrapper,
`lib.rs` mod) is infrastructure-owned per CLAUDE.md; the **policy content** those guards encode is
observability's. main.rs one-line startup wiring is the owning service's file (Mechanical).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/observability/metrics.rs` | Mine / Domain-judgment | — |
| `crates/gc-service/src/observability/metrics.rs` | Mine / Domain-judgment | — |
| `crates/ac-service/src/observability/metrics.rs` | Mine / Domain-judgment | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine / Domain-judgment | — |
| `crates/mc-service/src/main.rs` | Not mine / Mechanical | meeting-controller (1-line startup call) |
| `crates/gc-service/src/main.rs` | Not mine / Mechanical | global-controller (1-line startup call) |
| `crates/ac-service/src/main.rs` | Not mine / Mechanical | auth-controller (1-line startup call) |
| `crates/mh-service/src/main.rs` | Not mine / Mechanical | media-handler (1-line startup call) |
| `docs/observability/metrics/mc-service.md` | Mine / Domain-judgment | — (zero-init-exempt + expected-empty annotations) |
| `docs/observability/metrics/gc-service.md` | Mine / Domain-judgment | — |
| `docs/observability/metrics/ac-service.md` | Mine / Domain-judgment | — |
| `docs/observability/metrics/mh-service.md` | Mine / Domain-judgment | — |
| `docs/decisions/adr-0029-dashboard-metric-presentation.md` | Mine / Domain-judgment | — |
| `infra/grafana/dashboards/mc-overview.json` | Mine (ADR-0029 shape, GUARD-2-covered) / Minor-judgment — review-only | — (operations + code-reviewer on panel) |
| `infra/grafana/dashboards/gc-overview.json` | Mine (ADR-0029 shape, GUARD-2-covered) / Minor-judgment — review-only | — |
| `infra/grafana/dashboards/ac-overview.json` | Mine (ADR-0029 shape, GUARD-2-covered) / Minor-judgment — review-only | — |
| `infra/grafana/dashboards/mh-overview.json` | Mine (ADR-0029 shape, GUARD-2-covered) / Minor-judgment — review-only | — |
| `infra/grafana/dashboards/mh-media.json` | Mine (ADR-0029 shape, GUARD-2-covered) / Minor-judgment — review-only | — |
| `crates/dt-guard/src/counter_zero_init.rs` (new) | Not mine (machinery=infra) / Domain-judgment (policy=me) | infrastructure — `Approved-Cross-Boundary: infrastructure` trailer |
| `crates/dt-guard/src/dashboard_panels.rs` (extend) | Not mine (machinery=infra) / Domain-judgment (policy=me) | infrastructure — `Approved-Cross-Boundary: infrastructure` trailer |
| `crates/dt-guard/src/common/metric_catalog.rs` (extend: shared annotation reader — both guards depend on it) | Not mine (machinery=infra) / Domain-judgment | infrastructure — `Approved-Cross-Boundary: infrastructure` trailer (same pairing as the two guard modules, infra item 1) |
| `crates/dt-guard/src/metric_macros.rs` (doc/pin-msg currency only; frozen literal :274 UNTOUCHED) | Not mine / Minor-judgment | infrastructure (M4) |
| `crates/dt-guard/src/main.rs` (clap arm) | Not mine / Mechanical | infrastructure |
| `crates/dt-guard/src/lib.rs` (mod decl) | Not mine / Mechanical | infrastructure |
| `scripts/guards/simple/validate-counter-zero-init.sh` (new wrapper) | Not mine / Mechanical | infrastructure |
| `scripts/guards/counter-zero-init.test.sh` (new self-test) | Not mine / Minor-judgment | infrastructure (M6) |
| `scripts/layer3.sh` (wire self-test) | Not mine / Minor-judgment | infrastructure |
| `infra/docker/prometheus/rules/gc-alerts.yaml` (OPS-9 stale empty-series prose + "why it never fired" note) | Not mine / Minor-judgment | operations (ADR-0011) — `Approved-Cross-Boundary: operations` trailer |
| `docs/observability/alerts.md` (OPS-9 empty-series prose) | Mine / Minor-judgment | — |
| `docs/runbooks/gc-deployment.md` (OPS-9 prose, ops sign-off) | Not mine / Minor-judgment | operations — `Approved-Cross-Boundary: operations` trailer |
| `docs/runbooks/gc-incident-response.md` (OPS-9 prose) | Not mine / Minor-judgment | operations — `Approved-Cross-Boundary: operations` trailer |
| `infra/grafana/dashboards/client-media.json` (OPS-9 stale MH-board cross-ref) | Mine (ADR-0029 shape) / Minor-judgment — review-only | — |
| `infra/grafana/dashboards/errors-overview.json` (FIX 2 empty-is-healthy description marker) | Mine (ADR-0029 shape) / Minor-judgment — review-only | — |
| `crates/env-tests/tests/30_observability.rs` (Layer-7 present-at-0 env-test) | Mine (observability) / Minor-judgment | test (co-owned) |
| `docs/TODO.md` (dt-guard subcommand-split re-debate slug; dry AC/GC route-through-typed-vocab follow-up) | Not mine / Mechanical | infrastructure / dry |
| `docs/devloop-outputs/2026-09-10-counter-zero-init-visibility/main.md` | Mine / Mechanical | — |
| `crates/mc-service/src/grpc/auth_interceptor.rs` (Gate-3 F1 clamp) | Not mine / Minor-judgment — metric-label hygiene, ZERO auth-behavior change | meeting-controller |
| `crates/gc-service/src/grpc/auth_layer.rs` (Gate-3 F1 clamp) | Not mine / Minor-judgment — metric-label hygiene, ZERO auth-behavior change | global-controller |
| `crates/mh-service/src/grpc/auth_interceptor.rs` (Gate-3 F1 clamp) | Not mine / Minor-judgment — metric-label hygiene, ZERO auth-behavior change | media-handler |
| `crates/mc-service/src/grpc/mc_service.rs` | Not mine / Mechanical — one-line `rejection_reason_label` → `pub(crate)` for the RejectionReason drift witness (TEST-F1) | meeting-controller |
| `crates/common/src/service_type.rs` | Not mine / Minor-judgment — NEW clamp SSoT; `common/**` outside the §6.4 Guarded subset (observability-motivated metric-label vocabulary) | DRY + code-reviewer approve (common/ rule); DRY follow-up: ac `ServiceType` should derive from it, `docs/TODO.md` |
| `crates/common/src/lib.rs` | Not mine / Mechanical — one-line `mod service_type;` decl | DRY + code-reviewer (review-only) |
| `crates/mc-service/tests/auth_layer_integration.rs` | Mine (observability) / Minor-judgment — clamp e2e test | test (co-owned) |
| `crates/mh-service/tests/auth_layer_integration.rs` | Mine (observability) / Minor-judgment — clamp e2e test | test (co-owned) |

`crates/common/src/observability/testing.rs` is **NOT** touched (render-based completeness test avoids adding
a counter `assert_value`); if a reviewer insists on the MetricAssertion path instead, that row becomes
`common/**` outside the Guarded subset → needs DRY + code-reviewer approval (infra P4). No `dt-guard`
`cross-boundary-ownership.yaml` key is added (not a GSA; `gsa-sync` rejects stray keys — infra).

---

## Planning

### Problem in mechanism-language (wider than the task's nouns — flagged to reviewers)

The task's nouns are "discrete-event `*_total` counters" and "Join Flow panels." The **mechanism** is
narrower-sounding but wider-reaching: a `metrics`-crate counter series is created **lazily** — it first
appears in `/metrics` at the instant of its first `.increment(n)`, already at value `n`. No `0` sample
precedes it. `increase()`/`rate()` over a range need a `0→N` transition *inside* the range to report `N`;
a series born at `N` yields `increase()==0` (and STAT `increase($__range)` shows 0/No Data) until a
*second* event lands in-window. For a single low-volume event this is **forever**.

This is true of **every** lazily-created counter series, not only "Join Flow" or a special "discrete-event"
class. The fix is **present-at-zero**: emit a `0`-valued series for each label combination at process start,
so the first real event is a visible `0→1` edge. The scope boundary ("which counters") is therefore a
*policy* choice, not a property of the metric: we present-at-zero every counter whose **label domain is
finitely enumerable**, and exempt (with a stated reason) only those whose domain is genuinely
**unbounded/runtime-discovered** (raw `status_code`, free-form `operation`/`table`/`reason`) where
present-at-zero is impossible without fabricating values. GUARD 1 enforces exactly that policy boundary.

### FIX 1 — `zero_initialize_counters()` per service, called at startup

Add `pub fn zero_initialize_counters()` to each service's `observability/metrics.rs`; call it in `main.rs`
immediately after `init_metrics_recorder()?` returns Ok (recorder is globally live at that point; verified
call sites: gc main.rs:108, ac main.rs:77, mh main.rs:185, mc main.rs:185). For each in-scope counter it
does `describe_counter!(name, help)` (HELP/TYPE metadata; help sourced from the catalog one-liner) then a
`counter!(name, labels…).increment(0)` **touch per label combination**.

**Label-combination enumeration source (single source of truth):**
- **Media/enum-backed with `ALL`** (5 in mc: `PolicyPushOutcome`, `SenderBindingOutcome`, `CapabilityOutcome`,
  `DirectiveOutcome`, `MuteOutcome`; mh: `PolicyApplyOutcome`, `MediaSessionStartOutcome`, `MediaDropReason`,
  `MediaDirection`, codec `ALL_REJECT_REASONS`): iterate `Enum::ALL`, map via `.label()`/`.as_str()`,
  × `KEY_CUSTODY_OPERATOR`. Zero drift risk — derived from the type.
- **Enumerable, drift-proofed by category (converged: code-reviewer / dry C2 / security B / observability).**
  Every enumeration is derived from a wildcard-free source so a new variant fails the build or reds a guard —
  never a bare hand-maintained list ("manual discipline" reintroduces THIS defect, code-reviewer/semantic-4):
  - *Enum with `pub const ALL`* (5 mc media, mh's + codec `ALL_REJECT_REASONS`): iterate `ALL`, map via
    `.label()`/`.as_str()`. House style.
  - *Fieldless enum with an EXISTING typed label method, no `ALL`* (GC `MeetingRefusal::metric_label`; MC
    `ActorType::as_str`, `LeaveReason::label`, `DisconnectCause::label`): **team-lead ruling — enumerate
    IN-DOMAIN, no cross-boundary edit to the enum's file.** In the service's own `observability/metrics.rs`
    (mine), a `const VARIANTS: &[Enum] = &[Enum::A, Enum::B, …]` list iterated as
    `.iter().map(|v| v.label())` — the array holds **VARIANTS, never restated strings** (all four are
    fieldless `Copy` so this is legal in const context; `metric_label`/`label` are NOT `const fn`, only
    `ActorType::as_str` is, which is exactly why the array holds variants and applies the label fn *in the
    loop* — dry mechanical note). Label strings therefore defer to the sole method home (no 2nd encoding;
    string-drift is caught because strings come from the method), and a co-located wildcard-free
    exhaustive-`match` **witness** makes a new variant fail to compile *in metrics.rs* (compiler-enforced,
    code-reviewer + dry CONFIRMED in-domain, team-lead FINAL: no cross-boundary edit, Mine). **Witness is an
    index-map, not an or-arm (dry refinement):** `const fn slot(v: Enum) -> usize { match v { A => 0, B => 1,
    … } }` — both forms red on a new variant, but the index form makes *discharging* the error require
    thinking about a slot in `VARIANTS`, whereas `A | B => ()` is dischargeable by typing `| New` without
    ever looking at the array. **Witness must actually BITE (infra — verify before relying on it as the sole
    mechanism if leg-2b drops):** `slot()` is a `const fn` in **non-`#[cfg(test)]` code** (release compiles
    it), **wildcard-free**, matching **exhaustively over the enum value** — so a variant-add genuinely fails
    the build. A `_ =>` arm, a match on some other value, or a witness in a test-only block would make "we have
    a witness" true and load-bearing-free at once. I'll confirm it bites with a throwaway added variant before
    the leg-2b-vs-witness decision is final. **dry doc line at each witness:** "discharging this compile error
    also requires adding the variant to the array below." **team-lead mitigation-1 doc line:** note it
    deliberately diverges
    from the `PolicyApplyOutcome` house doctrine (`mh-service/src/observability/metrics.rs:256-271`) for
    ownership-containment, and that the **no-wildcard witness, not the bare list**, is what reds on
    variant-add (that distinction is why the divergence is safe). Hoist-to-`ALL` on the enums tracked in
    `docs/TODO.md` §Cross-Service Duplication for when global-controller + meeting-controller are next on a
    panel (retiring the witnesses).
    **Implementation guardrail (dry): touch NONE of the four enum files** — `errors.rs`, `actors/messages.rs`,
    `actors/metrics.rs`, `repositories/meetings.rs`. All four enums are already `pub` + `Copy` with `pub` label
    methods, so containment compiles against them unchanged. In particular do **not** remove the (stale but
    out-of-scope) `#[allow(dead_code)]` on `McError::error_type_label` at `errors.rs:229` — correct on merits
    but would reopen the closed cross-boundary question. If enumeration needs any edit to those files, STOP and
    flag team-lead (it means the containment premise doesn't hold).
  - *proto enums* (`SlotState`, `RejectionReason`): same in-domain `const VARIANTS` + exhaustive-`match`
    witness in `mc/observability/metrics.rs`, labels deferred to `slot_state_label`/`rejection_reason_label`
    (a prost enum is a normal fieldless Rust enum; the witness fires on regen). Cite `codec.rs:177`.
  - *Data-bearing enum, cannot enumerate variants cheaply* (`McError`, variants carry `String`s): the only
    genuine containment `const [&str]` case. `mc_session_join_failures_total{error_type}` uses a
    `const ERROR_TYPES: &[&str]` of the 23 `error_type_label()` values, drift-proofed per dry by a **test that
    constructs each `McError` variant and asserts `error_type_label()` ∈ the const** (catches both a new
    variant and a changed string). **dry catalog-drift fix (in-changeset):** `mc-service.md:830` lists 18 of
    23 values inline (already drifted — missing `invalid_argument`, `mh_assignment_missing`,
    `identity_key_invalid`, `sender_id_space_exhausted`, `media_policy_divergence`); point the catalog at
    `McError::error_type_label()` as the home instead of re-listing. (dry verified all 13 call sites in
    `webtransport/connection.rs` pass `error_type_label()`, never a literal — the const is correctly sourced.)
    **One sentence at the const (dry):** it enumerates all 23 labels while only a subset is reachable on the
    join path, so zero-init stands up series for failure modes that cannot occur here — the *presence* of such
    a series is not evidence the failure mode is reachable on this metric (the acceptable direction of the
    trade: a live "0 jwt_validation failures" is the visibility this task creates).
  - *Genuinely enum-less string vocabularies* (`status`, `heartbeat_type`, `event_type`, `presence`, display
    `outcome`, `state`, mh_status_dropped `reason`, `token_refresh` status, etc.): containment `const [&str]`
    in `metrics.rs`, and **repoint the existing in-file tests** (`test_cardinality_bounds`, adjacency tests,
    and consolidate `actor_metrics_integration.rs:40`/`auth_layer_integration.rs:64` hand-lists) at it — net
    de-dup. **Security-B closure note per domain** (one line at each const): every domain is proven closed at
    the emit site (all call sites pass literals, verified — e.g. `ac_token_issuance_total{grant_type}` all
    literal), OR if any call site forwards an external/request/DB/peer string it is **EXEMPT (unbounded)**,
    never const-listed. **dry-C3 residual note**: "consumed by zero-init + tests; call sites pass free `&str`,
    not checked against this list." **dry-C1**: kill the doc-comment value-list copy too (point rustdoc at the
    const); keep the per-value explanatory prose.
- **EXEMPT — honest classes, in catalog (see GUARD 1 marker):**
  - *unbounded/runtime-discovered domain*: gc `gc_http_requests_total`, `gc_db_queries_total`,
    `gc_errors_total`; ac `ac_db_queries_total`, `ac_errors_total`, `ac_audit_log_failures_total`,
    `ac_http_requests_total`; mh `mh_errors_total`. Reason names the specific unbounded dimension + origin.
  - *no emit site* (dry — a catalogued `*_total` with **no emitter** is a different defect than lazy-init and
    is NOT in scope): **`mc_errors_total` DROPPED from FIX 1** — `grep` finds only the reserved doc at
    `errors.rs:214` (`#[allow(dead_code)]`, "Phase 6b+"); zero-initing it would manufacture ~23×5×6
    permanently-zero series for a counter no code path can move (fail-open). Exempt reason: "no emit site;
    reserved, see errors.rs:214". **Sweep confirms** every other catalogued `*_total` has a real emitter.
  - *absence-is-load-bearing*: **`gc_telemetry_ingest_total` (OPS-2)** — `absent_over_time(
    gc_telemetry_ingest_total[15m])` at `gc-alerts.yaml:144` detects restart-silence by the series being
    absent; present-at-zero defeats it and a freshly-restarted pod has no `[1h] offset 15m` baseline for the
    fallback clause. Exempting preserves the alert unchanged (no prose fix needed). Swept all rule files;
    this is the only `absent`/`absent_over_time` on a FIX-1 counter.
- **mh media frames counters** (`mh_media_frames_dropped_total`, `_forwarded_total`): already present-at-zero
  via `resolve_media_handles()` (unconditional at `mh/main.rs:367`). **Mark `resolve_media_handles` with the
  zero-init-entrypoint comment** so GUARD 1 counts it natively — **no duplicate touch, no `MediaDropReason::ALL`
  second home, no exemption** (dry Q1, observability A). (OPS-12/13: this means the mh-media by-reason panels
  4/5 series are a pre-existing property of `resolve_media_handles`, NOT introduced by FIX 1 — I change no
  query there; I will verify the actual `/metrics` state and correct any now-stale "No data is normal" prose
  (OPS-13) per OPS-9, description-marker only, never `noValue` on those timeseries.)

`describe_counter!` help text is the catalog one-liner and **must not embed an example label value, id, or
key material** (security). A describe-only pass does NOT create a series — the `.increment(0)` does; state
this at the code site (observability B). **Witness-site comment (infra):** each `slot()` witness carries a
3-line note — "this exhaustive match is the SOLE drift control for the zero-init containment array; do NOT add
a catch-all arm" + dry's "discharging this E0004 also requires adding the variant to `VARIANTS` below" +
mitigation-1 (diverges from `PolicyApplyOutcome` doctrine). With leg-2b dropped these doc-lines are
**load-bearing, not optional** (code-reviewer) — they ship at every site, verified at Gate 3.

**OPS-8 — `zero_initialize_counters()` is INFALLIBLE:** no `Result`, no `unwrap`/`expect`, no panicking path
(ADR-0002). It is now load-bearing at boot in four services (GUARD 1 FAILs if a marked entrypoint isn't
called from `main.rs`), so a metrics-presentation fix must not be able to fail a service's boot. **Rollback:**
dashboards revert + re-provision; zero-init reverts with the pod and `increase()` regresses to today's
behavior; no persistent state.

**Completeness component test = the render-based leg 1** (see GUARD 1): the recorder is built through
**prod's own `init_metrics_recorder()` builder config, NOT a fresh `PrometheusBuilder::new()`** (test +
code-reviewer, SSoT — THE real correctness crux): if prod sets `idle_timeout` and the test uses defaults, the
test renders 0 and greens while prod reaps the idle 0-series and ships the defect. So factor
`init_metrics_recorder()` into a shared builder fn the test and `main.rs` both use (one config, not two), bound
thread-local via `metrics::with_local_recorder` (NOT process-global `install_recorder` which runs once per
process and collides across tests — observability), + `handle.render()`, parsing the exposition TEXT (not
`MetricSnapshot`), catalog↔emitted symmetric. The `idle_timeout`-absence pin (above) is asserted **against
that shared prod builder**, not a standalone check, so a divergence reds (code-reviewer). Confirm the touch at
each site actually registers via side effect (statement-position `.increment(0)`, not a bound-and-dropped
`let _ =` the optimizer could skip — test/code-reviewer).
`MetricAssertion::assert_delta(0)` is **vacuous for presence** (passes on absent too — test, testing.rs:1061),
so it is NOT used; and because leg 1 parses `render()` text rather than enumerating a `MetricSnapshot`, it
needs **no new enumeration API** — `crates/common/src/observability/testing.rs` stays untouched (test + infra
confirmed; clarifies observability's Fact-2 concern, which assumed the MetricSnapshot path). Two positive
controls before the symmetric compare (test + observability #2): assert `render()` contains **≥1 `_total`
line** for the service (distinct env-sanity token, not a drift token) so a both-empty vacuous pass can't slip
through; and assert a **series-count ceiling per service** so a future enum variant can't silently multiply
the startup cross-product (security). **Comment at the test site (observability):** leg 1 is not only a
catalog-drift check — because it exercises the real `metrics-exporter-prometheus` recorder and reads the real
exposition, it is transitively **the only thing proving the story's core premise**: that a
registered-but-never-incremented counter actually appears in `/metrics` at 0 (metrics-rs registering the
series says nothing about what the *exporter* emits). The `≥1 _total` control's failure message names **both**
purposes so a red there isn't misread as a flaky environment. **Guard-enforced presence (observability #1):** dt-guard (via the
`test_registration`/`test_coverage` machinery) asserts each service HAS its leg-1 render test, so the leg is
not opt-in.

**Layer-7 END-TO-END env-test (team-lead #3 + code-reviewer + infra — co-owned with @test).** Beyond the
in-process render unit test, add an env-test that scrapes a **real service's live `/metrics` after startup and
BEFORE any event**, asserting a zero-init'd counter is present at 0 — the end-to-end proof against the actual
defect and a permanent regression guard (unit render proves the mechanism hermetically; env-test proves the
deployed artifact through the real binary + scrape + real `idle_timeout`). Patterned on the proven
`crates/env-tests/tests/30_observability.rs` `media_path_metrics_..._drop_reason_is_registered` (line 325),
reusing its discipline verbatim. **Non-vacuity requirements (@test's review bar):**
- **Present-0 ≠ absent, two-part:** do NOT query `metric == 0` (PromQL `==0` returns empty for BOTH absent and
  non-zero — an absent series, the bug, would pass). Query raw `metric{labels}`, assert the result vector is
  **non-empty AND** the sample parses to exactly `0.0`.
- **Anchor / positive control** (`MH_MEDIA_ANCHOR` style, line 250): a known-always-present series so a
  wholesale-empty scrape FAILS distinctly rather than the for-all passing vacuously; feed every assertion from
  one fetched `Vec`. Anchor-absence message: "DO NOT satisfy by deleting the check."
- **Distinct triage tokens:** scrape/registration-failure token ≠ present-at-0-violation token.
- **Target criterion = STRUCTURALLY impossible on a healthy idle cluster, not merely rare** (test — the
  decisive sharpening; ADR-0028 retries:0, so a once-a-week ambient red is worse than a deterministic one). The
  existing `mh_media_frames_dropped_total` reasons are safe because with no media flowing those paths are
  UNREACHABLE — quiescence is structural. A failure/rejection combo only qualifies if tracing its `.increment`
  emit sites confirms **nothing in the idle/probe/readiness/reconnect/teardown path can reach it** — and my two
  first candidates (`mc_session_joins_total{status="failure"}`, `mc_webtransport_connections_total{status=
  "rejected"}`) are NOT obviously in that class (readiness probes, reconnect churn, port scans, auth churn can
  ambient-trigger them), so they need emit-site tracing before use.
- **Fallback (may be the cleaner guarantee): fresh-pod-isolation.** If no combo is provably structurally
  quiescent, this test provisions its **own per-run org** and, on a pod nothing else has touched, EVERY
  zero-init'd combo — including `mc_session_joins_total{status="success"}` — is deterministically 0; idle
  stability comes from pod exclusivity, not combo choice. Depends on whether the harness can give this test an
  exclusive mc pod. The `{success}` 0→1 demo capstone (present-at-0 → one join → reads 1 AND `increase()`=1) is
  viable only under this isolation. **I'll send @test the counter+label schema AND each candidate's emit-site
  list so they trace idle-reachability before anything is wired.**
- **Scope = ONLY the newly-zero-init'd enumerable counters**, target list derived from the catalog `Zero-init`
  annotations (shared source with the fix; if restated per-service, carry the "deliberate restatement, update
  in same commit" note). Do NOT gate the exempt/unbounded counters — they're legitimately absent on idle and
  gating them reds every idle run and gets the control muted.
I'll send @test my target-counter picks to sanity-check they stay 0 on idle before wiring assertions.

### FIX 2 — empty-is-healthy descriptions + window fixes (FINAL 2026-09-10; NO cumulative sweep, NO noValue)

> **✅ noValue RESOLVED — USER RULED: DROP `noValue:0`, keep the empty-is-healthy DESCRIPTIONS (team-lead
> relaying the user, 2026-09-10).** The user agreed with observability: after FIX 1, `noValue:"0"` is either
> dead code (zero-init'd counters are present-at-0, so "No data" never shows when healthy) or **masks a fault**
> (on exempt counters, "No data" = recorder-not-installed / scrape-failing / pod-down / wrong-job-label —
> painting it green `0` violates "fail loudly; never mask"). So FIX 2's second half **shrank by user ruling**:
> NO `noValue` edits to any panel; keep the informative descriptions; GUARD 2's expected-empty rule is
> description-marker-only. This is recorded here so the record is honest about *why* it shrank.

**Superseded plans removed.** (1) Per the user's earlier correction: keep ADR-0029's `increase()` presentation
— no cumulative `sum()` sweep. (2) Per this ruling: no `noValue` additions.

What FIX 2 actually is now (all `noValue` dropped):
- **ADR-0029 amendment** (Category A/B/C PromQL functions UNCHANGED):
  (a) **zero-init precondition (OPS-10)** — Category A's `increase(m[$__range])` on a discrete-event counter
  is honest *only if* the series exists at 0 from pod start; point at GUARD 1 as its enforcement, so a future
  service can't follow the classification rule, skip zero-init, and silently reproduce this defect. Record that
  Category A's stat-`$__range` / timeseries-`$__rate_interval` split (previously documented, unenforced) is now
  enforced by GUARD 2's window rule.
  (b) the **empty-is-healthy DESCRIPTION** convention (NO noValue), with the empty-is-healthy **token defined
  ONCE here** (phrase `empty is healthy`, matched case-insensitively) cited by both GUARD 2 and every such
  description — one documented home (observability).
- **Dashboard edits** = (i) the two **window-violation fixes** (mh-overview stat 25 `$__rate_interval`→
  `$__range`; ac-overview timeseries 44 `$__range`→`$__rate_interval`); (ii) add the **empty-is-healthy
  description marker** to catalog-declared expected-empty counter panels (mc-overview 38,39,40,4,31,10,46;
  ac-overview 43,33; mh-overview 25; mh-media 4/5) — description ONLY, **no `noValue` on any panel**; (iii)
  **mh-media panel 6**: observability removes its **dead `noValue` field** (it can never fire — `sum(increase())`
  over pre-registered reasons returns 0, never empty), landing with their panel-6 description rewrite. Set
  derived from the `Expected-empty` catalog annotations (kept — they drive the description-marker guard + the
  shared reader) so guard and dashboards can't drift.
- **Empty-is-healthy description wording — 4-clause shape (OPS-5, carries the token in clause 1):**
  (1) what a zero means (no events; healthy steady state — carries the token); (2) that 0 also renders when the
  series is absent, so this is **not** a liveness signal; (3) where liveness IS read — name the specific
  neighbouring traffic/liveness panel by title per dashboard (a real pointer, not "check service health");
  (4) the runbook scenario to open when non-zero. Anchors (OPS): mc JWT-failures → mc-incident-response §10;
  mc join-failures → §8; mc WebTransport-rejections → §9; mc actor-panics → §2 (`MCActorPanic`); ac
  token-validation-failures → ac-service-incident-response §5; ac audit-log-failures → §7; mh
  RegisterMeeting-timeouts → mh-incident-response §13. mh-media 4/5 → mh-incident-response §17 (+ §16 ingress
  for 4); keep panel 5's `mh-service.md §Media Forward Path` pointer.

### GUARD 1 — `counter-zero-init` (new dt-guard subcommand) — THREE-LEG design (REWRITTEN per Gate-1)

Naming per infrastructure M1 / team-lead: subcommand `counter-zero-init`, module
`crates/dt-guard/src/counter_zero_init.rs`, wrapper `scripts/guards/simple/validate-counter-zero-init.sh`
(the `validate-` prefix reads consistently with ~2/3 of the dir) sourcing `_dt_guard_wrapper.sh
counter-zero-init` (ADR-0034 §3). `--root`/`--explain` args; findings via `common::explain::print_finding`.
Reuses `CANONICAL_SERVICES`+`SERVICE_METRIC_PREFIX_RE`, `common/metric_catalog.rs` (extended, see below),
`MacroKind::ALL`/`MACRO_INVOCATION_WITH_FIRST_ARG_RE` (+ the derivation test M4; **doc-currency only on
`metric_macros.rs` — the frozen literal at :274 is untouched**), `common/scope.rs::assert_scope_live`+
`ScopeRoot`, `common/test_code_filter.rs` (exclude `#[cfg(test)]` bodies — observability), `status`/`explain`,
and `ignore::is_lazy_reason`. Rationale for a NEW subcommand vs a rule in `metric_coverage` (which is ADR-0032
*test* coverage): different artifact set (catalog+init-site vs tests), different operator first-action, and a
different scope-liveness polarity (empty is FATAL here, safe there) — infrastructure M1.

**Why not name-level presence (the design reviewers blocked).** A `counter!` for a name inside `metrics.rs`
is satisfied by every counter including the broken ones (hot-path `record_*` and startup registration are the
same capture — observability), and name-presence says nothing about *label-combination* coverage, which is
the dimension the defect lives in. GUARD 1 is therefore a **three-leg** check, and the tautology trap
(deriving the expected set from the same `ALL` the loop iterates) is designed out by making leg 1's
independent signal the **catalog** and leg 1's realization a **render-based runtime test**:

- **Zero-init entrypoints** are self-declaring: a marker comment `// dt-guard:zero-init-entrypoint` directly
  above a fn in `metrics.rs`. `Z(svc)` = counter-name literals inside the balanced-brace body of ANY marked
  fn (no central registry to drift; observability). The guard keys on the counter-name presence in a marked
  body (either idiom), so mh's `resolve_media_handles()` is a satisfying entrypoint by marking it — **no
  duplicate `MediaDropReason::ALL` touch, no MH exemption** (dry Q1, observability A).
  - **TOUCH IDIOM = explicit `.increment(0)` (code-reviewer correctness crux).** `zero_initialize_counters()`
    uses `.increment(0)`, NOT bare `counter!(name,…)` handle-resolution — because *does registration-without-
    increment actually render a scrapeable 0-series in `metrics-exporter-prometheus`* is **the exact
    assumption whose failure IS this defect**, and must be PROVEN, not assumed. `.increment(0)` guarantees the
    0-series regardless. **The render-based leg-1 test IS that runtime proof** (scrape `/metrics` after
    startup, assert present-at-0) — it is the belt code-reviewer demands, not the macro's registration
    semantics.
  - **`resolve_media_handles()` contingency (code-reviewer + OPS-15 coupled).** It resolves handles WITHOUT an
    increment, so whether its media counters are present-at-0 rests on the same unproven assumption. The
    render/OPS-15 check gates it: if handle-resolution-alone does emit at 0 → it stands as the entrypoint
    (observability's panels 4/5 "pre-registered at zero" prose is then correct); if it does NOT → add explicit
    `.increment(0)` per combo in the SAME existing `ALL` loop (one added statement, not a duplicate
    enumeration), and panels 4/5 prose changes accordingly. Not assumed either way — proven at impl.
    (observability keeps 4/5 prose as-is regardless — it asserts the committed end state; the fix, if needed,
    goes in `resolve_media_handles`, not in walking the prose back to a broken state.)
  - **The premise gates GUARD 1's DESIGN, so run the render check FIRST (observability).** Whether GUARD 1 may
    accept a bare `counter!` site as a satisfying touch — or must require an explicit `.increment(0)`/
    `.absolute(0)` — depends on the same fact: if handle-resolution alone does not render, a marked entrypoint
    of bare resolutions satisfies GUARD 1 statically while emitting nothing scrapeable ("guard lies" through a
    new door). So the cheap render check (build a prod-config recorder, resolve one handle WITHOUT
    incrementing, `render()`, look for the `_total 0` line) runs **before GUARD 1's extraction is written**, not
    after — negative → GUARD 1 keys on `.increment(0)`/`.absolute(0)` AND `resolve_media_handles` gets explicit
    touches. The leg-1 test comment records that this one check settles **three** consumers: a doc claim
    (panels 4/5), a code question (resolve_media_handles), and a guard-design question (GUARD 1's touch idiom).
- **Leg 2 (static, in the guard): `ALL`/init ↔ required set.** `R(svc)` = catalog `### `name`` metrics ending
  `_total`, emitted via `counter!` in that svc's `metrics.rs` (non-test), **not** exempt-marked. FAIL for any
  `m ∈ R \ Z`.
- **Leg 3 (static, in the guard): default-deny + mutual exclusion.** A counter must be in the init set **xor**
  the exempt set. Security-A: a counter that is BOTH exempt-marked AND touched in an entrypoint is a FAIL
  (own rule id + fixture) — closes the "satisfy the guard by touching one invented label value instead of
  writing an exemption" escape.
- **Leg 2b DROPPED — no `syn`, no cross-crate enum parsing (team-lead FINAL, whole-team converged).** The
  array↔enum-variants set-diff would have needed `syn` to be safe (a regex Rust-parser false-negatives on the
  exact drift it guards — infra); a new security-gated dep is disproportionate for a bug-fix, and the residual
  it would close (discharge the `slot()` witness, forget the adjacent `VARIANTS`) is exactly the residual the
  house-blessed `PolicyApplyOutcome` pattern accepts at its 5 sites. Matching that bar (co-located wildcard-free
  index-map witness + residual doc-line + TODO hoist) IS the codebase standard. So instead of parsing:
  - **One cheap in-domain predicate in GUARD 1 (team-lead #1), covering BOTH silent-degradation modes:** for
    each `slot()` witness in a service's own `metrics.rs`, assert (i) it is **wildcard-free** (no `_ =>`/
    catch-all) AND (ii) its **enclosing item is not `#[cfg(test)]`-gated** (a relocation into a test module a
    release build never compiles evaporates the sole drift control with nothing red — infra). Both are cheap
    textual/structural checks in the span the guard already has; distinct reason tokens; each with its own fire
    fixture. This inverts leg-2b's division correctly (infra): rustc does the hard part (exhaustiveness), the
    guard does the cheap textual part that keeps rustc's check load-bearing — which is why it needs no `syn`.
    The wildcard-free check **subsumes `#[non_exhaustive]`** for both same- and cross-crate enums
    (code-reviewer): a `non_exhaustive` proto enum would force a wildcard in `slot()`, which the predicate then
    reds — so the predicate catches the symptom regardless of cause. (The third fail-open mode — matching on
    something other than the enum value — is left out: genuinely hard textually and least likely by accident.)
  - **Verify-the-witness-bites done-check (infra):** during impl, add a throwaway extra variant and confirm the
    build reds at `slot()` (converts "dry says it reds" into "I watched it fail"), and that `slot()` is NOT in
    a `#[cfg(test)]` block a release skips.
  - **Residual recorded HONESTLY (team-lead FINAL, dry):** the "discharged `slot()`, forgot `VARIANTS`" residual
    is at **house-parity and TODO-tracked — NOT "closed."** It closes properly (compiler-clean, no guard) only
    at the `docs/TODO.md` hoist-to-`ALL` when MC/GC specialists are next on a panel. The decisive reason not to
    take `syn` is not dep cost (dry repriced it low — already in Cargo.lock via 37 edges) but that it would add
    a SECOND Rust-source-reading lane to dt-guard (disciplined-regex + `syn`), a permanent architectural
    boundary not worth a narrow house-accepted witness-guarded residual.
- **Leg 1 (runtime, render-based component test — the only non-tautological leg): catalog documented label
  values ↔ actually-emitted series, SYMMETRIC.** Per service: build a real `PrometheusBuilder` recorder, run
  the marked entrypoints, `handle.render()`, extract each counter's emitted `(label→value)` set from the
  exposition (these ARE the `ALL` values, made observable without the guard parsing enum defs — code-reviewer
  discouraged that), parse the catalog's `- **Labels**:` parenthesized backtick groups, and assert **set
  equality both directions** (variant-in-`ALL`-catalog-lacks AND catalog-value-`ALL`-lacks). This is the leg
  that catches vocabulary drift with no code symptom (observability), and it's where @test's required
  partial-combo negative fires. Fire-proof (test conditions 2/3): a **pure comparison-fn unit test** over a
  synthetic (rendered-set, catalog-set) pair deliberately missing one combo, and one with an extra combo,
  each asserting the checker reds — so the completeness check has a does-it-fire half despite reading the real
  init fn. **Line matching is anchored** (split on `\n`, exact-match the `name{sorted-labels} 0` line — a raw
  `contains("} 0")` also matches `} 0.5` on an unrelated series; test). The expected combos are **looped from
  the catalog**, not from the co-located const/`ALL` the init iterates (else tautological; test cond. 1), and
  the check is **symmetric** — rendered ⊆ catalog is asserted in leg 1 here, not delegated elsewhere (test
  cond. 3 / observability). (Retire note: if a catalog is ever *generated* from the enum, leg 1 becomes
  tautological and must be DELETED, not kept green — observability/test.)

**Exempt marker** (new catalog convention, observability-owned, parsed by the shared `common/metric_catalog.rs`
reader): `- **Zero-init**: exempt — <reason>` = "not zero-init'd for a stated, reviewer-validated reason"
(team-lead). Three honest reason classes (semantic-guard + owning reviewer judge truth):
(i) **unbounded/runtime-discovered domain** (raw `status_code`, free-form `operation`/`table`/`reason`);
(ii) **absence-is-load-bearing** — a liveness alert keys on the series being *absent*, so present-at-zero
would defeat it (`gc_telemetry_ingest_total` / `absent_over_time`, OPS-2);
(iii) **deliberate-absence-is-signal** — a triage panel where an empty series IS the reading (the mh-media
by-reason drop family, IF OPS-15 verification confirms it is not already present-at-zero — see FIX 1).
Reason passes `is_lazy_reason` (≥10 chars, substantive) AND **fails closed** (semantic-guard, test): a
malformed/misspelled/blank marker resolves to REQUIRED with a distinct `PARSE-ERROR` token — never silently
exempt. Leg-1 parse failures likewise emit a distinct token, not "no values ⇒ empty" (observability leg-1
hazard). **OPS-14 dual-end enforcement:** an `absence-is-load-bearing` exemption whose reason cites an alert
must be mirrored by a comment at that alert (`gc-alerts.yaml:144` → back-pointer to the catalog exemption), so
the coupling is visible from both ends and neither the guard nor the alert can silently regress.

**Deny-leg scope (security):** leg 2's `Z` and leg 3's mutual-exclusion scan the bodies of **all** marked
zero-init entrypoints (`zero_initialize_counters` AND `resolve_media_handles`), so a counter reachable from
*any* startup touch path is in the init set and therefore cannot be exempt-marked. The rule doc states which
spans are scanned.

**Vacuity / does-it-apply (positive controls — the guard's steady state is green, so these are its value):**
- `assert_scope_live` with **distinct tokens** for "root absent" vs "root present, zero matches" (infra M2,
  polarity = empty-is-fatal). FAIL if `CANONICAL_SERVICES` resolves to no metrics.rs, or a svc has a catalog
  but **zero marked entrypoints**, or `R` is empty for a svc that emits `_total` (semantic-guard §5).
- FAIL if a marked entrypoint is **never called from that svc's `main.rs` startup path** (a marker on an
  uncalled fn satisfies the guard statically while registering nothing — the case where the guard *lies*;
  observability). Grep the fn name in `main.rs`; sources in-repo (gc:108, ac:77, mh:185, mc:185).
- Non-literal first arg to `counter!` → distinct `non_literal_metric_name` finding, not a silent `continue`
  (infra M3).
- `SCOPE:` line: counters-required / exemptions-honored / entrypoints-found / services-scanned.

**Self-test shape (infra exact spec).** TWO tiers:
- *Inline `#[cfg(test)]` unit tests* (ADR-0034 §2, pure functions): `R \ Z` set-difference; balanced-brace
  span extraction of a marked entrypoint body; catalog marker parse (exempt + expected-empty); leg-1
  comparison-fn; `MacroKind::ALL` derivation oracle (M4).
- *Synthetic-root suite* `scripts/guards/counter-zero-init.test.sh` (executable, NOT under `simple/` — a
  header comment states why: `run-guards.sh:121` `find … -name '*.sh'` would auto-run it as a prod guard).
  Wired into `scripts/layer3.sh` beside `:157` via `run_and_emit "counter-zero-init-selftest"
  "${__here}/guards/counter-zero-init.test.sh" || true` (SSoT emitter; hermetic — no cluster/network/cargo;
  missing binary = loud `[precondition]` exit 1, never skip). Template `media-telemetry-deny.test.sh`
  (mktemp roots + EXIT trap, real binary via `--root`, **no `DEVLOOP_TEST` scope seam**, source
  `_test_helpers.sh` for `assert_absent`/`assert_marker`/`assert_no_marker`). Six cases:
  (1) **positive control** — planted lazy counter (catalogued+emitted, absent from any entrypoint) MUST fire;
  (2) zero-initialized counter passes; (3) each scope-liveness token separately (catalog dir absent; catalog
  present but zero `### \`name\``; metrics.rs absent; catalog present but zero marked entrypoints) — distinct
  reason tokens; (4) exempt-marked passes, lazy/blank exemption reason fires; (5) **scope-boundary** — a lazy
  counter planted in a sibling dir *outside* scope stays GREEN (catches a walk-root-widening regression);
  (6) output shape — no `VIOLATION:`/`ERROR:` on a continuation line (run-guards greps + `head -5`);
  (7) **wildcard-free-`slot()` predicate** — a planted `slot()` witness with a `_ =>` catch-all MUST fire
  (own token); a wildcard-free one passes (protects the sole containment drift control now leg-2b is dropped).

### GUARD 2 — `dashboard_panels.rs` (REWRITTEN per Gate-1: window arm in Rule 4, `:592` UNTOUCHED)

Team-lead BLOCKER 2 + infra M10-retraction/M13/M14: the existing `counter_misuse` arm (`:592`, counter must
be inside `rate`/`increase`/`irate`, all panel types) stays **UNTOUCHED** — a compliant stat using
`increase($__range)` already passes it, so there is no contradiction to resolve and **no second "is this a
counter" classifier**: both new checks select on the **declared metric type** (`metric_types.get(...) ==
Some("counter")`), treating the `_total` suffix as convention, not classifier (infra M14).

**Change 1 — make the WINDOW rule panel-type-conditional (modify `RATE_WINDOW_RULE_ID`, `:525-545`; do NOT
add a sibling rule — infra P3, observability).** Today that rule blanket-accepts `$__rate_interval`,
`$__range`, `$__interval` on any panel type (`TIME_RANGE_WINDOWS`, `:39`), so ADR-0029 Category A's existing
stat-vs-timeseries split is documented but unenforced. Condition it, for a `*_total` counter ref only:
- `stat`/`gauge`/`bargauge` → window must be `$__range`; `$__rate_interval` (or hardcoded `[5m]`) FAILs.
- `timeseries` → window must be `$__rate_interval`; `$__range`/`$__interval` FAILs.
- Category B (ratio/quantile: `rate()` under a division or `histogram_quantile`) keeps `$__rate_interval` on
  all types; SLO dashboards stay exempt (`is_slo_dashboard`) as today.
This one change also fixes Rule 4 currently telling a **stat** panel "use `[$__rate_interval]`" (infra M13 — no
two rules over the same field with opposed advice). Keep a `status_code=~"[45].."` fixture so the window regex
can't regress into matching a regex-label-matcher as a window (observability's phantom-`[45]` caution). This
catches **two live in-scope violations** observability found: `mh-overview.json` panel 25 (stat wrongly
`$__rate_interval` → `$__range`; under-reports exactly like Join Flow) and `ac-overview.json` panel 44
(timeseries wrongly `$__range` → `$__rate_interval`) — both fixed in FIX 2.

**Change 2 — NEW rule `EXPECTED_EMPTY_DESCRIPTION_RULE_ID` (simplified per USER ruling — description-marker
ONLY, NO `noValue` branch).** A panel whose expr references a **catalog-declared expected-empty** counter must
carry the **empty-is-healthy description marker** (the token from the ADR-0029 amendment, matched
case-insensitively). No `noValue` check anywhere (the user dropped `noValue`). Panel types that fall through
unchanged: `row`, `logs`, and any **unknown** `type` string must NOT take the strict path (infra M11 —
unknown-type silent strictness is a false-positive generator).

**Expected-empty source (infra M15 / M14):** a real declared catalog field `- **Expected-empty**: yes —
<why empty is healthy>`, parsed by the shared `common/metric_catalog.rs` reader (below) with the
absent-vs-present-but-unset distinction and its own reason token — not prose, not the panel JSON.

**Shared catalog annotation reader (infra P7, dry D4, team-lead).** ONE new helper in
`common/metric_catalog.rs` (the F-DRY-2 home) returns per-metric annotations for both markers so the two
guards **cannot disagree**. **Model = TWO independent tri-state fields, NOT one enum (infra correction):** a
metric can carry `Zero-init: exempt` AND `Expected-empty: yes` at once — in fact that is the *common* case
(the unbounded-domain counters exempt from zero-init are exactly the set genuinely absent-when-healthy that
needs `noValue`), so a single `Annotation` enum with one variant per marker would silently drop one depending
on parse order. Instead a struct `{ zero_init: Option<Exempt|Malformed>, expected_empty: Option<Yes|Malformed> }`
where each field distinguishes **absent / present-and-parsed / present-but-malformed** independently; a blank
reason on one marker is fail-closed for that marker only and doesn't invalidate the other. **Two DISTINCT
vacuity mechanisms (infra):** the per-metric tri-state answers "does *this* metric carry the marker"; it does
NOT answer "did the parse find *any* annotations at all" — a format drift / moved file returns `None` for every
metric and every lookup succeeds (GUARD 2 requires `noValue` nowhere, silent-green). So the collection-level
**non-empty assertion** (GUARD 2's expected-empty set is non-empty, distinct reason token) is a SEPARATE
mechanism from the per-metric tri-state, not derivable from it. **Asymmetric-vacuity (infra P7):** the same
parse bug fails LOUD in GUARD 1 (every counter becomes required → red) but SILENT in GUARD 2, which is why
GUARD 2 carries that own non-empty assertion independent of GUARD 1's redness. **Reader fixture (infra):** a
metric carrying BOTH markers at once, asserting both fields come back populated — the case the two-field
struct exists for (the old single-enum would pass it only by parse-order luck), with a comment pinning that so
a future "simplify to one enum" is caught.

The empty-is-healthy **token** is defined ONCE in the ADR-0029 amendment text and cited by both the guard and
every dashboard description (observability — single documented home, no third-consumer variant).

Both guards answer does-it-fire AND does-it-apply per ADR-0036 coverage doctrine (fire fixtures per rule with
the wrong-WINDOW shape so a negative fires EXACTLY its rule id and not `counter_misuse` — @test; whole
per-rule hit-map asserted, not `is_ok`).

### Phasing
1. ADR-0029 amendment (zero-init precondition + empty-is-healthy DESCRIPTION convention + token; NO noValue —
   user ruled) + catalog annotations (exempt + expected-empty) + shared `common/metric_catalog.rs` two-field
   reader.
2. FIX 1 across mc→gc→ac→mh: factor `init_metrics_recorder()` into a shared builder fn; zero-init entrypoints
   with explicit `.increment(0)` + in-domain containment (variant array + wildcard-free index-map `slot()`
   witness + witness-site comments); main.rs calls; **verify the witness bites** (throwaway variant); render
   completeness tests through the shared prod builder; idle_timeout-absence pin against it; **Layer-7
   present-at-0 env-test** (co-owned @test — the end-to-end proof, do this early to settle the touch-idiom).
3. FIX 2 (NO noValue — user ruled): two window-violation fixes + empty-is-healthy DESCRIPTION markers on
   expected-empty panels + observability's mh-media 4/5 prose (done) + panel 6 dead-noValue removal (with
   observability's panel-6 description). OPS-9 prose sweep (paired with operations).
4. GUARD 1 (module + clap + wrapper + `layer3.sh` self-test + unit + synthetic-root fixtures; leg-1 + leg-3 +
   wildcard-free-`slot()` predicate; NO leg-2b/`syn`).
5. GUARD 2 (window-rule conditioning in `RATE_WINDOW_RULE_ID` + expected-empty DESCRIPTION-marker rule (no
   noValue) + tests; `:592` untouched).
6. Build + run both guards + `cargo test -p dt-guard` + affected service tests + `layer` guards (incl. the new
   Layer-7 env-test) + `promtool` on touched rule files.

---

## Reviewer Requirements Ledger (Gate-1 converged)

**Previously-silent alerts that BEGIN paging after FIX 1 (OPS-1 generalized — operator-facing, flag in PR).**
These are single-event `increase(...)>0` alerts defeated today by the lazy-init defect; FIX 1 makes them fire
correctly. The six: `MCActorPanic` (`mc-alerts.yaml:35`, `mc_actor_panics_total`); `MCMediaPolicyDivergence`
(`:162`, `mc_media_policy_pushes_total`); `MCSenderIdSpaceExhausted` (`:314`,
`mc_session_join_failures_total{sender_id_space_exhausted}`); GC meeting-creation-org (`gc-alerts.yaml:325`,
`gc_meeting_creation_failures_total`); `MHCallerTypeRejected` (`mh-alerts.yaml:173`,
`mh_caller_type_rejected_total`); MH sender-binding out-of-range/conflict (`mh-alerts.yaml:229,251`,
`mh_media_session_starts_total`).
**OPS-17 (blocker resolution a):** correctness is **derived by inspection; the repo has NO `promtool test
rules` harness** (`gc-alerts.yaml:321` says so in-tree; two of the six can't be provoked in Kind) — so I do
NOT claim "verify each fires." Residual recorded here + the six rule ids added to `docs/TODO.md` §Observability
Debt (alert-rule unit tests). A promtool harness is task-sized; not built here.
**OPS-16 (PR-body operator instruction):** these fire on genuine, previously-invisible conditions — the
correct first response is **triage each against its runbook scenario, NOT silence**; silencing them restores
the exact defect this PR fixes.

**OPS-2 (absent_over_time):** `gc_telemetry_ingest_total` is **EXEMPT** (absence-is-load-bearing), preserving
`gc-alerts.yaml:144` unchanged. It is the only `absent`/`absent_over_time` on a FIX-1 counter (swept).

**OPS-6 (zero-init'd-but-never-incremented ≈ healthy-zero):** GUARD 1 leg-2 (name coverage) + the per-service
render test (present-at-0 spot-checks + a series-count band) + existing `metric_coverage` (test-reference)
together pin emit-site reachability; residual noted in the ADR. **TEST-F1 honesty correction (Gate-3):** the
designed leg-1 *symmetric catalog-label-VALUES ↔ emitted-VALUES* check is **NOT wired** — the render test is
hardcoded present-at-0 probes + a count band, not catalog-derived, and GUARD 1 itself is name-level. See the
TEST-F1 entry in §Implementation Summary for what actually ships (an enum-witness drift test for
`JOIN_FAILURE_ERROR_TYPES` + a fire-proof comparison primitive) and why full leg-1 is deferred.

**OPS-9:** sweep `*-alerts.yaml`, `docs/runbooks/*`, `docs/observability/alerts.md` for now-wrong
empty-series/absent/No-data prose on FIX-1 counters (e.g. `gc-alerts.yaml:333` "empty series is the normal
healthy state" on `gc_meeting_creation_failures_total`, now zero-init'd). Alert-file prose = operations-owned
(paired).

**OPS-11 (dashboard positive control — Join Flow panel → counter/label map).** The mc-overview Join Flow
panels that read 0/No-Data on the demo, each mapped to the counter+labels FIX 1 zero-inits:
Session Joins by Status (ts 29) → `mc_session_joins_total{status∈success,failure}`; Session Join Failures
(stat 39 / ts 31) → `mc_session_join_failures_total{error_type∈McError::error_type_label}`; JWT Validation
Failures (stat 38 / ts 33) → `mc_jwt_validations_total{result=failure,token_type,failure_reason}`; Rejected
Connections (stat 40 / ts 32) → `mc_webtransport_connections_total{status∈accepted,rejected,error}`; Actor
Panics (stat 4 / ts 10) → `mc_actor_panics_total{actor_type∈ActorType}`. Demo headline `mc_session_joins_total=1`
→ the `status="success"` series now present-at-0 from start, so the `0→1` edge is visible.

**OPS-14 (exemption enforceable both ends):** `gc_telemetry_ingest_total`'s exemption lives in the catalog
(reason string GUARD 1 reads) AND is mirrored by a back-pointer comment at `gc-alerts.yaml:144`, so GUARD 1
passes *because of the recorded decision*, not oversight, and a future dev can't add it to the zero-init set
without seeing the coupling and silently killing `GCTelemetryIngestStalled`'s restart-silence shape.

**OPS-15 (verify, don't assert — mh-media 4/5):** code-inference says `resolve_media_handles()` registers each
series at 0 at startup (so the panels are *already* present-at-zero and their "No data is normal" prose is
*already* stale — itself an instance of this defect). I will paste the **actual `/metrics` lines** into the
Implementation Summary rather than cite the site. Present-at-0 → mark `resolve_media_handles` as the
entrypoint, no exemption, fix prose. Empirically absent → OPS-12 option (a): EXEMPT the family
(`deliberate-absence-is-signal`), fix prose. **Fail-closed (OPS-15, team-lead): if no exporter is available at
impl, the fallback is EXEMPT (option a), NOT "assume present-at-zero and mark the entrypoint"** — an unverified
entrypoint mark would pass GUARD 1 on an unverified premise and flatten the by-reason breakdown if wrong.

**OPS-16 (rollout note, PR body):** the six alerts above un-mute at once; the first deploy may surface a
genuine backlog, not a regression. PR body + operator note name all six with rule files, say "expect new
pages — triage each against its runbook scenario," and warn that silencing them restores the exact defect.

**Ledger of the other converged asks:** semantic-2 (fail-closed exempt) → GUARD 1 marker parse; semantic-3
(empty-R vacuity) → scope-liveness positive control; semantic-4 (const drift = silent defect) → the
ENUM-BACKED containment consts are compile-witness-derived (`slot()` witnesses); the ~38 pure string-vocab
`&[&str]` consts are hand-listed with the catalog as their only external tie, and `JOIN_FAILURE_ERROR_TYPES`
(an enum SUBSET) gained a dedicated McError-witness drift test at Gate-3 (TEST-F1) — **corrected from the
earlier "all compile-witness-derived" claim, which was false**; test-1/2/3 (catalog-derived, fire-proof,
symmetric) → **PARTIAL: the fire-proof comparison primitive ships (`diff_label_sets` + its
missing/extra unit test), but the catalog-derived per-service symmetric render check does NOT** (catalog value
lists are non-exhaustive "e.g." prose today; normalizing them is the tracked prerequisite) — see TEST-F1 in
§Implementation Summary; security-A (exempt XOR init) → leg 3 mutual-exclusion; security-B
(closed-at-emit-site) → per-domain closure notes; dry-C1/C2/C3 → kill doc copy / `MeetingRefusal::ALL` / residual
note; infra M1-M15 → naming, scope-liveness, non-literal finding, macro-family+derivation test, wrapper,
self-test shape, `--explain`, no preflight probe, `:592` untouched, one-rule-id-per-fix, declared-type
classifier, expected-empty catalog field. INDEX pointers deferred to story-close (team-lead — do NOT touch
INDEX files here). `docs/TODO.md`: dt-guard subcommand-split re-debate follow-up + dry AC/GC route-through-typed
-vocab follow-up.

---

## Implementation Summary

### Phase 1 — Touch-idiom premise PROVEN (2026-09-10)

Ran an empirical probe (temporary `#[test]` in mc metrics.rs, since removed) against the **real
`metrics-exporter-prometheus` 0.16.2** recorder via `PrometheusBuilder::new().build_recorder()` +
`set_default_local_recorder` + `handle.render()`. Result — all three idioms render present-at-0:

```
# TYPE probe_bare_total counter
probe_bare_total{k="v"} 0          <- BARE counter! registration, NO increment
# HELP probe_desc_total desc
# TYPE probe_desc_total counter
probe_desc_total{k="v"} 0          <- describe_counter! + increment(0)
# TYPE probe_incr0_total counter
probe_incr0_total{k="v"} 0         <- explicit increment(0)
```

**Bare `counter!` handle resolution DOES emit a scrapeable `…_total 0` line** (default builder, no
`idle_timeout` — same as prod `init_metrics_recorder()`). Consequences, all now settled empirically not assumed:
- **`resolve_media_handles()` needs NO change** — its bare handle resolution already makes the mh media
  counters present-at-0. observability's panels 4/5 "pre-registered at zero" prose is **correct as written**.
- **GUARD 1 may key on counter-name presence in a marked entrypoint body** (either idiom) — the dropped
  `.increment(0)` filter stands.
- `zero_initialize_counters()` will still use explicit `.increment(0)` (legible intent, statement-position),
  but the fix does not depend on the distinction. **OBS-F3 correction (Gate-3):** `production_builder_has_no_idle_timeout`
  does NOT actually pin `idle_timeout`-absence — it renders synchronously at t≈0, so a realistic minutes-scale
  `idle_timeout` would not reap before the render and the test would still pass (it is the present-at-0 render
  assertion under a stronger name). idle_timeout-absence is a **config-review checkpoint, not a test-enforced
  invariant**; neither that test nor the Layer-7 env-test reliably backstops it. The 4 test doc-comments +
  messages + the `configured_prometheus_builder` module docs were rewritten to say so.

The render-based present-at-0 unit test + Layer-7 env-test remain the permanent regression guards for the
present-at-0 property (they prove a registered-but-never-incremented counter renders at 0). They do NOT guard
`idle_timeout`-absence (OBS-F3) — that stays a config-review checkpoint.

### Phases 2–6 — landed & green (2026-09-10)

**Phase 2 — shared annotation reader** (`crates/dt-guard/src/common/metric_catalog.rs`): `parse_annotations()`
returning `MetricAnnotations { zero_init_exempt, expected_empty }` as TWO independent `Marker` tri-states
(`Absent`/`Present`/`Malformed`, fail-closed). 8 unit tests incl. both-markers + blank/lazy → Malformed. Sent
to @infrastructure first.

**Phase 3 — FIX 1, all four services** (mc by me; gc/ac/mh by forks): `configured_prometheus_builder()` SSoT
refactor; `pub fn zero_initialize_counters()` marked `// dt-guard:zero-init-entrypoint`, infallible,
`.increment(0)`; in-domain containment (`const VARIANTS` + index-map `slot_*` witness with load-bearing
doc-lines, no cross-boundary enum edits); mh marks `resolve_media_handles()` (no duplicate enumeration);
main.rs wired; render-based `zero_init_renders_counters_present_at_zero` + `production_builder_has_no_idle_timeout`
per service. Lib tests: mc 411, gc 371, ac 394, mh 243 — all green. GUARD 1 caught two mc counters missed on
first pass (`mc_caller_type_rejected_total`, `mc_participant_outbound_messages_dropped_total`) — now zero-init'd.

**Exempt re-audit (team-lead-required correctness sweep, 2026-09-10):** every exemption was re-checked against
ACTUAL call sites (not the recorder `&str` signature). THREE were misclassified as unbounded and were
masking real low-volume failures — moved to zero-init: `gc_grpc_mc_calls_total` (method is the literal
`assign_meeting_with_mh`), `ac_audit_log_failures_total` (un-exempted as an 11-**pair** closed set — pair-set +
witness, NOT the fabricated ~36 cross-product; OPS-18), `ac_token_validations_total` (the "no emit site" claim
was stale — crypto/mod.rs calls it; zero-init'd the live clock-skew combo). Exempt 11→6; required 63→68 (db_queries reversal: team-lead+semantic-guard ruled gc/ac_db_queries_total enumerable — moved to observed pair-set zero-init, NOT exempt).
KEPT exempt with verified runtime-source reasons: `*_errors_total`/`*_http_requests_total` (status_code is the
runtime response/error code), `gc_telemetry_ingest_total` (absence-is-load-bearing). JUDGMENT CALL flagged to
team-lead + semantic-guard: `gc/ac_db_queries_total` kept exempt as "high-traffic present-within-seconds, not
the low-volume defect + growing per-query-site literal set" (a third exemption rationale). @security looped.

**Phase 4 — FIX 2** (fork): kept `increase()`; two live window-violation fixes (mh-overview 25 → `$__range`,
ac-overview 44 → `$__rate_interval`); `Expected-empty: yes` annotations on 20 wholly-bad-event counters;
`empty is healthy` description marker added to 27 panels; **NO `noValue`** (user ruling). ADR-0029 amended
(zero-init precondition + window-split enforcement + empty-is-healthy DESCRIPTION convention; no noValue,
no cumulative). `dashboard-panels` + `application-metrics` STATUS=OK; all 16 dashboards parse.

**Phase 5 — GUARD 1** (`counter-zero-init`): module + clap arm + `validate-counter-zero-init.sh` wrapper +
synthetic-root self-test `scripts/guards/counter-zero-init.test.sh` (13 cases incl. positive control) wired
into `layer3.sh`. Over the real tree: STATUS=OK, required=68, exempt=6, entrypoints=5. Derivation-oracle
test + `metric_macros.rs` consumer-list doc-currency (frozen literal untouched).

**Phase 6 — GUARD 2** (fork): `dashboard_panels.rs` window arm made panel-type-conditional
(`counter_window` rule; `:592` counter_misuse untouched) + `expected_empty_description` rule +
`expected_empty_set_empty` collection-vacuity control, all off the shared reader. 13 dashboard_panels tests.

**Guard totals**: `cargo test -p dt-guard --lib` → 545 green. Both guards STATUS=OK over the real tree.
Also: `mc-service.md` error_type roster de-drifted (points at `error_type_label()`); `docs/TODO.md`
follow-ups filed (hoist-to-`ALL`; promtool alert-exerciser for the six un-muting alerts + OPS-17 residual;
dt-guard subcommand-split re-debate).

### Done since first "Ready" + Gate-2 attempt-1 (fmt)
- **Layer-7 env-test** WRITTEN: `crates/env-tests/tests/30_observability.rs::zero_initialized_counter_is_present_at_zero_on_a_running_pod` (the OPS-15 deployed-artifact evidence — @test CONFIRMED `mc_actor_panics_total{controller}` is structurally unreachable in prod, so a rendered 0 can only be zero-init's product). Incorporates @test's F1 (for-ALL pods, not `.first()`), F2 (distinct `TRIAGE_ZI_BADVALUE` token), F3 (`up{mc}==1` + target-presence both `assert_eventually`-polled). Compiles + clippy clean.
- **db_queries reversal** (team-lead + semantic-guard): `gc_db_queries_total` (46 = 23 ops × {success,error}) and `ac_db_queries_total` (24 = 12 observed (op,table) pairs × status) moved from exempt to OBSERVED PAIR-SET zero-init (NOT cross-product — no fabricated impossible series); catalogs express the pair-set for leg-1. exempt 8→6, required 66→68.
- **OPS-18 DISSOLVED** (operations): after the re-audit, `mh_errors_total` is the only expected-empty ∩ unbounded-exempt counter and it appears on ONE panel — a **timeseries** (mh-overview id27), not a stat, so no "No data" tile. The intersection-with-a-stat set is EMPTY; nothing reverses the user's noValue ruling, no carve-out needed. Escalation dropped.
- **OPS-9 done**: the four operations-specified sites + the widened-sweep false-fragment sites (gc-service.md org_* absence-semantics, gc-deployment.md:1242, mh-overview id32 PolicyApplyOutcome, client-media id3/8 MH-board cross-ref, mh-media id7 ratio justification) corrected. The shared `empty is healthy` marker phrase left as-is (soft but not false; a blanket "absent is a fault" clause would be WRONG on the exempt-counter panels — per-variant refinement is a follow-up flagged to operations).
- **infra F4/F5**, **security db_queries-honest-class + pair-set catalog note**, **observability ADR token-artifact note** — all landed.

Tree quiesced + `cargo fmt --all` clean; layers 1-2 green locally; all 5 guards STATUS=OK; self-test 13/13 (release binary); wrapper end-to-end OK.

### Gate-3 SECURITY BLOCKING fix — `actual_type` cardinality clamp (2026-09-14)

Security-B truthfulness finding (team-lead-confirmed): `{mc,gc,mh}_caller_type_rejected_total{actual_type}` forwarded the **peer-supplied** `claims.service_type` JWT claim (`Option<String>`, only the signature validated) RAW into a Prometheus label at the three Layer-2 rejection emit sites. Since the metric only records on the reject branch, any signature-valid peer could drive **unbounded `actual_type` cardinality** → exporter-memory DoS. The closure notes + help text asserting a "closed literal domain / 24 max" were therefore **untruthful**.

**Fix — clamp the class (mc + gc + mh), one shared mechanism:**
- **New SSoT** `common::service_type` — `SERVICE_TYPE_IDENTITIES` (the 3 recognized identities) + `service_type_metric_label(Option<&str>) -> &'static str`. Recognized identity → preserved (real service-confusion diagnostics kept); present-but-unrecognized → `other`; absent → `unknown`. Return is always one of N+2 `'static` constants, never peer bytes — bounded **by construction**, independent of the claim.
- **Bucket choice = distinct `other`, not fold-into-`unknown`.** `unknown` already means "claim absent" (the `unwrap_or` fallback); `other` means "present but unrecognized" — a stronger tampering signal, worth keeping separable for triage. Per the task's rule, `other` was added to each service's zero-init const **and** catalog so present-at-0 + GUARD-1 leg-1 symmetry hold.
- **Auth behavior IDENTICAL** — rejection unchanged; `tracing::warn!` still logs the RAW `actual_type`. Only the metric label is clamped.
- **Consts/catalog reconciled:** mc `CALLER_TYPE_REJECTIONS` now stands up every recognized identity except the expected one + `unknown` + `other` per path (also fixes the pre-existing catalog-lists-`meeting-controller`-but-const-didn't gap); gc `CALLER_ACTUAL_TYPES` → 3 identities + `unknown` + `other` (adds the previously-missing `global-controller`); mh drops the never-reachable `auth-controller` (now clamps to `other`) and adds `other`. Catalogs + the mc label-summary table + the three closure notes + the `record_caller_type_rejected` "24 max" doc rewritten to cite the clamp as the bound.
- **Truthfulness of "bound":** JWKS auth is now documented as defense-in-depth, **the clamp is the bound.**

**Verification (real tree):** `common` unit tests (recognized/absent/unrecognized/bounded-domain) green; mc/gc/mh lib tests green (243 mc-run shown, all pass); NEW end-to-end clamp tests through the real interceptor — mc `auth_layer_clamps_unrecognized_actual_type_to_other`, mh `unrecognized_service_type_clamps_metric_label_to_other` (forged claim → `other`, forged string asserted delta 0) — green; `cargo test -p dt-guard --lib` 554 green; `validate-counter-zero-init` STATUS=OK (services=4 required=68 exempt=6 entrypoints=5), `validate-dashboard-panels`/`validate-application-metrics`/`validate-metric-coverage` STATUS=OK; clippy clean on all four crates.

**Non-blocking spot-check (item 4):** all four render tests already assert a series-count ceiling band (ac 15..=200, mc 60..=400, gc 60..=300, mh 30..=200) — no change needed.

**GC e2e note:** GC has no gRPC-layer-driving test harness (its caller_type integration is wrapper-level `record_*`, and the full real-recording-site drive is deferred per that file's WRAPPER-CAT-C comment). The GC clamp is the *same* `common::service_type` fn (unit-tested) invoked identically to mc/mh (both e2e-covered), so GC clamp behavior is covered transitively; a GC interceptor-level clamp test rides the deferred harness item.

### Gate-3 consolidated review batch — items 2–10 (2026-09-14)

**TEST-F1 (ESCALATED → FIX) — vocab-const drift control.** The designed leg-1 *symmetric catalog-VALUES ↔
emitted-VALUES* check was never wired (GUARD 1 is name-level; the render test is hardcoded present-at-0 probes +
a count band). Judgment call, team-lead's authorized **minimum**: full leg-1 is disproportionate AND fragile
here — the catalog documents several counters' label values non-exhaustively (`mc_session_join_failures_total`
uses "e.g."), so a value-parsing comparison would false-positive until the catalog is normalized. Shipped
instead: (1) **`JOIN_FAILURE_ERROR_TYPES` enum-witness drift test** (`join_failure_error_types_tracks_mcerror_enum`,
mc) — a wildcard-free exhaustive match over `McError` ties the hand-listed join-reachable subset to the enum:
a new variant fails to COMPILE (must be classified in/out), labels are read from `error_type_label()` so a
rename is caught. Directly closes the named egregious case. (2) **`diff_label_sets` comparison primitive + its
fire test** (`diff_label_sets_fires_on_missing_and_extra`, mc) — reds on a missing combo AND an extra combo, the
plan's does-it-fire requirement, ready to wire once the catalog is machine-readable. The §Reviewer Requirements
Ledger (OPS-6 + the semantic-4/test-1/2/3 line) was corrected to record this honestly (was falsely marked
"catalog-derived, symmetric, all compile-witness-derived"). Follow-up filed in `docs/TODO.md`: normalize catalog
value lists → wire full per-service symmetric leg-1.

**TEST-F1 addendum — RejectionReason proto witness (test re-review escalation, team-lead ruled FIX).** The
plan (§Planning, proto enums) promised a `const VARIANTS` + wildcard-free witness for BOTH `SlotState` and
`RejectionReason`; `SlotState` shipped one (`slot_slot_state`) but `RejectionReason` did not, leaving
`ASSIGNMENT_REJECTION_REASONS` (mc `metrics.rs`) with a **silent proto-regen drift path**: `rejection_reason_label`
(`grpc/mc_service.rs`) is wildcard-free, so a regen adding `RejectionReason::Foo` reds the mapping → a dev adds
`Foo => "foo"` → a new `reason="foo"` series emits → but the string const stays stale → that combo is never
zero-init'd (`increase()` reads 0 forever) — this devloop's exact defect. (Unlike `MH_STATUS_STATES`, whose
caller has an `unspecified` catch-all, and unlike the pure-literal siblings deferred to full-leg-1, this is the
lone proto-backed const with a silent-regen path.) FIX (the JOIN/SlotState pattern): added non-test
`const REJECTION_REASONS: &[RejectionReason]` + wildcard-free `slot_rejection_reason` index-map witness (in the
keep-live `const _` block, same load-bearing doc-lines) so a new proto variant fails to COMPILE in-file; made
`rejection_reason_label` `pub(crate)`; and added `assignment_rejection_reasons_track_the_rejectionreason_enum`
(beside the JOIN test) asserting the enum-derived label set == `ASSIGNMENT_REJECTION_REASONS` (reds on a
new/renamed variant AND a stale const). mc lib 413→414; validate-counter-zero-init still OK (its `slot_*`-witness
predicate now also covers `slot_rejection_reason`). Closes the §Planning proto-enum witness gap.

**TEST-F2 — two missing self-test cases** added to `scripts/guards/counter-zero-init.test.sh` (now 19 assertions,
was 15): (10) scope-boundary — a catalogued+emitted lazy counter planted in a NON-canonical `xc-service` sibling
stays GREEN (catches walk-root widening); (11) output-shape — a failing root's findings are each exactly one
`VIOLATION:` line, no `VIOLATION:`/`ERROR:` on a continuation line (run-guards `head -5` safe).

**OBS-F1 + INFRA-F1 (coherent, both touch `is_category_b` in `dashboard_panels.rs`).** INFRA-F1: `is_category_b`
now tests for `/` on the expr with label-matcher `{…}` spans stripped (new `strip_label_matchers` + `LABEL_MATCHER_RE`,
mirroring the phantom-`[45]` defense), so a `/` inside `{path="/health"}` no longer misclassifies a Category-A
`*_total` stat as a ratio and skips `COUNTER_WINDOW_RULE_ID` (false negative) — fixture
`category_a_stat_with_slash_in_label_matcher_still_fires_counter_window`. OBS-F1: the `expected_empty_refs`
accumulation is now gated on `!is_category_b`, so a Category-B ratio (legit 0/0 empty) is not forced to carry the
`zero is healthy` present-at-0 marker — fixture `category_b_ratio_over_expected_empty_counter_does_not_require_marker`.
The self-contradictory marker was removed from `mh-media.json` panel 7 (Egress Delivery Ratio), keeping its
accurate 0/0 prose. dt-guard lib tests 554→556.

**OPS-14 — dual-end back-pointer** added at `gc-alerts.yaml` `GCTelemetryProxySilent` (~4-line comment): the
`absent_over_time` clause is why `gc_telemetry_ingest_total` is `Zero-init: exempt`; do not add it to
`zero_initialize_counters()`. The catalog side already cited `gc-alerts.yaml:144`, so the coupling is now visible
from both ends.

**CODE-F1** — the orphaned `use common::observability::testing::MetricAssertion;` in `ac` `metrics.rs` moved up
beside `use super::*;`.

**OBS-F3 (adopted from CODE non-blocking) — honest reframing of `production_builder_has_no_idle_timeout`.** The
test renders synchronously at t≈0, so a realistic minutes-scale `idle_timeout` would not reap first: it does NOT
pin idle_timeout-absence, it duplicates the present-at-0 render assertion. Doc/message-only fix (not hermetically
fixable — no readback API): the 4 test doc-comments + assert messages + the 4 `configured_prometheus_builder`
module docs now state only what is proven (present-at-0 under the exact prod config) and record the honest
residual (idle_timeout-absence = config-review checkpoint, not test-enforced; neither this test nor the env-test
backstops it). The §Implementation Summary lines that called the pin "load-bearing"/"permanent guard" were
corrected.

**SEC non-blocking spot-check (item 7):** confirmed — all four render tests already assert a series-count band
(ac 15..=200, mc 60..=400, gc 60..=300, mh 30..=200); nothing added.

**Verification (non-cluster, real tree):** mc/gc/ac/mh lib tests green (413/371/394/243); `cargo test -p dt-guard
--lib` 556 green; self-test 19/19 (release binary); `validate-counter-zero-init` STATUS=OK (req=68 exempt=6
entrypoints=5), `validate-dashboard-panels` STATUS=OK (16 files); clippy clean on all six crates. Layer-7
env-tests not run (docker/cluster down post-crash; the zero-init env-test passed on a real cluster earlier).

---

## Files Modified

- `crates/{mc,gc,ac,mh}-service/src/observability/metrics.rs` — `configured_prometheus_builder()` +
  `zero_initialize_counters()` + containment witnesses + render/idle_timeout tests.
- `crates/{mc,gc,ac,mh}-service/src/main.rs` — call `zero_initialize_counters()` after recorder install.
- `crates/dt-guard/src/common/metric_catalog.rs` — shared annotation reader (+ tests).
- `crates/dt-guard/src/counter_zero_init.rs` (NEW) — GUARD 1 (+ tests + derivation oracle).
- `crates/dt-guard/src/dashboard_panels.rs` — GUARD 2 window + expected-empty rules (+ tests).
- `crates/dt-guard/src/{lib.rs,main.rs}` — module + clap arm.
- `crates/dt-guard/src/metric_macros.rs` — consumer-list doc-currency (frozen literal untouched).
- `scripts/guards/simple/validate-counter-zero-init.sh` (NEW wrapper), `scripts/guards/counter-zero-init.test.sh`
  (NEW self-test), `scripts/layer3.sh` (wire self-test).
- `docs/decisions/adr-0029-dashboard-metric-presentation.md` — amendment.
- `docs/observability/metrics/{mc,gc,ac,mh}-service.md` — Zero-init/Expected-empty annotations + drift fix.
- `infra/grafana/dashboards/{mc,gc,ac,mh}-overview.json, mh-media.json, errors-overview.json` — 2 window fixes
  + empty-is-healthy descriptions.
- `docs/TODO.md` — follow-up entries.
- **(Gate-3 F1 clamp)** `crates/common/src/service_type.rs` (NEW SSoT + tests) + `crates/common/src/lib.rs` (mod decl); `crates/{mc,gc,mh}-service/src/grpc/{auth_interceptor,auth_layer}.rs` (clamp at emit site); `crates/{mc,gc,mh}-service/src/observability/metrics.rs` (consts + closure notes + `record_caller_type_rejected` doc); `docs/observability/metrics/{mc,gc,mh}-service.md` (actual_type value list + cardinality truthfulness); `crates/{mc,mh}-service/tests/auth_layer_integration.rs` (clamp e2e tests); `docs/TODO.md` (F1 marked done + ac `ServiceType` DRY follow-up).
- **(Gate-3 batch, items 2–10)** `crates/mc-service/src/observability/metrics.rs` (TEST-F1: `JOIN_FAILURE_ERROR_TYPES` enum-witness drift test + `diff_label_sets` primitive & fire test; TEST-F1/RejectionReason: `REJECTION_REASONS` const + `slot_rejection_reason` witness + `assignment_rejection_reasons_track_the_rejectionreason_enum` test) + `crates/mc-service/src/grpc/mc_service.rs` (`rejection_reason_label` → `pub(crate)`); `crates/dt-guard/src/dashboard_panels.rs` (INFRA-F1 `strip_label_matchers`/`LABEL_MATCHER_RE` + `is_category_b` de-matched; OBS-F1 `!is_category_b` gate; 2 fixtures); `infra/grafana/dashboards/mh-media.json` (OBS-F1: panel 7 contradictory marker removed); `scripts/guards/counter-zero-init.test.sh` (TEST-F2: scope-boundary + output-shape cases); `infra/docker/prometheus/rules/gc-alerts.yaml` (OPS-14 back-pointer); `crates/ac-service/src/observability/metrics.rs` (CODE-F1 import move); `crates/{ac,gc,mc,mh}-service/src/observability/metrics.rs` (OBS-F3: 4 idle-test docs/messages + 4 `configured_prometheus_builder` module docs).

**Commit trailer note**: `counter_zero_init.rs`, `dashboard_panels.rs`, `metric_catalog.rs` carry
`Approved-Cross-Boundary: infrastructure <reason>` (guard machinery). clap arm / lib mod / wrapper are Mechanical.

---

## Gate 2 (Validation) Results — authoritative run 2026-09-14 (post quota-pause recovery)

Full `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh`. `TOTAL_RESULT=FAIL`, but **every failure is pre-existing / environmental and unrelated to counter-zero-init**; all layers that actually exercise this diff are green.

| Layer | Result | Notes |
|-------|--------|-------|
| 1 compile | OK | |
| 2 fmt | OK | |
| 3 guards | OK | all guards incl. counter-zero-init (68 req / 6 exempt / 5 entrypoints), dashboard-panels (16 files), cross-boundary-scope/classification |
| 4 tests | tests PASS (172 `test result: ok`, 0 failed — mc/gc/ac/mh/dt-guard/env-tests) | aggregate reported `N/A` (proto intentional-gap placeholder dominates the worst-child roll-up); **substance = pass**. Reporting nuance to flag to operations, not a test failure. |
| 5 clippy | OK | (Layer-5 masked-panic in the four zero-init tests was fixed pre-recovery) |
| 6 audit | **FAIL — pre-existing/environmental** | `cargo-audit`: RUSTSEC-2026-0285 on **`rustls`** — a newly-published advisory (during the Sep 11-14 pause) against a pre-existing workspace dep; NOT introduced here (no Cargo.toml dep changes). `pnpm-audit`: GHSA-7w5x-hrqm-74c2 `smol-toml` HIGH — pre-existing, entered via earlier web-app commits' pnpm-lock. Both branch-wide, routed to security/operations. |
| 7 env-tests | **Rust env-test PASS; browser-e2e FAIL — pre-existing** | **`zero_initialized_counter_is_present_at_zero_on_a_running_pod` PASSED on the real cluster** — the end-to-end proof that zero-init reaches `/metrics`. Browser-e2e: 9/10 specs fail with one shared root cause, `"SIGNALING: Signaling transport closed"` during `connecting-mc` — the in-progress media/join WebTransport flow (this feature branch's own work), not a metric assertion and not in any file this diff touches. Likely branch-in-progress state or cluster degradation over the multi-day pause. Routed to operations/infrastructure for env-vs-regression triage. |

**Lead determination (first recovery run):** counter-zero-init is fully validated (compile, fmt, all guards, clippy, workspace tests, and the Layer-7 real-cluster zero-init proof). The `TOTAL_RESULT=FAIL` was entirely pre-existing branch/environmental (2 dependency advisories + a WebTransport-signaling browser-e2e failure), none introduced by or related to this diff.

### Final authoritative validation — 2026-09-14 (post-crash recovery, final tree)

After the user reported the audit + certificate branch issues were fixed, and after all Gate-3 fixes landed, the pipeline was re-validated on the final tree. The WSL host is memory-constrained (a full `layer-all.sh` with the cluster up was OOM-killed at L4), so layers were validated individually; **every layer is green on the final tree**:

| Layer | Result | Notes |
|-------|--------|-------|
| 1 compile | OK | |
| 2 fmt | OK | |
| 3 guards | OK | full `layer3.sh` green (`guards-passed`) after the Gate-3 scope-drift fixes; counter-zero-init 68 req / 6 exempt / 5 entrypoints; dashboard-panels 16 files; cross-boundary-scope + classification clean |
| 4 tests | PASS | mc 414 / gc 371 / ac 394 / mh 243 lib, dt-guard 556, self-test 19/19, + Layer-7 env-tests 104+ passed |
| 5 clippy | OK | clean, 6 crates |
| 6 audit | **OK** (was FAIL) | `cargo-audit-passed` + `pnpm-audit-passed` — the rustls RUSTSEC-2026-0285 and smol-toml GHSA-7w5x-hrqm-74c2 advisories are cleared (user's audit fix landed, commit `fe8f0948`) |
| 7 env-tests + browser-e2e | **OK** (browser-e2e was FAIL) | `zero_initialized_counter_is_present_at_zero_on_a_running_pod` PASSED on a freshly-recreated cluster (deployed-artifact present-at-0 proof); **browser-e2e 10 passed** — the signaling/certificate suite that was 9/10 failing is now green (user's cert fix landed) |

**Final Lead determination: Gate 2 GREEN.** Both prior branch-level failures (dependency advisories, browser-e2e signaling/cert) are resolved on the final tree. Cluster was torn down + recreated fresh during recovery (the crash left the old cluster's nodes dead); no persistent state. Validated layer-by-layer due to the host OOM constraint, not a full single-pass run — each layer carries an independent OK.

---

## Code Review Results

**Gate 3 — all 8 reviewers cleared, zero ESCALATED, zero blocking findings remaining.**

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | Blocking `actual_type` peer-claim cardinality-bomb (mc/gc/mh) — closed via `common::service_type` clamp SSoT (bounded by construction, N+2 static labels); closure notes + "24 max" help text made truthful |
| Test | RESOLVED-DEFERRED | 3 | 3 | 1 | F1 JOIN + F1 RejectionReason drift witnesses added (both re-escalated then fixed; RejectionReason closed the main.md:192-194 gap); F2 self-test scope-boundary + output-shape cases. Deferred: full symmetric leg-1 (blocked on catalog value-list normalization; `diff_label_sets` primitive ships tested-for-firing) |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | mh-media panel-7 ratio marker contradiction (guard gated on `!is_category_b`); OPS-14 back-pointer; idle_timeout-pin coverage-overstatement reframed honestly |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | ac `use` placement; approved new `common/service_type.rs` (ADR-0011 clause-c / ADR-0003, common-outside-Guarded → code-reviewer + DRY co-sign) |
| DRY | RESOLVED-DEFERRED | 0 | 0 | 0 | 0 true-duplication; 2 extraction opportunities filed (TOKEN_ERROR_CATEGORIES triplication; ac `ServiceType` route-through `common::SERVICE_TYPE_IDENTITIES`) |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | OPS-14 dual-end back-pointer. OPS-16 six-alert "triage-don't-silence" note = PR-body commit-gate |
| Semantic Guard | CLEAR | 0 | 0 | 0 | Fail-closed marker parse, const-drift compile-witness, vacuity controls all confirmed |
| Infrastructure | RESOLVED-FIXED | 1 | 1 | 0 | INFRA-F1 `is_category_b` label-matcher-strip (latent false-negative) fixed with biting fixture; machinery co-signed; `Approved-Cross-Boundary: infrastructure` trailers required at commit |

Two reviewers landed RESOLVED-DEFERRED (Test, DRY) — see §Accepted Deferrals.

---

## Accepted Deferrals

- **Full symmetric leg-1** (catalog-label-VALUES ↔ emitted-VALUES, end-to-end) — Test. Blocked on catalog value-list normalization (some catalog entries document values non-exhaustively via "e.g.", so a value-parse would false-positive today). The `diff_label_sets` primitive ships tested-for-firing for when it is wired; the two proto/enum-backed consts (JOIN_FAILURE_ERROR_TYPES, ASSIGNMENT_REJECTION_REASONS) are now witness-covered, and the pure-literal-emit siblings have no silent-regen path. → `docs/TODO.md` §Observability Debt.
- **TOKEN_ERROR_CATEGORIES triplication** (mc/gc/mh restate `common::token_manager::error_category()`) — DRY. Needs a public enumerable surface on `token_manager.rs` (a §6.4 GSA auth/crypto primitive → auth-controller + security co-sign). → `docs/TODO.md` §Cross-Service Duplication.
- **ac `ServiceType` route-through `common::SERVICE_TYPE_IDENTITIES`** — DRY (from the SEC-1 fix). Fixing now = a non-owner refactoring auth-controller's security-typed enum (§6.3 Domain-judgment); residual is safe-by-construction (drift → `other`, no cardinality/security regression). → `docs/TODO.md:251`.

**PR-body commit-gate (OPS-16):** the six alerts that un-mute after this deploy (`MCActorPanic`, `MCMediaPolicyDivergence`, `MCSenderIdSpaceExhausted`, `GCMeetingCreationOrgStateInvalid`, `MHCallerTypeRejected`, MH sender-binding) must be named in the PR body with "expect new pages — triage each against its runbook scenario, do NOT silence; silencing restores the exact defect this PR fixes."
