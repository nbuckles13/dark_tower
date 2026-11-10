# Devloop Output: MH-CLOSE — MH numeric close codes on every intentional client-session close

**Date**: 2026-11-10
**Task**: run-story task #14 (MH-CLOSE) — see /home/dev/.cache/devloop/story-runs/story-runner/2026-10-31-survive-handler-loss/task-14.prompt; story docs/user-stories/2026-10-31-survive-handler-loss.md; ADR-0039 §7
**Specialist**: media-handler (paired: client)
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~4h30m (one Layer-7 iteration: cargo-chef cook lacked `vendor/`; fixed)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d926e48399bf007023ffdd8f89ffba71cf1da331` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer [1370bd] (media-handler)` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `2` |
| Security | `security [6f6788] — RESOLVED-FIXED` |
| Test | `test [ff4720] — RESOLVED-DEFERRED` |
| Observability | `observability [dc5287] — RESOLVED-DEFERRED` |
| Code Quality | `code-reviewer [a74b48] — RESOLVED-FIXED` |
| DRY | `dry-reviewer [389fce] — RESOLVED-FIXED` |
| Operations | `operations [a3c3b7] — RESOLVED-FIXED` |
| Semantic Guard | `semantic-guard [23c4c2] — RESOLVED-FIXED` |
| Paired Client | `paired-client [e65551] — RESOLVED-FIXED` |
| Paired Protocol | `paired-protocol [ff2bd0] — RESOLVED-DEFERRED` |
| Paired Infrastructure | `paired-infrastructure [1cb9d3] — RESOLVED-FIXED` |

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
Every client session MH ends on purpose closes with exactly one numeric `MhCloseCode`, delivered as a WebTransport SESSION close. Every other end (crash, idle timeout, client close, shutdown before MH-3) carries no code (ADR-0039 §7, R-19 MH half, R-34 MH row, R-40).

### Scope
- **Service(s)**: mh-service. Also a vendored dependency patch (wtransport), env-tests and runbooks.
- **Schema**: No
- **Cross-cutting**: Yes. This is a client-facing wire contract (C1 consumes it), and it needs a dependency patch (see Planning §0).

