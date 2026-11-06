# Devloop Output: SEC-A — Security policy content for MC-minted dial credentials

**Date**: 2026-11-06
**Task**: Story 4 task #3 (SEC-A): extend the semantic-guard credential lenses (`scripts/guards/semantic/checks.md`) for the MC-minted `media_dial` credential (ADR-0039 §5) and record the changed MH-ownership residual in `docs/TODO.md` (R-15, R-38).
**Specialist**: security
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h (incl. ~22m Layer 7)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `08520f67be321ad07c3f084eca1f435f2d4d2f58` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (security) — done` |
| Implementing Specialist | `security` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security — RESOLVED-FIXED` |
| Test | `test — RESOLVED-FIXED` |
| Observability | `observability — RESOLVED-FIXED` |
| Code Quality | `code-reviewer — RESOLVED-FIXED` |
| DRY | `dry-reviewer — RESOLVED-FIXED` |
| Operations | `operations — RESOLVED-FIXED` |
| Semantic Guard | `semantic-guard — RESOLVED-FIXED` |
| Meeting Controller (paired review) | `meeting-controller — RESOLVED-FIXED` |
| Media Handler (paired review) | `media-handler — RESOLVED-FIXED` |
| Client (paired review) | `client — RESOLVED-DEFERRED` |

---

## Task Overview

### Objective
Update the security review lenses so MC-D (MC-minted media_dial) and MH-1 (MH media_dial-only admission) are reviewed against the right rules, and record the changed MH-ownership residual.

### Scope
- **Service(s)**: none (docs/guard-lens only)
- **Schema**: No
- **Cross-cutting**: Yes (lens names MC, MH and client emission sites)

### Debate Decision
NOT NEEDED - governed by ADR-0039 §5/§6/§7 and ADR-0036 §4; this task carries that text, invents nothing.

### Not in this task (self-containment)
- The `internal-proto-no-key-material` allow-list ENTRY lands with the field in P1, after I-4.
- The S6 TODO line (periodic MH bearer; GC allowlist is not peer authentication) lands in GC-2 with the allowlist.
- No code, config or metrics change.

### Cross-task obligations (a)-(e)
None applies: no config keys, no retirements, no series, not a service task, no labels.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/semantic/checks.md` (§Credential Leak items 11-13 + language-scope preamble; §Client Credential Lifetime) | Mine (credential-lens policy content, security-owned per CLAUDE.md guard-ownership note; not a GSA, no key in `cross-boundary-ownership.yaml`) | — (semantic-guard reviews as the lens's consumer; meeting-controller, media-handler, client review the emission sites that name them) |
| `.claude/agents/semantic-guard.md` (Output Format verdict vocabulary only) | Not mine, Mechanical (requested by owner) | semantic-guard |
| `docs/TODO.md` (one dated paragraph appended to "MH does not verify meeting ownership"; "MUST cover BOTH" extended by name in the new paragraph, original text unchanged) | Mine (security owns the entry's authorization half, per the entry's own owner line) | — (media-handler + meeting-controller co-own the entry; on panel) |

---

## Planning

Governing text: ADR-0039 §1 (registration upsert), §5, §6, §7, §8, Consequences "Recorded residual risks (security)"; ADR-0036 §4 "MH stays keyless". Prose only — no fenced code block is introduced (self-check: `git diff -U0 | grep '^+.*```'` empty). No new `## Check:`. Nothing inserted between the `fixture-poles:must-fire` markers; the `## Check: Credential Leak` heading is unchanged.

