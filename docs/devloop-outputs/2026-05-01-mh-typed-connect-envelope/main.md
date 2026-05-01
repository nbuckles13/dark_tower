# Devloop Output: MH Typed Connect Envelope

**Date**: 2026-05-01
**Task**: Align MH client→server WebTransport wire format with MC's typed-protobuf envelope (toe-hold for future fields)
**Specialist**: media-handler
**Mode**: Agent Teams (full, paired-with=protocol)
**Branch**: `feature/mh-quic-env-tests`
**Duration**: ~7h (incl. ~5h blocked on stale-image debugging)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `dda4000de87192b3d7d4a4db9ccd9c993a83485a` |
| Branch | `feature/mh-quic-env-tests` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `gate-3` |
| Implementer | ready |
| Implementing Specialist | `media-handler` |
| Iteration | 1 |
| Security | CLEAR |
| Test | CLEAR (2 nits deferred) |
| Observability | CLEAR |
| Code Quality | CLEAR |
| DRY | CLEAR |
| Operations | CLEAR |
| Paired-Protocol | CLEAR |

---

## Task Overview

### Objective

Introduce a minimal typed connect message for MH's WebTransport accept-path. Today MH expects raw JWT bytes as the first framed message; MC expects a typed `ClientMessage{JoinRequest{...}}`. Align MH on a typed envelope while no client SDK has shipped — the cheapest moment to make this wire-breaking change. Toe-hold for future fields.

### Scope

- **Service(s)**: MH service (`crates/mh-service/`)
- **Schema**: No (no DB)
- **Cross-cutting**: Yes — proto schema (Guarded Shared Area), MH server, MH tests, env-tests

### Debate Decision

NOT NEEDED — design discussion happened during R-33 env-test devloop close-out (2026-04-30). Recorded in conversation; not formal ADR-worthy.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2 (Mine / Mechanical / Minor-judgment / Domain-judgment) and §6.4 (Guarded Shared Areas).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/signaling.proto` | Not mine, Minor-judgment | protocol |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_integration.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_accept_loop_integration.rs` | Mine | — |
| `crates/mh-service/tests/common/wt_client.rs` | Mine | — |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Mechanical | test |

### Notes per row

- **`proto/signaling.proto`** — Add `MhClientMessage` envelope + `MhConnectRequest` message. GSA disallows Mechanical; Minor-judgment per ADR-0024 §6.4 with protocol confirming at Gate 1 + Gate 3 (active collaborator via `--paired-with=protocol`).
- **`crates/mh-service/src/webtransport/connection.rs`** — Swap raw-bytes decode for `MhClientMessage::decode` + oneof match. JWT validation logic unchanged.
- **`crates/mh-service/src/errors.rs`** — Possible only if a new `MhError` variant is needed for decode-failure; preference is to reuse `WebTransportError("Invalid message format")` to mirror MC's discipline.
- **`crates/mh-service/tests/webtransport_integration.rs`** — Update `connect_and_send_jwt` helper (or inline) to encode `MhClientMessage` instead of raw JWT bytes. Adds two new tests (`malformed_envelope_bytes_rejected_on_wt_accept_path`, `empty_envelope_oneof_rejected_on_wt_accept_path`) per @test Gate 1 coverage gaps.
- **`crates/mh-service/tests/webtransport_accept_loop_integration.rs`** — Same helper update.
- **`crates/mh-service/tests/common/wt_client.rs`** — Add a `write_mh_connect(jwt)` helper that frames the typed envelope. Keep `write_framed` available for negative tests.
- **`crates/env-tests/tests/26_mh_quic.rs`** — Encoder swap in `encode_jwt_frame` + `send_jwt_on_bi_stream`. Same call sites, same assertions, same outcomes. Doc-comment blocks (lines 43-46, 149-159) updated.

**GSA intersection check**: `proto/signaling.proto` is a single GSA (wire format only — not auth-routing, not detection/forensics). Single owner: protocol. No intersection rule applies.

---

## Planning

### Recommendation on message shape (for @paired-protocol)

