# Devloop Output: proto-gen architectural cleanup — idiomatic OUT_DIR + nested modules

**Date**: 2026-05-20
**Task**: Replace flat-re-export-with-extern_path proto-gen pattern with the idiomatic Rust+protobuf pattern (single-pass OUT_DIR + nested modules mirroring proto package hierarchy). Closes the stale-file bug class surfaced post-#30/#31 absorption.
**Specialist**: protocol
**Mode**: Agent Teams (v2)
**Branch**: `feature/validation-check`
**Duration**: ~25m (setup → review verdicts complete)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `80b0aba275639e17eee5f1e9556ef7b8db5acf46` |
| Branch | `feature/validation-check` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@proto-gen-arch-cleanup` |
| Implementing Specialist | `protocol` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `RESOLVED-FIXED` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `CLEAR` |
| DRY | `RESOLVED-FIXED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |

---

## Task Overview

### Objective
Replace the unusual `extern_path` + flat-re-export pattern in `crates/proto-gen/` with the idiomatic Rust+protobuf pattern. After this devloop:

- `build.rs` is a single-pass `tonic_build::compile_protos` (~10 lines) — no `extern_path`, no `.out_dir(...)`.
- Generated code lives in `OUT_DIR` (no `src/generated/` tree, no `.gitignore` plumbing).
- `lib.rs` exposes nested modules mirroring the proto package hierarchy: `proto_gen::dark_tower::signaling::v1::*` and `proto_gen::dark_tower::internal::v1::*`. **No flat re-export aliases.** Every consumer says `::v1::` explicitly so schema-version visibility is truthful at every import.

### Root Bug Being Closed
#31's `build.rs` introduced `extern_path(.dark_tower.signaling.v1, "crate::signaling")` which suppresses `signaling.v1.rs` generation (prost-build semantic). The bug was masked in #31's worktree because stale pre-#30 generated files under `src/generated/` were being `include!`'d — Gate 2 looked clean. Fresh-container validation post-absorb caught it: every consumer of `proto_gen::signaling::*` fails to compile.

The idiomatic OUT_DIR pattern makes the stale-file bug class **structurally impossible** — generated artifacts only ever live under `target/.../build/.../out/` and are regenerated each build.

### Scope
- **Service(s)**: `crates/proto-gen/` (owner-implementing); consumer `use`/path renames across services that depend on proto-gen.
- **Schema**: No — proto files unedited. Wire format unchanged. `buf breaking` passes clean.
- **Cross-cutting**: Yes — consumer crates need mechanical `use`-statement rewrites.

### Debate Decision
NOT NEEDED — task is wire-format-neutral and the architectural choice (idiomatic OUT_DIR + explicit `::v1::` at every import) is settled in the devloop prompt. The anti-pattern check at Gate 2 explicitly rejects flat re-export aliases.

---

## Cross-Boundary Classification