Names verified against the tree: `dt-guard internal-proto-no-key-material` and its `ALLOWED_BYTES_FIELDS` const exist and are EMPTY (I-4); `ts-no-retained-credentials` exists and `token` is in `SESSION_TOKENS`, not `CREDENTIAL_TOKENS` (`crates/dt-guard/src/common/pii_vocabulary.rs`), and a finding needs credential AND retention; `mh_jwt_validations_total` exists. FUTURE names (phrased as landing in a later task): `RegisterMeetingRequest.media_dial_public_key` and its allow-list entry (P1), `dial_signing_key` (MC-D), `MhConnectRequest.media_dial_token` (P1/MH-1), `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (MH-3).

### CHANGE 1 — §Credential Leak, Rust — MC key custody (items 11-13)
1. Language-scope preamble: items 11-13 "MC-scoped, plus one named MH sink extension (the `media_dial` verify path)".
2. Scope line: add the per-meeting `media_dial` signing key (ADR-0039 §5) as key material in scope, and the MH-verify-path extension (item 12 only, tokens + connect payload, ADR-0039 §5 "MH verifies at accept"), explicitly NOT the frame-forwarding path.
3. New emphasis block after the KEK-rotation block: "story 4 — MC-minted `media_dial`" with four bullets — keypair generation in the meeting actor beside/separate from the KEK (private half `dial_signing_key` in a redacting wrapper with no Debug output, lands with MC-D); token minting; the `MediaHandlerSet` encode carrying tokens; the `RegisterMeeting` push carrying the PUBLIC half.
4. Entitlement table: one row for the private signing key (MC meeting actor, in memory | MH, any internal.v1 message, disk, any JWKS, any log/metric/span/error/panic payload). Minted tokens covered by item-12 sentence + SAFE bullet (not asserting more than §5 says about tokens on MC→MH messages).
5. Item 11: "stays absolute for KEK and transmit keys"; the private dial signing key or any symmetric key crossing is an item-11 finding. The only permitted key bytes are the 32-byte public verification key on the ONE pair ADR-0039 §5 names, (RegisterMeetingRequest, media_dial_public_key) — the ADR is the authority; `ALLOWED_BYTES_FIELDS` is the guard-side home (entry lands in P1). Any other `ALLOWED_BYTES_FIELDS` entry is NOT covered by this exception: key bytes on it stay item 11 until an ADR amends §5 (security E1). Narrow on TWO axes: field (only that one §5 pair) and VALUE (only bytes from the public half) — the guard checks declarations, not what Rust assigns, and the 32-byte check cannot tell a seed from a public key, so signing key / seed / PKCS#8 / derived value into that field is item 11 even with the guard green. Public key string-encoded to dodge the guard is a finding (§5).
6. Item 12: sinks also apply to the private dial signing key and every minted token. Item 13: applies to the redacting wrapper around the private half.
7. SAFE list: public half on its one allow-listed field (next to `identity_public_key`); token delivery in `MediaHandlerSet` to the participant it was minted for = entitled-holder delivery, like `JoinResponse.meeting_kek`.
8. MH note (short paragraph after "Opaque relay"): token or raw connect payload reaching an item-12 sink or a gRPC `Status` message (cite the TODO entry "Five MC↔MH control-plane obligations from the ADR-0036 `internal.proto` reshape", its "Observability forward-carry for task 16" note, point (i)) on MH's verify path is item-12-class. Reasons are counted on the EXISTING `mh_jwt_validations_total{failure_reason}` (values: ADR-0039 §8, not restated; no new series/label) and never reach the client: the client sees only the close code — `DECLINED` for every failure before the `aud` check, `WRONG_HANDLER` only for `aud` mismatch (§5, §7). A bounded failure-reason value is not credential material, so it is outside THIS check (whether it may be logged is governed by ADR-0039 §8 / the telemetry contract, not here); the token, its raw (including not-yet-verified, §5 reads `meeting_id` before the signature) claims or header, or the connect payload in any sink is item-12-class (security E2). "Opaque relay" paragraph's "outside items 11-13's MC scope" sentence gets a qualifier pointing at this note.

### CHANGE 2 — §Client Credential Lifetime
1. New `###` sub-section "Scoped exception — the `media_dial` dial credential (ADR-0039 §5 'Client retention')" after Two gates: not a retained credential; SAFE only when ALL hold — current-set state, latest per in-set ref; replaced wholesale by each set (incl. token-only refresh, same refs/version — replaced, never accumulated); cleared on ref removal and at session end (cleared in `MediaTransport.disconnect()`, reached from `MeetingSession`'s teardown on every end path — leave, terminal failure; one anchor); passed only into `MhConnectRequest.media_dial_token` (a helper/closure that builds that request and retains nothing is SAFE), and opaque to the client — never decoded or parsed (e.g. reading `exp`; MC re-pushes every TTL/3, ADR-0039 §5 Delivery; story 4 client-retention line) — a decoded claims object is a second holding. Dead token for a parked in-set ref after `DECLINED` is still SAFE (still the latest for an in-set ref).
2. UNSAFE prose items, lettered (not numbered, to avoid colliding with §Credential Leak items 5-10): stored outside set state (store, prop, session object, localStorage/sessionStorage); kept after its ref left the set, incl. merging the new set into the old; reaching any §Credential Leak client sink (items 5-10) — attributed under §Credential Leak per the attribution rule; a fabricated request path (no `MediaDialTokenRequest` exists, §5) — the sanctioned path is `MediaConnectionUpdate` FAILED + wait for the next set, which is NOT a finding.
3. SAFE prose item F: one concrete instance pointing back to the conditions (no restatement).
4. Known blind spots: one bullet — `token` in `SESSION_TOKENS`, so a type holding only a dial token yields no finding; this lens is the only enforcement.

