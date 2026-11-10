# Devloop Output: GC AcquireMediaHandler (Story 4 task GC-3)

**Date**: 2026-11-10
**Task**: Story 4 task GC-3 — GC AcquireMediaHandler rpc + internal.proto slice, select(region,count,exclude), max_exclude_handler_ids cap, replacement telemetry, R-15 GC auth pin, gray-failure TODO
**Specialist**: global-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired-with protocol, test; + database, auth-controller conditional reviewers)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h30m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5fb66877ee691e1e5a42682ae21c108e6c01370d` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `spawned` |
| Implementing Specialist | `global-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Paired Protocol | `spawned` |
| Database | `spawned` |
| Auth Controller | `spawned` |
| Infrastructure | `spawned (mid-loop, dt-guard machinery owner)` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/internal/v1/internal.proto` | Not mine, Domain-judgment (GSA; paired-protocol co-implements) — `rpc AcquireMediaHandler` on `GlobalControllerService`; `AcquireMediaHandlerRequest`/`Response`; `RegisterMCResponse.max_exclude_handler_ids = 5`; ReleaseCause validity block line extended (unknown numbers). Additive only. auth-controller + security confirm per intersection rule | protocol |
| `packages/proto-gen/scripts/verify-codegen.sh` | Not mine, Minor-judgment (paired) — "Story 4 GC-3" presence block | protocol |
| `crates/proto-gen/tests/internal_roundtrip.rs` | Not mine, Minor-judgment (paired) — populated roundtrip; absent vs empty `replaced_handler_id`; default response `assignment == None`; default `max_exclude_handler_ids == 0` | protocol |
| `crates/mc-service/tests/gc_integration.rs` | Not mine, Minor-judgment — `acquire_media_handler` stub (`Status::unimplemented`) on MockGcServer (trait has no default methods) | meeting-controller |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` | Not mine, Minor-judgment — same stub on MockGc | meeting-controller |
| `crates/gc-service/src/grpc/mc_service.rs` | Mine — `acquire_media_handler` (thin: timer → inner → record once → Status); cap in `register_mc` from config | — |
| `crates/gc-service/src/grpc/mod.rs` | Mine — `validate_handler_id` + `MAX_HANDLER_ID_LENGTH` hoisted here (shared by MH + MC services) | — |
| `crates/gc-service/src/grpc/mh_service.rs` | Mine — use the hoisted validator | — |
| `crates/gc-service/src/grpc/auth_layer.rs` | Mine (auth-controller + security review) — test-only: AcquireMediaHandler path pinned (MH token → PERMISSION_DENIED, MC token → passes); no routing change | — |
| `crates/gc-service/src/services/mh_replacement.rs` | Mine — new: `acquire()` service fn, `ReplacementTrigger` projection, `GcView` + pure `classify()` | — |
| `crates/gc-service/src/services/mh_selection.rs` | Mine — `select(pool, region, staleness_bound, count, exclude, rng)`; `SelectionRng` seam; metric recording moved out | — |
| `crates/gc-service/src/services/mc_assignment.rs` | Mine — initial caller (count=N, empty exclude) records `gc_mh_selections_total`; DRY anchor rewritten | — |
| `crates/gc-service/src/services/mc_client.rs` | Mine — `MhAssignment::from` (shared mapping, DRY review F-DRY-2) | — |
| `crates/gc-service/src/services/mod.rs` | Mine — module decl / re-exports | — |
| `crates/gc-service/src/repositories/media_handlers.rs` | Mine (database reviews) — `AND NOT (handler_id = ANY($3::text[]))` before ORDER BY/LIMIT; `LOAD_BALANCING_CANDIDATE_COUNT` made `pub` for the exclude-before-LIMIT test | — |
| `crates/gc-service/src/repositories/meeting_assignments.rs` | Mine (database reviews) — new `get_active_assignment_region(meeting_id, controller_id)` (fetch_all LIMIT 2; 2 rows = loud error carrying context); its private `weighted_random_select` removed in favour of the shared kernel | — |
| `crates/gc-service/src/services/weighted_pick.rs` | Mine — new: the one weighted-random kernel + `SelectionRng` seam | — |
| `crates/gc-service/src/repositories/mod.rs` | Mine — re-exports | — |
| `crates/gc-service/src/config.rs` | Mine — `GC_MH_MAX_EXCLUDE_HANDLER_IDS` REQUIRED in `MhRegistryConfig` ([4,64], > initial count); `MH_REGISTRY_KEYS` → 5; table tests | — |
| `crates/gc-service/src/observability/metrics.rs` | Mine (observability reviews) — `gc_mh_replacement_requests_total`, `gc_mh_replacement_duration_seconds`, closed label enums, zero-init, buckets, new `gc_db_queries_total` operation | — |
| `crates/gc-service/tests/**` | Mine — new `mh_replacement_tests.rs`, `mh_replacement_metrics_integration.rs`; R-15 arms in `auth_tests.rs`; `common/jwt_fixtures.rs` helpers; `common/mh_registry.rs` ctor arg; trait stubs in `otel_grpc_inbound_continuity.rs`, `otel_request_span_tests.rs`; register_mc cap test; selection callsite updates | — |
| `crates/gc-test-utils/src/reference_config.rs` | Mine — doc comments count five registry keys (operations review) | — |
| `infra/services/gc-service/config.env` | Mine — `GC_MH_MAX_EXCLUDE_HANDLER_IDS=16` with comment block | — |
| `infra/services/gc-service/deployment.yaml` | Not mine, Mechanical — one hard `configMapKeyRef` env entry | infrastructure |
| `crates/env-tests/tests/03_gc_deployment_config.rs` | Not mine, Minor-judgment (paired-test) — reads the hoisted mirror (5 keys) instead of its local undrift-checked list | test |
| `crates/env-tests/src/fixtures/gc_fleet.rs` | Not mine, Minor-judgment (paired-test) — `MH_REGISTRY_KEYS` mirror + drift test vs `config.rs` (same pattern as `MH_ENDPOINT_ALLOWLIST_KEYS`) | test |
| `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md` | Not mine, Minor-judgment (paired-test) — GC AcquireMediaHandler + token gate lines updated to what is implemented | test |
| `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md` | Not mine, Minor-judgment — GC line: `gc_view` gains `pending` + `unevaluated`, `trigger` gains `invalid` (only with `invalid_request`) | observability |
| `docs/observability/metrics/gc-service.md` | Not mine, Minor-judgment — two catalog entries; `gc_mh_selections_total` 1:1 note; new db operation | observability |
| `infra/grafana/dashboards/gc-overview.json` | Not mine, Minor-judgment — replacement panels (by outcome, trigger×gc_view, duration p50/95/99) | observability |
| `docs/runbooks/gc-deployment.md` | Not mine, Minor-judgment — config row (raise/lower procedure); Rollback note re MC-C | operations |
| `docs/TODO.md` | Mine — gray-failure quarantine entry (owner global-controller) | — |
| `docs/specialist-knowledge/global-controller/INDEX.md` | Mine — pointers | — |
| `crates/dt-guard/src/common/promql.rs` | Not mine, Domain-judgment (guard MACHINERY; Lead ruling Option A) — new: shared `mask()` (lifted from client_metrics_export), `LABEL_LIST_MODIFIERS` (one const), `strip_label_positions()`, `service_metric_refs()` + tests | infrastructure |
| `crates/dt-guard/src/common/mod.rs` | Not mine, Mechanical — module decl | infrastructure |
| `crates/dt-guard/src/common/services.rs` | Not mine, Domain-judgment (guard machinery) — `SERVICE_METRIC_PREFIX_RE` made `pub(crate)`, doc names the helper | infrastructure |
| `crates/dt-guard/src/common/kind_labels.rs` | Not mine, Domain-judgment (guard machinery) — `CLAUSE_KEYWORDS` = the shared `LABEL_LIST_MODIFIERS` | infrastructure |
| `crates/dt-guard/src/client_metrics_export.rs` | Not mine, Domain-judgment (guard machinery) — `prepare_expr` uses shared `mask()`; `PROMQL_GROUPING_RE` derived from the const | infrastructure |
| `crates/dt-guard/src/dashboard_panels.rs` | Not mine, Domain-judgment (guard machinery) — `metric_refs` via the helper; `LABEL_MATCHER_RE`/`strip_label_matchers` retired for the shared stripper; e2e test | infrastructure |
| `crates/dt-guard/src/application_metrics.rs` | Not mine, Domain-judgment (guard machinery) — dashboard + alert expr refs via the shared helper | infrastructure |

