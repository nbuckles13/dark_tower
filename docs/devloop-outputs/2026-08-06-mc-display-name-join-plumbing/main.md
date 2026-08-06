# Devloop Output: Plumb participant display name through MC join flow

**Date**: 2026-08-06
**Task**: Carry `display_name` from the validated meeting-token claims through MC's join path so roster entries render the registered name instead of the `Participant N` stopgap (MINOR-003). Investigate the `JoinResponse.user_id == 0` finding.
**Specialist**: meeting-controller
**Mode**: Agent Teams (full, headless run-story task #65)
**Branch**: `feature/user-story-run-test`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c1df740305b4d8399f19de670a1207f0247b6817` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (a49e0b66bc32e7491) |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

---

## Task Overview

### Objective
Task #63 minted the registered display name into the meeting token (AC) and MC has
`claims.display_name` in scope at the connection layer, but the join path never
carries it: `handle_join` hardcodes `format!("Participant {}", ...)` (MINOR-003
stopgap, `crates/mc-service/src/actors/meeting.rs:612`). Every roster entry renders
as `Participant N` regardless of the token. Plumb `display_name` from the validated
claims through `connection_join` → `ConnectionJoin` → `handle_join` → the
`Participant` record broadcast in `JoinResponse` / `ParticipantJoined`, keeping a
generic label ONLY as a genuine-absence fallback (empty claim).

Secondary (same plumbing): `JoinResponse.user_id` is hardcoded `0`
(`crates/mc-service/src/webtransport/connection.rs:986`) — bus-visible as uniformly
"0" across distinct users. Investigate in the join path: if a trivial dropped claim,
fix here; if design-scoped, add a `docs/TODO.md` entry with finding + file pointers.

### Scope
- **Service(s)**: mc-service only
- **Schema**: No
- **Cross-cutting**: No (all edits under `crates/mc-service/`)

### Debate Decision
NOT NEEDED — mechanical plumbing of an existing claim through an existing message
chain within a single service's actor model; no new architectural boundary.

### Acceptance
Task #60's retry (stashed E2E: rejoin + rendered-roster assertions that sign in as
distinct users and assert registered names render) goes green with no test weakening.
The #60 red tests are in `stash@{0}` and run at #60's retry, NOT in this session.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mc-service/src/actors/controller.rs` | Mine | — |
| `crates/mc-service/src/actors/messages.rs` | Mine | — |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` (fallback-outcome counter) | Mine | — |
| `docs/observability/metrics/mc-service.md` (metric catalog entry) | Mine | — |
| `infra/grafana/dashboards/mc-overview.json` (metric↔dashboard panel) | Minor-judgment | observability (dashboard/panel taxonomy; MC-service-owned per ADR-0031, hunk-ACK requested) |
| `crates/mc-service/tests/join_tests.rs` | Mine | — |
| `crates/mc-service/tests/disconnect_latency_integration.rs` (call-site arity) | Mine | — |
| `docs/TODO.md` (userId finding — design-scoped) | Mine | — |

No Guarded Shared Area paths. No `proto/`, `crates/common/`, or auth/crypto edits.
`display_name` is already in the minted token and validated claims (task #63 / AC);
this devloop only carries an existing field through MC-internal messages.

---

## Planning

### Diagnosis verification (read the actual code — all confirmed)

- `MeetingTokenClaims.display_name` is **`String`** (not `Option`), `crates/common/src/jwt.rs:383`.
  Fallback rule is therefore the **empty-string** case: `display_name.is_empty()` → generic
  label; any non-empty value is used verbatim. No `Option` handling needed.
- `handle_join` hardcodes `format!("Participant {}", self.participants.len() + 1)` at
  `meeting.rs:612` (MINOR-003 stopgap) — confirmed.
- `connection.rs:397` `controller_handle.join_connection(...)` passes `claims.sub.clone()`
  (user_id) but NOT `claims.display_name` — confirmed. `claims.display_name` is in scope.
- The 7-hop chain in the task is accurate: `connection.rs` → `controller.rs:127` handle →
  `messages.rs` `ControllerMessage::JoinConnection` + `MeetingMessage::ConnectionJoin` →
  `controller.rs:365` match arm → `meeting.rs:65` `connection_join` → `meeting.rs:458` match
  arm → `meeting.rs:548` `handle_join`.
- `build_join_response` (`connection.rs:950-955`) ALREADY maps `p.display_name` → proto
  `Participant.name` for `existing_participants`. So once `display_name` reaches the
  `Participant` record, the rendered roster is correct with NO extra change there. The
  `Participant.to_info()` → `ParticipantInfo.display_name` path (meeting.rs:284) already
  carries it through the `ParticipantJoined` broadcast too.

### Approach — carry the field through, empty-claim fallback at the sink

1. `connection.rs:~397` — add the token display name as a new arg to `join_connection`,
   **length-bounded at the trust boundary** (security + semantic-guard REQUIRED): pass
   `truncate_utf8(&claims.display_name, MAX_PARTICIPANT_NAME_LEN)` (the existing helper +
   256-byte constant already in this file, char-boundary safe). Truncate, do NOT reject —
   fail-safe / graceful. `display_name` has no field-level length bound in the token
   (`common/src/jwt.rs`; guest display_name is self-reported), so bounding it here stops a
   multi-KB name being broadcast to every peer (amplified DoS). This borrows the field (no
   `.clone()` needed — supersedes the move-vs-clone idiom point) and `claims` is dropped
   right after, so an empty claim stays empty → the empty-fallback still fires.
2. `controller.rs:127` `join_connection` handle — add `display_name: String` param; forward
   into `ControllerMessage::JoinConnection`.
3. `messages.rs` — add `display_name: String` field to `ControllerMessage::JoinConnection`
   (~L48) AND `MeetingMessage::ConnectionJoin` (~L70), with doc comments.
4. `controller.rs:~365` match arm — destructure `display_name`, pass to
   `meeting_handle.connection_join(...)`.
5. `meeting.rs:65` `connection_join` handle — add `display_name: String` param; forward into
   `MeetingMessage::ConnectionJoin`.
6. `meeting.rs:458` match arm — destructure `display_name`, pass to `handle_join`.
7. `meeting.rs:548` `handle_join` — add `display_name: String` param. Replace the L612
   hardcode with the fallback sink:
   ```
   let display_name = if display_name.is_empty() {
       format!("Participant {}", self.participants.len() + 1)
   } else {
       display_name
   };
   ```
   Fallback keeps the exact existing shape, ONLY for the genuine-absence (empty) case — not
   weakened, not applied to non-empty names. Also **replace the stale L611 MINOR-003 comment**
   (`// Create participant (MINOR-003: use generic display name, not derived from user_id)`)
   which becomes misleading once the name comes from the claim — new comment e.g.
   `// Prefer the token's display_name; fall back to a generic label only when the claim is empty`.

### PII / logging discipline (observability + security + semantic-guard REVIEW points — baked in)
- `display_name` is PII: it is NEVER added to any log or tracing span. `handle_join`'s
  `#[instrument(skip_all, fields(meeting_id = ...))]` stays as-is (no `display_name` in
  `fields(...)`). No existing "Participant joining/joined"/connection join log gains the field.
- The `ControllerMessage::JoinConnection` / `MeetingMessage::ConnectionJoin` enums derive
  `Debug` and now carry `display_name`, but verified NO `debug!(?msg)` / `?message` /
  whole-struct Debug-log exists on the join path (only `error = ?join_error` at
  controller.rs:715 / meeting.rs:1257, which log an error value, not the message) — so the
  derived Debug never reaches a sink. Re-verified at implementation.
- If a branch log is wanted, log ONLY the bounded categorical decision
  (`name_source = "claim" | "fallback"`), never the value. (Optional; likely omitted.)

### Secondary finding — `JoinResponse.user_id == 0` is DESIGN-SCOPED (TODO, not fixed here)

`JoinResponse.user_id` (`proto/dark_tower/signaling/v1/signaling.proto:97`) is **`uint64`**
("8-byte user ID for media frames"). The only user identity MC holds is the claim
`sub: String` (`common/src/jwt.rs`), a string subject id — there is **no numeric user id**
in the claims to drop in. Coercing a `String` sub into a `uint64` would be a lossy/incorrect
conversion (parse-or-zero, hash, etc.), which the task explicitly forbids. `proto/` is a
Guarded Shared Area (protocol) and out of scope for this MC devloop. → **Disposition: add a
`docs/TODO.md` entry** with the finding and pointers (`connection.rs:986`, proto field L97,
claim source `common/src/jwt.rs`), leave `user_id: 0` untouched. This is the correct
non-masking disposition: a real contract decision (numeric media-frame id vs string subject)
is owned by protocol + GSA, not hacked here.

### Tests (no weakening)

- `meeting.rs` inline `#[cfg(test)]`: ~14 `connection_join(...)` call sites gain a
  `display_name` arg (arity fix). DISTINCT-NAME discriminating test: two-participant join with
  distinct names ("Alice" for part-1, "Bob" for part-2); assert each renders to its OWN
  `participant_id` (not derived from count/position — guards the old counter bug AND an
  all-same-name regression). Plus a dedicated **empty-claim** fallback test passing
  `display_name = ""` explicitly, asserting EXACT equality `== "Participant 1"` (not
  `.contains`), genuinely exercising the `is_empty()` branch.
- `join_tests.rs`: 4 `join_connection(...)` call sites gain the arg. Strengthen
  `test_actor_level_second_joiner_sees_first_in_roster` with DISTINCT names — "Alice" for
  part-1, "Bob" for part-2 — and assert `result2.participants[0].display_name == "Alice"` (the
  FIRST joiner's OWN name; discriminating — proves it's the joiner's name, not count/position,
  which `== "Test Participant"` on two identical minter defaults would NOT). Source is
  `claims.display_name` (validated claim), never `join_request.participant_name` (client-supplied).
- `join_tests.rs` T10 broadcast (`test_participant_joined_notification_via_bridge`): REQUIRED
  broadcast-sink coverage — the #60 rendered-roster assertion for an already-present peer
  arrives via the `ParticipantJoined` BROADCAST, a SEPARATE sink from `JoinResult`. Override
  `claims2.display_name = "Bob"` (like the existing `claims2.sub` override) and assert the
  broadcast `ParticipantJoined` participant's `name == "Bob"` (currently only asserts
  `!participant_id.is_empty()`).