### CHANGE 3 — docs/TODO.md "MH does not verify meeting ownership"
One dated paragraph appended to the entry (2026-10-31, story 4, ADR-0039); the original RE-DERIVED text, including the "MUST cover BOTH" sentence, is left intact and the new paragraph extends that sentence BY NAME (no second copy). Tense: the design as of story 4, taking effect as P1/MH-1 (dial key) and MH-3 (owner-silence timer) land. Content: a RegisterMeeting passing REQUEST-level validation (ADR-0039 §1: the record (mc_id, MC endpoint, dial public key, heartbeat interval) upserts independently of the apply; an apply refusal does not roll it back) under self-asserted `mc_id` replaces, on that MH, the meeting's dial public key (new) and the MC endpoint (already overwritten today; now also MH's sender-binding and heartbeat target) → the registrant controls dial admission and binding answers there; ciphertext only (ADR-0036 §4; ADR-0039 Consequences residual-risks bullet 4). Owner-silence (§6, `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS`, no default restated): the timer then runs against the new registrant, and any message from it — re-registration or a heartbeat answered with a verdict at the endpoint it installed — resets it; a one-shot takeover nobody answers defers release by at most one timeout. Detection: the existing register-path takeover WARN is the signal for these effects too (same registration); recovery stays Scenario 13 by pointer. "MUST cover BOTH" extended to the dial key and the heartbeat-verdict path. Checkbox stays `- [ ]`.