```proto
// MH client→server connect envelope (parallel to ClientMessage for MC).
// First framed message on a new bidi stream after WebTransport accept.
// Toe-hold for future fields (codec preferences, ICE, correlation IDs).
message MhConnectRequest {
  string join_token = 1;  // Meeting JWT; same semantics as JoinRequest.join_token.
}

// Wrapper envelope for client→MH messages.
message MhClientMessage {
  oneof message {
    MhConnectRequest connect_request = 1;
  }
}
```

**Rationale**:
- **Separate oneof envelope** (not reusing `ClientMessage`): MH and MC have orthogonal message lifecycles. Sharing the wrapper would couple them and force every MC-only variant onto MH's surface (and vice versa). Mirrors the asymmetry already present in `ServerMessage` vs. (no MH server-message exists yet).
- **Field named `join_token`**: matches `JoinRequest.join_token` for cross-service symmetry. The token is a meeting JWT today and could be others (e.g., reconnect-binding) tomorrow; `join_token` reads correctly for either.
- **Why `MhConnectRequest`, not `JoinRequest`**: the MC `JoinRequest` already exists in this package; a second message named `JoinRequest` would be confusing even with type-system disambiguation. `MhConnectRequest` reads as "establish the MH-side leg of the join."
- **No defensive `meeting_id` field**: per task brief NON-GOALS, no redundant cross-check today. JWT carries `meeting_id` claim (used by MH today). Adding a parallel `meeting_id` field is a follow-up if/when client SDK starts sending it for early correlation logging.

### Open questions for @paired-protocol

1. **Envelope vs. bare message**: Recommendation = oneof envelope. Alternative = bare `MhConnectRequest`, no wrapper, with a future-fields path of "decode `MhConnectRequest` first; if a new variant is needed later, version the wire by adding a magic-byte prefix or a discriminator field." The envelope is the cheaper future-proofing. Confirm or push back.
2. **Field naming**: `join_token` vs. `meeting_token`. Recommendation = `join_token` (cross-service symmetry). Either works.
3. **Error-handling shape**: MC sends an `ErrorMessage` back to the client on decode failure (`signaling.ErrorCode::InvalidRequest` / `Unauthorized`) before closing the stream. MH today does NOT send an error frame back — it just closes the WT session by returning Err from the handler. Recommendation = keep MH's current "close, no error frame" discipline for this devloop (no `ServerMessage` shape exists for MH; defining one is out of scope). Generic client-facing error matches via WT session close + recv stream finish, same observability as today's path. Confirm.
4. **Default-empty semantics**: protobuf `oneof` with no variant set decodes to `message: None`. Recommendation = treat that as decode-error (return `WebTransportError("Invalid connect message")`), same as MC's `_ => return Err(...)` arm at `mc/connection.rs:147-167`.

### Implementation steps (post Gate 1 approval)

1. **Add the new messages** to `proto/signaling.proto`. Re-run `cargo build -p proto-gen` (build.rs handles codegen).
2. **Update MH wire decoder** (`crates/mh-service/src/webtransport/connection.rs:173-182`):
   - Replace the raw-bytes `String::from_utf8` path with `MhClientMessage::decode(...)`.
   - Match on the `connect_request` oneof variant; pull `join_token` field; pass that string to `jwt_validator.validate_meeting_token(...)`.
   - On decode failure or wrong/empty oneof variant: return `MhError::WebTransportError("Invalid connect message".to_string())` (mirrors MC's "Invalid message format" but uses MH's existing error variant — generic client-facing string, no echo of decode internals).
   - Tracing: emit a `warn!` with `target: "mh.webtransport.connection"`, `connection_id = %connection_id`, and `error = %e` for the prost decode error (mirrors today's UTF-8-failure warn at `connection.rs:176-180` and MC's pattern at `mc-service/.../connection.rs:130-134`). Do NOT introduce a new tracing target like `mh.webtransport.envelope` or `mh.webtransport.decode` — fragments a stable observability target (per @observability Gate 1 ask).
   - Metric observability: pre-validation decode failures take the `WebTransportError` path which surfaces at the `accept_loop` as `mh_webtransport_connections_total{status=error}` — no `mh_jwt_validations_total` emission. **This is identical to today's observable** for both the missing-frame and UTF-8-garbage-frame cases (today's UTF-8 path at `connection.rs:175-182` short-circuits before reaching the `record_jwt_validation` call at line 197, so the counter is never incremented for non-UTF-8 input — verified against source per @observability Gate 1 correction). No matrix shift; no dashboard impact.