- `disconnect_latency_integration.rs`: 4 `connection_join(...)` call sites — arity fix only.
- `mc-test-utils/src/jwt_test.rs`: minter already sets `display_name`; no change unless a
  distinct-name fixture is needed for a new assertion.
- The `participant.rs` `display_name: "New User"` fixture is a `ParticipantInfo` literal (not
  a join-path call) — untouched unless a broadcast assertion needs it.

### Guards / gates
`cargo check -p mc-service` and `cargo test -p mc-service` after each step. Signature changes
are compile-enforced across all call sites (no silent drift). No metric/label, proto, or
crypto surface touched.

---

## Implementation Summary

Plumbed the token's `display_name` through MC's join path and removed the MINOR-003 stopgap.

Production (all `crates/mc-service/`):
- `webtransport/connection.rs`: at the boundary, `let display_name = truncate_utf8(&claims.display_name, MAX_PARTICIPANT_NAME_LEN);` and pass it as a new arg to `controller_handle.join_connection(...)`. Truncate-not-reject; UTF-8-safe; empty stays empty. `claims.sub` still used for `user_id`.
- `actors/messages.rs`: added `display_name: String` field (with doc comment) to `ControllerMessage::JoinConnection` and `MeetingMessage::ConnectionJoin`.
- `actors/controller.rs`: `join_connection` handle gains a `display_name` param → forwarded into the message; the `JoinConnection` match arm destructures it and passes to `meeting_handle.connection_join(...)`.
- `actors/meeting.rs`: `connection_join` handle + `ConnectionJoin` match arm + `handle_join` all gain the `display_name` param. Sink replaces the hardcode with the empty-only fallback:
  `let display_name = if display_name.is_empty() { format!("Participant {}", self.participants.len() + 1) } else { display_name };`
  and the stale MINOR-003 comment is replaced. `build_join_response` already maps `display_name` → proto `Participant.name`, and `Participant::to_info()` already carries it into the `ParticipantJoined` broadcast — no change needed there.

