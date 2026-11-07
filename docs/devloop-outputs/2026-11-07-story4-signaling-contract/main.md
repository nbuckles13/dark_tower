# Devloop Output: P2 — Story-4 signaling contract (additive) + TS codegen

**Date**: 2026-11-07
**Task**: Story 4 task #6 (P2): additive signaling.proto contract for handler sets, handler refs, media-dial tokens, MhCloseCode; redaction; TS regen
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, Gate-1 present (paired with client)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~75m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `77144490069fede1163426acde3aca86978b0d74` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `protocol` |
| Tier | `full` |
| Iteration | `1` |
| Paired | `paired-client` |
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
Task description: exact contents of `/home/dev/.cache/devloop/story-runs/story-runner/2026-10-31-survive-handler-loss/task-6.prompt` (story `docs/user-stories/2026-10-31-survive-handler-loss.md`, ADR-0039 §4, §5, §7).

### Scope
- **Service(s)**: proto contract, proto-gen (Rust + TS), compile-only consumer edits in mc-service / sdk-core
- **Schema**: No
- **Cross-cutting**: Yes (GSA: `proto/**`, `proto-gen/**`, `build.rs`)

### Debate Decision
NOT NEEDED — governed by Accepted ADR-0039.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/signaling/v1/signaling.proto` (additive: new messages/fields/enum on fresh tags) | Mine (GSA owned by protocol; security reviews credential fields + `MhCloseCode`) | protocol |
| `crates/proto-gen/build.rs` (`skip_debug` gains `MediaServerInfo`, `MediaHandlerSet`; `MhConnectRequest` note) | Mine (GSA owned by protocol; security reviews redaction) | protocol |
| `crates/proto-gen/src/lib.rs` (redacting `Debug` impls) | Mine (GSA owned by protocol; security reviews redaction) | protocol |
| `crates/proto-gen/tests/signaling_roundtrip.rs` (roundtrip + redaction tests; paired with test) | Mine (GSA owned by protocol) | protocol |
| `packages/proto-gen/scripts/verify-codegen.sh` (presence checks + `assert_oneof_case` helper) | Mine | — |
| `crates/mc-service/src/**` (compile-only struct literals: `webtransport/connection.rs`, `media_signaling/directive.rs`, `media_signaling/assignments.rs`, `actors/meeting_media.rs`) | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/**` (compile-only struct literals) | Not mine, Mechanical | meeting-controller |
| `crates/mh-service/tests/**` (compile-only struct literals: `common/wt_client.rs`, `media_keyframe_integration.rs`) | Not mine, Mechanical | media-handler |
| `crates/env-tests/**` (compile-only struct literals: `src/fixtures/mc_session.rs`, `tests/26_mh_quic.rs`) | Not mine, Mechanical | test |
| `scripts/guards/semantic/checks.md` (lines 65/69 task-name fix P1 -> P2, security-approved) | Not mine, Mechanical | security |
| `docs/TODO.md` (§Media Path Obligations: signaling runtime pins, owner task per rule) | Mine | — |
| `docs/specialist-knowledge/protocol/INDEX.md` (navigation pointers) | Mine | — |

Generated TS in `packages/sdk-core/src/proto/` is gitignored output of `codegen.sh` (not a committed edit). No sdk-core source edit: `SignalingClient.ts` `#dispatch` has a `default:` arm (logs case name only), `MhConnectRequest` built via `create()` defaults the new field. Exact set of compile-fix files is confirmed by `cargo check --workspace --all-targets`; any extra file found there is Mechanical, same owner as its crate.

---

## Planning

Reviewer co-ownership (prose, not Owner cells): `security` reviews the credential fields, `MhCloseCode` and the `build.rs`/`lib.rs` redaction; `test` pairs on `signaling_roundtrip.rs`; `client` pairs on `verify-codegen.sh`.

### Mechanism restatement
"Every signaling message that can carry a credential has a hand-written redacting `Debug`, and every path a credential reaches a `Debug` goes through one" -- the wider class is all `skip_debug` members plus all their containers. Audit: `MediaServerInfo` is embedded only by `JoinResponse.media_servers` (custom Debug) and the new `MediaHandlerSet`; `MediaHandlerSet` only by `JoinResponse.media_handler_set` (custom, must list it) and `ServerMessage` oneof (derived -> safe once `MediaHandlerSet` is custom). `MhConnectRequest` is custom and gains the field. No other signaling message carries a token/key field without being in the set (grep of `string|bytes *token|key|kek*`: `JoinRequest`, `JoinResponse`, `MeetingKekUpdate`, `MhConnectRequest` already covered; `identity_public_key` is public). So the invariant is complete with the two additions.

Second rule-class: "addresses reach clients only from the set; every other handler mention is a ref". Same-owner siblings beyond the task's list: `JoinResponse.media_servers`, `SendTarget/StreamAssignment.media_handler_url`, `MhConnectionStatus.mh_url` -- all named by the task and kept until P3, each getting a pointer to the set. `RedirectToMc.new_mc_endpoint` is an MC address, not MH -- out of class.

### Proto (signaling.proto), fresh tags
- `message MediaHandlerSet { uint64 handler_set_version = 1; repeated MediaServerInfo handlers = 2; }` -- the ONE home for: full snapshot / replace semantics; sent on JoinResponse + as ServerMessage on every membership change and every token refresh; version = membership epoch (advances only on membership change; token-only refresh keeps version + refs -> client replaces tokens only, no dial/close); used to correlate loss reports via `MhConnectionStatus.handler_set_version` (cross-ref), never to discard, never for ordering (one ordered stream); listed only after MC confirmed RegisterMeeting, so the join-time set may be empty with a set message following; no two entries share a url; addresses reach clients only from this set; client never dials outside the latest set; NON-AUTHORITATIVE-for-send-placement text moved here from `media_servers` (which keeps a pointer until P3). Placed beside `MediaServerInfo`.
- `MediaServerInfo`: `uint32 handler_ref = 3` (per incarnation, never reused within a meeting even at a reused url, zero invalid, client diffs on ref: new ref at old url = close + redial); `string media_dial_token = 4` (CREDENTIAL; the one home of the token rule: compact JWS, EdDSA, claims {token_type=media_dial, sub from MC session never the request, meeting_id, aud=handler_id, iat, exp}, MC per-meeting key; present on every entry of every set; TTL by reference `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` (default 300, MC refuses start outside [60,900]), refreshed at TTL/3; reusable within lifetime; opaque to the client, never decoded; never logged, never in a metric, trace, error, serialized state, UI; client retains only the latest per in-set ref, cleared on removal and session end (scoped R-23 amendment, ADR-0039 §5)). `media_handler_url` comment points at the per-incarnation address rule. That block does NOT exist above `RegisterMHRequest` yet (it lands in GC-2), so the pointer cites ADR-0039 §7 today and names GC-2 as the task that adds the `internal.proto` home and retargets it (pinned in `docs/TODO.md`), rather than pointing at a block that isn't there (@dry-reviewer). Stale "§9/§10 expected to add fields" descriptor text updated.
- Zero is never a member of any set (@paired-client): a `MediaServerInfo` entry with ref 0 is a contract fault, and a `SendTarget`/`StreamAssignment` ref of 0 falls under the non-member rule (no separate zero branch, no `optional`).
- `SendTarget.handler_ref = 3`, `StreamAssignment.handler_ref = 12`: MUST be a member of the client's current set; non-member is counted and ignored; url fields point at the set and say "until P3".
- `JoinResponse.media_handler_set = 16`; `ServerMessage` oneof `media_handler_set = 15` (below 20).
- `MhConnectRequest.media_dial_token = 2` (credential, points to the MediaServerInfo home). "MH-1 makes MH accept media_dial_token ONLY; join_token is deleted in P3" -- no "either" wording.
- `MhConnectionStatus.handler_ref = 6`, `handler_set_version = 7`; `mh_url` points at them (until P3).
- `MediaConnectionUpdate`: prohibition stays the governing sentence. Two permitted uses, both still never routing/reachability/placement input and client-asserted: (1) input to the clients-detected-loss-before-MC evidence (ADR-0039 §8; the counting rule -- join keys, receipt-time-only, which reports count -- has ONE home, `docs/debates/2026-10-28-mh-replacement-and-migration/telemetry-contract.md`, cited not restated) and the NON-TRIGGERING second opinion to MC's health function (ADR-0039 §2); the version's own "correlate, never discard" sentence is cross-referenced, not restated (@code-reviewer #1, resolving the overlap with @observability #3 in favour of the contract as SSoT); (2) a FAILED report for an in-set ref MAY trigger a rate-limited credential re-push of the set to its sender only. Reports are sent on every transition.
- `enum MhCloseCode { MH_CLOSE_CODE_UNSPECIFIED = 0; _WRONG_HANDLER = 1; _BINDING_CONFLICT = 2; _MC_UNAVAILABLE = 3; _HANDLER_RELEASED = 4; _DECLINED = 5; }` beside `MhServerMessage`. Comment: carried as the WebTransport application close code, numeric only; security's three conditions (every credential failure incl. expired-beyond-skew, signature, type, cross-meeting/unregistered meeting, malformed, oversized, AC meeting token -> the single generic DECLINED, specific codes only after full validation, aud checked last so WRONG_HANDLER is reachable only after signature/type/exp/meeting pass; every code concerns only the caller's own token-validated meeting; no reason strings, numbers defined once here); UNSPECIFIED never sent, received 0/unknown = counted client contract fault; every intentional MH close (decline, release, drain-deadline shed) carries exactly one code, no code = transport loss; DECLINED = token dead not handler dead (wait for the next set carrying a token for that handler, exactly one dial); WRONG_HANDLER/BINDING_CONFLICT/MC_UNAVAILABLE retried, not loss; HANDLER_RELEASED expected after a set change. Per-value one-liners say only what the value MEANS; enum-level rules (never sent, one code per close, retried vs loss) live only at the enum (@code-reviewer #2).
- Obligation (e) as a sentence at EACH of the 8 new ref/version/token fields: "never a metric label, span attribute, or per-frame log field" in ONE identical wording at every site; the two token fields carry it inside the credential sentence ("never logged ..."), not as a second sentence (@code-reviewer #3). Pointers (MhConnectRequest.media_dial_token, superseded url fields) point at the one home and add only what is local ("until P3"); no copied TTL/claims/retention (@code-reviewer #4). Redaction tests share one assert helper (negative needles + positive control).

### proto-gen
- `build.rs`: `.skip_debug` for `MediaServerInfo`, `MediaHandlerSet`; reasoning paragraph; `MhConnectRequest` note now covers both tokens (join_token until P3, media_dial_token).
- `lib.rs`: `MediaServerInfo` Debug = url + handler_ref in the clear, `media_dial_token` as `RedactedLen` (security's choice; SAFETY INVARIANT block gains the JWS case: length varies with sub/meeting_id lengths, not with secret bytes; signature is fixed-size); `MediaHandlerSet` = version + handlers via the redacting element Debug; `MhConnectRequest` adds `media_dial_token` `RedactedLen`; `JoinResponse` adds `media_handler_set` (through its redacting Debug). Header "these five messages" -> membership wording ("the messages in build.rs's skip_debug set"), no restated count. Token redaction cites the existing variable-length-JWT reasoning in the SAFETY INVARIANT (no second justification). `MediaHandlerSet` delegates to `MediaServerInfo`'s Debug (token redaction lives in one impl).

### Tests (`crates/proto-gen/tests/signaling_roundtrip.rs`)
- Helpers from `tests/common/mod.rs` only (`roundtrip`, `top_level_field_numbers`, `UNKNOWN_ENUM_VALUE`). Fresh tags pinned with `top_level_field_numbers` (MediaServerInfo never emits reserved 2; ServerMessage variant tag < 20 asserted from bytes). JoinResponse None / Some(empty) / Some(populated) distinct, legacy `media_servers` populated alongside. Refs near u32::MAX; versions > u32::MAX; >= 2 handlers with distinct refs/tokens, order preserved; MhCloseCode: every number pinned, `from_str_name` each, `try_from(UNKNOWN_ENUM_VALUE)` is_err.
- Roundtrip: `MediaHandlerSet` (empty + populated, u64 > u32::MAX version), `MediaServerInfo` new fields, `SendTarget`/`StreamAssignment.handler_ref`, `JoinResponse.media_handler_set` (present + absent distinguishable), `ServerMessage::MediaHandlerSet` variant, `MhConnectRequest.media_dial_token` alongside `join_token`, `MhConnectionStatus` ref/version, `MhCloseCode` values pinned numerically (0..5, `from_i32`/`try_from` of 6 -> err) -- numbers are the wire contract since it rides a close code, not protobuf.
- Redaction: JWS-shaped synthetic token `eyJ...<payload>.<sig>`; needles derived from the value set (whole + each of the 3 segments) absent from Debug of `MediaServerInfo`, `MediaHandlerSet` (nested), `MhConnectRequest` (both tokens), `JoinResponse` with populated set, and `ServerMessage{media_handler_set}`; positive control: url / handler_ref / version appear and a `<redacted` placeholder appears; `MediaHandlerSet` with a DIFFERENT token per entry; `JoinResponse` with a token in BOTH `media_servers` and `media_handler_set`, and asserts `media_handler_set` is rendered; `MhClientMessage{MhConnectRequest{join_token, media_dial_token}}` neither prints (extends `debug_never_prints_the_mh_connect_join_token`).
- `verify-codegen.sh`: presence via `assert_exported` (MediaHandlerSet, MhCloseCode), `assert_message_field` (10 fields), `assert_enum_member` (6), new `assert_oneof_case` presence helper on `oneof_case_count` (and `assert_oneof_case_absent` reuses it for its control) for `mediaHandlerSet`. Each new presence assert is proven to go red once by a local rename (vacuity check), reported in review. Absence -> P3.
- Test-gates Protocol section, signaling parts: roundtrips (here); "ref not in current set counted+ignored" is client BEHAVIOUR (C1) -- here it is stated once at the field; C1 OWNS this gate (named in `docs/TODO.md`, confirmed by @test). The SDK's existing unknown-variant `default:` arm is NOT extended with a `mediaHandlerSet` test here; C1 replaces that path with the real arm. Absence checks -> P3. ServingState is internal (P1, landed).

### Consumer edits (compile only)
New fields `0` / `String::new()` / `None` in the Rust literals listed above; no SDK source edit. No dispatcher matches the `ServerMessage` oneof exhaustively: SDK `SignalingClient.ts` `#dispatch` has `default:` (case name only); MC matches are test-side `if let`/`matches!`/`_ =>`. No `MhCloseCode` constant mirror lands in MH or SDK (C1/MH-CLOSE use the generated enums). No SDK type/event/store carries the token. No real-looking tokens outside proto-gen tests.

### Cross-task obligations
(a) no config read; TTL is a contract reference. (b) none. (c) no series, no recorder, no label const. (d) above. (e) at each field.

### SAME-COMMIT NOTE
SEC-A landed (d9afe953). checks.md names `MediaHandlerSet`, `MediaServerInfo.media_dial_token`, `MhConnectRequest.media_dial_token`, `handler_set_version`, `mediaDialToken`, `DECLINED`/`WRONG_HANDLER` -- all kept exactly. Only drift: checks.md line 69 says "once P1 adds them to build.rs's skip_debug" -- it is P2 (this task). Offered to security.

### Validation
`buf lint` + `buf breaking` (additive: expected 0 findings, no Expected Layer-State declaration), `cargo test -p proto-gen`, `cargo check --workspace --all-targets`, `codegen.sh` + `verify-codegen.sh`, `./scripts/layer-fast.sh`.

### Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Paired Client | confirmed |
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

- `signaling.proto` (additive, fresh tags): `MediaHandlerSet {handler_set_version=1, handlers=2}` (single home of the set rules incl. moved NON-AUTHORITATIVE text; `JoinResponse.media_servers` keeps a pointer); `MediaServerInfo.handler_ref=3`, `.media_dial_token=4` (single credential home); `SendTarget.handler_ref=3`; `StreamAssignment.handler_ref=12`; `JoinResponse.media_handler_set=16`; `ServerMessage.media_handler_set=15`; `MhConnectRequest.media_dial_token=2`; `MhConnectionStatus.handler_ref=6`, `.handler_set_version=7`; `MediaConnectionUpdate` two permitted uses (cites telemetry contract); `enum MhCloseCode` (0..5) beside `MhServerMessage`. `StreamAssignments.unreachable_sender_ids` pointer moved from `media_servers`/url to set/ref.
- `proto-gen`: `skip_debug` + hand-written `Debug` for `MediaServerInfo` (token `RedactedLen`) and `MediaHandlerSet` (delegates); `MhConnectRequest` and `JoinResponse` Debug gain the new fields; lib.rs header uses membership wording.
- Tests: 13 new/extended tests in `signaling_roundtrip.rs` (shared `assert_redacts_tokens` helper; segment-level needles; positive controls); `verify-codegen.sh` 19 presence asserts + `assert_oneof_case` helper (absence twin now uses it for its control).
- Compile-only consumer edits (zero/empty), `docs/TODO.md` signalling runtime pins (C1, GC-2, P3), `checks.md` task-name fix (security-approved), INDEX update.

## Devloop Verification Steps

- `buf lint` clean; Layer 6 `buf breaking` OK (additive, no Expected Layer-State declaration).
- `cargo test -p proto-gen`: 63 signaling + 31 internal pass.
- `codegen.sh` + `verify-codegen.sh`: all pass. Vacuity: each of the 19 new presence asserts was driven red by a local rename of its generated declaration (19/19 rc=1 with the matching FAIL line), then restored.
- `./scripts/layer-fast.sh`: Layers 1-6 OK (one iteration fixed INDEX.md 76>75 lines); Lead fast check OK.
- Gate 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, final tree after Gate 3): L1-L7 all OK, `TOTAL_RESULT=OK` (L7 env-tests + browser E2E passed, 1385s). 1 attempt.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Paired Client | RESOLVED-FIXED | 2 | 2 | 0 | MhCloseCode transport-loss made browser-observable; absent JoinResponse set = empty |
| Security | CLEAR | 0 | 0 | 0 | GSA co-owner confirmed Gate 1 + Gate 3; SEC-A names match; re-confirmed after post-CLEAR comment edits |
| Test | CLEAR | 0 | 0 | 0 | test-gates Protocol section covered; mutation-checked verify-codegen asserts |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | (e) tags cite ADR-0039 §8; per-field never-logged on MhConnectRequest token; comment wrap |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | "legacy"→"superseded" in tests; comment wrap |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | media_handler_url pointer-only; restored connected-set sentence; comment wrap |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | MhCloseCode EVOLUTION ordering + fault handled as transport loss |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | native SAFE after fix; checks.md:65 stale "none exists yet" |

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `77144490069fede1163426acde3aca86978b0d74`
2. Review all changes: `git diff 77144490..HEAD`
3. Soft reset: `git reset --soft 77144490`
4. **Safe-revert unit**: this commit alone, and ONLY while no consumer task (MC-A, MC-D, C1, MH-1, MH-CLOSE) has landed on top; after that, revert consumers first, newest first (their struct literals and generated-TS imports reference these fields). Code-only: no data or schema state (@operations).

---

## Issues Encountered & Resolutions

### Issue 1: Gate-1 classification guard rejected compound Owner cell
**Problem**: `validate-cross-boundary-classification.sh` requires exactly one manifest owner per GSA row; the plan listed `protocol (+ security ...)`.
**Resolution**: table rewritten with one owner per row; security's GSA co-review recorded in prose. Guard then OK.

### Issue 2: MhCloseCode "no code = transport loss" not observable in browsers
**Problem**: per draft-ietf-webtrans-http3 a clean close without a capsule surfaces as code 0, colliding with the "0 is a fault" rule (paired-client F1).
**Resolution**: transport loss defined as `WebTransport.closed` rejecting; MH never closes cleanly without a code; clean 0/unknown = counted fault handled as loss. Security re-confirmed CLEAR. Proto comment is the precise contract home for ADR-0039 §7's wording.

## Lessons Learned

1. Owner cells must name a single manifest specialist; co-reviewers go in prose.
2. Close-code contracts must be stated in terms of what the receiving runtime can observe.