3. **Update MH test helpers** (`crates/mh-service/tests/common/wt_client.rs`):
   - Add `pub async fn write_mh_connect(send: &mut SendStream, jwt: &str) -> Result<...>` that builds `MhClientMessage{connect_request: MhConnectRequest{join_token: jwt}}`, encodes it, and writes a length-prefixed frame.
   - Keep `write_framed` for negative tests that need raw-bytes / garbage / oversized payloads.
4. **Update MH integration tests**:
   - `webtransport_integration.rs::connect_and_send_jwt` — switch to `write_mh_connect` helper. Affected tests: `valid_meeting_jwt_connection_accepted_and_tracked`, `expired_meeting_jwt_rejected_on_wt_accept_path`, `oversized_jwt_rejected_on_wt_accept_path` (oversized still > 8KB validator cap), `wrong_token_type_guest_rejected_on_wt_accept_path`, `provisional_connection_kicked_after_register_meeting_timeout`, `provisional_connection_survives_when_register_meeting_arrives_within_window`, `mc_notify_connected_fires_on_join_and_disconnected_fires_on_client_drop`.
   - `missing_jwt_stream_closed_before_write_rejects_connection` — assertion shape unchanged (still expects `status=error`, no JWT counters); the test never sent any frame so the decode-vs-raw-bytes distinction does not surface.
   - `webtransport_accept_loop_integration.rs::connect_and_send_jwt` — same swap.
   - **NEW** `malformed_envelope_bytes_rejected_on_wt_accept_path` (per @test Gate 1 coverage gap 1): in `webtransport_integration.rs`, send a framed message of garbage bytes (e.g., `&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]`) via existing `write_framed` (NOT `write_mh_connect`). Pinned to `#[tokio::test(flavor = "current_thread")]` per the file-preamble recorder-binding requirement (lines 47-57). Asserts: `mh_webtransport_connections_total{status="error"}` delta = 1, `mh_jwt_validations_total{result="failure"}` delta = 0, `mh_jwt_validations_total{result="success"}` delta = 0. Locks in plan §step 2 metric-shape contract: pre-validation decode failures emit only the accept-path error counter, never JWT validation counters.
   - **NEW** `empty_envelope_oneof_rejected_on_wt_accept_path` (per @test Gate 1 coverage gap 2): in `webtransport_integration.rs`, build `MhClientMessage { message: None }`, encode via prost, framed-write via existing `write_framed`. Same assertion shape as the malformed-bytes test. Pinned to `current_thread`. Distinguishes "decode succeeded but no variant" from "decode failed" — both must close, neither emits validation counters. Locks in plan §Q4 contract.
5. **Update env-test** (`crates/env-tests/tests/26_mh_quic.rs`):
   - Replace `encode_jwt_frame` with the typed-envelope encoder (proto encode, then 4-byte BE length prefix).
   - `send_jwt_on_bi_stream` body — same shape, different payload.
   - Update the `# Wire format` doc-comment (lines 43-46) and the `encode_jwt_frame` doc-comment (lines 149-159).
   - Negative tests: `test_mh_rejects_forged_jwt` and `test_mh_rejects_oversized_jwt` use `assert_mh_rejects` which calls `send_jwt_on_bi_stream`. The forged-JWT test still wraps the bad token in a typed envelope (validates signature path). The oversized-JWT test (9000 bytes filler, no dots) — under the new wire format, this wraps 9000 bytes into a `MhConnectRequest.join_token` string field, encodes, frames; the encoded `MhClientMessage` is ~9015 bytes (under 64KB framing cap), validator sees a 9000-byte token and rejects on size. Same observable outcome (peer-close on the session). No test-shape change beyond the encoder swap.