PII / logging: `display_name` never enters a log or span; `handle_join` keeps `#[instrument(skip_all, fields(meeting_id=...))]`. Re-verified no logging macro references `display_name` and no whole-struct Debug-log of the message enums exists on the join path (grep-clean). `cargo clippy -p mc-service --tests` is clean.

Secondary finding (`JoinResponse.user_id == 0`): DESIGN-SCOPED (proto `uint64` media-frame id vs claim `sub: String`; `proto/` is a GSA). Left `user_id: 0` untouched; recorded in `docs/TODO.md` with pointers. The pre-existing "MC-consumption spin-out" TODO for MINOR-003 is marked resolved.

Tests (no weakening) — 16 inline `connection_join` + 4 `join_connection` + 4 disconnect-integration call sites got the new arg (compile-enforced arity). Discriminating coverage added:
- `meeting.rs` `test_handle_join_uses_claim_display_name_per_participant`: distinct names (Alice/Bob), each asserted against its OWN `participant_id` via `get_state()`.
- `meeting.rs` `test_handle_join_empty_claim_falls_back_to_generic_label`: empty claim → exact `== "Participant 1"`.
- `join_tests.rs` `test_actor_level_second_joiner_sees_first_in_roster`: distinct Alice/Bob; asserts `participants[0].display_name == "Alice"` (first joiner's own name).
- `join_tests.rs` T10 `test_participant_joined_notification_via_bridge`: `claims2.display_name = "Bob Registered"` (distinct from the client `participant_name` "Bob" AND the minter default), asserts the `ParticipantJoined` broadcast `name == "Bob Registered"` — proves the token claim is the source, not the client-supplied name or a generic label.

Result: `cargo check -p mc-service --tests` clean; `cargo test -p mc-service` = 284 lib tests + all integration suites pass, 0 failed.

---

## Files Modified

Production:
- `crates/mc-service/src/webtransport/connection.rs` — boundary truncation + pass `display_name` to `join_connection`.
- `crates/mc-service/src/actors/messages.rs` — `display_name: String` on the two join message variants.
- `crates/mc-service/src/actors/controller.rs` — `join_connection` handle param + `JoinConnection` match arm. Adding `display_name` pushed the handle to 8 args; carries a localized `#[expect(clippy::too_many_arguments, reason=...)]` (ADR-0002 prefers `#[expect]` over `#[allow]`) with the `JoinConnectionParams` refactor tracked in `docs/TODO.md`.
- `crates/mc-service/src/actors/meeting.rs` — `connection_join` handle + `ConnectionJoin` match arm + `handle_join` param and empty-only fallback sink (MINOR-003 removed) + `mc_join_display_name_resolved_total{outcome}` emission at the sink + 2 new tests + inline call-site arity.
- `crates/mc-service/src/observability/metrics.rs` — `record_display_name_resolution(outcome)` bounded counter (observability finding: MC-consumer-side visibility of the empty-claim fallback; "fail loudly").

Docs:
- `docs/observability/metrics/mc-service.md` — catalog entry for `mc_join_display_name_resolved_total` (ADR-0032 emission↔doc coupling).

Dashboards:
- `infra/grafana/dashboards/mc-overview.json` — new "Display Name Resolution by Outcome" timeseries panel (id 51) for `mc_join_display_name_resolved_total`, satisfying the ADR-0031 metric↔dashboard coverage guard. Mirrors the "Participant Leaves by Reason" counter-rate-by-label convention: `sum by(outcome) (increase(...[$__rate_interval]))`, `{{outcome}}` legend (ADR-0029 rate presentation).
  - Minor-judgment hunk-ACK from observability (ADR-0024 §6.6). Commit trailer:
    `Approved-Cross-Boundary: observability mc-overview.json panel 51 matches ADR-0031 metric-dashboard-coverage + ADR-0029 counter-rate convention`

Tests:
- `crates/mc-service/tests/join_tests.rs` — call-site arity; strengthened roster + broadcast assertions; new `mc_join_display_name_resolved_total` component test (real actor path, present/fallback + `(0)` adjacency) satisfying the ADR-0032 metric-coverage guard.
- `crates/mc-service/tests/disconnect_latency_integration.rs` — call-site arity.

Docs:
- `docs/TODO.md` — `JoinResponse.user_id == 0` design-scoped entry added; MINOR-003 spin-out marked resolved.

---

## Devloop Verification Steps (Gate 2)

Final authoritative full-pipeline run (`./scripts/layer-all.sh`, attempt 5) — all layers green:

| Layer | Verb | Result | Notes |
|-------|------|--------|-------|
| 1 | Compile | OK | rust + ts + proto |
| 2 | Format | OK | |
| 3 | Guards | OK | 35/35 incl. `validate-application-metrics`, `validate-dashboard-panels`, `validate-metric-coverage`, `validate-metric-labels`, `validate-kustomize`, `validate-cross-boundary-scope`, `validate-cross-boundary-classification` |
| 4 | Test | OK | mc-service 284 lib + all integration suites, 0 failed |
| 5 | Lint | OK | clippy `-D warnings` clean (localized `#[expect(too_many_arguments)]`) |
| 6 | Audit | N/A (benign) | `cargo audit` / `pnpm audit` / `buf breaking` all OK; aggregate reports N/A |
| 7 | Env-tests | OK | Rust env-tests passed + browser E2E `join-happy-path` passed, on a real cluster |

Iteration notes: attempt 1 failed L3 (scope-drift on a speculative `participant.rs` plan row — removed); attempt 2 failed L5 (`too_many_arguments` — localized `#[expect]`); attempt 3 passed all 7; the observability finding was then folded in (new metric), so attempt 4 re-ran and failed L3 (`metric_no_dashboard` — added mc-overview.json panel 51); attempt 5 passed all 7. An earlier L7 `PRECONDITION_FAILURE` was a self-inflicted concurrency collision between two overlapping pipeline runs (a `pgrep`-based watcher gave a false "finished" — `pgrep` is not installed in this environment), NOT an infra defect; resolved by running a single clean pipeline with a `ps`-based wait.

---

## Code Review Results

All seven reviewer verdicts final — no escalations.

| Reviewer | Verdict | Findings |
|----------|---------|----------|
| Security | CLEAR | display_name length-bounded at the trust boundary (`truncate_utf8`, 256B) before any broadcast; no PII in logs/spans; fail-safe empty-claim fallback; no GSA touched. |
| Test | CLEAR | No test weakening; discriminating per-participant assertions (Alice/Bob own-name), exact empty-claim `== "Participant 1"`, T10 broadcast-sink `== "Bob Registered"` proving the token claim is the source. |
| Observability | RESOLVED-FIXED | PII containment clean. Raised + folded-in finding: silent empty-claim fallback → added bounded counter `mc_join_display_name_resolved_total{outcome=present\|fallback}` + catalog + component test + Grafana panel 51 (hunk-ACK'd). |
| Code Quality | RESOLVED-DEFERRED | ADR-0002/0001/0023 compliant; Ownership Lens all Mine (+ 1 observability-owned dashboard hunk). 2 Gate-1 findings fixed (needless clone→borrow; stale MINOR-003 comment). 1 accepted deferral: `JoinConnectionParams` refactor (`#[expect(too_many_arguments)]`), tracked in docs/TODO.md. |
| DRY | CLEAR | Fallback in exactly one hop; new field follows the existing user_id/display_name threading; no reimplementation of `common`; JoinConnectionParams extraction already TODO-tracked. |
| Operations | CLEAR | Backward-compat verified (pre-#63 empty-name token → `#[serde(default)]` "" → fallback, no panic/blank roster); zero config/manifest/rollback/topology surface; TODO hygiene good. |
| Semantic Guard | CLEAR | No credential/PII leak (claims Debug-redacts sub/display_name/jti); bounded wire input via truncate_utf8; fallback not fail-open; no error-context loss. |

---

## Accepted Deferrals

- `docs/TODO.md` §"MC join-handle arg count — JoinConnectionParams refactor (task #65)" — bundle the 8-arg join tuple into a struct so the `#[expect(too_many_arguments)]` can be removed (owner: meeting-controller).

Related design-scoped item recorded in `docs/TODO.md` (a scope decision, not a review deferral of the diff): §"JoinResponse.user_id == 0 — design-scoped type/semantics mismatch (task #65)" — proto `uint64` vs claim `sub: String`; needs a protocol/GSA contract decision, so `user_id: 0` was left untouched rather than lossy-coerced (owner: protocol).
