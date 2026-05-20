# Devloop Output: Proto STANDARD-lint rename sweep (R-61 part 3, task #31)

**Date**: 2026-05-20
**Task**: Wire-breaking rename sweep — `dark_tower.{internal,signaling}` packages → `…v1` (with `v1/` subdirectory layout per `docs/protocol/CONVENTIONS.md`); 13 RPC request/response type renames per Clarification Q14; split `HeartbeatResponse` into `Fast`+`Comprehensive` variants per Q15; coordinated update across every Rust consumer of `proto-gen`. Drains all 17 residual `buf lint` STANDARD findings; closes the Track 2 exclusive-sequencing window opened by #29.
**Specialist**: protocol (paired with auth-controller per ADR-0024 §6.4 intersection rule)
**Mode**: Agent Teams (v2) — full, 12 teammates
**Branch**: `feature/browser-client-join-task30`
**Duration**: TBD (estimated 2–3 days)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5535b8ddcdcd597d6b2d66f355920e02181e4246` |
| Branch | `feature/browser-client-join-task30` |
| User Story | `docs/user-stories/2026-05-02-browser-client-join.md` (task #31) |
| Requirement | R-61 part 3 (STANDARD-lint rename sweep) |
| ADR-0024 §6.4 Intersection Rule | YES — `proto/dark_tower/internal/internal.proto` is auth-routing-policy. Tri-cosign required: protocol (impl), auth-controller (paired co-implementer), security (reviewer). |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-task31-proto-rename-sweep` |
| Implementing Specialist | `protocol` |
| Paired Specialist | `auth-controller` (cross-boundary co-implementer for `proto/dark_tower/internal/v1/internal.proto` auth-routing-policy hunks) |
| Iteration | `1` |
| Security | CLEAR |
| Test | RESOLVED-DEFERRED |
| Observability | CLEAR |
| Code Quality | CLEAR |
| DRY | CLEAR |
| Operations | CLEAR |
| Semantic Guard | CLEAR |
| Paired Auth-Controller | CLEAR |
| Global-Controller | CLEAR |
| Meeting-Controller | CLEAR |
| Media-Handler | CLEAR |

---

## Task Overview

### Objective

Drain the 17 residual `buf lint` STANDARD findings opened by task #29 and intentionally kept open through #30's file-layout move. After this devloop, Layer 5 (`buf lint` always-run) passes clean repo-wide and the Track 2 exclusive-sequencing window closes.

Concretely:

1. **Package version suffix** — Rename `dark_tower.internal` → `dark_tower.internal.v1` and `dark_tower.signaling` → `dark_tower.signaling.v1`. Move files into `proto/dark_tower/{internal,signaling}/v1/` per `docs/protocol/CONVENTIONS.md` §1.
2. **RPC request/response naming** — 13 type renames per Clarification Q14 (bare `Foo`/`FooResponse` convention per service):
   - `MediaHandlerService.Register`: `RegisterParticipant` → `RegisterRequest`; `RegisterParticipantResponse` → `RegisterResponse`
   - `MediaHandlerService.RouteMedia`: `RouteMediaCommand` → `RouteMediaRequest`
   - `MediaHandlerService.StreamTelemetry`: `MediaTelemetry` → `StreamTelemetryRequest`; `TelemetryAck` → `StreamTelemetryResponse`
   - `MediaCoordinationService.NotifyParticipantConnected`: `ParticipantMediaConnected` → `NotifyParticipantConnectedRequest`; `ParticipantMediaConnectedResponse` → `NotifyParticipantConnectedResponse`
   - `MediaCoordinationService.NotifyParticipantDisconnected`: `ParticipantMediaDisconnected` → `NotifyParticipantDisconnectedRequest`; `ParticipantMediaDisconnectedResponse` → `NotifyParticipantDisconnectedResponse`
   - `MediaHandlerRegistryService.SendLoadReport`: `MHLoadReportRequest` → `SendLoadReportRequest`; `MHLoadReportResponse` → `SendLoadReportResponse`
3. **Distinct response type per RPC** — Split `HeartbeatResponse` (shared by `FastHeartbeat` + `ComprehensiveHeartbeat`) into `FastHeartbeatResponse` + `ComprehensiveHeartbeatResponse` per Clarification Q15. Both new types have the same shape today; the split satisfies `RPC_REQUEST_RESPONSE_UNIQUE` and gives each RPC its own evolution path.
4. **Workspace-wide consumer update** — every Rust consumer of `proto-gen` rebases against the new type names + new package paths:
   - `crates/{ac,gc,mc,mh}-service`
   - `crates/proto-gen` (build script + module re-exports)
   - `crates/env-tests`
   - `crates/{ac,gc,mc}-test-utils`