<!-- proto-gen/** is a Guarded Shared Area (ADR-0024 §6.4 — wire format).
     proto-gen is the protocol specialist's domain → Mine.
     Consumer crates (mc/mh/gc/ac/env-tests) are outside proto-gen GSA, and the
     edits are pure use-path rewrites — value-neutral, structure-preserving,
     sed-test clean. Layer 1 (cargo check) catches any partial-rename: if a
     single `use proto_gen::signaling::*` survives after the flat alias is
     removed, cargo check fails to compile the workspace. Guard coverage =
     compiler. Mechanical per §6.2. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/proto-gen/build.rs` | Mine | — |
| `crates/proto-gen/src/lib.rs` | Mine | — |
| `crates/proto-gen/src/generated/.gitkeep` (deletion) | Mine | — |
| `.gitignore` | Mine | — |
| `crates/mc-service/src/actors/participant.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/grpc/gc_client.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/grpc/mc_service.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/grpc/media_coordination.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/grpc/mh_client.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/main.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/webtransport/connection.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/webtransport/handler.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/gc_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/join_tests.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/media_coordination_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/register_meeting_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/webtransport_accept_loop_integration.rs` | Not mine, Mechanical | meeting-controller |
| `crates/mh-service/src/grpc/gc_client.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/src/grpc/mc_client.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/src/grpc/mh_service.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/src/main.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/src/webtransport/connection.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/auth_layer_integration.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/common/grpc_rig.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/common/mock_mc.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/common/wt_client.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/gc_integration.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/register_meeting_integration.rs` | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/webtransport_integration.rs` | Not mine, Mechanical | media-handler |
| `crates/gc-service/src/grpc/mc_service.rs` | Not mine, Mechanical | global-controller |
| `crates/gc-service/src/grpc/mh_service.rs` | Not mine, Mechanical | global-controller |
| `crates/gc-service/src/main.rs` | Not mine, Mechanical | global-controller |
| `crates/gc-service/src/services/mc_client.rs` | Not mine, Mechanical | global-controller |
| `crates/ac-service/fuzz/fuzz_targets/signaling_messages.rs` | Not mine, Mechanical | auth-controller |
| `crates/env-tests/tests/24_join_flow.rs` | Not mine, Mechanical | test |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Mechanical | test |

**Sed-test argument**: Every change is `proto_gen::signaling::X` → `proto_gen::dark_tower::signaling::v1::X` and `proto_gen::internal::X` → `proto_gen::dark_tower::internal::v1::X`. The encoded concept (which type from which proto file) is preserved; the path now exposes the schema version that was always implicit. Partial-rename failure mode is caught by `cargo check` (Layer 1) because the old flat aliases stop existing.

**Note**: `proto-gen/**` IS a GSA per §6.4, but all proto-gen edits are by the GSA owner (protocol) — classification = Mine, no cross-boundary involvement table needed for those rows. Implementer's grep at planning time confirmed `crates/common/` and `crates/media-protocol/` have zero `proto_gen::*` references — no rows needed for those crates.

---

## Planning

1. Rewrite `crates/proto-gen/build.rs` — drop `.out_dir("src/generated")` and `.extern_path(".dark_tower.signaling.v1", "crate::signaling")`; single-pass `tonic_build::configure().compile_protos(&[signaling, internal], &["../../proto/"])`; keep both `cargo:rerun-if-changed` lines verbatim.
2. Rewrite `crates/proto-gen/src/lib.rs` — nested `dark_tower::{signaling,internal}::v1` modules via `include!(concat!(env!("OUT_DIR"), "/dark_tower.<pkg>.v1.rs"))`. Keep `pub use prost::Message;` + `pub use tonic;`. **NO** flat re-export aliases.
3. Delete `crates/proto-gen/src/generated/` entirely.
4. Remove the `crates/proto-gen/src/generated/*.rs` entry from `.gitignore`.
5. Workspace-wide mechanical rewrite, two substitutions only:
   - `proto_gen::signaling::` → `proto_gen::dark_tower::signaling::v1::`
   - `proto_gen::internal::` → `proto_gen::dark_tower::internal::v1::`
6. Run `cargo check --workspace --all-targets` (Layer 1 partial-rename guard).
7. Run `cargo fmt --all` (Layer 2 catches line wrap due to longer paths).
8. Run `scripts/layer-all.sh`. Accept the cross-boundary-scope guard outcome only if it points at the consumer-touch we explicitly listed in §Cross-Boundary Classification; Layer 6 RUSTSEC-2023-0071 + buf-breaking-deletion-from-#30 are pre-existing and out of scope.

Plan broadcast to security, test, observability, code-reviewer, dry-reviewer, operations, semantic-guard. All reviewers acked `plan-confirmed`. Team-lead phase set to `implementation` in main.md — implementation proceeded.

---

## Pre-Work

None.

---

## Implementation Summary

The cleanup closes the stale-file bug class by moving proto-generated code from in-tree `src/generated/` (under `extern_path` remapping) to `OUT_DIR` (regenerated each `cargo build`), and restructures `lib.rs` to mirror the proto package hierarchy.

- `build.rs` shrinks from ~37 to ~26 lines (including the comment header). The single `tonic_build::configure().compile_protos(...)` call covers both `.proto` files. `extern_path` is gone — cross-package references (`internal.proto` referencing `signaling.MediaStream`) resolve naturally to `super::super::signaling::v1::MediaStream` since both packages now share the same generator output dir under the same nested module structure.
- `lib.rs` exposes `proto_gen::dark_tower::signaling::v1::*` and `proto_gen::dark_tower::internal::v1::*`. No alias re-exports. `pub use prost::Message` and `pub use tonic` retained.
- Consumer crates: 32 files touched, all mechanical rewrites of `use ...` paths and fully-qualified expressions. Two files (`crates/mc-service/src/webtransport/{connection,handler}.rs`) and one test (`crates/mc-service/tests/join_tests.rs`) had `use proto_gen::signaling::{self, ...}` which brought `signaling` into local scope as a short alias; the mechanical follow-up was to rename call-site references from `signaling::X` to `v1::X` (the `self` import now binds to `v1`). This preserves the architectural goal — `v1::` is literally visible at every call-site, no `as signaling` aliases.

### Cross-boundary-scope outcome
After the explicit per-file Cross-Boundary Classification entries (replacing the planning-time globs), `validate-cross-boundary-scope` lists every touched file. No leftover guard violations expected from this devloop's diff.

---

## Files Modified

Proto-gen (Mine, GSA owner):
- `crates/proto-gen/build.rs` — single-pass tonic compile, no `extern_path` / `out_dir`.
- `crates/proto-gen/src/lib.rs` — nested `dark_tower::{signaling,internal}::v1` modules.
- `crates/proto-gen/src/generated/.gitkeep` — deleted.
- `.gitignore` — removed `crates/proto-gen/src/generated/*.rs` entry (line 13 pre-cleanup).

Consumer rewrites (Not mine, Mechanical — listed in §Cross-Boundary Classification):
- mc-service: 8 src files + 5 test files
- mh-service: 5 src files + 7 test files
- gc-service: 4 src files
- ac-service: 1 fuzz target
- env-tests: 2 integration tests

### Key Changes by File

| File | Hunk Class |
|------|-----------|
| `crates/proto-gen/build.rs` | Rewrite — drop `out_dir` + `extern_path`, single-pass compile |
| `crates/proto-gen/src/lib.rs` | Rewrite — nested `dark_tower::{signaling,internal}::v1` modules with `OUT_DIR` includes |
| `.gitignore` | Delete `crates/proto-gen/src/generated/*.rs` entry |
| All 32 consumer files | Path-rewrite only: `proto_gen::signaling::` → `proto_gen::dark_tower::signaling::v1::`, `proto_gen::internal::` → `proto_gen::dark_tower::internal::v1::` |
| `crates/mc-service/src/webtransport/connection.rs` | Additionally: call-site `signaling::X` → `v1::X` (uses the `self`-imported `v1` alias) |
| `crates/mc-service/src/webtransport/handler.rs` | Additionally: call-site `signaling::X` → `v1::X` |
| `crates/mc-service/tests/join_tests.rs` | Additionally: call-site `signaling::X` → `v1::X` |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS. `cargo check --workspace --all-targets` builds clean after the rewrite.

### Layer 2: cargo fmt
**Status**: PASS after `cargo fmt --all`. Initial run flagged line wraps in three test files due to longer paths; `cargo fmt` resolves automatically.

### Layer 3: Simple Guards
**Status**: PASS after Cross-Boundary Classification refinement. `validate-cross-boundary-scope` initially flagged 5 files because the planning-time globs (`crates/mc-service/src/**/*.rs` etc.) didn't match the guard's exact-file lookup; replacing globs with explicit per-file rows resolves the violation. All 31 guards green on re-run.

### Layer 4: Unit Tests
**Status**: PASS. `cargo nextest` workspace-wide passes.

### Layer 5: All Tests (Integration)
**Status**: PASS.

### Layer 6: Audit (cargo audit + buf breaking)
**Status**: FAIL (expected and accepted per Gate 2 Validation Expectations).
- `cargo audit`: RUSTSEC-2023-0071 — rsa 0.9.10 Marvin Attack timing sidechannel via sqlx-mysql. No upstream fix. Pre-existing, out of scope.
- `buf breaking`: `internal.proto` + `signaling.proto` reported as "deleted" — artifact of #30 file-layout cleanup (the files moved under `proto/dark_tower/{internal,signaling}/v1/`, not actually deleted on the wire). Pre-existing, out of scope.

### Gate 2 Validation Expectations (recorded from user 2026-05-20)

The validation gate for this devloop, run against a fresh container with `scripts/layer-all.sh`, should show:

| Layer | Expected | Action if observed |
|-------|----------|--------------------|
| 1 (compile) | GREEN — proto-gen builds, dt-guard builds, all consumer crates compile | proceed |
| 2 (fmt) | GREEN | proceed |
| 3 (guards) | GREEN — OR down to the cross-boundary-scope leftover only | accept cross-boundary-scope leftover; anything else surfaces for discussion |
| 4 (test) | GREEN | proceed |
| 5 (lint) | GREEN | proceed |
| 6 (audit + buf breaking) | RED — pre-existing RUSTSEC + buf-breaking-deletion from #30 | accept; NOT in scope |
| 7 (env-tests) | as applicable | proceed |

Anything outside the above is likely something to fix; surface it to the user before proceeding past Gate 2.

### Layer 7: Env-tests
**Status**: N/A (`STATUS=N/A REASON=wave2-pending`).

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found

No new deps, no unsafe, no panics, no `pub use ... as ...` aliases, proto files unedited (wire format unchanged). ac-fuzz touch is a pure `use`-path rewrite; existing fuzz corpus remains valid. `ErrorCode` i32 casts in `connection.rs` are bit-identical pre/post since the proto file is unedited.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 0 found

All 13 test/fuzz files in changeset reviewed. Every diff hunk is a pure import-path rewrite — zero changes to test logic, assertions, mocks, fixtures, or rigs. Fuzz target wire-format coverage preserved (`ClientMessage::decode` path unchanged). `self`-alias rewrite verified correct in `join_tests.rs`. Independent local `cargo check` on ac-fuzz + tests of mc/mh/env-tests confirmed compile-clean.

### Observability Specialist
**Verdict**: RESOLVED-FIXED (CLEAR)
**Findings**: 0 found

Zero diff lines touching `#[instrument]`, `tracing::*!`, `metrics::*!`, or stringified `"proto_gen::..."` paths. `HealthStatus` / `DisconnectReason` references are pure use-path rewrites against byte-identical generated types. Dashboards, alerts, log queries that reference proto field names remain valid (proto field names are independent of Rust module paths).

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0 found