6. **Verify**:
   - `cargo check -p proto-gen` (codegen).
   - `cargo build` (compile MH against new types).
   - `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings`.
   - Guards: `bash scripts/guards/run-guards.sh`.
   - `cargo test -p mh-service` (unit + integration).
   - **Live cluster**: rebuild MH image, redeploy to Kind, then `cargo test -p env-tests --features all --test 26_mh_quic` — must stay 6/6 green.

### Observability guard-rails (per @observability Gate 1 review)

- **Tracing target unchanged**: new decode-failure `warn!` keeps `target: "mh.webtransport.connection"`. No new sub-target.
- **Tracing field shape**: include `connection_id = %connection_id` at minimum; include `error = %e` for the prost `DecodeError` (logging only, never propagated into the user-facing `MhError` per @security guard-rail).
- **No new metrics**: confirmed observability-neutral. The `record_webtransport_connection("error")` emission at the accept loop is the canonical signal for this path.
- **Today's UTF-8 short-circuit at `connection.rs:175-182` does NOT increment `mh_jwt_validations_total`** — the counter only fires inside the validator-Err arm at line 197, which the UTF-8 short-circuit bypasses. Tomorrow's protobuf-decode-failure produces the identical observable. Plan corrected after @observability traced source to verify (replaces my earlier "matrix shift" framing — there is no matrix shift).

### Security guard-rails (per @security Gate 1 review)

- **Decode-error string opacity**: on `prost::DecodeError`, log via `warn!(... error = %e, ...)` (mirrors MC's pattern at `mc-service/.../connection.rs:132`) but `MhError::WebTransportError("Invalid connect message".to_string())` MUST be a fixed string. Do not interpolate the `DecodeError` into the user-facing error — would echo internal protobuf field offsets/tags. Same rule applies for empty-oneof and unknown-variant arms.
- **Single field, no scope creep**: `MhConnectRequest` carries one field (`join_token`) this devloop. Any additional field (e.g., parallel `meeting_id` for cross-check, `client_version`, `participant_id`) is explicitly out of scope per @security — each creates a new validation surface.
- **No `MhServerMessage`** this devloop (already a NON-GOAL; @security confirmed the "no error frame back to client" close-only discipline is preferable from a security stance: no oracle for the attacker to learn malformed-envelope vs. failed-JWT distinction).
- **Wire-breaking confirmation — CLEARED at Gate 1** (2026-05-01): @team-lead and @paired-protocol independently audited; the only consumer of MH's raw-JWT wire format is `crates/env-tests/tests/26_mh_quic.rs` (updated in lockstep by this devloop). No client SDK, demo, internal harness, scripts/, tools/, or browser code consumes it. Wire-break is fully contained.

### Why this scope, not more

- No `MhServerMessage` envelope today — task brief: no defensive additions. MH currently never writes back to the JWT-carrier stream. Adding a server-side envelope ahead of need is the kind of pre-emptive abstraction the brief explicitly wants to avoid. **Tech-debt follow-up captured below**: per @paired-protocol Gate 1 review, the asymmetry (MC has `ServerMessage` + structured `ErrorMessage`; MH has neither) is now load-bearing. The first MH→client message — likely a redirect, capacity-exceeded, or binding-token rotation — will need to define `MhServerMessage` and gets the option to wire-back structured errors retroactively at that point.
- No `meeting_id` field today — JWT carries it; cross-check is a defensive add the brief excludes.
- No new metrics — wire format swap is observability-neutral; existing accept-path counters cover the change.

### Proto file placement decision

Per @paired-protocol Gate 1 suggestion: place `MhConnectRequest` and `MhClientMessage` adjacent to `ClientMessage` in `proto/signaling.proto` (around line 285-300) rather than at the file's end. Keeps client→server envelopes co-located for future readers. No section divider added — the `// ===…===` separators in `signaling.proto` (lines 110, 151, 187) delineate ADR-0023 logical groupings; cargo-culting that pattern for a 2-message addition would clutter the file. Inline `//` field comments and a 1-line purpose comment above each `message` (matching the file's existing style).

