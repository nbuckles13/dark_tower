# Devloop Output: P1 — Story-4 internal contract (additive) + media_dial public-key allow-list entry

**Date**: 2026-11-06
**Task**: Story 4 task #5 (P1): additive internal.proto contract for MH replacement (ADR-0039 §1/5/6/8) + single dt-guard key allow-list entry
**Specialist**: protocol (paired-with security; auth-controller GSA co-reviewer)
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h (incl. ~23m Layer 7)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e188a5172b0bdd36332fbb7f734e85edafde68c1` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (protocol)` |
| Implementing Specialist | `protocol` |
| Tier | `full` |
| Iteration | `1` |
| Security (paired-security) | `verdict received` |
| Test | `verdict received` |
| Observability | `verdict received` |
| Code Quality | `verdict received` |
| DRY | `verdict received` |
| Operations | `verdict received` |
| Semantic Guard | `verdict received` |
| Auth Controller (GSA co-owner) | `verdict received` |

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
| `proto/dark_tower/internal/v1/internal.proto` | Mine (GSA owned by protocol; co-owners auth-controller and security plan and review; `media_dial_public_key` field comment is security's verbatim text) | protocol |
| `crates/dt-guard/src/internal_proto_no_key_material.rs` (allow-list entry + membership pin + real-tree tests) | Not mine, Domain-judgment (paired: security owns membership; infrastructure owns construct, untouched) | security |
| `crates/proto-gen/tests/internal_roundtrip.rs` | Mine | — |
| `packages/proto-gen/scripts/verify-codegen.sh` (presence oracle for new TS symbols) | Mine | — |
| `crates/mc-service/src/grpc/mh_client.rs` (struct literals: `heartbeat_interval_ms: 0`, `media_dial_public_key: Vec::new()`, `cause` from `EndMeetingCall`) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/src/media_routing/teardown.rs` (`EndMeetingCall.cause`, set `MEETING_ENDED` in `end_meeting_plan`) | Not mine, Minor-judgment | meeting-controller |
| `crates/mc-service/tests/meeting_teardown_integration.rs` (assert the wire `EndMeetingRequest.cause` is MEETING_ENDED through the real client) | Not mine, Minor-judgment | meeting-controller |
| `crates/mh-service/src/**` (test-only struct literals in `grpc/mh_service.rs`, `routing/tests.rs`) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/tests/**` (struct literals) | Not mine, Minor-judgment | media-handler |
| `crates/mh-test-utils/src/media_policy.rs` (struct literals) | Not mine, Minor-judgment | media-handler |
| `crates/env-tests/**` (fixture struct literals; explicit `cause`) | Not mine, Minor-judgment | test |
| `crates/dt-guard/tests/proto_ports_e2e.rs` (through-the-binary rename case, REASON token) | Not mine, Domain-judgment (paired; test content on the security allow-list) | security |
| `docs/TODO.md` (§Media Path Obligations: runtime pins for the new contract rules, owner task named per rule) | Mine | — |
| `docs/user-stories/2026-10-31-survive-handler-loss.md` (MC-D TESTS bullet: value-axis key check, security/semantic-guard request, Lead-applied) | Mine | — |
| `docs/decisions/adr-0039-mh-replacement-and-migration.md` (§1 erratum: replay overclaim, matching the proto fix from security F1; Lead-applied) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_contract_crossing_renamed_material.rs` (text-only fixture; stale "internal.v1 has NO bytes field" premise — its own TRIGGER note fires on this commit; comment-only correction) | Not mine, Minor-judgment | semantic-guard, security |

---

## Planning

### Problem, in mechanism language

Story 4's MC and MH tasks need one wire text to implement against: a per-(meeting, handler) liveness/lag report MH->MC, a verdict MC->MH that doubles as the retry for a lost release, one cause vocabulary shared by the release RPC / verdict / (later) AcquireMediaHandler, a per-meeting cadence MC hands MH, and a public verification key MH needs to check MC-minted dial tokens. Every one of these is ADDITIVE in proto3 (fresh tags, new types), so the only compile-forced consumer edits are prost struct literals. The wider class: **a field whose zero value is a reject** (serving_state, cause, verdict, heartbeat_interval_ms) is an ordering trap whenever enforcement can land before the producer emits non-zero (the `policy_generation` precedent). This plan closes it in-loop for `EndMeetingRequest.cause` (MC sends MEETING_ENDED now, single-sourced in `end_meeting_plan`) and documents it at `heartbeat_interval_ms` (MH-2 after MC-B1). The heartbeat request/response have no producer until MC-B1/MH-2 so no trap there. `ApplyRefusal` zero = "no refusal" is the one deliberately non-reject zero, stated at the enum.

### internal.proto (all fresh tags; buf STANDARD enum prefixes)

1. `enum ReleaseCause { RELEASE_CAUSE_UNSPECIFIED=0; _HEARTBEAT_TIMEOUT=1; _APPLY_LAG=2; _PLANNED_TRANSFER=3; _MEETING_ENDED=4; }` -- per-use UNSPECIFIED/MEETING_ENDED validity stated once here; single `trigger` vocabulary (lowercase suffix; UNSPECIFIED never a label value; misroute counted as MC heartbeat outcome `released_unassigned`); MH's `owner_silence` trigger is deliberately MH-local and OUTSIDE this enum (pointer to telemetry-contract.md), so nobody adds an OWNER_SILENCE variant.
2. `enum ServingState { SERVING_STATE_UNSPECIFIED (rejected); SERVING; DRAINING; }`
3. `enum HeartbeatVerdict { HEARTBEAT_VERDICT_UNSPECIFIED (malformed: MH counts, no action); OWNED; RELEASED; }` -- named `HeartbeatVerdict` not bare `Verdict` (a package-level enum named `Verdict` is ambiguous once a second verdict exists); field name stays `verdict`. Not "the outcome vocabulary": MC's `outcome` label splits RELEASED by cause and adds owner_mismatch.
4. `enum ApplyRefusal { APPLY_REFUSAL_UNSPECIFIED (= no refusal); INVALID; TRANSPORT_INCONSISTENT; STREAM_CEILING; EDGE_CAP; }` -- 1:1 projection of MH's deterministic refusals (named), latest-apply, clears on success, load-dependent vs contract-fault split, transients never appear.
5. `MeetingHeartbeatRequest {meeting_id=1; mc_id=2; handler_id=3; ServingState serving_state=4; uint32 drain_remaining_ms=5; uint64 applied_generation=6; ApplyRefusal apply_refusal=7; repeated string connection_ids=8;}` and `MeetingHeartbeatResponse {HeartbeatVerdict verdict=1; ReleaseCause cause=2;}` placed in the MediaCoordinationService (MH->MC) section after `NotifyParticipantDisconnectedResponse`, with NO rpc line (MC-B1 adds it). All CHANGE-5 rules stated ONCE at these messages, MUST-level for the three security invariants (remove-only reconcile / heartbeat never adds; non-registered mc_id -> FAILED_PRECONDITION never RELEASED; S6 plaintext MC-unauthenticated channel, not an authorization control). Owner-silence release names `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` as a contract reference only (no config read here). Label hygiene: every id, generation and drain value never a metric label; ids in logs only, heartbeats logged on transitions only.
6. `RegisterMeetingRequest`: `uint32 heartbeat_interval_ms = 9;` (MC-derived N/3 from `MC_MH_HEARTBEAT_TIMEOUT_MS`; 0 rejects; enforcement owner MH-2 after MC-B1, ordering-trap sentence pointing at the policy_generation constraint; MH has no cadence config) and plain `bytes media_dial_public_key = 10;` with security's comment VERBATIM, plus a separate sentence: MH selects it by the media_dial token's meeting_id claim (ADR-0039 §5); never string-encoded; never in any JWKS; MC-D populates, MH-1 consumes.
7. `EndMeetingRequest`: `ReleaseCause cause = 3;` UNSPECIFIED rejected (enforcement MH-2); removal causes close await-bind connections with HANDLER_RELEASED, MEETING_ENDED leaves them to the bind outcome; projection -> pointer to ReleaseCause. The header's "NO GENERATION AND NO REASON FIELD, deliberately" becomes "NO GENERATION FIELD" plus "`cause` says why, it never conditions whether MH releases".
8. Comment edits: all-or-none invariant once at `applied_generation` (refusal list, prior snapshot keeps forwarding, response AND heartbeat report it or 0, equal = no-op applied, registration upsert independent of apply and not rolled back); CANONICAL ENUMERATION block untouched (consumer-owned); "re-assert" in the zero rule -> "re-register" (rule kept). `handler_id`: "never a metric label on MC/GC series ...; per-handler views come from the MH pod's own series", aggregation-floor justification deleted. MediaHandlerService header + RegisterMeetingRequest header + source_streams + `SubscriberSlot` "divergence-and-re-assert": periodic-re-assert framing removed; registration stays the full-snapshot policy. EndMeetingRequest: race (C) deleted ("THREE RACES" -> "TWO RACES"; (A)'s "orthogonal to both fence options for (C)" clause removed); ROLLOUT (R-24) paragraph deleted. **Question for @team-lead**: ROLLOUT carries three LIVE caller rules not about older MH (mismatch MUST NOT be retried; transport failures retried bounded with backoff; MC teardown never blocks on the ack). I plan to keep those three sentences as a short "CALLER RETRY" bullet under SEMANTICS and delete only the older-MH compatibility text. policy_generation ORDERING CONSTRAINT untouched; process_start_epoch_ms and connection_id legacy text untouched.

### dt-guard (paired-security content)
- `ALLOWED_BYTES_FIELDS` = exactly `[AllowedBytesField { message: "dark_tower.internal.v1.RegisterMeetingRequest", field: "media_dial_public_key" }]`, entry inside the array; the const's first line and the 9-line doc block untouched (ALLOWED_BYTES_FIELDS_AT pin holds).
- `shipped_key_allowlist_is_reviewed_content` -> exact `assert_eq!` on the written-out literal; doc no longer says "P1 replaces this".
- `the_real_tree_is_clean_and_non_vacuous` also asserts `key_allowlist_entries == 1`.
- New real-tree + SHIPPED-list tests: (b) second `bytes` field in RegisterMeetingRequest -> exactly `internal-proto-bytes-field` at the planted line (positive control); (c) renamed -> `KeyAllowlistInvalid` `field-not-found` (+ the bytes red, per documented precedence); (d) retyped to `string` -> `KeyAllowlistInvalid` `non-plain-bytes`. Exact rule + reason asserted.
- Existing I-4 fixture tests use test-local lists over the REAL tree, which now carries a real bytes field; `eval_with` composes `ALLOWED_BYTES_FIELDS` with the test list so those fixtures stay exact (the shipped entry is always in force, as in production), and `key_allowlist_entries` counts re-pinned. If any fixture needs the field absent instead, I'll say which.
- `tests/proto_ports_e2e.rs`: through-the-binary rename case asserting `REASON=internal-proto-key-allowlist-invalid`; `source_streams = 8` anchors (unit + e2e) remain unchanged.

### Consumer edits (compile-forced only)
- Every Rust `RegisterMeetingRequest` literal: `heartbeat_interval_ms: 0, media_dial_public_key: Vec::new()` explicit (no helper, no placeholder key, production and tests). `built_request_carries_no_key_material` (mc mh_client.rs) updated to assert `media_dial_public_key.is_empty()` (MC-D populates the public half only) and its "no bytes-typed field" comment corrected.
- `EndMeetingCall` gains `cause: ReleaseCause`, set to `MeetingEnded` in `end_meeting_plan` (single source; MC-B2 adds removal-cause plans); `mh_client::end_meeting` sends `cause: call.cause.into()`. Existing teardown test updated to assert every planned call carries MEETING_ENDED. No metric, label or recording site added.
- All other `EndMeetingRequest` literals (mh-service tests, env-tests, auth_layer test, mh_service.rs tests): explicit `cause: ReleaseCause::MeetingEnded as i32`. `..Default::default()` literals found by grep get explicit values too.

### Tests (proto-gen/tests/internal_roundtrip.rs; test-gates §Protocol, internal half)
Full by-value roundtrips of both heartbeat messages, the two new RegisterMeetingRequest fields and EndMeetingRequest.cause; frozen tags from bytes via `top_level_field_numbers`; enum number pins (UNSPECIFIED=0) for all four enums; default-decode fail-closed readings (verdict/serving_state/cause UNSPECIFIED, interval 0; ApplyRefusal zero = no refusal, documented as the one non-reject zero); open-enum UNKNOWN_ENUM_VALUE survives and `try_from` errs on every new enum field; connection_ids order+duplicates survive, empty emits no field (positive control), empty = "MH holds none"; 32-byte key with 0x00/0xFF byte-exact, empty emits no field, 31/33 representable; applied_generation > u32::MAX; RELEASED+UNSPECIFIED vs RELEASED+HEARTBEAT_TIMEOUT distinct. Header "what these do NOT assert" gains the runtime rules with owners (ServingState UNSPECIFIED reject MC-B1; FAILED_PRECONDITION on non-registered mc_id MC-B1; remove-only reconcile MC-B1; verdict UNSPECIFIED no-action MH-2; cause UNSPECIFIED reject MH-2; interval 0 reject MH-2; 32-byte key reject MH-1), each pinned in `docs/TODO.md` §Media Path Obligations. verify-codegen.sh: presence of the new messages, enums+members and fields in internal_pb.ts.

**Obligation (d) scope**: the other test-gates §Protocol items (SendTarget/StreamAssignment ref not in set; verify-codegen absence checks for process_start_epoch_ms, connection_id legacy, media_handler_url, media_servers, mh_url) belong to signaling P2 / consumer tasks that delete those fields, not here.


### Gate-1 input folded in (security, observability, semantic-guard, test, operations, dry-reviewer)
- **ROLLOUT (R-24)**: delete the older-MH compat text; its three live caller rules (mismatch MUST NOT be retried; transport failures retried bounded with backoff; MC teardown never blocks on the ack) move into EndMeetingRequest SEMANTICS (operations #1; same as my question above). Operator rollout order stays in `docs/runbooks/mh-deployment.md`.
- **Contradicting text edited in the same commit** (dry #1): EndMeetingRequest "NO GENERATION AND NO REASON FIELD" -> "No generation field; `cause` is a release REASON, never an ordering or authorization control; teardown stays unconditional by meeting id"; RegisterMeetingRequest "NO KEY MATERIAL CROSSES THIS CONTRACT" keeps its rule and gains one pointer: the single allow-listed public verification key is `media_dial_public_key` (see its comment) -- no second explanation. `teardown.rs:16-17` ("carries no generation") stays true; untouched.
- **ReleaseCause per-carrier validity matrix lives ONLY at the enum** (dry #2); `EndMeetingRequest.cause` and `MeetingHeartbeatResponse.cause` say "valid values: see `ReleaseCause`" plus their own carrier-specific behaviour (await-bind close rule; misroute meaning).
- **All-or-none** (dry #3/#4): EXTEND the existing "REFLECTS MEANS..." paragraph at `applied_generation`; refusal list names the ApplyRefusal values by reference ("every non-UNSPECIFIED `ApplyRefusal`, plus stale, per-meeting bounds and transient"). `MeetingHeartbeatRequest.applied_generation` = pointer + heartbeat-only rule (lower than last accepted = lag, never liveness).
- **ApplyRefusal projection** (dry #5): names `PolicyApplyOutcome` members and `ApplyFailure::EdgeCapExceeded`, no count; its one code home is MH-2's exhaustive match (not this task).
- **Config values** (dry #6 vs prompt text): keys named as contract references with owning service; I propose NOT copying default/range numbers into the proto (third home next to config.rs + runbook row with no guard) and citing ADR-0039 §1/§6 for provenance instead; `/3` stated as "derived by MC from `MC_MH_HEARTBEAT_TIMEOUT_MS` (ADR-0039 §1)". **@team-lead ruling requested** since the prompt's parentheticals list the numbers.
- **Id validation** (dry #7): reference `validate_id_field` in `crates/mc-service/src/grpc/media_coordination.rs`; no length restated.
- **Rollout ordering per zero-default field** (operations #3): one ORDERING sentence each at `heartbeat_interval_ms` (MH-2 after MC-B1 fleet-wide; rollback MH first), `media_dial_public_key` (MH-1 after MC-D; sentence placed AFTER security's verbatim sentence, not inside), `EndMeetingRequest.cause` (MH-2 enforcement after this commit's MC is fleet-wide; rollback reverses).
- **Owner-silence vs pre-heartbeat MC** (operations #4): at the owner-silence rule: "MC gains MeetingHeartbeat before any MH heartbeats; rollback reverses (MH first). An MH answered UNIMPLEMENTED is a deploy-order fault, not owner silence" -- whether MH-2 exempts UNIMPLEMENTED is MH-2's behaviour decision.
- **Observability** (obs #1-#5): `trigger` label projection stated once at ReleaseCause (lowercase suffix values, UNSPECIFIED never a label value, misroute = MC outcome `released_unassigned`), `owner_silence` deliberately outside the enum (pointer to telemetry-contract.md); `handler_id` narrowing with aggregation-floor justification deleted; heartbeat ids/generations/drain never labels, logged on transitions only; bounded enums MAY back labels but the label set is owned by the consumer metric; CANONICAL ENUMERATION block untouched.
- **Semantic-guard** (#1-#7): no skip_debug entry for the public key; `Vec::new()` everywhere, no helper/placeholder reading key material; no signaling.proto change.
- **Shared builders** (dry #9): new fields added once in `mh-test-utils/src/media_policy.rs`, `env-tests/src/fixtures/mh_grpc.rs` and per-file `mixed()/policy()` helpers, not per caller.
- **Safe-revert unit** (operations): the whole commit; safe only while no consumer task (MC-B1/B2/D, MH-1/2) has landed on top -- revert those first.

### Gate-1 round 2 (lead rulings, code-reviewer, auth-controller, test, security, dry)
- **Lead rulings**: ROLLOUT -> CALLER RETRY bullet (approved). Config keys named with owning service, defaults/ranges cited to ADR-0039 §1/§6, NOT copied. The `/3` derivation is a rule and stays at `heartbeat_interval_ms`. `HeartbeatVerdict` accepted.
- **Guard test helpers** (code-reviewer #1, supersedes "eval_with composes" above): `eval_with(internal, list)` stays LITERAL. Composition is visible at the call site via a named helper `eval_shipped_plus(internal, extra)` (= `[ALLOWED_BYTES_FIELDS, extra].concat()`), used by the I-4 fixtures that need the real field exempt. Fixtures that are about the empty list (e.g. the `_unlisted_reds` negative arm) keep calling `eval_with` with an explicit list. Each fixture's expected findings are re-checked against what it means to show: negative arms red ONLY on their planted line, with the real field's red either exempted by the explicit shipped entry or asserted. `check_with` stays parametric.
- **Non-vacuity pair** (test, code-reviewer): new test: REAL tree + EMPTY list -> exactly one `internal-proto-bytes-field` at `media_dial_public_key`'s declaration line (via `line_of`), paired with `the_real_tree_is_clean_and_non_vacuous` (+ `key_allowlist_entries == 1`).
- **Rename/retype mutations** (security ii): exact-line replace on the real file, asserting the original line is present first.
- **Owner per rule in two places only** (dry): proto field + `docs/TODO.md` pin; the roundtrip header lists the unasserted rules and points to §Media Path Obligations for owners.
- **credential_leak fixture** (code-reviewer #6): `pos_contract_crossing_renamed_material.rs` is a text-only lens fixture (read via `read_to_string`, not compiled), so its `..Default::default()` literal is untouched. Its comment premise "`internal.v1` contains NO `bytes`-typed field at all" becomes false with this commit, and its own TRIGGER paragraph says this is where the note is to be read. Comment-only correction (one allow-listed public-key bytes field; still nowhere for secret/media key bytes; items 11-13 are now the remaining enforcement), wording confirmed with semantic-guard/security.
- **auth-controller 1-4** (all comment text in internal.proto):
  1. At `media_dial_public_key`, after security's verbatim sentence: MH selects the verification key ONLY by the token's `meeting_id` claim from that meeting's current registration; `kid`/`jwk`/`x5*` headers never select or supply a key; an unregistered meeting is DECLINED; never in any JWKS, and an AC-signed token never verifies against it (nor a media_dial token against AC's).
  2. Record-field rejects vs apply refusals: stated ONCE in the RegisterMeetingRequest header: a malformed registration-record field (`media_dial_public_key` not 32 bytes, `heartbeat_interval_ms` 0) rejects the WHOLE request BEFORE the upsert; nothing is recorded and the prior record (key, interval, mc_id) stays in force. These are distinct from policy-apply refusals (`ApplyRefusal`, stale, per-meeting bounds, transient), which follow the independent upsert (`applied_generation`). The two fields point at that sentence.
  3. At the field: each accepted registration replaces the meeting's key; MC never rotates it within a meeting actor, so a changed key means a new MC meeting actor; MH does not verify the registrant owns the meeting, so a scope holder can replace it (ADR-0039 residual, ciphertext-only impact); not an ownership or authorization control.
  - EndMeetingRequest `mc_id` paragraph: "most recent VALIDATION-PASSING `RegisterMeeting`" -> "most recent RECORD-VALIDATION-PASSING `RegisterMeeting`" (agrees with ADR-0039 §1: the record upsert precedes, and is independent of, policy-apply validation).
  4. At MeetingHeartbeatRequest beside the S6 sentence: `handler_id` and `mc_id` are self-asserted by the sending MH pod; another MH pod can forge heartbeats for a dead handler and suppress failover (S1b-extended residual, detected by `mc_media_handler_client_loss_disagreements_total`); the mc_id FAILED_PRECONDITION rule is anti-misroute, not authentication.

### Cross-task obligations
(a) no config read; keys named as references only. (b) retirements are proto-comment only. (c) no series, no metrics.rs/catalog/dashboard/alert edits. (e) every id-naming comment says never a label. buf breaking: additive only, expected zero findings.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed |
| Auth Controller (GSA co-owner) | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files` (after one fix: single Owner per row). Lead rulings: ROLLOUT live caller rules kept as CALLER RETRY bullet; config keys named without numbers (SSoT; cite ADR-0039 §1/§6).

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### internal.proto (additive; buf lint/format/breaking green)
| Item | Before | After |
|------|--------|-------|
| `ReleaseCause`, `ServingState`, `HeartbeatVerdict`, `ApplyRefusal` | — | new top-level enums (per-carrier validity + `trigger` projection + owner_silence-outside note at `ReleaseCause`) |
| `MeetingHeartbeatRequest` / `MeetingHeartbeatResponse` | — | new, fields 1-8 / 1-2, no rpc line (MC-B1) |
| `RegisterMeetingRequest` | 8 fields | + `heartbeat_interval_ms = 9`, `bytes media_dial_public_key = 10` (security verbatim comment + selection/ownership/ORDERING sentences) |
| `EndMeetingRequest` | 2 fields | + `ReleaseCause cause = 3` |
| Comments | re-assert framing; ROLLOUT (R-24); race (C); handler_id "acceptable label" | retired; CALLER RETRY bullet; record-reject vs apply-refusal split in RegisterMeetingRequest header; all-or-none extended in place at `applied_generation`; "RECORD-VALIDATION-PASSING"; handler_id never a label on MC/GC |

### dt-guard
- `ALLOWED_BYTES_FIELDS` = the one security pair (inside the array; first line + doc block unchanged).
- Membership pin is an exact written-out `assert_eq!`; `the_real_tree_is_clean_and_non_vacuous` asserts `key_allowlist_entries == 1`.
- New real-tree tests: empty list reds exactly at the key line; second bytes field reds `internal-proto-bytes-field`; rename -> `field-not-found`; retype to string -> `non-plain-bytes` (exact-line mutation with presence precondition). e2e: rename -> `REASON=internal-proto-key-allowlist-invalid`.
- `eval_with` stays literal; `eval_shipped_plus` is used by EVERY I-4 construct fixture (they plant `listed` beside the real key field, so the shipped entry must be in force for their findings to stay exact); counts re-pinned +1. `an_identical_same_line_declaration_elsewhere_stays_red` now plants after the key field (its early `}` would otherwise move the real field into `Evil`). `a_drifted_declaration_id_exempts_nothing` unchanged (calls `resolve_key_allowlist` directly).

### Consumers (struct literals only)
- MC: `EndMeetingCall.cause` set to `MeetingEnded` in `end_meeting_plan` (single source); `mh_client::end_meeting` sends `call.cause.into()`; register literal sends `heartbeat_interval_ms: 0`, `media_dial_public_key: Vec::new()`. Plan test + integration test (`stub.end_meeting_requests()[0].cause`) assert MEETING_ENDED.
- MH / mh-test-utils / env-tests: explicit `0` / `Vec::new()` / `MeetingEnded` at every literal (shared builders once; `..populated_request()`-style literals inherit).

### Tests + docs
- `internal_roundtrip.rs`: story-4 section (by-value roundtrips, proto-derived frozen tags, enum pins, fail-closed defaults, open enums, empty-vs-populated controls, 31/33-byte representability); header lists the unasserted runtime rules, owners only in proto + TODO.
- `verify-codegen.sh`: presence of the new exports, enum members and fields.
- `docs/TODO.md` §Media Path Obligations: "Story-4 internal-contract RUNTIME rules" pin with owner per rule.
- credential-leak fixture comment premise corrected (semantic-guard wording).
- Obligation (d) scope: the other test-gates §Protocol items (SendTarget/StreamAssignment ref-not-in-set; verify-codegen ABSENCE checks for process_start_epoch_ms, connection_id legacy, media_handler_url, media_servers, mh_url) belong to P2 / the consumer tasks that delete those fields.

**Safe-revert unit**: the whole commit; safe only while no consumer task (MC-B1/B2/D, MH-1/2) has landed on top — revert those first.

---

## Files Modified

```
{Output of: git diff --stat HEAD}
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Devloop Verification Steps

### Gate 2 — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`: TOTAL_RESULT=OK (no FMT_APPLIED)

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 7s |
| 2 Format | OK | 5s |
| 3 Guards | OK | 49s |
| 4 Test | OK | 154s |
| 5 Lint | OK | 1s |
| 6 Audit | OK (cargo audit SKIPPED-NO-DIFF no-dep-changes; proto N/A placeholders) | 6s |
| 7 Env-tests | OK (attempt 1) | 1370s |

Fast check before review: TOTAL_RESULT=OK (L1-6).

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 2 | 2 | 0 | F1 replay overclaim at heartbeat applied_generation; F2 S6 trust note on MeetingHeartbeatResponse. Also requested MC-D TESTS value-axis tightening (applied in story doc) and the ADR-0039 §1 erratum (applied, wording security-approved) |
| Auth Controller (GSA) | RESOLVED-FIXED | 3 | 3 | 0 | owner-check precedence before any verdict; replay overclaim; record-vs-apply INTERIM (MH-2) |
| Test | CLEAR | 0 | 0 | 0 | test-gates §Protocol internal half implemented |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | per-series trigger admission owned by catalog; media_dial_public_key OBSERVABILITY line |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | reflow; merged imports; proto_tag resolved by removal (Lead ruling) |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | private proto_tag parser removed; extraction opportunities: none |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | owner-silence "communication" excludes MH-rejected requests |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |

Lead rulings: ROLLOUT live caller rules kept (CALLER RETRY); config keys named without numbers; DRY over code-reviewer on proto_tag (delete, literal tags).

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

**This devloop**: - (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft e188a5172b0bdd36332fbb7f734e85edafde68c1`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit (proto + allow-list entry + consumer literals are mutually dependent); safe only before MC-B1/MC-B2/MC-D/MH-1/MH-2/MH-3 land on top — revert those first. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

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