**ADR Compliance**: ADR-0024 §6.2 PASS (Mechanical, sed-test holds, guard coverage = compiler), ADR-0024 §6.4 PASS (GSA owner = implementer = Mine for proto-gen rows), task prompt anti-pattern PASS (zero `pub use ... as ...` / `as signaling` / `as internal` hits). The `use proto_gen::dark_tower::signaling::v1::{self, ...}` pattern in 3 files is NOT a flat alias — `self` re-imports the terminal module under its real name (`v1`), so every call-site reads `v1::ErrorCode::...` with `::v1::` token literally visible.

**Ownership Lens**: every cross-boundary edit confirmed Mechanical with sed-test holding. `gc_integration.rs`/`mc_client.rs`/`media_coordination.rs` `+3 -2` deltas are rustfmt re-wrapping the longer fully-qualified paths.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings**: None.
**Extraction opportunities** (appended to `docs/TODO.md`): None.

Single source-of-truth strengthened (OUT_DIR-only generated artifacts). `build.rs` consolidated ~37 → ~12 lines. Nested module structure mirrors proto packages — adding a future `v2` will not silently rename `v1` semantics for any caller (preserving the architectural value called out in the task prompt). Zero new abstractions / no copy-paste / no type-alias wrappers introduced.

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 0 found