### Plan-stage reviewer ledger (running)

| Reviewer | Verdict | Outstanding action for me |
|----------|---------|---------------------------|
| @paired-protocol | CLEAR (Gate 1) | None for plan; Gate 3 Ownership Lens entry pending. |
| @security | APPROVED | Both conditions cleared (decode-error opacity captured; wire-break audit cleared). |
| @observability | CLEAR | Note one factual correction landed re today's metric path. |
| @code-reviewer | CLEAR | Nit re metric label parity for decode vs wrong-oneof — add a one-line rationale comment near the match arm at implementation time. |
| @dry-reviewer | CLEAN | Gate 2 byte-identity check requested for the parallel encoders (env-test encoder vs MH-test helper). Capture as Gate 2 self-check below. |
| @test | Pending | 2 coverage gaps + 2 nits being addressed in next iteration. |
| @operations | Pending | Confirmation incoming. |

**Gate 2 self-check items (deferred from plan-stage):**
- @dry-reviewer: byte-identity check — env-test encoder and MH-test helper produce the same wire bytes for the same JWT. Implementation note: assert via a small in-test `assert_eq!` once both encoders exist, OR confirm-by-construction by routing both through the same `MhClientMessage{...}.encode_to_vec()` builder pattern (different call sites, identical encoded output by virtue of using the generated codec). Plan to use the latter (no shared helper, just identical-shape `MhClientMessage` construction) and add a one-time sanity test that both produce equal bytes if @dry-reviewer wants the explicit guard.
- @code-reviewer: one-line rationale comment near the match arm explaining why decode-failure and empty-oneof / unknown-variant arms collapse to the same `MhError::WebTransportError("Invalid connect message")` (security guard-rail + label parity). **Decision**: deliberate single-label collapse — all three paths surface as `mh_webtransport_connections_total{status=error}` at accept_loop with no JWT-validations counter increment. Splitting into two labels (`decode_failed` vs `wrong_variant`) would create a distinction without a difference for SRE — both are "client sent a malformed connect envelope, never reached validation." The rationale comment will say so, near the match arm.

---

## Pre-Work

None.

---

## Implementation Summary

Added `MhConnectRequest` and `MhClientMessage` to `proto/signaling.proto` adjacent to `ClientMessage`. Swapped MH's WebTransport accept-path decoder from raw `String::from_utf8` to `MhClientMessage::decode` + oneof match (`crates/mh-service/src/webtransport/connection.rs:175-203`). Decode-failure and empty-oneof both surface as `MhError::WebTransportError("Invalid connect message")` — a fixed string with no echo of decode internals (security guard-rail). Added `write_mh_connect` helper in `crates/mh-service/tests/common/wt_client.rs`; updated all 7 MH integration test call sites and the env-test encoder. Added two new component tests (`malformed_envelope_bytes_rejected_on_wt_accept_path`, `empty_envelope_oneof_rejected_on_wt_accept_path`) per @test Gate 1 coverage gaps; both lock in the metric-shape contract that pre-validation decode failures emit only `mh_webtransport_connections_total{status=error}`, never `mh_jwt_validations_total`.

---

## Files Modified

| File | Lines | Note |
|------|-------|------|
| `proto/signaling.proto` | +12 | New `MhConnectRequest` + `MhClientMessage` (GSA Minor-judgment, owner=protocol) |
| `crates/mh-service/src/webtransport/connection.rs` | +27/-4 | Decoder swap + rationale comment on label-collapse |
| `crates/mh-service/tests/common/wt_client.rs` | +22 | New `write_mh_connect` helper; `write_framed` retained for negative tests |
| `crates/mh-service/tests/webtransport_integration.rs` | +75/-6 | 7 call-site swaps + 2 new contract tests |
| `crates/mh-service/tests/webtransport_accept_loop_integration.rs` | +5/-1 | 3 call-site swaps |
| `crates/env-tests/tests/26_mh_quic.rs` | +43/-11 | Encoder swap + signature tightening to `&str` (Option A nit) |