### Debate Decision
NOT NEEDED. ADR-0039 §7 governs. The wire-mechanism gap (§0) goes to @team-lead as an escalation, not as a debate.

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
| `crates/mh-service/src/webtransport/close.rs` | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/src/webtransport/mod.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/grpc/mc_client.rs` | Mine | — |
| `crates/mh-service/src/webtransport/server.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_close_code_integration.rs` | Mine | — |
| `crates/mh-service/tests/close_emission_scan.rs` | Mine | — |
| `crates/mh-service/tests/media_session_binding_integration.rs` | Mine | — |
| `crates/mh-service/tests/common/**` | Mine | — |
| `vendor/wtransport/**` | Not mine, Domain-judgment (GSA by §6.4 criterion: wire-format runtime coupling — writes CLOSE_WEBTRANSPORT_SESSION bytes; Lead upgrade) | protocol, infrastructure, security |
| `vendor/patches/wtransport/**` | Not mine, Domain-judgment | infrastructure, protocol, security |
| `Cargo.toml` | Not mine, Domain-judgment (Lead upgrade) | infrastructure |
| `Cargo.lock` | Not mine, Mechanical | infrastructure |
| `.dockerignore` | Not mine, Domain-judgment | infrastructure |
| `infra/docker/ac-service/Dockerfile` | Not mine, Domain-judgment (builder: `COPY vendor/ vendor/` before the cook) | infrastructure |
| `infra/docker/gc-service/Dockerfile` | Not mine, Domain-judgment (same, byte-identical builder) | infrastructure |
| `infra/docker/mc-service/Dockerfile` | Not mine, Domain-judgment (same, byte-identical builder) | infrastructure |
| `infra/docker/mh-service/Dockerfile` | Not mine, Domain-judgment (same, byte-identical builder) | infrastructure |
| `.github/dependabot.yml` | Not mine, Domain-judgment | infrastructure |
| `.github/workflows/audit-scheduled.yml` | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/Cargo.toml` | Not mine, Domain-judgment (machinery) | infrastructure |
| `crates/dt-guard/src/lib.rs` | Not mine, Domain-judgment (machinery) | infrastructure |
| `crates/dt-guard/src/main.rs` | Not mine, Domain-judgment (machinery) | infrastructure |
| `crates/dt-guard/src/vendor_patches.rs` | Not mine, Domain-judgment (machinery) | infrastructure |
| `crates/dt-guard/src/common/cargo_manifest.rs` | Not mine, Domain-judgment (machinery) | infrastructure |
| `crates/dt-guard/src/docker_context.rs` | Not mine, Domain-judgment (machinery) | infrastructure |
| `crates/dt-guard/src/workspace_deps.rs` | Not mine, Domain-judgment (machinery) | infrastructure |
| `crates/dt-guard/src/common/git_changes.rs` | Not mine, Domain-judgment (machinery: one vendored predicate; active-edit scope keeps vendor/) | infrastructure |
| `crates/dt-guard/src/common/test_code_filter.rs` | Not mine, Mechanical (literal folded onto the predicate) | infrastructure |
| `crates/dt-guard/src/api_version.rs` | Not mine, Mechanical (literal folded onto the predicate) | infrastructure |
| `crates/dt-guard/tests/**` | Not mine, Domain-judgment | infrastructure |
| `scripts/guards/simple/validate-vendor-patches.sh` | Not mine, Domain-judgment | infrastructure |
| `scripts/lang/_audit_gate.sh` | Not mine, Domain-judgment (Lead upgrade) | infrastructure |
| `scripts/lang/_audit_gate.test.sh` | Not mine, Domain-judgment | infrastructure |
| `scripts/lang/rust/audit.sh` | Not mine, Domain-judgment | infrastructure |
| `scripts/lang/_audit_attribute.py` | Not mine, Domain-judgment (vendored findings always NEW — security policy, infrastructure machinery) | infrastructure, security |
| `scripts/setup.test.sh` | Not mine, Minor-judgment (MSRV scan excludes vendor/: upstream manifest) | infrastructure |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mod.rs` | Not mine, Mechanical | test |
| `crates/env-tests/src/fixtures/mh_close.rs` | Not mine, Minor-judgment | test |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-overview.json` | Not mine, Minor-judgment | observability |
| `docs/runbooks/mh-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/WEBTRANSPORT_FLOW.md` | Not mine, Minor-judgment | protocol |
| `docs/TODO.md` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` | Not mine, Mechanical | dry-reviewer |

No `proto/**` path is touched; `MhCloseCode` is consumed from proto-gen as it is. `vendor/wtransport/**` is a Guarded Shared Area by the ADR-0024 §6.4 criterion (wire-format runtime coupling, Lead ruling). Its owners are protocol, infrastructure and security. Whether it also needs an enumerated entry in ADR-0024 and the `cross-boundary-ownership.yaml` mirror is an open Gate-1 question for paired-infrastructure / paired-protocol.

---

## Planning

### 0. Mechanism restatement, and the gap that is wider than the task (ESCALATED to @team-lead)

Mechanism: MH must end a WebTransport **session** with a code. That means a `CLOSE_WEBTRANSPORT_SESSION` capsule (0x2843, u32 code, empty message) on the session's CONNECT stream, then a FIN, so that the browser's `WebTransport.closed` RESOLVES with `closeCode`. This is the contract `signaling.proto` already states above `MhCloseCode` ("CLEAN close", where transport loss = `closed` rejects), and C1 reads `WebTransportCloseInfo.closeCode`.

**wtransport 0.7.2 (the latest on crates.io) cannot send that capsule.** It only parses it. `Connection::close(code, reason)` sends a QUIC CONNECTION_CLOSE. A browser treats that as a connection error: `closed` rejects with no closeCode, which is transport loss. In addition, 1..5 are not valid H3 codes. The CONNECT stream is owned by wtransport's driver task, so `quic_connection()` cannot reach it either. Shipping `Connection::close(code)` would make R-19 vacuous for browsers. It would also make a Rust test vacuous: a Rust wtransport client reports both a capsule and a QUIC close as `ApplicationClosed(code)` on some paths. Client, test and operations found this independently.

**Proposed path (b): a vendored, patched wtransport.** `vendor/wtransport/` is the 0.7.2 source with its LICENSE files kept, wired in through `[patch.crates-io]`. The patch adds one API, `Connection::close_session(code: u32)`, about 100 lines:
- a command channel from `Connection` to the driver worker that owns the CONNECT stream;
- the worker writes one HTTP/3 DATA frame containing the capsule (type 0x2843, length, u32 BE code, zero reason bytes) and FINs the send half;
- the worker then awaits the peer's teardown up to a bound and only after that closes QUIC with H3_NO_ERROR (0x100), never with app code N. This is the flush-before-drop rule: quinn `close` discards unsent stream data.

MH awaits the result, so `handle_connection` does not drop the connection before the capsule is flushed or the bound expires. The bound is a compile-time constant in `config.rs`, not an env key (obligation (a)). Patch hygiene: a `vendor/wtransport/PATCH.md` records the upstream version, the diff and how to drop the patch once upstream gains the API.
Patch provenance (security S4): it is a PATH patch, not a git fork. PATCH.md records the crates.io 0.7.2 sha256 (from Cargo.lock), and a script check (`vendor/wtransport/verify.sh`, run by an existing Layer-3 guard hook) diffs the tree against that pristine source plus the recorded patch, so any other drift fails loudly. Layer-6 `audit.sh` must pass. The hand-written encoding is a fixed 4-byte BE code, a zero-length message, and nothing parsed from the peer; security reviews it.
Test observability (test C1/C4): the patch adds a READ accessor, `Connection::session_close_received() -> Option<ApplicationClose>`, populated ONLY by upstream's existing capsule parser in `driver/streams/connect.rs`, not by the new send code. A capsule-less FIN decodes to code 0, which MH never sends. So both the Tier-1b tests and the env-tests (which link the same vendored crate) observe the capsule itself, not `closed()`.
Alternatives: (a) upgrade. There is no upstream release with the API, so this is not available. (c) QUIC close only. Client, test and operations reject it as vacuous for browsers. **Decision requested from @team-lead: approve (b), which fits this session, or redirect.**

### 1. Inventory: every way an MH client session ends (mh-service, post-accept)

| # | Site | End | Code |
|---|---|---|---|
| 0 | `server.rs` capacity reject; `connection.rs` session request/accept failure | no session exists (browser `ready` rejects) | none; not a session close |
| 1 | `accept_bi` failure | connection error | none (connection already gone) |
| 2 | `read_connect_message`: FramingBreak / Finished / Failed | pre-validation, no credential | DECLINED if the connection is still live |
| 3 | decode failure, empty oneof, non-connect first variant | pre-validation (malformed) | DECLINED |
| 4 | JWT validation failure (every kind) | credential rejected | DECLINED (kinds stay only on `mh_jwt_validations_total`) |
| 5 | register-meeting timeout (pending path; MH-1 deletes it) | | DECLINED |
| 5b | pending, server shutdown | | none |
| 6 | `SenderBindingOutcome::Declined(..)`: no_sender_binding, out_of_range, mc_auth_rejected, mc_endpoint_unknown | | DECLINED |
| 7 | declined_sender_binding_conflict | | BINDING_CONFLICT |
| 8 | declined_mc_unavailable | | MC_UNAVAILABLE, after the existing jitter sleep |
| 9 | hold loop `ConnectionEnd::MeetingReleased` (EndMeeting, including the add-after-release race in `handle_add_connection`) | | HANDLER_RELEASED |
| 10 | hold loop ClientClosed / Error (control stream FIN / failure) | client-initiated or transport | none |
| 11 | hold loop Shutdown (until MH-3) | | none |
| — | MH-1 aud mismatch | | WRONG_HANDLER (in the mapping, no emitter yet) |

The security conditions hold by construction. The inner lifecycle function returns `Err` only before validation or on the timeout, and the wrapper maps every inner `Err` to DECLINED. A specific code can only come from the `Ok(Some(SessionClose))` path, which is reachable only after `validate_meeting_token` succeeds. Every code is about the token's own meeting. The message bytes are always empty.

### 2. Types and the one mapping (`webtransport/close.rs`)
- `StartDecline` is a decline-only enum with the 6 decline outcomes. `SenderBindingOutcome::Declined` carries it. A total `From<StartDecline> for MediaSessionStartOutcome` feeds the existing counter. `mc_may_hold_a_registration` takes `StartDecline`, so its `Started` arm and that `#[expect]` go away.
- `SessionClose { CredentialRejected, RegistrationTimedOut, StartDeclined(StartDecline), MeetingReleased, WrongHandler /* MH-1; #[expect(dead_code)] with reason */ }`.
- `const fn SessionClose::code(self) -> MhCloseCode`: wildcard-free, with no UNSPECIFIED arm. `fn wire_code(MhCloseCode) -> u32` is the ONLY numeric conversion (from the prost enum, no retyped numbers).
- `ConnectionEnd::session_close(self) -> Option<SessionClose>` is wildcard-free: MeetingReleased gives Some; ClientClosed, Error and Shutdown give None. Code-less is a stated decision, not an omission.
- `async fn close_session(connection, SessionClose)` is the ONE emitter. It skips when the QUIC connection already has a `close_reason()` (nothing can be delivered, and nothing is counted). Otherwise it records the counter and awaits the patched `close_session(wire_code(..))`.
- `handle_connection` becomes accept + `serve_session(..) -> Result<Option<SessionClose>, MhError>` + one emission. The decline path still returns `Ok` (F9: no `status="error"`). The register-timeout and JWT-failure `Err`s keep their existing accept-loop accounting.
- Code-less paths do what they do today: drop, which is a quinn implicit QUIC close and never a capsule or CONNECT FIN, so the browser `closed` rejects (= loss).

### 3. Telemetry
ADR-0039 telemetry-contract.md lists "session closes by code", and story §MH-CLOSE asks for it, so I add it: `mh_webtransport_session_closes_total{code=wrong_handler|binding_conflict|mc_unavailable|handler_released|declined}`. One count per emitted close. All 5 are zero-initialised (wrong_handler included, so MH-1 has no gap). The label comes from a wildcard-free match over `MhCloseCode`, with no ids. It gets a catalog entry, an mh-overview panel and an ADR-0032 test. No other series changes.

### 4. Tests (Tier 1b; answering @test T1–T8)
- Primary observer (test C1(i)/C2): positive arms assert `session_close_received() == Some(code N, empty reason)`. A QUIC `ApplicationClosed(N)` with no capsule FAILS. Negative arms assert `None`, meaning no capsule and no CONNECT FIN; the secondary assertion is the QUIC-level end (`closed()`: ApplicationClosed(0) / LocallyClosed / TimedOut). The rig is a real in-process QUIC pair, so this is also the real-impl proof.
- Golden bytes (client A, protocol P1): minimal-varint encoding, as upstream `put_varint` produces it. HANDLER_RELEASED = `00 07 68 43 04 00 00 00 04`; DECLINED = `00 07 68 43 04 00 00 00 05`. Each is one DATA frame on the CONNECT stream, followed by FIN, with nothing else on that stream. A table over every `CloseCode` checks that the last 4 bytes equal `wire().to_be_bytes()`. The test lives in the vendored crate and is wired by protocol P3.
- Single emission (C3): one `close_session` in `close.rs`. A unit test over `include_str!` of `webtransport/*.rs` fails on any `.close(` call outside `close.rs`.
- Security S1/S5: one table-driven test covers read failure (framing break), undecodable envelope, empty oneof, RecoveryRequest first, and every JWT kind (expired, bad signature, unknown kid, alg none, HS256 confusion, wrong token_type, wrong meeting / garbage). Each must give byte-identical capsules (DECLINED, empty), with no non-DECLINED code on any of them. These arms have no jitter and no kind-dependent round-trip. Exits that cannot carry a code: no session yet (capacity reject, request/accept failure; the browser's `ready` rejects), and `accept_bi` failure (the connection is already gone, so `close_reason()` is set). Both are named and asserted to emit nothing.
- Security S2 (structural): the non-DECLINED variants of `SessionClose` can only be constructed through `ValidatedSession` (holds the validated claims' `MeetingKey`; its constructor is private to `close.rs` and is called once, right after `validate_meeting_token` returns Ok). Every pre-validation site can only name `SessionClose::declined()`. No code is ever derived from a meeting other than `claims.meeting_id`.
- T1: the observation point is the REAL wtransport client over the in-process QUIC pair (`tests/common/wt_client.rs`), not the `MediaTransport` shim. The seam keeps no `close()`. "Shim-observed" in the task means this rig, because the seam deliberately carries no lifecycle.
- Anti-vacuity (capsule vs QUIC close): with a capsule, the client's driver surfaces `ApplicationClosed(code)` on `accept_uni`/`receive_datagram`, and the client then closes QUIC itself, so `closed()` == `LocallyClosed`. A server QUIC close instead gives `closed()` == `ApplicationClosed(remote)`. Every coded arm asserts both. A raw-quinn capsule decode helper may be added if wtransport's surface proves ambiguous.
- Mapping unit tests: exhaustive over `StartDecline::ALL` and `SessionClose` (compile-coupled), no zero, the expected table, codes ∪ {WRONG_HANDLER} == every non-zero proto variant, and a pin of the proto numbers.
- Per path: table of every credential-failure kind from `tokens.rs` → identical DECLINED with empty bytes, plus a valid-token positive control on the same rig; malformed envelope → DECLINED; each decline outcome → its code; conflict → BINDING_CONFLICT with the incumbent still open; MC unreachable → MC_UNAVAILABLE, with latency ≥ the retry budget and ≤ budget + `MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS` + slack (constant read, not retyped), and the jitter on that arm only; register timeout → DECLINED (paused time); EndMeeting on a session that has already forwarded a frame → HANDLER_RELEASED.
- Negative arms: client-initiated close and shutdown cancellation → no capsule. Pinned exactly: client close gives `closed()` == `LocallyClosed` with no driver ApplicationClosed; shutdown gives the remote QUIC close (`ApplicationClosed(0)` from quinn's implicit close) on `closed()` and no capsule. The counter is unobserved on both.
- Counter: ADR-0032 deltas per arm, with unobserved on the code-less arms.

### 5. Env-tests (paired with test)
- HANDLER_RELEASED: tighten `35_mc_server_mute_teardown.rs` straggler (the only real-cluster session open at EndMeeting) to `code == HANDLER_RELEASED`, empty reason, capsule path, and fix its comment. **29 is not extended**, because it has no participants and its module doc forbids shared-capacity effects. A session there would need a real MC join to get a sender binding. The choice of 35 is recorded here.
- DECLINED: `26_mh_quic.rs` dials a live MH (slot-label discovery) with a structurally valid but invalid credential → DECLINED, empty reason. Positive control in the same test: a valid dial to that handler is admitted.

### 6. Docs, same commit
`close_declined_connection` doc; `docs/TODO.md` retryable/terminal entry (closed, with a pointer); runbooks (mh-incident-response Scenario 13 area, mh-deployment ~740–760: MH now sends the code and the client acts on it from C1, keep "do not shorten the retry budget"; a code → outcome → series triage table; code-less = crash or idle timeout); a rollback note: MH must stay at or ahead of C1, because rolling MH below this commit after C1 makes declines and releases look like loss; catalog; INDEX.

### 6b. What code-less paths put on the wire (operations / test T2 / protocol P4)
- **Problem.** Dropping a session is NOT code-less on the wire. In quinn 0.11, `Drop for SendStream` finishes the stream. When MH drops a wtransport Connection, the driver's CONNECT SendStream can send a FIN before `implicit_close` runs, especially on a multi-thread runtime. A browser reads that bare FIN as `closed` resolving with code 0, which the client counts as a contract fault.
- **Fix.** Every code-less end that MH initiates explicitly closes QUIC with H3_NO_ERROR 0x100 and an empty reason BEFORE anything is dropped. Once quinn has a local close, `SendStream::drop` sends no FIN. This applies to the hold-loop ClientClosed and Error ends, Shutdown, pending-shutdown (5b), and pre-validation exits where the connection is already closed (a no-op there).
- **Single place.** This is the second, code-less function in close.rs, `end_without_code`. close.rs stays the only file that calls `.close(VarInt`.
- **Client-initiated close.** The client closed, so MH's explicit close is a no-op.
- **What the browser sees.** Per RFC 9114 §9 and W3C semantics, a browser is EXPECTED to treat a QUIC close as a connection error, so `closed` rejects (= loss). This is unverified; see the §7 residual.
- **Contract faults.** No MH path sends a capsule with code 0 or a bare CONNECT FIN. If the browser behaves as expected, rollouts produce no C1 contract faults.
- **Negative arms.** They run on a MULTI-THREAD runtime and assert `session_close_received() == None` (no capsule AND no FIN), with `closed()` == ApplicationClosed(0x100) as the pinned secondary.

### 7. Residual for C1 (not blocking this task)
No browser runs in this task, so browser resolution is evidenced by the W3C/draft-ietf-webtrans-http3 semantics plus the capsule-path assertion. Also C1's: the code-less wire is now a QUIC CONNECTION_CLOSE with H3_NO_ERROR (0x100) and no capsule. C1 must verify in a browser how `closed` settles on that: reject (loss, expected), or resolve `{closeCode: 0}`. If it resolves with 0, C1 must treat that as loss WITHOUT counting a contract fault, because the recovery is identical and the fault count would be spurious. C1's Layer-7 browser spec must assert that `closed` resolves with `closeCode` against a real MH (HANDLER_RELEASED / DECLINED).

### 8. Gate-1 refinements (all accepted; folded into the design above)

**Vendored patch (b): shape and hygiene**
- Public surface is exactly two items (code-reviewer R3, security P5):
  - `Connection::close_session(code: u32)`.
  - `Connection::session_close_received() -> Option<SessionEnd { Capsule { code, reason }, Fin }>`. It is filled from upstream's capsule branch AND its ImmediateFin branch, so a bare CONNECT FIN is visible (test P1).
- The bounded wait runs on the CALLER side (semantic-guard 1b). The driver worker only writes the frame, FINs, and signals back over a oneshot, so the driver loop never stalls. `close_session` then awaits `quic.closed()` under the linger bound, then closes QUIC with H3_NO_ERROR 0x100 and an empty reason (the linger ends early on peer reaction, see protocol P2).
- `close_session` is idempotent: a second call is a no-op. If the CONNECT stream is already reset or finished, it returns Ok and never falls back to a QUIC close carrying the app code (security P3).
- Encoding uses wtransport-proto's existing VarInt/frame encoders (no hand-rolled varints): one DATA frame holding capsule 0x2843, length 4, the u32 BE code, and zero message bytes, written once, then FIN.
- A golden-bytes test typed from RFC 9297 / draft-ietf-webtrans-http3 lives in the vendored crate; PATCH.md records where (client A, test P2).
- `vendor/wtransport` is an explicit `[workspace] exclude`. A path dependency inside the workspace root would otherwise become a member automatically. It is also explicitly excluded from fmt, clippy and guard scans, and PATCH.md states every exclusion.
- wtransport-proto stays on crates.io, unpatched.
- `verify.sh`:
  - fetches the pristine 0.7.2 `.crate`, checks its sha256 against the checksum recorded in PATCH.md (`b4273ce3…`, the only home for it), and fails loudly if it cannot fetch it or the vendor tree is missing;
  - diffs the vendor tree against pristine + PATCH.diff;
  - asserts the patch only ADDS lines, so MC (which also links wtransport) runs upstream code byte-for-byte (operations C2);
  - is run by the Layer-3 guard hook. It takes the pristine source from the local cargo registry cache first (sha-checked), then the network, and fails loudly if neither works. The hermetic self-test feeds a local fixture tarball through an env seam (DEVLOOP_TEST precedent) and needs no network;
  - has a hermetic `verify.test.sh`: pristine + patch gives PASS, a one-byte drift gives FAIL with its reason token (test P7).
- Supply chain (security P4):
  - an advisory check fails loudly when any RustSec advisory names wtransport at the vendored upstream version, and also fails if the advisory DB is unavailable;
  - `scripts/lang/_audit_gate.sh`'s dependency-change predicate is extended to `vendor/*/Cargo.toml`;
  - both are cross-boundary rows owned by infrastructure.
- A guard test asserts that mh-service `src/` never calls `session_close_received`. The peer's own claim about how the session ended must not feed any MH decision.

**Linger bound**
- `CLOSE_SESSION_LINGER: Duration = 1s` is a compile-time constant in config.rs (no env key), with a `const` assertion that it is below `SHUTDOWN_MARGIN_SECONDS`. MH-3's shed must not overrun grace.
- The constant's doc explains the value: one generous RTT plus the peer's teardown, and the MH_MAX_CONNECTIONS hold cost.
- The connection slot stays held until `close_session` returns, because `handle_connection` awaits it before the accept loop decrements. The rig asserts the session is still counted during the linger.
- Nothing is read or dispatched during the linger: the media loops are already cancelled, and on the pre-validation path nothing was ever spawned.
- Linger time is driven by the peer only and does not depend on the failure kind. The S1 table asserts this explicitly (security P2).
- The runbook triage table states that a coded close can take up to 1 s before the QUIC connection ends.

**Types (code-reviewer R1, security P1, semantic-guard 2)**
- `CloseCode` is a five-variant domain enum (no zero) with `ALL`, `proto()` (the one prost touch), `wire() -> u32` (the one numeric conversion: through proto(), checked, pinned by test) and `as_label()`.
- `SessionClose::code() -> CloseCode`. A test iterates the prost enum's values and requires `CloseCode::ALL` to map one-to-one onto the non-zero variants.
- `close::validate(&MhJwtValidator, &str) -> Result<ValidatedSession, SessionClose>` is the ONLY way to obtain a `ValidatedSession`, and it calls `validate_meeting_token` itself.
- `ValidatedSession` holds only the `MeetingKey` plus the meeting_id and participant_id strings, never the claims or the token. The post-validation path takes the meeting and participant from it (one binding, keeping the sub-provenance rule).
- Logs from close.rs carry only the code label, connection_id and meeting_id. The JWT error keeps its single existing sink.

**Errors and metrics (semantic-guard 3/4)**
- The Result of `close_session` is never discarded. A bound expiry or write error is logged at `debug!`/`warn!` with `error = %e`.
- The wrapper returns the inner `MhError` to the accept loop unchanged.
- The counter is recorded BEFORE the await. The catalog entry says it counts EMITTED closes, so `session_closes{declined}` below the decline count means closes skipped because the connection was already gone. That is expected for client races; a sustained gap needs investigating (operations C3, in the runbook table too).
- `record_media_session_start` and `record_jwt_validation` stay on every arm.

**Tests (test P3–P6, code-reviewer R2)**
- Bound-expiry arm: a peer that never tears down still receives the capsule. MH returns within the bound (constant read, not retyped) plus slack, then closes QUIC with 0x100, never with app code N.
- Coded-arm secondary assertion: `closed()` is one of {LocallyClosed, ApplicationClosed(0x100)} and is NEVER ApplicationClosed(n) for any MhCloseCode n.
- Single-emitter scan:
  - walks `src/` with read_dir, with a non-empty control that has its own message and a control that connection.rs is present;
  - requires close.rs to contain exactly one emission call (positive control);
  - matches the specific needles `close_session(` and `.close(VarInt`, and fails if either appears anywhere else.
- Register-timeout arm: a small timeout on the real clock, not paused time, because quinn's timers would advance too.

**Rollback / ops**
- Safe-revert units, direction EMISSION FIRST, PATCH SECOND (operations Gate-3 F1): (1) the MH emission + `close.rs` + counter + panel + runbook rows revert as one unit, safe alone (the patch stays, unused); (2) the vendored patch and its machinery (`[patch.crates-io]`, `.dockerignore`, `dt-guard vendor-patches`, the audit.sh/_audit_gate vendored plumbing, the dependabot note) revert only together with or after (1) — mh-service calls the patch's API.
- Direction: MH stays at or ahead of C1.
- Host-side step: the env-tests in 26/35 need the MH image rebuilt with the patch. A stale image fails red, not falsely green.

**Additional cross-boundary rows (infrastructure, Minor-judgment):** `scripts/lang/_audit_gate.sh`, `scripts/audit.sh` (vendored advisory check), and the guard and fmt exclusion lists under `scripts/**` where a scan would otherwise sweep `vendor/`.

**Observability items 1–8 (accepted)**
- Catalog meaning: one count = one close capsule MH initiated, not one delivered. Bound expiry is still counted; already-closed is not counted, and its decline stays on starts_total. Counted once, in the single emitter, before the await.
- `code` tokens are the client's `dt_client_mh_connection_total{outcome}` tokens. They come from `CloseCode::as_label` over `ALL`, which is also the zero-init source (no second list).
- The catalog states the relationships: declined ⊇ JWT failures (kinds only on `mh_jwt_validations_total`) ∪ malformed pre-validation (peer-drivable, so it never pages alone) ∪ the declined_* starts outcomes. Panels must not add this series to starts_total.
- F9 delta-0 assertions stay.
- `no_media_session` is still sampled after the close completes; the catalog wording is updated.
- Panel: `sum by (code) (rate(...[$__rate_interval]))` with legend `{{code}}`. No alert.
- ADR-0032 test: 5 zero-init series; each coded arm moves its own code by exactly 1 and the other four by 0; client-close, shutdown and already-closed arms are unobserved.
- One debug/warn event on bound expiry.

**DRY items 1–7 (accepted)**
1. No second proto-number pin. `proto-gen/tests/signaling_roundtrip.rs` stays the only home for the numbers; the close.rs tables map to MhCloseCode VARIANTS.
2. Option (b), patching wtransport only: vendoring wtransport-proto too would double the vendored surface and the advisory checks. The DATA frame is built with `Frame::new_data(..).write(..)` and `put_varint`. The 0x2843 literal is a second copy of upstream's private const, tied back by an encode-then-parse round-trip test through `Capsule::with_frame` and `CloseWebTransportSession::with_capsule` (code and empty reason), alongside the RFC golden bytes.
3. A wildcard-free reverse match `StartDecline::from_outcome(MediaSessionStartOutcome) -> Option<StartDecline>` (Started => None) enforces the coupling at compile time.
4. The label lives in close.rs (`CloseCode::as_label`) only. A test checks that it equals the lowercased `as_str_name()` with `MH_CLOSE_CODE_` stripped.
5. Two thin observer helpers that take an `MhCloseCode`: `mh-service/tests/common/wt_client.rs`, and ONE env fixture `env-tests/src/fixtures/mh_close.rs` used by both 26 and 35. A non-collapse note goes in `docs/specialist-knowledge/dry-reviewer/INDEX.md` §False Positive Boundaries (cross-boundary row: dry-reviewer, Mechanical).
6. Env-tests tighten the EXISTING invalid-credential checks in 26 (~230-250, ~543-581) and the straggler in 35 (~624). No parallel dials.
7. The pristine sha256 lives only in PATCH.md, and verify.sh reads it from there and sources `scripts/guards/common.sh`. The single-emitter scan skips comment and doc lines (config.rs test precedent).

**Protocol items P1–P4 and docs (accepted)**
- P1: the golden bytes use minimal varints (see §4). The earlier 4-byte `80 00 28 43` form was wrong.
- P2: after `close_session` has sent the capsule, the patched driver treats ANY peer read-side outcome on the CONNECT stream (FIN, reset, capsule, unexpected FIN) as session-close completion and closes QUIC with 0x100, never with upstream's 0x104 ClosedCriticalStream. This is a guarded early branch ahead of the existing match, so the patch stays additive. The completion signal also ends the linger as soon as the peer reacts; otherwise the linger is still bounded at 1 s. Tests: a peer that resets the CONNECT stream without closing QUIC sees `closed()` == ApplicationClosed(0x100), and a peer that FINs sees the same.
- P3: the vendored crate is excluded from the workspace, so `cargo test --workspace` never runs its tests. `verify.sh` therefore runs `cargo test --manifest-path vendor/wtransport/Cargo.toml <filter>`, with a positive control that fails if zero tests matched or ran. PATCH.md records the hook.
- P4: see §6b (`end_without_code`, multi-thread negative arms).
- Docs: a "MH session close (ADR-0039 §7)" subsection in `docs/WEBTRANSPORT_FLOW.md`, covering the bytes, FIN, the linger (ends on peer reaction or the bound), 0x100, code-less = explicit QUIC close with no capsule or FIN, and MhCloseCode as the SSoT for the numbers. `signaling.proto` is NOT edited.
- Residual: a capsule still unacknowledged when the 1 s bound expires (heavy loss) is discarded by quinn's close, so the browser sees loss, which the client handles safely. This is recorded in PATCH.md and the runbook table.

**Test follow-ups on the protocol changes (accepted)**
- The golden bytes carry their derivation in a comment beside them: DATA type 0x00, length 7, then 0x2843 as a 2-byte varint (0x40|0x28, 0x43) per RFC 9000 §16, then capsule length 04, then the code as a u32 BE.
- The reset-peer and FIN-peer arms each assert that the QUIC close is 0x100 and NOT 0x104, the upstream behaviour the patch overrides.
- Code-less red evidence: I will temporarily change `end_without_code` into a plain drop, run the negative arm 20 times, and record the red rate here. If it is not reliably red, the behavioural arm becomes supporting evidence only. Either way, a structural check is added: the single-emitter scan also requires that every code-less exit in `serve_session`/`handle_connection` routes through `end_without_code`, with the same three controls (non-empty scan, connection.rs present, exactly one emitter call) and no `drop(connection)` outside the two emitters.
- The test doc states why the arm runs multi-thread: on current_thread the dropped stream's FIN may never be driven, so the arm would pass vacuously.

**Infrastructure C1–C8 (accepted; these SUPERSEDE the earlier verify.sh, exclusion and vendored-test wording above)**
- GSA: classified by criterion only (wire-format runtime coupling; owners protocol + infrastructure + security), stated in PATCH.md. `cross-boundary-ownership.yaml` is NOT edited.
- C1 (Docker build context):
  - `.dockerignore` gets `!vendor/wtransport/` with a comment.
  - A new rule in dt-guard `docker_context.rs`: every root `[patch.*]` path must be admitted by a `!` entry. Paths are read through `common/cargo_manifest.rs`; the rule has its own reason token and a negative fixture.
  - The Dockerfiles are unchanged.
  - Host action: ALL FOUR service images rebuild (Cargo.lock changes) before Layer 7.
- C2 (crate tree stays pristine): `vendor/wtransport/` is EXACTLY pristine 0.7.2 plus the patch. The metadata lives outside the crate in `vendor/patches/wtransport/{PATCH.md,PATCH.diff}`. The verifier is `scripts/guards/simple/validate-vendor-patches.sh` (common.sh, STATUS contract, auto-discovered).
- C3 (verifier split by layer):
  - Layer-3 guard, hermetic (no network, no cargo), each check with its own reason token, plus positive controls (tree found, diff non-empty):
    - (i) reverse-applies PATCH.diff and compares the result to the pristine tree digest in PATCH.md;
    - (ii) the diff has no '-' lines;
    - (iii) Cargo.lock has `wtransport` with NO `source` line, at PATCH.md's upstream version;
    - (iv) the crate is in root `exclude` and is a `[patch.crates-io]` path target.
  - The self-test lives outside `guards/simple/`. It is wired into layer3.sh's parallel_queue, uses the DEVLOOP_TEST root seam, and covers: PASS, one-byte drift, a '-' line, an unpatched lock, a missing tree.
  - Layer 6 anchors the pristine digest to crates.io: fetch the .crate, check its sha256 against the crates.io INDEX checksum, unpack, compute the digest, compare; fail loudly on fetch failure. The design does not rely on a registry-cache hit.
- C4 (the vendored crate's own tests never run, so the golden test moves):
  - Choice (b): the patch exposes one `#[doc(hidden)]` capsule encoder, which the driver itself uses. This is a third surface item and needs security's agreement.
  - mh-service tests golden-test it (minimal-varint bytes, RFC 9000 §16 derivation) and round-trip it through wtransport-proto's parser (crates.io dev-dep), with a nextest filter that must match at least 1 test.
  - No `cargo test --manifest-path vendor/...`.
- C5: dt-guard `workspace_deps.rs` derives the vendored set from root `[patch.*]` path targets (one derivation in `common/cargo_manifest.rs`) and skips R2/R3 for those crates with a stated reason. It gets an e2e fixture.
- C6 (audit):
  - `scripts/lang/rust/audit.sh` excludes vendored crates from discovery using the C5 derivation, with an AUDIT_GRAPH "skipped as vendored" line.
  - Coverage: the audit runs on a derived copy of the root Cargo.lock in which patched path packages get the crates.io registry source, through the existing `scan_lock` (suppressions and fail-closed tokens unchanged).
  - The `_audit_gate.sh` predicate becomes `vendor/**`, and its SSoT-drift test is extended to match.
- C7: a dependabot `ignore` for wtransport, with a comment pointing to PATCH.md's drop procedure. The root `[patch]` entry and the workspace dependency entry also get comments.
- C8 (exclusions): no fmt or clippy exclusion, because a non-member path patch is not selected; PATCH.md says so. Guard exclusions are added only where a guard actually reds with the tree in place, each one cited, through ONE shared vendored-path predicate per language (common.sh helper, dt-guard const), with the existing `vendor/` literals folded onto it. validate-cross-boundary-scope keeps seeing vendor/.

**Security conditions on the encoder (accepted)**
1. `encode_close_session_capsule` returns the EXACT bytes the driver writes to the CONNECT stream, DATA frame header included. The driver writes them verbatim, with no second inline encoding.
2. The golden values are typed literals from RFC 9297 / draft-ietf-webtrans-http3 with the RFC 9000 §16 minimal varint (protocol P1): `00 07 68 43 04 00 00 00 04` for code 4. A second literal for 0x01020304 (`00 07 68 43 04 01 02 03 04`) pins endianness. Neither is captured from the encoder's output.
3. Round trip through wtransport-proto's parser: code equals the input, and the message length is 0.
4. The src/ guard test has a positive control so an empty scan cannot pass. The encoder is listed in PATCH.md's public-surface list (three items).

**Implementation-time rulings (supersede the matching lines above; recorded so the record matches the tree)**
- **C3 → infra option A** (paired-infrastructure approved, conditions A1–A6): the Layer-3 verifier is the dt-guard subcommand `vendor-patches` behind `scripts/guards/simple/validate-vendor-patches.sh`. It is hermetic: native strict reverse-apply, no `patch` binary, no network, no cargo. Its self-test is `crates/dt-guard/tests/vendor_patches_e2e.rs` (Layer 4), not a bash `*.test.sh` in layer3.sh. The Layer-6 anchor in `scripts/lang/rust/audit.sh` calls `vendor-patches --upstream-meta` / `--pristine-digest`, `--attributed-lock` and `--list` (bash parses neither Cargo file). The audit harness `scripts/lang/_audit_gate.test.sh` gains a tool-bins-derived dt-guard stub, a curl stub and 5 vendored cases.
- **C4**: the golden-bytes and round-trip tests are in `crates/mh-service/src/webtransport/close.rs` (unit tests) against the `#[doc(hidden)]` encoder, not in the vendored crate. `verify.sh`, `verify.test.sh` and `--manifest-path` test runs are NOT used.
- **C7 deviation** (paired-infrastructure accepted): NO Dependabot `ignore:`, because the security-owned `audit-suppressions` guard forbids any ignore block (`dependabot-ignore-present`). A wtransport bump PR goes RED on `vendor-patch-not-in-lock`; the dependabot.yml comment names that token and `vendor/patches/wtransport/PATCH.md`.
- **C8**: no fmt, clippy or guard exclusion was needed. The one Rust vendored predicate is now `common::git_changes::{VENDORED_PREFIX, is_vendored}`, and the two scattered literals (`test_code_filter.rs`, `api_version.rs`) are folded onto it. The active-edit scope used by `cross-boundary-scope` now KEEPS vendor/ paths, so an edit there must be declared.
- **Test P3(a) mutation evidence**: with `end_without_code` changed into a plain drop, the multi-thread shutdown arms went RED 20/20. The red came from the pinned secondary (`closed()` == ApplicationClosed(0) instead of 0x100); a bare CONNECT FIN was not observed in this environment. So the behavioural arm is supporting evidence, and the structural control is `close_emission_scan.rs::every_codeless_end_routes_through_end_without_code`. A second mutation, sending the code as a QUIC-only close instead of the capsule, turned every positive arm RED ("ended WITHOUT a capsule").
- **Test P3 as built**: the linger arm silences the peer through a UDP relay blackhole (`tests/common/udp_proxy.rs`). It asserts the slot is HELD during the linger (a dial is rejected with max_connections=1), and freed no earlier than `CLOSE_SESSION_LINGER` and no later than the bound + 2 s. "Capsule still received" and "MH then closes with 0x100" cannot both be observed in one arm: a peer that receives the capsule closes QUIC itself, so it never sees MH's close. MH's 0x100 is pinned on every other arm's secondary assertion.
- **Deferred, UNTESTED behaviourally (protocol P2 test arms)**: the "peer RESETS (or FINs) the CONNECT stream without closing QUIC → MH closes with 0x100, not 0x104" arms, and observing "capsule received AND MH's 0x100" in one arm (test P3). They need a raw HTTP/3 WebTransport test peer: wtransport's client, on receiving a capsule, resets AND closes QUIC itself, so MH's subsequent QUIC close never reaches it. The coded arms' "never 0x104 / never an MhCloseCode" secondary assertion therefore cannot observe this property (it is real only on the shutdown/code-less arms, where MH closes first). The branch is reviewed code only. A raw-peer test util is estimated at ~200–250 LoC and is task-sized. Low impact: a 0x104 after a capsule the browser has already acted on does not change the outcome. Tracked: `docs/TODO.md` §Media Path Obligations (owner: test, with protocol).
- **Host-side action (Lead)**: ALL FOUR service images rebuild (Cargo.lock changes), then Layer 7: env-tests 26 and 35 need the patched MH image and fail red against a stale one.

**Gate-3 review fixes (iteration 1)**
- **Unbounded capsule write** (semantic-guard, protocol F1, security F1): ONE `CLOSE_SESSION_LINGER` deadline now covers the capsule write AND the wait for the peer. The vendored driver writes the capsule in its own cancel-safe select arm, so a credit-withholding peer never stalls the driver loop. The patch returns `Result<(), SessionCloseError>`, logged as `error = %e`. New arm `a_peer_withholding_credit_cannot_hold_its_slot_past_the_linger_bound` (8-byte stream window plus blackhole) is red without the deadline (mutation-checked).
- **Counted after the write** (observability F4, code-reviewer F4); the catalog, metric doc and runbook reconcile only the 1:1 pairs (operations F2, observability F3).
- **F9 delta-0 checks were vacuous** (observability F1): every decline arm now waits for the handler to return (`AcceptLoopRig::wait_held`, on the server's own held-connection counter) before asserting.
- **Failure kind** (observability F2): the docs now say the kind is in the log only; MH-1 splits `failure_reason` per the ADR-0039 telemetry contract. Security agreed a bounded kind label would be fine, and that is MH-1's work.
- **Vendored-advisory attribution** (infrastructure F3 + security ruling): always `AUDIT_FINDING=NEW(vendored)` via `vendor-patches --packages`. Harness: registry base, path base, non-vendored PREEXISTING control, governed suppression.
- **dt-guard**: modes are a clap group (a usage error on conflict); `[patch]` targets must be `vendor/<name>`; one typed Cargo.lock reader (`cargo_manifest::read_lock`) shared with workspace_deps; subcommand doc order fixed.
- **Scan**: skips only `#[cfg(test)]` mod bodies (security F2); the needle is any argumented `.close(` (code-reviewer F8); the builder is renamed `ValidatedSession::closing`.
- **Code quality**: a compile-time positivity assertion for `wire()`; a bounded proto scan window; a literal mapping table; no duplicated meeting id; `counts_undelivered_datagrams()` derived from the close; no needless allocations.
- **Tests**: a jitter-arm function plus unit test; MC_UNAVAILABLE latency floor/ceiling from `mc_client::retry_backoff_floor` / `retry_budget_ceiling`; a skip-path arm; `meeting_claims` and close-metric asserts hoisted (DRY).
- **Docs**: the revert direction is EMISSION FIRST, PATCH SECOND; the PATCH.md surface is four items, plus the setup.test exclusion and the drop procedure; the `h3_no_error` doc now says "expected"; the env-test 26 serial rationale is updated.

**Gate-2 Layer-7 fix (attempt 1 failed at `build_first_party_images`)**
- **Cause**: cargo-chef's recipe skeletonizes workspace MEMBERS only. `vendor/wtransport` is a root-`exclude` `[patch]` path, so the builder stage (which receives only `recipe.json` from the planner) had no `vendor/wtransport` when `cargo chef cook` resolved the graph: "patch location ... does not contain packages matching `wtransport`".
- **Fix**: `COPY vendor/ vendor/` right after `COPY --from=planner ... recipe.json` in the builder stage of all four service Dockerfiles. The block stays byte-identical, and `validate-shared-image-builder` passes.
- **Verified locally with the pinned cargo-chef 0.1.78** (no docker here), on a scratch context mirroring the `.dockerignore` allowlist:
  - `cargo chef prepare`: the recipe has NO vendor manifest, even with vendor present, so the cook never rewrites it.
  - Builder without vendor: reproduces the deploy's exact error.
  - With `COPY vendor/`: `cargo chef cook --check --package mh-service` succeeds, and `vendor/wtransport` is byte-identical afterwards (`diff -r`).
  - The `COPY . .` overlay, then `cargo check` of all four services, succeeds and reuses the cooked wtransport: identical files, preserved mtimes.
- **Guard**: `dt-guard docker-context` gains `patch-path-not-in-cook-stage`. In every `infra/docker/*/Dockerfile` stage that runs `cargo chef cook`, each `[patch.*]` path must be COPYed from the context before the cook line; a `--from=` copy or a COPY after the cook does not count. It has unit tests, and it reds on the real tree with the COPY removed (mutation-checked).

**Stated residual (security):** an unknown kid can trigger a JWKS refetch, so the time to DECLINED inside validate_meeting_token differs by kind. This is pre-existing, reveals only whether a kid is published (public JWKS), and MH adds no timing difference of its own.

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (S1–S5, P1–P5; 3-item vendored surface incl. doc(hidden) encoder) |
| Test | confirmed (conditional on (b)) |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (C1–C3; conditional on (b)) |
| Semantic Guard | confirmed |
| Paired Client | confirmed (conditional on (b)) |
| Paired Protocol (GSA-by-criterion owner, Lead-added) | confirmed (P1–P4) |
| Paired Infrastructure (Domain-judgment owner, Lead-added) | confirmed (C1–C8) |

**Lead rulings at Gate 1**
- Option (b), vendored + patched wtransport 0.7.2, accepted: the most complete option that fits one session and crosses no hard safety line. Security confirmed the supply-chain shape; it is reversible.
- `vendor/wtransport/**` is GSA by the §6.4 criterion (wire-format runtime coupling), with owners protocol + infrastructure + security. It is NOT enumerated in cross-boundary-ownership.yaml; that needs a micro-debate.
- Owner-implements for the infrastructure rows is declined. The skill and ADR-0024 removed owner-implements, so the implementer makes every edit and paired-infrastructure stays the independent reviewer.
- The env-test targets move from 29 to 35 (HANDLER_RELEASED) and 26 (DECLINED); the reason is recorded in §5.
- The per-code counter `mh_webtransport_session_closes_total{code}` is added in full, per obligation (c).
- Classification-sanity guard: `STATUS=OK`.

---

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 | F1: unbounded capsule write let a peer pin a slot pre-auth (co-signed with protocol); F2: emission-scan blind past `#[cfg(test)]`; F3: vendored advisories always NEW/blocking |
| Test | RESOLVED-DEFERRED | 6 | 5 | 1 | Deferred: raw HTTP/3 peer reset/FIN arms (TODO §Media Path Obligations) |
| Observability | RESOLVED-DEFERRED | 4 | 3 | 1 | Deferred: meeting-path `failure_reason` split (owned by MH-1; docs corrected) |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | no TODO extraction entries |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | revert order corrected; triage reconciliation fixed |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 2 | 2 | 0 | actor-blocking (same as security F1); error-context typed SessionCloseError |
| Paired Client | RESOLVED-FIXED | 2 | 2 | 0 | C1 residual: browser reading of capsule-less 0x100 close |
| Paired Protocol | RESOLVED-DEFERRED | 2 | 1 | 1 | Deferred: P2 raw-peer test arms (same TODO entry as test) |
| Paired Infrastructure | RESOLVED-FIXED | 5 | 5 | 0 | image build with vendored crate first proven at Layer 7 |

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Wire mechanism (vendored wtransport, GSA by criterion)
| Item | Before | After |
|------|--------|-------|
| wtransport | crates.io 0.7.2 (receive-only session close) | `vendor/wtransport` = pristine 0.7.2 + additive patch (242 added lines, 0 removed), root `[patch.crates-io]` + `exclude` |
| Session close API | none | `Connection::close_session(code) -> Result<(), SessionCloseError>` (capsule write is its own cancel-safe driver arm), `error::SessionCloseError`, `session_close_received()`, `#[doc(hidden)] encode_close_session_capsule` |
| Patch record | — | `vendor/patches/wtransport/{PATCH.md,PATCH.diff}` (upstream sha256, pristine tree digest, drop procedure) |

### mh-service
| Item | Before | After |
|------|--------|-------|
| Intentional closes | close by drop (no code) | ONE emitter `close.rs::close_session`: capsule with code, empty message, linger ≤ 1 s, QUIC `H3_NO_ERROR` |
| Code-less ends | drop (could FIN the CONNECT stream) | `close.rs::end_without_code`: explicit QUIC `H3_NO_ERROR` before drop |
| Mapping | — | `SessionClose::code() -> CloseCode` (5 variants, no zero), wildcard-free; non-DECLINED only via `ValidatedSession` (from `close::validate` only) |
| Declines | `MediaSessionStartOutcome` | `StartDecline` (decline-only; reverse map wildcard-free) |
| `handle_connection` | one long body, many exits | accept + `serve_session` → `Result<SessionEnd>`; every `Err` = DECLINED; `MhError` returned unchanged |
| Metric | — | `mh_webtransport_session_closes_total{code}`, zero-init ×5, catalog, mh-overview panel |
| Config | — | `CLOSE_SESSION_LINGER` = 1 s (constant, `const` assert < `SHUTDOWN_MARGIN_SECONDS`) |

### Guards and audit (infrastructure machinery)
- `dt-guard vendor-patches` (+ wrapper, 20 e2e cases); the `[patch.*]` derivation in `cargo_manifest`; the docker_context `patch-path-not-admitted` rule; workspace_deps vendored partition; one vendored predicate in `git_changes`; the active-edit scope keeps vendor/.
- `scripts/lang/rust/audit.sh`: vendored discovery skip, attributed root lock, crates.io provenance anchor (verified live in Layer 6: `AUDIT_ANCHOR=vendor/wtransport upstream=wtransport@0.7.2 RESULT=anchored`); `vendor-patches-unavailable`; gate `vendor/*`; harness +5 cases.
- `.dockerignore` admits `vendor/wtransport/`; audit-scheduled builds dt-guard; dependabot comment (no `ignore:`); `setup.test.sh` MSRV scan excludes vendor/.

### Tests and env-tests
- Tier 1b: `webtransport_close_code_integration.rs` (8 arms: the credential/pre-validation byte-identical DECLINED table with a valid-token control, registration timeout, EndMeeting HANDLER_RELEASED, client close and shutdown on current_thread and multi_thread, linger + slot holding via a UDP blackhole relay); `media_session_binding_integration.rs` decline arms extended in place (DECLINED ×4, BINDING_CONFLICT with incumbent untouched, MC_UNAVAILABLE); `close_emission_scan.rs` (one emitter, code-less routing, no production use of the peer accessor or the encoder); `close.rs` unit tests (mapping, proto coupling, labels, golden bytes, upstream round trip).
- Env-tests: 35 straggler asserts HANDLER_RELEASED via the capsule; 26 `assert_mh_rejects` asserts DECLINED plus a valid-dial control on the same handler; fixture `fixtures/mh_close.rs`.

### Docs
Catalog, runbooks (§Close codes triage table, retry-budget wording, MH-ahead-of-C1 rollback), `WEBTRANSPORT_FLOW.md` §MH session close, `docs/TODO.md` (close-code item: MH half done), INDEX ×2.

---

## Gate 2 — Full Validation

- Attempt 1: L1–L6 OK; **L7 FAIL** `deploy-failed` (ac-service `cargo chef cook`: the `[patch]` path `vendor/wtransport` was absent from the builder stage). This was caused by the diff and consumed L7 attempt 1. Fix: `COPY vendor/ vendor/` before the cook in all 4 Dockerfiles, plus a dt-guard `docker-context` `patch-path-not-in-cook-stage` rule. paired-infrastructure re-reviewed it: RESOLVED-FIXED.
- Attempt 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → **TOTAL_RESULT=OK** (L1 6s, L2 5s, L3 114s, L4 135s, L5 2s, L6 6s incl. `vendor-pristine-anchored-wtransport-0.7.2`, L7 1610s). All four images were rebuilt with the vendored patch. Env-tests `test_meeting_end_releases_every_handler_even_with_a_straggler` (HANDLER_RELEASED capsule) and `test_mh_rejects_forged_jwt` (DECLINED with a valid-dial control) passed.

## Devloop Verification Steps

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

- `docs/TODO.md` §Media Path Obligations — raw HTTP/3 peer arms for MH close completion (test + protocol)
- `docs/TODO.md` §Observability Debt — meeting-path JWT `failure_reason` split, scheduled for MH-1 (observability)

Out-of-diff flakes recorded during validation (not findings on this diff):
- `docs/TODO.md` §Flaky Tests — ac-service `test_jwt_iat_one_second_past_boundary` wall-clock race
- `docs/TODO.md` §Flaky Tests — gc-service `eligibility_predicate_matches_sql` staleness under load

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: two units, direction EMISSION FIRST, PATCH SECOND. (1) MH close-code emission (`crates/mh-service/src/webtransport/close.rs`, its wiring in `connection.rs`/`config.rs`/`metrics.rs`, the counter, mh-overview panel, runbook rows and tests) is safe to revert alone; the vendored patch stays, unused. (2) The vendored wtransport patch and its machinery (`vendor/**`, `[patch.crates-io]` + `exclude`, `.dockerignore`, `dt-guard vendor-patches` + the git_changes/workspace_deps/docker_context changes, audit.sh/_audit_gate/_audit_attribute vendored plumbing, audit-scheduled build step, dependabot note) reverts only together with or after (1), never before. Also: after story 4 C1 ships, do not revert (1) without accepting that declines and releases read as transport loss to new clients (`docs/runbooks/mh-deployment.md` §Rollback). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

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