`.gitignore` line removed cleanly; `generated/` directory + `.gitkeep` deleted; both `cargo:rerun-if-changed` lines preserved verbatim in build.rs; no CI / IaC / deployment manifest changes. Stale-file bug class structurally impossible after this change. Rollback: file-only revert against `80b0aba` is sufficient.

Layer 6 noise (RUSTSEC-2023-0071, #30 buf-breaking) acknowledged as pre-existing and not blocking.

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE
**Findings**: 0 found

Per-check results across 19 non-test production files:
- `credential-leak`: no findings.
- `actor-blocking`: no findings (only diff hunk in `actors/participant.rs` is inside `mod tests`).
- `error-context-preservation`: no findings (no `.map_err` touched, `From` impls untouched).
- `metrics-path-completeness`: no findings (zero `histogram!`/`counter!`/`gauge!` macro changes).

Sed-test holds across every hunk: encoded concept preserved.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Verify start commit: `80b0aba275639e17eee5f1e9556ef7b8db5acf46`
2. Review changes: `git diff 80b0aba275639e17eee5f1e9556ef7b8db5acf46..HEAD`
3. Soft reset: `git reset --soft 80b0aba275639e17eee5f1e9556ef7b8db5acf46`
4. Hard reset (clean revert): `git reset --hard 80b0aba275639e17eee5f1e9556ef7b8db5acf46`
5. No schema or infra changes — file-only revert is sufficient.

---

## Issues Encountered & Resolutions

### Issue 1: Inline `signaling::X` references after `use proto_gen::signaling::{self, ...}` rewrites
**Problem**: Three files (`mc-service/src/webtransport/connection.rs`, `handler.rs`, `tests/join_tests.rs`) had `use proto_gen::signaling::{self, ClientMessage, ...}` patterns that brought `signaling` into local scope, and call-sites used the short form `signaling::ErrorCode::*`. After the bulk sed-style import rewrite, these call-sites failed Layer 1 (`cargo check`) because the local `signaling` binding now pointed at the nested `v1` module under its leaf name.
**Resolution**: Mechanical follow-up — rewrote the affected call-sites to `v1::ErrorCode::*` (the natural form when `self`-importing the `v1` terminal module). This preserves the architectural goal: `::v1::` is literally visible at every call-site, with NO `pub use ... as signaling` aliases anywhere. Code-quality reviewer confirmed this is NOT the rejected anti-pattern (the rejected form is a crate-wide flat re-export that would hide `v1` from every consumer; the `self`-import pattern is a per-file local binding that still surfaces `v1` at every use).

### Issue 2: Cross-boundary-scope guard initially flagged 5 files
**Problem**: `validate-cross-boundary-scope` at Layer 3 flagged 5 files because main.md's planning-time globs (e.g. `crates/mc-service/src/**/*.rs`) didn't satisfy the guard's exact-file lookup.
**Resolution**: Implementer refined the §Cross-Boundary Classification table to explicit per-file rows (35 entries). Subsequent Layer 3 run: 31/31 guards pass.

---

## Lessons Learned

1. **OUT_DIR is the right idiom for prost+tonic.** The `extern_path` + `.out_dir("src/generated")` pattern from #31 was unusual and not warranted by any actual cross-package reference need. The default `OUT_DIR` flow with nested modules mirroring the proto package hierarchy is simpler, idiomatic, and makes the stale-file bug class structurally impossible.
2. **`include!(concat!(env!("OUT_DIR"), "/foo.rs"))` is the canonical pattern.** Generated artifacts only ever live under `target/.../build/.../out/`; they cannot be checked in, cannot go stale, cannot mask a build.rs bug during in-worktree validation.
3. **`use foo::{self, ...}` is a legitimate consumer pattern** for nested modules — it's NOT the same as a crate-wide `pub use foo::v1 as v1Removed;` flat alias. The architectural intent (use-site `::v1::` visibility) survives the `self`-import; only crate-wide flat aliases hide schema versioning.
4. **The compiler IS the partial-rename guard for Mechanical rewrites.** Removing the old `pub mod signaling` / `pub mod internal` modules from `lib.rs` made any missed consumer call-site fail Layer 1. No additional guard needed.
5. **Plan classification tables should be explicit per-file once the file list is known.** Globs are useful at planning time but the Layer 3 cross-boundary-scope guard wants exact-file rows for traceability. Refine post-implementation.

---

## Appendix: Verification Commands

```bash
# Verify generated/ is gone
[ ! -d crates/proto-gen/src/generated ] && echo "OK: generated/ deleted"

# Verify .gitignore cleaned
! grep -q "proto-gen/src/generated" .gitignore && echo "OK: .gitignore cleaned"

# Verify lib.rs has no flat aliases (the anti-pattern)
! grep -E "pub use dark_tower::(signaling|internal)::v1 as" crates/proto-gen/src/lib.rs && echo "OK: no alias re-exports"

# Verify build.rs has no extern_path or out_dir
! grep -E "extern_path|out_dir" crates/proto-gen/build.rs && echo "OK: build.rs cleaned"

# Run full pipeline
./scripts/layer-all.sh
```