---

## Planning

### Mechanism restatement

Instance: "add AcquireMediaHandler". Mechanism: **one region-scoped, exclude-aware MH selection function with caller-owned telemetry**, plus a GC-owned bound pushed to MC. Same-owner siblings checked: the only other MH pick site is `select_mhs_for_meeting` (folded in). MC selection (`repositories::meeting_assignments::weighted_random_select`) is a different pool but the SAME weighted kernel (dry-reviewer): folded into one generic kernel (below). Second invariant touched: REQUIRED `GC_MH_*` keys are wired in config.rs → config.env → deployment.yaml → gc-test-utils → env-test 03; 03's local key list has no drift check today, so it is hoisted to a `gc_fleet.rs` mirror with a drift test (completes the invariant the allowlist keys already follow).

### 1. Proto (paired-protocol's slice, adopted as proposed)
- `RegisterMCResponse.max_exclude_handler_ids = 5` (uint32; GC-owned cap; 0/absent fails MC registration in MC-C; takes effect on MC re-registration).
- `rpc AcquireMediaHandler` on `GlobalControllerService` (inherits the existing prefix routing to `service_type=meeting-controller`, scope `service.write.gc`; no auth_layer change).
- `AcquireMediaHandlerRequest { meeting_id=1; controller_id=2; repeated exclude_handler_ids=3; ReleaseCause trigger=4; optional string replaced_handler_id=5; }`, `AcquireMediaHandlerResponse { MhAssignment assignment=1; }`. No region field. Header comment: exclude semantics, unknown ids ignored, duplicates count toward the cap, cap pointer, statuses, selection reads neither trigger nor replaced_handler_id, gc_view never a veto, controller_id self-asserted (anti-misroute only, never an identity). ReleaseCause validity block: "UNSPECIFIED, MEETING_ENDED and any unknown number are INVALID_ARGUMENT".
- Consumers in the same commit: verify-codegen.sh block; internal_roundtrip.rs pins; `Status::unimplemented` stubs on the 2 MC test mocks + 2 GC test mocks.