Net: +184/-22 across 6 files.

---

## Devloop Verification Steps

1. `cargo check -p mh-service --tests` — clean.
2. `cargo check -p env-tests --features all --tests` — clean.
3. `cargo clippy -p mh-service --all-targets -- -D warnings` — clean.
4. `cargo clippy -p env-tests --all-targets --features all -- -D warnings` — clean.
5. `cargo test -p mh-service --test webtransport_integration` — 10/10 (incl. 2 new contract tests).
6. `cargo test -p mh-service --test webtransport_accept_loop_integration` — 3/3.
7. `bash scripts/guards/run-guards.sh` — 22/22 PASS.
8. **L8 single-test smoke** against fresh Kind image (SHA `f050c7af7ac8...`): `test_mh_accepts_valid_meeting_jwt` PASS in 2.91s; MH log signature confirmed NEW (`"JWT validation succeeded"`, no `"JWT is not valid UTF-8"`).
9. **L8 file-scoped**: `cargo test -p env-tests --features all --test 26_mh_quic` — 6/6 passed, 1 ignored (pre-existing #[ignore]'d component-tier stub).
10. **L8 full regression sweep**: `cargo test -p env-tests --features all` — 115/115 across 12 binaries (`24_join_flow` 9/9, `21_cross_service_flows` 5/5, `30_observability` 2/2, `40_resilience` 2/2). No regressions.

---

## Code Review Results

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| @security | CLEAR | Both Gate 1 conditions satisfied (scope discipline, wire-break audit). Validator contract preserved. Error-string opacity confirmed. |
| @test | CLEAR | 2 nits raised, both fixed before commit (drop(conn) → _conn binding consistency; encode_jwt_frame &[u8] → &str signature tightening). Mechanical classification on env-test approved. |
| @observability | CLEAR | No new metrics; decode-failure metric-shape contract preserved AND newly guarded by 2 contract tests. No PII in logs. |
| @code-reviewer | CLEAR | Plan match; ADR-0001/0002/0003/0019/0024 compliance verified; rationale comment in place. |
| @dry-reviewer | CLEAR | Construction-equivalence of parallel encoders accepted. 3 hoist opportunities recorded as tech debt (not in scope). |
| @operations | CLEAR | Wire-break consumer audit confirmed in-tree; rollback unification verified; no infra/CI changes; observability matrix unchanged. |
| @paired-protocol | CLEAR (Ownership Lens for GSA) | Proto change matches Gate 1 plan; placement adjacent to `ClientMessage`; field naming symmetric with MC's `JoinRequest.join_token`. |

---

## Tech Debt

- **MH lacks a server→client envelope.** MH today never writes structured signals back to the client; the WT session close from `Err` return is the only client-observable failure signal. The first MH→client message (likely a redirect, capacity-exceeded, or binding-token rotation) needs to define `MhServerMessage` (parallel to MC's `ServerMessage`) and at that point gets the option to wire-back structured `ErrorMessage` signals retroactively to the connect path. Captured per @paired-protocol Gate 1 review (2026-05-01); explicitly out of scope for this devloop.
- **Frame read/write logic duplicated** between MH and MC (`read_framed_message` / `write_framed_message` — same 4-byte BE length-prefix wire format). Acknowledged at plan-stage; consolidating would require a shared `crates/common/src/webtransport/` helper, which is itself a GSA. Separate refactor scope.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `dda4000de87192b3d7d4a4db9ccd9c993a83485a`
2. Review all changes: `git diff dda4000..HEAD`
3. Soft reset (preserves changes): `git reset --soft dda4000`
4. Hard reset (clean revert): `git reset --hard dda4000`
5. No schema or infra changes — `git reset` is sufficient. Proto changes mean any in-flight clients on the new envelope will break — but no clients exist yet.

---

## Issues Encountered & Resolutions

1. **L8 first-attempt failed 3/6 (positive-path tests) on stale image.** Implementer's hypothesis matched team-lead's: deployed MH binary still ran the pre-envelope `String::from_utf8` path. Confirmed via MH-1 logs showing `"JWT is not valid UTF-8"` warns on protobuf-envelope payload bytes (the 2-byte varint length prefix `0xAC 0x02` for a ~300-byte JWT is an unanchored UTF-8 continuation byte). Two `dev-cluster rebuild mh` attempts both completed in 2.5s with full layer cache hit and produced byte-identical images.

2. **Root cause: build-context mismatch in devloop tooling.** `infra/devloop/devloop.sh:232` launches the helper with `--project-root "$REPO_ROOT"` (host main checkout), while `/work` inside the dev container is bind-mounted from `$CLONE_DIR` (the worktree, sibling of REPO_ROOT). All service rebuilds (`cmd_rebuild`, `cmd_deploy`, `cmd_setup` in `crates/devloop-helper/src/commands.rs`) use `ctx.project_root` as the podman build context — so dev-container edits never reach the build, and `COPY . .` cache-hits at the same hash as yesterday's setup. Diagnosed via `crictl inspecti localhost/mh-service:latest` showing `"created": "2026-04-30T21:04:40"` despite multiple "successful" rebuilds. Filed as a separate operations follow-up; current devloop unblocked by user manually invoking `setup.sh --only mh` from the worktree path on the host (PROJECT_ROOT in setup.sh derives from `SCRIPT_DIR/../../..`, which correctly resolves to the worktree when invoked from there).

3. **Two test nits surfaced in @test review, both accepted at user direction and fixed before commit.** (a) New negative tests used `drop(conn)` immediately after write, inconsistent with sibling tests that hold `_conn` for the processing window — created a theoretical race between bytes-arrival and connection-close; observably benign on loopback but not free. (b) `encode_jwt_frame(jwt: &[u8])` panicked on non-UTF-8 input — type-system mismatch with proto `string` invariant; tightened to `&str` (Option A) along with two upstream callers in the same chain.

---

## Lessons Learned

1. **Devloop builds are silently invisible to source edits.** Until `infra/devloop/devloop.sh` is patched per the follow-up (helper `--project-root` should be `$CLONE_DIR`, not `$REPO_ROOT`; clone must exist before `launch_helper`), any service rebuild from inside the dev container will produce stale images with no warning — `dev-cluster rebuild` reports `exit 0` and pods restart, but they pull byte-identical images. **Diagnostic-of-record**: log-signature comparison against a known-old-vs-known-new string in the binary. **Sanity check**: `crictl inspecti localhost/<svc>:latest` and verify `"created"` timestamp is recent. Workaround until fix lands: from the host, `cd <worktree-path> && DT_CLUSTER_NAME=devloop-<slug> DT_PORT_MAP=/tmp/devloop-<slug>/port-map.env DT_HOST_GATEWAY_IP=<gw> ./infra/kind/scripts/setup.sh --yes --only <svc>`.

2. **L8 scope must be the full env-test suite, not just the targeted file.** The first L8 run was scoped to `--test 26_mh_quic` (6 tests). User correctly flagged "6/6 is suspiciously low" — the full suite is 115 tests across 12 binaries, and several files (`24_join_flow`, `21_cross_service_flows`, `30_observability`) exercise the boundary the change touched. Always run the full env-test suite as the L8 regression sweep; targeted runs are useful for smoke iteration but not for final L8.

3. **The two reviewer nits in test were both real and worth fixing.** Test reviewer marked them deferred-acceptable (tests passed; gaps were theoretical). User asked for explanation of cost/options before deciding. Both were ~5 minutes of mechanical work with measurable removal of future hazard. Lesson: when reviewers flag "deferred but doable," surface cost/option explicitly to user before defaulting to defer — small fixes that remove latent risk are usually worth doing now rather than carrying as tech debt.

4. **`drop(conn)` immediately after a `write_framed` on a QUIC stream introduces a benign-but-real race.** The sender's flush vs. CONNECTION_CLOSE delivery order on loopback usually favors the data, but the test no longer cleanly identifies which failure mode (decode vs. read-disconnect) was exercised. Pattern across this test file: hold `_conn` alive through the processing window, let it drop at end-of-function. New tests should follow this convention.