### Scope

- **Service(s)**: protocol (lead) + AC + GC + MC + MH (every Rust consumer of `proto-gen`).
- **Schema**: No DB schema changes.
- **Cross-cutting**: Yes — wire-breaking on `internal.proto` (gRPC service-to-service contracts) and `signaling.proto` (client-MC contract). No on-the-wire clients exist outside this codebase per R-60 precedent; the wire break is acceptable.

### Debate Decision

NOT NEEDED proactively — rename map is in the user story (`docs/user-stories/2026-05-02-browser-client-join.md` §protocol "Rename map for P-5"). Per task spec: `/debate` is an optional pre-step only if Gate 1 surfaces sustained reviewer disagreement on the map.

### Wire-Breaking Justification

This devloop intentionally breaks wire compatibility. Same precedent as R-60 (`MediaConnectionFailed` → `MediaConnectionUpdate`) earlier in this story: no on-the-wire clients exist outside this codebase, so `buf breaking` will fire at Layer 6 and that's expected. Generated Rust + TS symbol sets will change shape (different type names, new module paths) — every consumer recompiles against the new names in this devloop.

---

## Cross-Boundary Classification

Per task #31 spec, the implementer is the **protocol** specialist. ADR-0024 §6.4 intersection rule fires on `proto/dark_tower/internal/v1/internal.proto` (auth-routing-policy: `ServiceType` enum, scope enums, identity fields). Tri-cosign required: protocol + auth-controller + security.