### 2. Config
`GC_MH_MAX_EXCLUDE_HANDLER_IDS` REQUIRED (no Rust default; one-line `MissingEnvVar` literal) inside `MhRegistryConfig` beside the initial count: `in_range` [4, 64] via MIN/MAX consts; cross-check `cap > initial_handlers_per_meeting` (error names both keys). `MH_REGISTRY_KEYS` → 5, so gc-test-utils' reference parse picks it up. config.env `=16` with comment; deployment.yaml hard `configMapKeyRef`; env-test 03 via the hoisted mirror. `register_mc` reads `state.config.mh_registry.max_exclude_handler_ids()` — the same accessor bounds the RPC length check (one source for push and enforcement). No config gauge (not in the telemetry contract; wiring is pinned by 03's hard-ref check).

### 3. Selection (R-23: one function)
- `MhSelectionService::select(pool, region, staleness_bound, count, exclude: &[String], rng: &dyn SelectionRng) -> Result<Vec<MhAssignmentInfo>, GcError>`; `Ok(empty)` = no eligible candidate (typed, so no_capacity vs DB error is structural — not a `ServiceUnavailable` coincidence). Pick loop unchanged (weighted by 1 − load_ratio, without replacement).
- `SelectionRng` (crate-owned trait, since ring's `SecureRandom` is sealed): `fn next_u64(&self) -> Option<u64>`; prod `SystemSelectionRng` wraps `SystemRandom` (CSPRNG-failure fallback unchanged); tests supply a seeded impl.
- **One weighted kernel (DRY, dry-reviewer):** `services/weighted_pick.rs`: `pub(crate) fn weighted_random_select<'a, T>(candidates: &'a [T], load_ratio: impl Fn(&T) -> f64, rng: &dyn SelectionRng) -> Option<&'a T>` + `SelectionRng`/`SystemSelectionRng` (owns the CSPRNG-failure fallback). MH `select` and MC assignment (`get_candidate_mcs` caller, passing `SystemSelectionRng`) both use it; both copies' tests move to the kernel; MC behaviour unchanged. `SelectionRng` is `pub` only because integration tests inject it (doc says so).
- **Metric recording leaves `select`.** The initial caller (`assign_meeting_with_mh`) records `gc_mh_selection_*` (success/error, has_multiple) and maps empty → `ServiceUnavailable` exactly as before; the replacement path records only `gc_mh_replacement_*`. DRY anchor rewritten: two callers; `gc_mh_selections_total` stays 1:1 with NEW assignments (recorded by the initial caller only); replacements are `gc_mh_replacement_requests_total`; the stickiness expression is unaffected.
- `get_candidate_mhs(pool, region, bound, exclude: &[String])`: `AND NOT (handler_id = ANY($3::text[]))` before ORDER BY, LIMIT → `$4`; empty array for initial. No Rust-side post-LIMIT filter. `LOAD_BALANCING_CANDIDATE_COUNT` → `pub` for the test.

### 4. RPC (`mc_service.rs` thin; logic in new `services/mh_replacement.rs`)
Outer handler `#[instrument(skip_all, name = "gc.grpc.acquire_media_handler")]`: start timer → `acquire(...)` returns `(Result<MhAssignmentInfo, Rejection>, ReportCtx)` with `Rejection = InvalidRequest|NotOwner|NoCapacity|Error` and one exhaustive `fn outcome(&Result) -> Outcome` (granted ⇔ Ok by construction) → record counter + histogram + event ONCE → map to Status. No `?` before recording. Order (cheapest reject first):
1. `exclude_handler_ids.len() > cap` → invalid_request (checked on raw length before per-id validation).
2. `validate_meeting_id`, `validate_controller_id` (existing McService validators); each exclude id and a present `replaced_handler_id` via `validate_handler_id` hoisted from mh_service.rs into `grpc/mod.rs` (shared); `ReleaseCause::try_from(i32)` → `ReplacementTrigger::from_release_cause` (exhaustive match, no wildcard; MEETING_ENDED/UNSPECIFIED → None; unknown int → None). Any failure → INVALID_ARGUMENT, bounded static message (no ids echoed).
3. Ownership: new `MeetingAssignmentsRepository::get_active_assignment_region(meeting_id, controller_id)` — `SELECT region … WHERE meeting_id=$1 AND meeting_controller_id=$2 AND ended_at IS NULL LIMIT 2`, `fetch_all`; 0 rows → FAILED_PRECONDITION (one generic message for no-meeting / ended / other-controller: no existence oracle), outcome not_owner; 2 rows → loud error log, INTERNAL, outcome error. (`get_healthy_assignment` keys on (meeting, region) and joins MC health — different semantics, hence a sibling.) Own `gc_db_queries_total` operation `get_active_assignment_region`. Doc comment: anti-misroute, not authentication (ADR-0039 §3, recorded gap). Region comes ONLY from this row.
4. gc_view: if `replaced_handler_id` present, `MediaHandlersRepository::get_handler` (existing; one read, also supplies the event's endpoint) → pure `classify(Option<(HealthStatus, age)>, bound)`. DB error here → outcome error (never silently `unregistered`).
5. `select(region, 1, exclude, rng)`: empty → RESOURCE_EXHAUSTED/no_capacity; DB error → INTERNAL/error (generic message; sqlx text logged server-side only); else OK with `MhAssignment`. Never reads trigger/replaced_handler_id/gc_view (not in its signature).
- **No write side effect**: a grant persists nothing (GC stores nothing per meeting) — bounding a forged controller_id to "learns an MH address" (recorded gap). No retry inside GC; existing pool timeouts; MC owns backoff.
- `McService` gets an `rng: Arc<dyn SelectionRng>` field: `new(state)` = system RNG; `with_selection_rng(state, rng)` is the test seam.

### 5. Label vocabularies (closed enums with `ALL` + `label()`, MhRegistrationOutcome pattern)
- `trigger`: `heartbeat_timeout | apply_lag | planned_transfer` (projected from ReleaseCause) **+ `invalid`** (only ever with outcome=invalid_request, for a request whose trigger itself is bad). Never a raw int.
- `outcome`: `granted | no_capacity | not_owner | invalid_request | error`.
- `gc_view`: `healthy`, `degraded`, `unhealthy`, `draining`, `pending`, `stale`, `unregistered` **+ `unevaluated`** (the request ended before the replaced-handler row was read: invalid_request, not_owner, or an error before step 4). `pending` added per observability (CHECK allows it; matches `gc_mh_available`). Precedence in `classify`: no replaced_handler_id or no row → unregistered; `age >= B` → stale (any status — a stale draining row is effectively gone; matches the SQL's fresh-iff `age < B`); else the status. Typed `HealthStatus` (existing `try_from_db_str`-backed) — an unparseable status is a contract fault: loud error, outcome error.
- Emitted (and zero-initialised) set, exactly: granted/no_capacity × 3 triggers × 7 evaluated views (42); error × 3 × 8 (24); not_owner × 3 × unevaluated (3); invalid_request × {3 + invalid} × unevaluated (4) = **73 series**; histogram `gc_mh_replacement_duration_seconds{outcome}` × 5. One `ReplacementLabels::ALL`-style enumerator feeds both zero-init and the render test (exact count asserted). Buckets: `Matcher::Prefix("gc_mh_replacement")` = 0.002…1.0 s. No handler_id/meeting_id/controller_id labels.
- telemetry-contract.md GC line updated to this vocabulary in the same commit (contract stays SSoT).

### 6. INFO event `gc.mh.replacement_report`
Exactly once per request whose `replaced_handler_id` is present AND well-formed, on every outcome (denominators for the log query). Structured fields: `handler_id` (replaced), `webtransport_endpoint` (only if the row was read and exists — `Option` field, omitted otherwise), `controller_id` (only if it passed validation), `trigger`, `gc_view`, `outcome`. Also `caller_sub` = the validated service token's `sub` (Claims extension from the auth layer; the one authenticated attribution field). `Option`: absent Claims (test-disabled auth layer, direct calls) omits the field — never unwrap/fail. A malformed replaced_handler_id → no event (raw input never logged), counted as invalid_request. No event when absent.
- **S9 evidence rule (security):** stated in the gc-service.md catalog, the TODO entry, and the event's doc comment (runbook Scenario 15 is OPS's; the rule is handed to it via the TODO text): only events whose outcome passed the ownership check (`granted|no_capacity|error`) count as gray-failure evidence, never `not_owner`/`invalid_request` (any MC-token holder can emit those naming any handler). Even owner-checked events carry a self-asserted `controller_id`, so "distinct MCs" is spoofable by anyone who knows a live (meeting_id, controller_id) pair — why automatic quarantine waits on authenticated MC identity; manual drain stays the operator's call.
- Event `trigger`/`gc_view`/`outcome` use the same `label()` strings as the counter (one source). Catalog states the emitted-combination rule in prose. No `?request`/metadata debug anywhere (bearer in metadata).

### 7. R-15 GC pin (`auth_tests.rs`, fixtures in `jwt_fixtures.rs`)
Fixtures: `TestKeypair::sign_claims<T: Serialize>(&self, claims, kid: Option<&str>)` (EdDSA) and `sign_hs256_claims(secret, kid, claims)` via jsonwebtoken `EncodingKey::from_secret` (test-only; guard:ignore with justification if a crypto guard trips — will report which). Claim set for arms 2/3 = media_dial claims {token_type=media_dial, sub, meeting_id, aud, iat, exp(valid)} ∪ a complete UserClaims set, so signature/alg is the only possible failure. Validator-level (CommonJwtValidator over the same wiremock JWKS, `validate::<UserClaims>`):
- Control: that claim set signed by the live AC key → Ok.
- Arm 1: header `{"alg":"EdDSA","typ":"JWT"}` (no kid) → `Err(MissingKid)`; JWKS request count unchanged (before/after delta).
- Arm 2: live kid, foreign Ed25519 key (`TestKeypair::new(2, live kid)`) → `Err(InvalidSignature)` (not KeyNotFound); JWKS consulted. Plus the literal media_dial-only claim set signed by the foreign key → rejected.
- Arm 3: live kid, real HMAC-SHA256 keyed with the served public key's 32 raw bytes → `Err(InvalidSignature)` from the EdDSA pin.
HTTP: every arm → 401 on the USER route `GET /api/v1/meetings/{code}` (require_user_auth; not `/me`); positive arm, AC-signed `TestUserClaims` on the same route → 404 meeting-not-found (auth passed; exact status and body code asserted).
Plus auth_layer unit test: MH token → `GlobalControllerService/AcquireMediaHandler` = PERMISSION_DENIED; MC token passes.

### 8. Tests (paired-test's table adopted)
`tests/mh_replacement_tests.rs` (driving `McService::acquire_media_handler` directly): {A,B,C}∖{A,B}→C; all excluded → RESOURCE_EXHAUSTED; unknown ids ignored (two arms); draining with lowest load never granted (+ fixed-seed positive); cap boundary with a non-16 cap (cap ok, cap+1 invalid); not_owner ×3 (ended, other controller, no row) + positive control; region from ownership row (other-region healthy MH never granted); invalid_request arms (meeting_id, controller_id, exclude id, replaced id, UNSPECIFIED, MEETING_ENDED, 99); DB error (closed pool) → INTERNAL/error; exclude-before-LIMIT (COUNT+1 rows strictly ordered, COUNT least-loaded excluded); MC-says-dead/GC-healthy → granted, gc_view=healthy; seeded equivalence over several seeds with ≥3 distinct-load candidates + a two-seeds-differ control; grant leaves `gc_mh_selections_total` unobserved while a new assignment records it once; register_mc returns a non-default cap. `tests/mh_replacement_metrics_integration.rs`: per-outcome cell deltas + adjacent unobserved + histogram count; render-based exact label-key set and id absence (with positive presence); event capture layer (exactly-once, fields, endpoint omitted with no row, no event without replaced id). Unit: `classify` table incl. age == B (stale) / B−1ms (fresh) and draining+stale, unhealthy+stale; trigger projection; config table (missing, 3/4/64/65, non-numeric, cap == initial=4 rejected, 5 accepted). Existing mc_assignment_rpc_tests/meeting_assignment_tests stay green.

- **Eligibility predicate (test-gates GC line, paired-test):** pure `is_selectable(&CandidateFacts, region, bound, exclude) -> bool` in mh_selection.rs (status == healthy ∧ age < B ∧ current < max ∧ row_region == region ∧ id ∉ exclude) + unit table (incl. age == B / B−1ms). `#[sqlx::test]` matrix: status{5} × age{B−1s, ≥B} × capacity{under, at} × region{own, other} × {excluded, not} — returned id SET of `get_candidate_mhs` == predicate-true set; vacuity guards: expected set non-empty and ≤ `LOAD_BALANCING_CANDIDATE_COUNT`, with 2–4 extra eligible rows (distinct ids/loads) so the expected set has several members (split across regions/runs if needed). Plus one initial-assignment `select` over the same matrix: every pick satisfies the predicate.
- Extra arms: (a) duplicates count toward the cap (cap duplicates granted, cap+1 invalid); (b) trigger-label pins — bad trigger (UNSPECIFIED, MEETING_ENDED, 99) → {invalid, invalid_request, unevaluated}; valid trigger + bad meeting_id → {projected, invalid_request, unevaluated}; both bad → `invalid` (trigger is decoded before any early return so the label is always determined); (c) same MC active in two regions → INTERNAL, outcome error, no grant; (d) DB error at the gc_view step vs at select: driven separately if separable (else outcome=error asserted with gc_view unevaluated for the lookup arm, documented); (e) malformed replaced id → INVALID_ARGUMENT + zero captured events (asserted); (f) event handler_id == replaced id ≠ granted id.
- Ownership: paired-test edits test-gates.md GC section after implementation; I own the 03/gc_fleet.rs edits (drift test proven to fail on a missing key).

### 9. Docs/ops
- gc-service.md catalog: two entries (labels, 73-series cardinality, disagreement query `trigger="heartbeat_timeout",gc_view="healthy"`, pointer to the event as the per-handler source); `gc_mh_selections_total` "1:1 with new assignments; replacements not counted here"; new db operation.
- gc-overview.json: requests by outcome; by trigger×gc_view; duration p50/p95/p99 (`increase`/`rate` over `$__rate_interval`, ADR-0029).
- gc-deployment.md: config row — MC derives attempt bound as cap − current set size; removed handlers stay excluded for the meeting's life (≈14 lifetime replacements at 16, then rate_limited); RAISE is safe and takes effect per MC on re-registration (roll MC after GC); LOWER = roll GC then MC right after — until each MC re-registers, its requests with exclude lists longer than the new cap fail, symptom `gc_mh_replacement_requests_total{outcome="invalid_request"}` rising. §Rollback: "Once MC-C is deployed, do not roll GC back past GC-3 without first rolling MC back past MC-C" (old GC sends cap 0 → MC registration fails fleet-wide).
- docs/TODO.md: gray-failure entry (agreed text) under §Media Path Obligations, owner global-controller (+ security), filed by ADR-0039 §3 / R4-5; manual mitigation named as "gc-incident-response.md Scenario 15 (gray-failure manual procedure; added by Story 4 OPS)" — no link until it exists.
- test-gates.md GC section updated to the implemented gates (paired-test).
- INDEX.md pointers.

### 10. Guard machinery: PromQL label positions (Lead ruling, Option A; infrastructure-reviewed design)
The contract label `gc_view` collides with dt-guard's metric-reference extraction (`SERVICE_METRIC_PREFIX_RE` over whole exprs reads `by(trigger, gc_view)` as a metric). Fix in the machinery, not the vocabulary. New `crates/dt-guard/src/common/promql.rs` is the ONE PromQL lexing home: `mask()` (comments out, string bodies blanked; lifted from `client_metrics_export::prepare_expr`, which now calls it), `LABEL_LIST_MODIFIERS` (one const; `kind_labels` and `client_metrics_export`'s grouping regex derive from it), `strip_label_positions()` (`{…}` matchers + modifier label lists, on masked text), and `service_metric_refs()`, the only metric-ref extractor, used at all three sites (dashboard_panels; application_metrics dashboard + alert exprs) and for dashboard_panels' Category-B `/` test (retiring `LABEL_MATCHER_RE`). `SERVICE_METRIC_PREFIX_RE` is `pub(crate)`. Tests: positive (`by(trigger, gc_view)`, `{gc_view="x"}`, infix `by`, every modifier, comments, string args), negative (undefined metric outside a label position is still a ref; a brace in a string does not swallow; `by(…)` inside a quoted value is inert), the documented `__name__` limit pinned, and a `fired_rules` e2e (no `metric_not_in_code` for `by(gc_view)`; fires for an undefined metric).

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Protocol (paired) | confirmed |
| Database | confirmed |
| Auth Controller | confirmed |
| Infrastructure (added mid-implementation for dt-guard fix) | confirmed |

---

### Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | ASCII-only id validators (homoglyph drain-evidence) |
| Test (paired) | RESOLVED-FIXED | 2 | 2 | 0 | caller_sub positive arm; exact-set initial-select matrix |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | evidence rule for `error`; contract field list; panel filter |
| Code Quality | RESOLVED-FIXED | 5 | 5 | 0 | test oracle out of prod API; Status carried; no clones; lint allows; doc |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | is_fresh; From<MhAssignmentInfo>; validate_region hoist; sign_claims |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | four→five required keys prose |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |
| Protocol (paired, GSA owner) | CLEAR | 0 | 0 | 0 | GSA sign-off internal.proto |
| Database | CLEAR | 0 | 0 | 0 | |
| Auth Controller | RESOLVED-FIXED | 1 | 1 | 0 | caller_sub is shared MC client id wording |
| Infrastructure (dt-guard owner) | RESOLVED-FIXED | 1 | 1 | 0 | kind_labels import |

## Pre-Work

None.


---

## Implementation Summary

### {Priority/Category 1}
| Item | Before | After |
|------|--------|-------|
| {field/function} | {old} | {new} |

### {Priority/Category 2}
{Description of changes}

### Additional Changes
{Any other notable changes made during implementation}

---

## Files Modified

```
 43 files changed, 2098 insertions(+), 755 deletions(-) (tracked) + 8 new files (promql.rs, mh_replacement.rs, weighted_pick.rs, tests: mh_replacement_tests.rs, mh_replacement_metrics_integration.rs, common/event_capture.rs, common/mc_grpc.rs, this main.md)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (log `/tmp/devloop-layer-all-1.log`) — **TOTAL_RESULT=OK**, TOTAL_DURATION=1963s, no FMT_APPLIED.

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | |
| 2 Format | OK | |
| 3 Guards | OK | incl. dt-guard promql fix (gc_view label no longer read as metric) |
| 4 Test | OK | budget WARN (164s) |
| 5 Lint | OK | |
| 6 Audit | OK | cargo-audit preexisting advisories; no dep changes; buf breaking OK |
| 7 Env-tests | OK | env-tests-passed + browser-e2e-passed (1656s) |

Fast check (Lead, pre-review): TOTAL_RESULT=OK. Implementer re-ran layer-fast green after review fixes.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

---

## Accepted Deferrals

- (none surfaced in this devloop)

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `5fb66877`
2. Review all changes: `git diff 5fb66877..HEAD`
3. Soft reset (preserves changes): `git reset --soft 5fb66877`
4. Hard reset (clean revert): `git reset --hard 5fb66877`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit (proto + GC rpc + config/deployment key + docs move together; the REQUIRED config key and deployment.yaml ref must revert with the config.rs change). Rolling back after MC-C ships requires rolling MC back past MC-C first (see gc-deployment.md §Rollback). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