### Additions from planning input (MC, client, semantic-guard)
- MC emission-site anchors (by file; function names that do not exist yet are "the implementing task"): keypair = a separate field on the meeting actor in `actors/meeting.rs`, sibling of the admission/KEK state, NOT inside `media_admission/kek.rs` or `epoch.rs`; a dial key created or replaced on any KEK path (rotate, epoch reset) is outside ADR-0039 §5 ("never rotated"). Minting in the meeting actor; `sub` from MC's session, never the request (§5). Two sites: (1) assembly + minting in the per-turn coalesced flush in `actors/meeting_media.rs` (where TTL/3 refresh and FAILED re-push enter) — a token-bearing set must NOT go through `envelope()` / `send_message()` / `SignalingPayload::server_message` (Raw); (2) encode to wire bytes in the participant actor, `actors/participant.rs`, the `handle_kek_update` analogue. RegisterMeeting push: `media_routing/pusher.rs` (spawned worker), `MeetingProgramming` and `MhClient::register_meeting` in `grpc/mh_client.rs` — only the 32-byte public verifying key crosses into the push worker; handing it the keypair/signing key is an item-12/13 exposure even if only the public half is encoded. In-crate counterpart: `built_request_carries_no_key_material`. `EndMeeting` carries no key.
- **Ruling on the token's mailbox hop (security reviewer P6, adopted; superseded my first draft which allowed Raw)**: a minted token never travels the meeting-actor → participant-actor hop as an untyped `SignalingPayload::Raw` `Vec<u8>`; it travels typed with a verified redacting Debug — the generated `MediaHandlerSet` / `MediaServerInfo` once P1 adds them to proto-gen's `skip_debug` set (story 4 contract line, `MediaServerInfo.media_dial_token` "credential, redacting Debug"; P1 step 8), or a dedicated redacting handle — and is encoded in the participant actor, mirroring `KekPush` / `handle_kek_update`. Scoped to token-bearing sets. Removing those `skip_debug` entries is item 13 (already named by item 13's existing `skip_debug` clause). Grounding: the existing KEK bullet's Raw-derives-Debug reasoning (a JWS is ASCII, recoverable from a byte Debug) + item 13.
- Entitlement table: three rows — private signing key; public key (MC; MH via the one allow-listed field); minted token (participant named by `sub` via the set; MH named by `aud` at verify; never any log/metric/span/error/close reason).
- MH note scope: `crates/mh-service/**` `media_dial` verify path only — connect/accept handling of the connect token (today `webtransport/connection.rs`'s connect decode → validate; MH-1 replaces it) and RegisterMeeting's key intake; the validator error's Display must not carry token/payload. Scope line + preamble updated in step; the Opaque-relay boundary stays true (wrapped keys on the frame path remain outside).
- Client (CHANGE 2): holder defined by ROLE (the structure holding the current set's refs in the media transport; a second `Map<ref, token>` e.g. on `MeetingSession` is outside it). SAFE transit = internal hand-off from the decoded `JoinResponse`/set message into that state, reference shape `takeMeetingKek` in `packages/sdk-core/src/signaling/kekIntake.ts` (KEK leaves the decoded message, never on `JoinedEvent`). UNSAFE adds: token on a public SDK event payload (`JoinedEvent`, any set/session event); a backoff timer or dial closure capturing the token string (SAFE: capture the ref, read the token at fire time); a ref-removal path bypassing ADR-0039 §4's one cleanup function; session-end clear in `MediaTransport.disconnect()` (same single anchor), not mere unreachability; version-gated token retention (ADR-0039 §4: a token-only refresh keeps the version and refs; a set with the same members only replaces tokens); merging new set into old. Token reaching `MediaConnectionError`/its cause = item 8 pointer. Persistence → reference existing UNSAFE 4 rather than restate. Debug panel / e2e bus named via SAFE-E projection rule, not as items 5-10. Sink reach is attributed under §Credential Leak, not lifetime. Exception is Gate 1 only; the connect-request send is the design, not Gate 2. R-23 qualified as browser-client-join R-23 (`docs/user-stories/2026-05-02-browser-client-join.md`). Blind-spot bullet scoped to RETENTION/lifetime (sink guards still read CATEGORY_A): the guard's segment matcher puts any `token`-segment name (e.g. `mediaDialToken`) in `SESSION_TOKENS`.
- Item 13: keep "verify the wrapper actually redacts"; no ed25519 crate type named SAFE.
- Extra file (semantic-guard request, fix-don't-defer): `.claude/agents/semantic-guard.md` Output Format verdict vocabulary drift (CLEAR/RESOLVED/ESCALATED vs review-protocol's CLEAR/RESOLVED-FIXED/RESOLVED-DEFERRED/ESCALATED) — split the RESOLVED template. Classified Not mine, Mechanical (requested by owner).
- Self-checks after implementation: no added fence lines; `cargo test -p dt-guard --test credential_leak_key_custody_fixtures` as positive control; layer-fast + run-guards.
- Follow-up obligation also covers client: no lens fixture for dial-token retention exists; owner is the client task that adds the set state.

### Follow-up obligation (recorded, not fixed here)
Recorded where MC-D will read it: one line in checks.md's "Fixture poles" paragraph (outside the must-fire markers) saying the dial-key and minted-token poles are owed by the task that introduces `dial_signing_key` (DRY (a); not also in TODO.md). The semantic-guard.md verdict split cites `.claude/skills/devloop/review-protocol.md` §Verdict Format (DRY (b)).
The key-custody lens fixtures (`crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/`) have no pole for the private dial signing key or a minted token; whichever task introduces `dial_signing_key` (MC-D) owns adding them.

### Deviations from the prompt text (surfaced)
- "Metrics only / one generic DECLINED" → precise per ADR-0039 §5: `WRONG_HANDLER` on `aud` mismatch; bounded reason in a log is not a finding (observability).
- Client sink parenthetical replaced by "items 5-10" (DRY).
- "validation-passing" → request-level validation, per ADR-0039 §1 (security); owner-silence wording bounded both ways (operations, media-handler).
- Reviewer conflict resolved: DRY asked in-place OR by-name amendment of "MUST cover BOTH"; operations asked new-paragraph amendment → by-name extension in the new paragraph satisfies both.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Meeting Controller (paired) | confirmed |
| Media Handler (paired) | confirmed |
| Client (paired) | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

Lead rulings at Gate 1: deviations (a)-(d) from the prompt text approved (each follows ADR-0039 text over the prompt's paraphrase); security's no-`SignalingPayload::Raw` token-hop ruling approved (grounded in story-4 `MediaServerInfo.media_dial_token` redacting-Debug contract + existing KEK Raw rule + item 13); `.claude/agents/semantic-guard.md` verdict-vocabulary fix approved (owner-requested, fix-don't-defer).

---

## Pre-Work

None

---

## Implementation Summary

CHANGE 1: checks.md §Credential Leak items 11-13: language preamble, Scope line (dial signing key + narrow MH verify-path extension), story-4 emission-site block (keypair, minting, token-bearing set with the no-Raw mailbox-hop rule, RegisterMeeting push of the public half), three entitlement-table rows, item 11 exception (one §5 pair; field + value axes; no string-encoding), item 12/13 extensions, opaque-relay qualifier, *MH verify path* paragraph, two SAFE bullets, and the story-4 fixture-pole obligation line (outside the markers). CHANGE 2: §Client Credential Lifetime scoped-exception subsection (Gate 1 only; set state by role; four conditions; transit; DECLINED parked ref), lettered UNSAFE (a)-(d) + attribution note, SAFE F, a blind-spot bullet. CHANGE 3: one dated paragraph on the TODO entry (open). Plus `.claude/agents/semantic-guard.md` verdict vocabulary, pointing to review-protocol §Verdict Format.

---

## Files Modified

- `scripts/guards/semantic/checks.md`
- `docs/TODO.md`
- `.claude/agents/semantic-guard.md`

---

## Devloop Verification Steps

- `./scripts/layer-fast.sh`: TOTAL_RESULT=OK (L1-L6 OK, Layer 3 includes docs guards).
- `cargo test -p dt-guard --test credential_leak_key_custody_fixtures`: 4 passed (positive control).
- `git diff -U0 -- scripts/guards/semantic/checks.md docs/TODO.md | grep '^+.*```'`: empty (no fence introduced); fixture-poles marker lines unchanged; no new `## Check:`.
- Lead fast check (Step 6): `DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh` TOTAL_RESULT=OK.
- **Gate 2** (final tree, after the last media-handler re-review edit): `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` TOTAL_RESULT=OK — L1 OK, L2 OK, L3 OK (55s), L4 OK (160s), L5 OK, L6 OK, L7 OK (1319s, env-tests + browser E2E). A first Gate-2 run was discarded because the tree changed mid-run.

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | Raw-hop bullet misattributed `envelope()`/`send_message()`/`server_message` to participant.rs; now names meeting_media.rs + messages.rs |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | Same file-pointer fix (F-T1) |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | Unsupported §8 clause removed; "raw claims" = claim set as a unit; takeover WARN qualified (SameMc unsignalled) |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | MC-endpoint-as-binding/heartbeat-target claim now cites ADR-0039 §1 + residual bullet 4 |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | Four restatements removed (field name, refresh parenthetical, marker drift rule, RESOLVED-DEFERRED definition). Extraction opportunities: none |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | Owning-MC side of a takeover (heartbeat_timeout replacement + rejected_ownership teardown) added |
| Semantic Guard | RESOLVED-FIXED | 2 | 2 | 0 | Client sink-coverage overclaim corrected (ts_pii reads CATEGORY_B, no `token`); claims-as-unit |
| Meeting Controller (paired) | RESOLVED-FIXED | 1 | 1 | 0 | Item-12 `{:?}` sentence carved out for a verified-redacting typed set (F-MC-1) |
| Media Handler (paired) | RESOLVED-FIXED | 2 | 2 | 0 | Re-review of later TODO edits: owning-MC-side heartbeat claim qualified to installed-endpoint case + kept-endpoint case added (FAILED_PRECONDITION ignored, §6); sentence order fixed |
| Client (paired) | RESOLVED-DEFERRED | 2 | 1 | 1 | F1 fixed (meeting JWT stays non-retained); F2 dial-token lens poles deferred to C1 by Lead scope ruling |

Semantic-guard native verdict: SAFE after fixes (docs-only diff; lens-consumer review).

Ownership lens: no GSA path and no Domain-judgment edit in the diff.

**At least one accepted deferral exists** (client F2) — see §Accepted Deferrals.

### Observation for story close (not a finding on this diff)
ADR-0039 §8 ("Clients still see only DECLINED") and the telemetry-contract `mh_jwt_validations_total` row ("the client always sees a generic DECLINED") are loose against ADR-0039 §5/§7 (`WRONG_HANDLER` on `aud` mismatch). This lens follows §5. Reconciling the ADR/contract wording is the ADR owner's call; flagged by observability + media-handler for `/close-story`.

---

## Accepted Deferrals

- **Client F2: dial-token retention lens poles** (scope decision by team-lead). The task excludes code, and dt-guard fixtures plus a count bump in a `.rs` test are code. The obligation is recorded durably in checks.md §Client Credential Lifetime, under Known blind spots ("Story-4 client poles are owed"), naming C1 as owner of `crates/dt-guard/tests/fixtures/ts_retained_credentials/lens/` and the count bump. MC-side poles are recorded the same way in §Credential Leak, owed by MC-D.

---

## Rollback Procedure

1. Start commit: `08520f67be321ad07c3f084eca1f435f2d4d2f58`
2. `git diff 08520f67..HEAD`
3. **Safe-revert unit**: the single docs-only commit (`scripts/guards/semantic/checks.md` + `docs/TODO.md` + `.claude/agents/semantic-guard.md`); no runtime, config or deploy effect, so `git revert` is safe at any time. Cost of a revert: MC-D/MH-1 Gate reviews lose the lens, so revert before those run or re-land ahead of them.

---

## Issues Encountered & Resolutions

None yet

---

## Lessons Learned

1. A lens task that names future emission sites benefits from the owning specialists as paired reviewers: MC, MH and client each caught a pointer or scope error the security lens alone would have shipped.
2. Late re-review edits after Gate 3 force a re-check by the reviewer whose text moved (operations re-checked the TODO paragraph) and a fresh Gate-2 run.

### Attribution discipline (code-reviewer P5)
Tree facts and reviewer rulings are attributed to the lens item or file they derive from, never to an ADR-0039 section that doesn't say them: guard checks declarations (`crates/dt-guard/src/internal_proto_no_key_material.rs`); 32-byte check vs seed (item 11 + judge-the-value); Raw-hop ruling (KEK-push bullet reasoning + item 13 + story-4 contract line); `MediaTransport.disconnect()` (the file); closure capture (existing blind-spot bullet). KEK-path dial key cites only §5 "beside and separate from the KEK" and "never rotated".