`--paired-with=auth-controller` covers the intersection rule for the proto edits themselves. AC consumer-crate touches (`crates/ac-service/`) are owned by auth-controller (paired co-implementer); GC/MC/MH consumer-crate touches are Mechanical-cross-boundary (workspace-wide rebase per ADR-0024 §6.3 review-only).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/internal/v1/internal.proto` | Mine | — |
| `proto/dark_tower/signaling/v1/signaling.proto` | Mine | — |
| `crates/proto-gen/build.rs` | Mine | — |
| `crates/proto-gen/src/lib.rs` | Mine | — |
| `packages/proto-gen/scripts/verify-codegen.sh` | Mine | — |
| `crates/ac-service/**` (consumer rebase) | Review-only — actual AC proto-consumer surface is `crates/ac-service/fuzz/fuzz_targets/signaling_messages.rs` (imports `proto_gen::signaling::ClientMessage` + `proto_gen::Message`). Both stay valid under Approach A: `ClientMessage` is not in the rename map (signaling envelope, not an RPC request/response), and the `proto_gen::signaling::` module path is stable. Zero `.rs` edits expected. Paired-auth-controller scope check 2026-05-20. | auth-controller |
| `crates/gc-service/**` (consumer rebase) | Mechanical (workspace rebase, name-swap only) | global-controller |
| `crates/mc-service/**` (consumer rebase) | Mechanical (workspace rebase, name-swap only) | meeting-controller |
| `crates/mh-service/**` (consumer rebase) | Mechanical (workspace rebase, name-swap only) | media-handler |
| `crates/env-tests/**` (consumer rebase) | Mechanical | — |
| `crates/ac-test-utils/**` (consumer rebase) | Review-only — no `proto-gen`/`tonic`/`prost` deps; zero renamed-type usage. Zero `.rs` edits expected. Paired-auth-controller scope check 2026-05-20. | auth-controller |
| `crates/gc-test-utils/**` (consumer rebase) | Mechanical | global-controller |
| `crates/mc-test-utils/**` (consumer rebase) | Mechanical | meeting-controller |
| `scripts/guards/simple/cross-boundary-ownership.yaml` (intersection-rule key rename to new v1 path) | Mechanical | infrastructure |
| `scripts/guards/simple/validate-gsa-sync.sh` (INTERSECTION_SUBPATHS + comment) | Mechanical | infrastructure |
| `docs/specialist-knowledge/{client,media-handler,meeting-controller,protocol}/INDEX.md` | Mine | — |

**Implementer note**: this table is the planned-edit set. The implementer may refine during planning, but cannot drop the intersection-rule rows (which are forced by ADR-0024 §6.4). The classification-sanity guard will validate format and GSA-owner rules before Plan approved is issued.

### Per-row context

- **`proto/dark_tower/internal/v1/internal.proto`** — protocol-owned. ADR-0024 §6.4 intersection rule fires (auth-routing-policy). Tri-cosign required: protocol (impl), auth-controller (paired co-implementer), security (reviewer). Commit MUST include `Approved-Cross-Boundary: auth-controller …` + `Approved-Cross-Boundary: security …` trailers per Gate 3 — Lead enforces via the trailer requirement at commit time + paired-auth-controller's joint-implementation cadence + security's plan confirmation. The table row uses bare `Mine`/`—` for classification-sanity parser compatibility (same pattern as #30); the intersection-rule semantics live here in prose.
- **`docs/specialist-knowledge/{client,media-handler,meeting-controller,protocol}/INDEX.md`** — protocol-owned per #30 precedent: INDEX.md edits inside a specialist's own knowledge dir are theirs, and the `docs/user-stories/*.md` carve-out from commit `8abe4b6` plus the broader `docs/` precedent auto-exempt these from `cross-boundary-scope` enforcement. Bare `Mine` in the table for parser compatibility.
- **`crates/ac-service/**`** and **`crates/ac-test-utils/**`** — paired-auth-controller scope-checked 2026-05-20 and confirmed zero `.rs` edits expected under Approach A. The only AC proto-consumer surface is `crates/ac-service/fuzz/fuzz_targets/signaling_messages.rs` (uses `proto_gen::signaling::ClientMessage` + `proto_gen::Message`); both stay valid since `ClientMessage` isn't in the rename map and the `proto_gen::signaling::` re-export path is stable. The non-bare `Review-only — …` Classification on those two rows is intentional — it documents the consumer-surface audit trail for downstream reviewers. Safety-net commitment: if `cargo check --workspace` surfaces a missed transitive AC dep, I stop and notify paired-auth-controller before any AC edit.

---

## Planning

### Approach (single-commit, atomic wire-break)

The wire-break is one logical atom: proto packages, message names, and every Rust consumer flip together. Splitting into sub-commits would leave intermediate states where `cargo check --workspace` fails (consumers compiled against names that no longer exist), so the plan is one commit, validated end-to-end, with `Approved-Cross-Boundary:` trailers for the intersection-rule hunks.

Order of operations:

1. **Proto file moves** (mechanical, `git mv` to preserve history):
   - `proto/dark_tower/internal/internal.proto` → `proto/dark_tower/internal/v1/internal.proto`
   - `proto/dark_tower/signaling/signaling.proto` → `proto/dark_tower/signaling/v1/signaling.proto`

2. **Package bumps** (in-file, single-line edits):
   - `package dark_tower.internal;` → `package dark_tower.internal.v1;`
   - `package dark_tower.signaling;` → `package dark_tower.signaling.v1;`
   - Update the internal.proto `import "dark_tower/signaling/signaling.proto";` → `import "dark_tower/signaling/v1/signaling.proto";`

3. **13 RPC request/response type renames** in `internal.proto v1` (per Q14 bare-name convention) — full map already cited in §Task Overview above. Each rename is a `replace_all` on the message-definition site AND any RPC signature that references it.

4. **HeartbeatResponse split** (Q15):
   - Delete the single `HeartbeatResponse` message.
   - Add `FastHeartbeatResponse` and `ComprehensiveHeartbeatResponse` with the same field shape (`bool acknowledged = 1; uint64 timestamp = 2;`).
   - Update `GlobalControllerService.FastHeartbeat` returns `FastHeartbeatResponse`; `ComprehensiveHeartbeat` returns `ComprehensiveHeartbeatResponse`.

5. **proto-gen build pipeline** (Rust + TS):
   - `crates/proto-gen/build.rs`: update both `compile_protos` paths and both `rerun-if-changed` lines to the `v1/` files.
   - `crates/proto-gen/src/lib.rs`: update the two `include!()` paths to `generated/dark_tower.signaling.v1.rs` and `generated/dark_tower.internal.v1.rs`.
   - The `pub mod signaling { ... }` and `pub mod internal { ... }` re-export module names stay the same — consumers continue to write `proto_gen::internal::FooRequest`. (Renaming the re-export modules to `signaling_v1` / `internal_v1` would multiply the consumer-rebase blast radius for no STANDARD-lint benefit; the `.v1` lives in the wire/package layer, not the Rust API surface.)
   - `packages/proto-gen/scripts/verify-codegen.sh` (TS codegen smoke test, parallel to the Rust build.rs path updates — flagged by @test in Gate 1, 2026-05-20). Four assert-level edits:
     - Update assert `dark_tower/signaling/signaling_pb.ts` → `dark_tower/signaling/v1/signaling_pb.ts` (`protoc-gen-es` mirrors the proto `package` path into the output tree per `proto/buf.gen.yaml`, so the `v1` suffix lands in the generated TS layout).
     - Update assert `dark_tower/internal/internal_pb.ts` → `dark_tower/internal/v1/internal_pb.ts`.
     - Update the symbol assertion on the internal file from `RegisterParticipant` → `RegisterRequest` (Q14 bare-name rename — the symbol asserted is the message-class name `protoc-gen-es` emits, which tracks the proto `message` declaration).
     - **Add** two new asserts so the Q15 `HeartbeatResponse` split has regression detection in the codegen smoke test: `assert_generated "dark_tower/internal/v1/internal_pb.ts" "FastHeartbeatResponse"` and `assert_generated "dark_tower/internal/v1/internal_pb.ts" "ComprehensiveHeartbeatResponse"`. (One emitted `class` per proto message; substring `grep` is robust to incidental formatting.)

6. **Consumer-crate rebase** (8 crates touched, ~32 `.rs` files):
   - **gRPC wire-path string constants** in auth interceptors/layers (auth-controller cross-boundary territory): `crates/{mc,mh}-service/src/grpc/auth_interceptor.rs`, `crates/gc-service/src/grpc/auth_layer.rs`, and the matching test files. Every `/dark_tower.internal.SomeService/Method` literal becomes `/dark_tower.internal.v1.SomeService/Method`. **These are the AC-paired intersection hunks** — auth-routing-policy strings drive interceptor dispatch.
   - **Generated-type imports**: `proto_gen::internal::RegisterParticipant` etc. become the new names. Rust-side type rename, mechanical.
   - **HeartbeatResponse callsites**: `gc-service` server impls + `mc-service` client (or test-utils) need to construct the now-split `FastHeartbeatResponse` / `ComprehensiveHeartbeatResponse` per RPC.

7. **GSA-sync + intersection-rule key updates** (mechanical, infra-owned but bundled here per #30 precedent):
   - `scripts/guards/simple/cross-boundary-ownership.yaml`: `proto/dark_tower/internal/internal.proto` key → `proto/dark_tower/internal/v1/internal.proto`. Same `[protocol, auth-controller, security]` value.
   - `scripts/guards/simple/validate-gsa-sync.sh`: update `INTERSECTION_SUBPATHS` array element + the two comment refs (lines 26, 58).

8. **Specialist-knowledge INDEX.md path sweep** — 4 files (`docs/specialist-knowledge/{client,media-handler,meeting-controller,protocol}/INDEX.md`): bare path strings get `/v1/` inserted. Per #30 precedent these are exempt from cross-boundary-scope (the `docs/user-stories/*.md` carve-out from commit `8abe4b6` covers `docs/` more broadly; INDEX edits inside a specialist's own dir are theirs).

### Intersection-rule scope (ADR-0024 §6.4)

The intersection rule fires on `proto/dark_tower/internal/v1/internal.proto` because it contains auth-routing policy (`ServiceType` enum membership via the service blocks, identity/scope fields). For task #31 specifically the intersection-rule hunks are:

- Every service-block RPC signature change (`rpc Foo(NewName) returns (NewName2)`) — these are the wire-policy contracts the auth-interceptors dispatch on.
- The package-decl bump (`dark_tower.internal` → `dark_tower.internal.v1`) — the wire-policy namespace is part of auth-routing identity.
- The split of `HeartbeatResponse` into Fast/Comprehensive — each becomes its own wire-method response identity.

**Joint implementation (@paired-auth-controller co-implementer)**:
- `proto/dark_tower/internal/v1/internal.proto` (the whole file is intersection territory by the rule, even the parts that aren't auth-routing fields per se; the cosign covers the file).
- `crates/mc-service/src/grpc/auth_interceptor.rs` wire-path constants (these are auth-routing-policy in code form).
- `crates/mh-service/src/grpc/auth_interceptor.rs` wire-path constants.
- `crates/gc-service/src/grpc/auth_layer.rs` wire-path constants.
- `crates/mc-service/tests/auth_layer_integration.rs` wire-path constants (test of the intersection-policy code).

  **Three-layer ownership** on the auth-interceptor wire-path-string hunks (per paired-auth-controller's GSA reading): the file owner (mc / mh / gc specialist) is the GSA owner of the file; the protocol implementer (me) makes the edits because they're part of the proto-package-rename atomic commit; @paired-auth-controller co-implements/cosigns on the wire-path-string content because it's auth-routing policy in code form. The file-owner specialist is reviewer-only per ADR-0024 §6.3 (Mechanical-cross-boundary).

**AC-led (paired auth-controller leads)**:
- Empty in practice. Paired-auth-controller scope check (2026-05-20) confirmed `crates/ac-service/**` and `crates/ac-test-utils/**` have zero `.rs` edits under Approach A. The only AC-side proto consumer is `crates/ac-service/fuzz/fuzz_targets/signaling_messages.rs` which uses `proto_gen::signaling::ClientMessage` — `ClientMessage` is not in the rename map and the Rust re-export module path is stable. Safety-net: if `cargo check --workspace` surfaces a missed transitive AC dep, I stop and notify paired-AC before any edit.

**Mechanical (protocol-led, review-only by service owners per ADR-0024 §6.3)**:
- `crates/{gc,mc,mh}-service/**` non-auth-interceptor consumer rebase (generated-type-name swaps).
- `crates/env-tests/**`, `crates/{gc,mc}-test-utils/**`.
- `crates/proto-gen/{build.rs, src/lib.rs}`.

**Reviewer-only**:
- @security on the intersection-rule hunks (proto + auth-interceptors). `Approved-Cross-Boundary: security <...>` trailer required on the final commit.
- @global-controller, @meeting-controller, @media-handler on their respective service-crate hunks (Mechanical workspace rebase per ADR-0024 §6.3 — review-only, no co-implementation).

### Commit trailer plan

Single commit, trailers:
```
Approved-Cross-Boundary: auth-controller (intersection: proto/dark_tower/internal/v1/internal.proto + auth-interceptor wire-path constants)
Approved-Cross-Boundary: security (intersection: proto/dark_tower/internal/v1/internal.proto auth-routing-policy review)
```

(Plus optional `Co-Authored-By:` for auth-controller per the paired-implementation pattern.)

### Risk callouts for reviewers

1. **`HeartbeatResponse` split blast radius**: any code that returned `HeartbeatResponse` from a heartbeat handler now constructs the right variant per RPC. The split is cheap (same shape) but the callsite count needs an audit; the failure mode is loud (Rust type error, not silent wire-break).
2. **gRPC wire-path strings are tested string-equal**: the auth-interceptor tests assert exact path-prefix matches. Anything I miss in the rebase shows up at Layer 4 (`cargo test`), not at runtime — that's the safety net.
3. **`buf breaking` will fire as the spec** at Layer 6. This is expected (R-60 precedent). The commit message + main.md call out the expected fire so the validation review doesn't mistake it for a regression.
4. **No `lint.ignore` carve-outs**: Revision 8 of the user story forbids them. If a STANDARD finding surfaces I didn't anticipate, the fix is in the `.proto` source, never in `buf.yaml`. (Same principle as ADR-0034.)


---

## Expected Layer-State

- **Layer 5** (`buf lint`): expected to go from 17 residual findings → **0** after this devloop. If non-zero at close, devloop cannot close. NO `lint.ignore` workarounds (Revision 8 forbids).
- **Layer 6** (`buf breaking`): expected to **FIRE** — this is the wire-breaking change. Accepted per R-60 precedent (no on-the-wire clients exist outside this codebase). Implementer + Lead acknowledge the expected fire at Gate 2; the failure shape is the spec, not a bug.
- **Layers 1-4**: should pass once consumer crates rebase against new type names. `cargo check --workspace` + `cargo test --workspace` are the load-bearing oracles.
- **Layer 3 (`no-dev-trust-path-in-prod-bundle`)**: pre-existing failure persists (R-14 enforcement gap, in TODO.md, unrelated).
- **Layer 6 (`cargo audit` RUSTSEC-2023-0071)**: pre-existing rsa Marvin Attack via sqlx-mysql, no upstream fix. Unrelated.

Acceptance: Layers 1, 2, 4, 5 (post-sweep) all PASS. Layers 3, 6 fail only on documented pre-existing items + Layer 6 `buf breaking` (expected wire-break).

---

## Pre-Work

Task #30 (proto file-layout cleanup) landed at commit `5535b8d` — proto files at `proto/dark_tower/{internal,signaling}/` and the consumer pipeline knows how to find them. No pre-work needed beyond branch state.

---

## Implementation Summary

Atomic single-commit wire-break per locked plan. All 11 steps landed:

1. `git mv` proto files into `proto/dark_tower/{internal,signaling}/v1/`.
2. Package decl bump `dark_tower.{internal,signaling}` → `…v1`.
3. Internal proto: `import "dark_tower/signaling/v1/signaling.proto";` + FQN bump `dark_tower.signaling.v1.MediaStream` on the cross-package reference at line 15.
4. 13 RPC request/response type renames per Q14 bare-name convention.
5. `HeartbeatResponse` split into `FastHeartbeatResponse` + `ComprehensiveHeartbeatResponse` per Q15 (same shape).
6. proto-gen build pipeline (Rust + TS) updated:
   - `crates/proto-gen/build.rs` — proto paths + rerun lines + new `extern_path` mapping `.dark_tower.signaling.v1` → `crate::signaling` so cross-package type refs in generated code use the flat Approach-A Rust API surface (single-tonic-build call still emits `internal.v1.rs` with bare same-package names — applying `extern_path` to BOTH packages would have suppressed in-package type emission and was the only mid-implementation deviation, documented under §Issues Encountered).
   - `crates/proto-gen/src/lib.rs` — both `include!()` paths bumped to `.v1.rs`. Re-export modules `proto_gen::signaling` / `proto_gen::internal` kept stable per Approach A.
   - `packages/proto-gen/scripts/verify-codegen.sh` — 4 assert-level edits: 2 path renames, `RegisterParticipant` → `RegisterRequest`, +2 new asserts for `FastHeartbeatResponse` + `ComprehensiveHeartbeatResponse` (Q15 split regression coverage per @test Gate-1 ask).
7. Rust consumer rebase across 8 crates / 11 `.rs` files (under workspace-rebase Mechanical classification, name-swap only):
   - `crates/{gc,mc,mh}-service/src/grpc/**.rs`
   - `crates/{mc,mh}-service/tests/*.rs`
   - `HeartbeatResponse` callsites split per-RPC (Fast vs Comprehensive variant) in `gc-service/src/grpc/mc_service.rs` + `mc-service/tests/gc_integration.rs`.
8. Joint w/ @paired-auth-controller: 16 wire-path-string literals across 4 files swapped from `/dark_tower.internal.` → `/dark_tower.internal.v1.` (single mechanical sweep; 14+15+19 = 48 auth-interceptor/auth-layer unit tests green post-sweep).
9. GSA-sync mirrors (`scripts/guards/simple/cross-boundary-ownership.yaml` + `validate-gsa-sync.sh`) bumped to v1 path on the intersection-rule key + accompanying comments.
10. Specialist-knowledge INDEX.md path sweep across 4 files (`docs/specialist-knowledge/{client,media-handler,meeting-controller,protocol}/INDEX.md`).
11. Atomic commit with `Approved-Cross-Boundary:` trailers (paired-AC + security) — see commit message for the locked trailer text.

---

## Files Modified

```
M crates/gc-service/src/grpc/auth_layer.rs              (intersection: wire-path strings; joint w/ paired-AC)
M crates/gc-service/src/grpc/mc_service.rs              (HeartbeatResponse split callsites)
M crates/gc-service/src/grpc/mh_service.rs              (MhLoadReport* → SendLoadReport*)
M crates/mc-service/src/grpc/auth_interceptor.rs        (intersection: wire-path strings; joint w/ paired-AC)
M crates/mc-service/src/grpc/media_coordination.rs      (ParticipantMedia* → NotifyParticipant*)
M crates/mc-service/tests/auth_layer_integration.rs     (intersection: wire-path strings; joint w/ paired-AC)
M crates/mc-service/tests/gc_integration.rs             (HeartbeatResponse split mock)
M crates/mc-service/tests/media_coordination_integration.rs  (rename map)
M crates/mc-service/tests/register_meeting_integration.rs    (rename map)
M crates/mh-service/src/grpc/auth_interceptor.rs        (intersection: wire-path strings; joint w/ paired-AC)
M crates/mh-service/src/grpc/gc_client.rs               (MhLoadReport* / heartbeat split client-side)
M crates/mh-service/src/grpc/mc_client.rs               (rename map)
M crates/mh-service/src/grpc/mh_service.rs              (rename map)
M crates/mh-service/tests/common/mock_mc.rs             (rename map)
M crates/mh-service/tests/gc_integration.rs             (rename map)
M crates/proto-gen/build.rs                             (v1 paths + extern_path mapping)
M crates/proto-gen/src/lib.rs                           (include!() v1 paths)
M docs/specialist-knowledge/client/INDEX.md             (v1 path sweep)
M docs/specialist-knowledge/media-handler/INDEX.md      (v1 path sweep)
M docs/specialist-knowledge/meeting-controller/INDEX.md (v1 path sweep)
M docs/specialist-knowledge/protocol/INDEX.md           (v1 path sweep)
M packages/proto-gen/scripts/verify-codegen.sh          (4 asserts + Q15 split regression coverage)
RM proto/dark_tower/internal/internal.proto -> proto/dark_tower/internal/v1/internal.proto    (intersection: tri-cosign)
RM proto/dark_tower/signaling/signaling.proto -> proto/dark_tower/signaling/v1/signaling.proto
M scripts/guards/simple/cross-boundary-ownership.yaml   (intersection-rule key → v1 path)
M scripts/guards/simple/validate-gsa-sync.sh            (INTERSECTION_SUBPATHS + comments → v1 path)
```

### Key Changes by File

- **`proto/dark_tower/internal/v1/internal.proto`** (intersection-rule hunks, tri-cosign required): package bump to `dark_tower.internal.v1`; import path + FQN bump for the cross-package `MediaStream` reference; 13 RPC type renames per Q14; `HeartbeatResponse` split into Fast/Comprehensive per Q15; RPC return types updated on `GlobalControllerService`. No semantic edits beyond rename map — auth-routing identity / scope semantics / service-token contracts unchanged.
- **`crates/{mc,mh}-service/src/grpc/auth_interceptor.rs` + `crates/gc-service/src/grpc/auth_layer.rs` + `crates/mc-service/tests/auth_layer_integration.rs`** (intersection-rule hunks, joint w/ paired-AC): 16 wire-path-string literals swapped to v1 path. No other text touched.
- **`crates/proto-gen/build.rs`**: added `.extern_path(".dark_tower.signaling.v1", "crate::signaling")` to remap the only cross-package reference (internal→signaling MediaStream) so the generated Rust code uses Approach-A's flat `crate::signaling::MediaStream` instead of the nested `super::super::signaling::v1::MediaStream` that the proto-package-path mirror would have produced. The internal package is NOT in `extern_path` because that would suppress type-definition emission for in-package types.

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS — `cargo check --workspace` clean.

### Layer 2: cargo fmt
**Status**: PASS — `cargo fmt --all` exit 0.

### Layer 3: Simple Guards
**Status**: FAIL (pre-existing only). The single failing guard is `no-dev-trust-path-in-prod-bundle` (R-14 enforcement gap, documented in `docs/TODO.md`, unrelated to #31). All other simple guards pass.

### Layer 4: Unit Tests
**Status**: PASS — `cargo test --workspace` shows every `test result: ok. … 0 failed` across the full suite. Spot-checks confirm: 14/14 in mh-service auth_interceptor, 15/15 in mc-service auth_interceptor, 19/19 in gc-service auth_layer (the load-bearing wire-path-string-prefix tests).

### Layer 5: All Tests (Integration) — including `buf lint`
**Status**: PASS — `cd proto && buf lint` returns 0 findings (drained all 17 STANDARD violations per spec). `pnpm exec nx run proto-gen:test` (verify-codegen.sh) passes all 4 asserts including the 2 new Q15-split coverage asserts (`FastHeartbeatResponse`, `ComprehensiveHeartbeatResponse`). `pnpm exec nx run proto-gen:format` passes. `pnpm exec nx run proto-gen:lint` (buf lint) passes.

### Layer 6: Audit (cargo audit / pnpm audit / buf breaking)
**Status**: FAIL (pre-existing only). `cargo audit` flags RUSTSEC-2023-0071 (rsa 0.9.10 via sqlx-mysql, Marvin Attack timing sidechannel, no upstream fix) — pre-existing per plan. `pnpm audit` clean. **`buf breaking` reported OK** (`STATUS=OK REASON=buf-breaking-passed`); the layer-all harness treats the proto-file move as new-file additions rather than wire-breakage on the old paths, so the expected fire didn't surface at the layer-all level. The wire break IS real (different package, different message names) — captured in the commit message + this main.md for downstream-system awareness, but the automated breaking check doesn't flag it. Layer 6 net FAIL is the pre-existing cargo-audit RUSTSEC item only.

### Layer 7: Env-tests
**Status**: N/A — Wave 2 pending, per plan.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Intersection-rule cosign trailer issued; per-hunk Ownership Lens recorded; identity fields / scope semantics / fail-closed dispatch / (service,service_type) mapping all preserved verbatim. |
| Test | RESOLVED-DEFERRED | 1 | 0 | 1 | All Gate 1 asks (verify-codegen.sh updates incl. Q15-split asserts) already incorporated by Gate 2. Single accepted deferral: `buf breaking` oracle gap on package-rename-via-file-move (needs separate devloop to either configure baseline or add move-aware check). |
| Observability | CLEAR | 0 | 0 | 0 | Zero observability surface touched. Trace fields (tags 20/21) preserved at v1/ paths; `MediaConnectionUpdate` preserved; metric label values unchanged (only RPC-method-name strings, not request/response message types). |
| Code Quality | CLEAR | 0 | 0 | 0 | ADR Compliance + Ownership Lens both PASS. Three observations flagged for post-#31 follow-up (three-layer ownership memorialization; INDEX.md exemption wording nit; `extern_path` lesson into CONVENTIONS.md). None blocking. |
| DRY | CLEAR | 0 | 0 | 0 | No new logic; no true duplication; no extraction opportunities. |
| Operations | CLEAR | 0 | 0 | 0 | No K8s/infra/runbook/CI touches; rollback plan valid; Layer 5 = 0 closes Track 2 exclusive-sequencing window opened by #29. |
| Semantic Guard | CLEAR | 0 | 0 | 0 | All four checks (credential-leak / actor-blocking / error-context / metrics-path) verified clean across 26-file diff. |
| Paired Auth-Controller | CLEAR (PASS/ACK) | 0 | 0 | 0 | Intersection-rule cosign trailer issued (long form). All 14 proto-diff edits + 16 wire-path-string hunks Mechanical sed-clean; identity fields, scope mapping, fail-closed dispatch preserved. AC consumer surface confirmed empty (zero `.rs` edits). |
| Global-Controller | CLEAR (APPROVED) | 0 | 0 | 0 | GC consumer rebase clean; HeartbeatResponse split semantics preserved; SendLoadReport rename shape preserved; Approach A invariant upheld (no `::v1::` in Rust paths). |
| Meeting-Controller | CLEAR (APPROVED) | 0 | 0 | 0 | MC consumer rebase clean; semantic-drift watchpoints (heartbeat MH-list semantics, MediaCoordinationService actor-binding, mock shimming) all clear. Tonic trait-level type-separation now structurally prevents future cross-wire-drift. |
| Media-Handler | CLEAR (APPROVED) | 0 | 0 | 0 | MH consumer rebase clean across 7 RPCs; `crates/media-protocol/**` GSA confirmed untouched (rename is gRPC-only); streaming-direction on StreamTelemetry preserved; field-tag preservation verified per-message. |

### Iteration history

One review iteration. 10/11 verdicts CLEAR; @test = RESOLVED-DEFERRED with single accepted deferral (buf-breaking oracle gap on file-move). Intersection-rule cosign trailers from @paired-auth-controller + @security both staged for the atomic commit.

---

## Accepted Deferrals

- `docs/TODO.md` §Test Coverage Debt — `buf breaking` oracle gap on package-rename-via-file-move (test F1)

---

## Rollback Procedure

1. Verify start commit: `5535b8ddcdcd597d6b2d66f355920e02181e4246`
2. Review changes: `git diff 5535b8d..HEAD`
3. Hard reset (clean revert): `git reset --hard 5535b8d`
4. No migrations, no infra state — `git reset` alone is sufficient.
5. **Wire-break recovery note**: if this devloop is rolled back AFTER any consumer has rebased independently, those rebase commits must also be reverted. Coordinate with the Lead before partial rollback.

---

## Issues Encountered & Resolutions

1. **`tonic-build` cross-package reference shape under Approach A** — First attempt at `crates/proto-gen/build.rs` had no `extern_path` mapping. Generated `dark_tower.internal.v1.rs` referenced `super::super::signaling::v1::MediaStream` (the proto-package-path mirror) — but Approach A's `pub mod signaling { include!(...) }` re-export is a flat module, not nested under `v1/`, so `super::super::signaling::v1` is "too many leading `super` keywords" (E0433). Second attempt added `extern_path` for both packages, which suppressed type-definition emission for in-package types in `internal.v1` (got "no `NotifyParticipantDisconnectedRequest` in `crate::internal`" — the types weren't generated because `extern_path` told tonic-build they were externally defined). **Resolution**: `extern_path` only the OTHER package (`.dark_tower.signaling.v1` → `crate::signaling`). The single cross-package reference (internal.proto's `import` of signaling MediaStream) is the only thing that needs remapping; in-package refs stay as bare names. Documented in `build.rs` comment for future v2 bumps.

## Lessons Learned

1. **Approach A + tonic-build `extern_path`** is the right combination but requires the *asymmetric* mapping above: cross-package refs need `extern_path`, in-package refs must not (otherwise the type defs themselves get suppressed). When the future v2 bump comes, this pattern carries directly: add a `crate::signaling_v2` sibling re-export, `extern_path .dark_tower.signaling.v2 → crate::signaling_v2`, and the same asymmetry rules apply.

2. **"Three-layer ownership" reading on the auth-interceptor wire-path-string hunks** (per @code-reviewer Gate-3 note relayed by team-lead): file-owner = GSA owner of the source file (mc/mh/gc); implementer = makes the edits because they're part of the proto-package-rename atomic commit; cross-boundary cosigner = AC because the literals are auth-routing policy in code form. This reading worked cleanly here and is worth promoting to ADR-0024 §6.4 as a follow-up micro-debate — the existing §6.4 doesn't explicitly enumerate the three-layer split, and codifying it would save future paired implementations the round-trip to derive it.

3. **`buf breaking` did not fire at layer-all** despite the wire-break being real (different package, different message names). The harness reads the proto-file move as new-file additions on the v1 paths rather than diffing old-path content. Worth a follow-up to confirm the layer-all harness's `buf breaking` reference shape — if it's not comparing against the pre-move state, the "expected fire" semantics in plans for future wire-breaks would benefit from a sharper precondition. Not blocking #31; flagging for future story scoping.
