# Devloop Output: Proto file-layout cleanup (R-61 part 2, task #30)

**Date**: 2026-05-20
**Task**: Move `proto/internal.proto` → `proto/dark_tower/internal/internal.proto` and `proto/signaling.proto` → `proto/dark_tower/signaling/signaling.proto`; update `crates/proto-gen/build.rs` include paths and `proto/buf.gen.yaml` out-path resolution; resolve 4 MINIMAL-tier `buf lint` file-layout findings.
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task30`
**Duration**: ~60m (setup → commit)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8abe4b6cafaf5f78271231fc94260700736595e3` |
| Branch | `feature/browser-client-join-task30` |
| User Story | `docs/user-stories/2026-05-02-browser-client-join.md` (task #30) |
| Requirement | R-61 part 2 (file-layout) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-task30-proto-layout` |
| Implementing Specialist | `protocol` |
| Iteration | `1` |
| Security | CLEAR |
| Test | RESOLVED-FIXED |
| Observability | CLEAR |
| Code Quality | RESOLVED-DEFERRED |
| DRY | CLEAR |
| Operations | CLEAR |
| Semantic Guard | CLEAR |

---

## Task Overview

### Objective

Move the two `.proto` files into package-matching directories so the four MINIMAL-tier `buf lint` file-layout findings drop off (2× `DIRECTORY_SAME_PACKAGE` + 2× `PACKAGE_DIRECTORY_MATCH`). Update consumer build-script include paths so Rust + TS codegen still produce. Wire-format unchanged.

### Scope

- **Service(s)**: protocol surface — affects every consumer of `proto-gen` indirectly via regenerated stubs (path-only; no API change).
- **Schema**: No DB schema changes.
- **Cross-cutting**: Yes (codegen consumers), but wire-format and generated symbol set are unchanged.

### Debate Decision

NOT NEEDED — pure file-layout move with no concept substitution. R-61 design (Revision 8) sequences this as the cheap middle step of the spec-first 29→30→31 chain.

---

## Cross-Boundary Classification

Per task #30 spec, the implementer is the **protocol** specialist. Both `proto/**` and `proto-gen/**` are Guarded Shared Areas owned by protocol — so for this implementer they are in-domain (`Mine`). Cross-boundary touches are limited to `proto/buf.gen.yaml` (also protocol-owned) and zero other rows; downstream Rust services see *regenerated stubs* on the next build but their own source files are untouched.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `proto/dark_tower/internal/internal.proto` | Mine | — |
| `proto/dark_tower/signaling/signaling.proto` | Mine | — |
| `crates/proto-gen/build.rs` | Mine | — |
| `scripts/guards/simple/cross-boundary-ownership.yaml` | Mechanical (path-string update, GSA-sync mirror) | infrastructure |
| `scripts/guards/simple/validate-gsa-sync.sh` | Mechanical (path-string update, GSA-sync mirror) | infrastructure |
| `packages/proto-gen/scripts/verify-codegen.sh` | Mine | — |
| `.gitignore` | Mine | — |

Note on GSA: `proto/**`, `proto-gen/**`, `build.rs` are all enumerated GSA paths (wire-format runtime coupling, ADR-0024 §6.4). For a non-protocol implementer these would route to owner-implements; here the implementer IS the owner, so the rows are `Mine` and the GSA "Mechanical-disallowed" rule does not apply (it gates *cross-boundary* edits, not in-domain ones).

**Per-row context** (kept lean so the cross-boundary scope-drift guard parser can read bare paths; see implementation step list and §Files Modified for details):

- `proto/dark_tower/internal/internal.proto` — moved from `proto/internal.proto`; line 5 import updated to `dark_tower/signaling/signaling.proto`.
- `proto/dark_tower/signaling/signaling.proto` — moved from `proto/signaling.proto`.
- `crates/proto-gen/build.rs` — 4 path strings updated (compile_protos args + rerun-if-changed).
- `scripts/guards/simple/cross-boundary-ownership.yaml` — intersection-rule key rename `proto/internal.proto` → `proto/dark_tower/internal/internal.proto`.
- `scripts/guards/simple/validate-gsa-sync.sh` — INTERSECTION_SUBPATHS array + adjacent prose comment updated to the new path.
- `packages/proto-gen/scripts/verify-codegen.sh` — `assert_generated` paths (lines 52-53) updated, `-maxdepth 1` dropped on the line-27 clean step to recurse into the new nested codegen layout.
- `.gitignore` — pattern `packages/sdk-core/src/proto/*_pb.ts` widened to `packages/sdk-core/src/proto/**/*_pb.ts` so the now-nested generated outputs stay untracked. Surfaced during Layer 3 validation when `git ls-files --others --exclude-standard` listed the new `_pb.ts` paths as scope-drift; root cause was the flat-layout glob no longer matching after the move. In-domain (protocol owns codegen pipeline + its outputs); same structural-followthrough character as the `verify-codegen.sh` `-maxdepth 1` and `build.rs` path updates.

**Dropped from table vs. earlier plan version**: the `proto/buf.gen.yaml` row was a "verify-only, no edit expected" entry — the scope-drift guard's planned-untouched check correctly flags rows with no diff. Verification of `buf.gen.yaml` still occurs (and passed: `inputs: [{ directory: . }]` recurses into the new dirs without change), but it's not a table row since no path changes.

**On the two GSA-sync mirror rows**: these are not in the original task spec but surfaced during planning — `scripts/guards/simple/validate-gsa-sync.sh` (lines 61–63) and `cross-boundary-ownership.yaml` line 33 both pin the intersection-subpath key `proto/internal.proto`. After the `git mv`, the YAML's stray-key check (lines 172–177 of the guard) would flag the old path as a stray key. Renaming both in lockstep keeps the sync guard passing. I'm classifying these as Mechanical (pure path-string replacement, no semantic change to the intersection rule itself — `internal.proto` still needs the protocol + auth-controller + security tri-cosign at its new path). The ADR-0024 §6.4 enumeration list itself (the `CANON` array) is unaffected because it only enumerates `proto/**` (glob), which still matches.

---

## Planning

### Approach

Pure file-layout move. Wire-format and generated symbol set (Rust + TS) are unchanged. Packages remain `dark_tower.internal` / `dark_tower.signaling` (no `v1` suffix — that's task #31).

### Step-by-step

1. **Create target directories** (no-op for `git`; `git mv` creates intermediate paths):
   - `proto/dark_tower/internal/`
   - `proto/dark_tower/signaling/`

2. **Move files via `git mv`** (preserves rename history; rename detection should fire because the file content moves with the path):
   - `git mv proto/internal.proto proto/dark_tower/internal/internal.proto`
   - `git mv proto/signaling.proto proto/dark_tower/signaling/signaling.proto`

3. **Update `proto/dark_tower/internal/internal.proto`'s import statement** (line 5). Currently `import "signaling.proto";` — this resolves because the `proto/` include root contained `signaling.proto` directly. After the move, signaling lives at `dark_tower/signaling/signaling.proto` relative to the same include root. Change to:
   ```
   import "dark_tower/signaling/signaling.proto";
   ```
   This is the canonical buf form — fully-qualified path relative to the module root — and is what `buf format` would produce.

4. **Update `crates/proto-gen/build.rs`** — change the four embedded paths:
   - `compile_protos` arg list: `"../../proto/signaling.proto"` → `"../../proto/dark_tower/signaling/signaling.proto"`, same for `internal.proto`.
   - Include-dir arg stays `"../../proto/"` (unchanged — it's the root from which `tonic-build` resolves relative imports; the new `import "dark_tower/signaling/signaling.proto"` statement resolves correctly from there).
   - Two `cargo:rerun-if-changed=` lines: update to the new paths.

5. **Verify `proto/buf.gen.yaml`** — no change expected. `inputs: [{ directory: . }]` is recursive, so it picks up `dark_tower/**/*.proto` automatically. `out: ../packages/sdk-core/src/proto` is relative to the `cwd: proto` declared in `packages/proto-gen/project.json` codegen target, which is unchanged. Buf-es codegen outputs files based on the proto file's path within the module; this will shift the generated TS file names — see "Generated-output shift" below.

6. **Update `scripts/guards/simple/cross-boundary-ownership.yaml`** line 33 — rename the intersection-rule key from `"proto/internal.proto"` to `"proto/dark_tower/internal/internal.proto"`. The owner list stays `[protocol, auth-controller, security]`.

7. **Update `scripts/guards/simple/validate-gsa-sync.sh`** lines 62 (the INTERSECTION_SUBPATHS array) and 57 (the prose comment that names the path) — same rename. The CANON array is untouched (still uses the glob `proto/**`).

8. **Update `packages/proto-gen/scripts/verify-codegen.sh`** (raised by @test, confirmed in-scope by team-lead — both sub-edits in Plan-approved scope):
   - **Lines 52-53 (assert_generated paths)**: change `assert_generated "signaling_pb.ts" "JoinRequest"` → `assert_generated "dark_tower/signaling/signaling_pb.ts" "JoinRequest"` and `assert_generated "internal_pb.ts" "RegisterParticipant"` → `assert_generated "dark_tower/internal/internal_pb.ts" "RegisterParticipant"`. Symbol names unchanged. Without this, `nx run proto-gen:test` fails with "expected file not produced".
   - **Line 27 (drop `-maxdepth 1` in the pre-generate clean)**: change `find "${OUT_DIR}" -maxdepth 1 -type f \( -name '*_pb.ts' -o -name '*_pb.js' -o -name '*_pb.d.ts' \) -delete` → `find "${OUT_DIR}" -type f \( -name '*_pb.ts' -o -name '*_pb.js' -o -name '*_pb.d.ts' \) -delete`. After the move, generated files live at `OUT_DIR/dark_tower/{internal,signaling}/*_pb.ts` — beyond maxdepth 1. Dropping the bound restores the invariant stated at lines 8-12 of the script (clean any stale `_pb.ts` under OUT_DIR so prior-config outputs can't mask a broken current config — credit: @test's 2026-05-06 Gate 3 finding).

Same in-domain rationale as the other entries: protocol owns `packages/proto-gen/` (TS-side codegen pipeline; mirrors `crates/proto-gen/`).

### Generated-output shift (TS side) — resolved

`protoc-gen-es` writes files at `<out>/<proto-rel-path-without-extension>_pb.ts`. Today: `packages/sdk-core/src/proto/{signaling,internal}_pb.ts`. After the move: `packages/sdk-core/src/proto/dark_tower/{signaling,internal}/{signaling,internal}_pb.ts`.

**Consumer audit**: `grep -rn 'signaling_pb\|internal_pb'` returns only `packages/proto-gen/scripts/verify-codegen.sh:52-53`. No TS, JS, Svelte, or JSON file imports the generated symbols. Team-lead independently confirmed no TS consumers exist. So the only edit required for the path shift is step 8 above; downstream TS code is untouched.

### Order of operations

Files first → then `internal.proto` import → then build.rs → then codegen verify → then GSA-sync mirrors → then full validation. The GSA-sync mirror update has no functional impact (it's metadata that the layer-3 sync guard reads), so order doesn't matter, but doing it after the codegen verify means I'm not chasing two unrelated failures if codegen breaks.

### Risks

- **`git mv` rename detection**: `git mv` is just `mv` + `git rm` + `git add`. Whether `git log --follow` shows the rename depends on the diff threshold; content is identical so it should be near 100% similarity. Low risk.
- **buf import resolution**: the new fully-qualified import must resolve. `buf lint` and the codegen pipeline will catch this immediately.
- **TS consumer paths** (see "Generated-output shift" above): potentially out-of-scope discovery.
- **Lint count != 17**: if I see != 17, I will NOT add `lint.ignore` entries (Revision 8 forbids the carve-out). I'll diff actual vs. expected findings and report to team-lead.

### Out of scope

- Adding `v1` suffix to package names (task #31).
- Renaming any of the RPC request/response types flagged by the residual 17 findings (task #31).
- Adding `lint.ignore` entries (forbidden per Revision 8).

---

## Expected Layer-5 State (per task spec)

Before #30 lands: **21** STANDARD `buf lint` findings (the planned exclusive-window state opened by #29 per Revision 8).

After #30 lands: exactly **17** findings remain. The 4 that resolve naturally:

1. `proto/internal.proto:3:1: Multiple packages "dark_tower.internal,dark_tower.signaling" detected within directory "."` (resolves: separate directories now)
2. `proto/internal.proto:3:1: Files with package "dark_tower.internal" must be within a directory "dark_tower/internal" relative to root but were in directory "."` (resolves: now in correct directory)
3. `proto/signaling.proto:3:1: Multiple packages "dark_tower.internal,dark_tower.signaling" detected within directory "."` (resolves: separate directories now)
4. `proto/signaling.proto:3:1: Files with package "dark_tower.signaling" must be within a directory "dark_tower/signaling" relative to root but were in directory "."` (resolves: now in correct directory)

If post-#30 count differs from 17, investigate before closing — DO NOT add `buf.yaml` `lint.ignore` entries (Revision 8 forbids the carve-out anti-pattern). #31 drains the remaining 17 via the rename sweep.

---

## Pre-Work

None.

---

## Implementation Summary

Pure file-layout move with consumer/path-fixture follow-through. Wire-format and generated symbol set are unchanged (verified via `cargo check --workspace` and `nx run proto-gen:test`). `buf lint` count dropped exactly as predicted: 21 → 17, with the 4 file-layout findings (2× multi-package, 2× package-directory-match) clearing naturally and the remaining 17 (`vN` package suffix + RPC type renames) deferred to task #31 per Revision 8.

Two discoveries during validation that landed in-scope as structural follow-through to the move (same character as the planned `build.rs` and `verify-codegen.sh` updates — all are flat-layout artifacts that needed widening for the nested layout):

1. **`.gitignore` pattern was flat-layout-only**. `packages/sdk-core/src/proto/*_pb.ts` doesn't match the new nested codegen outputs at `packages/sdk-core/src/proto/dark_tower/{internal,signaling}/*_pb.ts`. Surfaced as scope-drift-inbound in Layer 3 (`validate-cross-boundary-scope`) when codegen output appeared as untracked-but-not-ignored. Widened pattern to `packages/sdk-core/src/proto/**/*_pb.ts`.

2. **4 specialist `INDEX.md` files had stale path pointers** (15 references across `client/`, `media-handler/`, `meeting-controller/`, `protocol/`). Surfaced as Layer 3 `validate-knowledge-index` failures. Mass-sedded all `proto/{signaling,internal}.proto` → `proto/dark_tower/{signaling,internal}/{signaling,internal}.proto`. Per cross-boundary-scope guard's auto-exclusion (`docs/specialist-knowledge/**/INDEX.md` line 165), these don't require Cross-Boundary table rows.

Cross-Boundary Classification table was also restructured during Layer 3 verification: paths in the Path column now stand alone (no trailing parentheticals) because the guard's `parse_cross_boundary_table` strips trailing parens, which had been hiding the post-move paths behind the pre-move ones. Per-row context moved into a prose list below the table. The `proto/buf.gen.yaml` row was dropped (verify-only, no edit, was tripping planned-untouched check).

### Layer-all final state

- Layers 1, 2, 4, 7: PASS / N/A.
- Layer 3: FAIL on **one pre-existing guard** — `no-dev-trust-path-in-prod-bundle` (R-14 enforcement gap, tracked in `docs/TODO.md`, fires at HEAD baseline too — confirmed via stash-and-run, unrelated to this task).
- Layer 5: FAIL on **17 buf-lint findings** — the planned exclusive-window state per Revision 8.
- Layer 6: FAIL on **`cargo audit` RUSTSEC-2023-0071** (rsa crate Marvin Attack via sqlx-mysql, no fixed upgrade available, pre-existing, unrelated to this task).

No new failures introduced; all task-spec verification targets pass.

---

## Files Modified

```
.gitignore                                           |  2 +-
crates/proto-gen/build.rs                            |  9 ++++++---
docs/specialist-knowledge/client/INDEX.md            |  2 +-
docs/specialist-knowledge/media-handler/INDEX.md     |  4 ++--
docs/specialist-knowledge/meeting-controller/INDEX.md|  4 ++--
docs/specialist-knowledge/protocol/INDEX.md          | 20 ++++++++++----------
packages/proto-gen/scripts/verify-codegen.sh         |  6 +++---
proto/{ => dark_tower/internal}/internal.proto       |  2 +-
proto/{ => dark_tower/signaling}/signaling.proto     |  0
scripts/guards/simple/cross-boundary-ownership.yaml  |  2 +-
scripts/guards/simple/validate-gsa-sync.sh           | 11 ++++++-----
11 files changed, 33 insertions(+), 29 deletions(-)
```

### Key Changes by File

- **`proto/dark_tower/{internal,signaling}/{internal,signaling}.proto`** — `git mv` from `proto/`. Rename detection preserved (`git diff --find-renames` shows the `{ => dark_tower/...}` pattern). `internal.proto` line 5 import updated from `signaling.proto` → `dark_tower/signaling/signaling.proto` (canonical fully-qualified form relative to the `proto/` include root).
- **`crates/proto-gen/build.rs`** — 4 path strings: 2 in `compile_protos` arg list, 2 in `cargo:rerun-if-changed` directives. Include root `../../proto/` unchanged.
- **`packages/proto-gen/scripts/verify-codegen.sh`** — `assert_generated` paths updated for the nested layout (lines 52-53). `find -maxdepth 1` dropped on the pre-generate clean step (line 27) so it recurses into `dark_tower/{internal,signaling}/` and continues to honor @test's 2026-05-06 Gate 3 invariant about stale-prior-config outputs.
- **`scripts/guards/simple/cross-boundary-ownership.yaml`** — intersection-rule key `proto/internal.proto` → `proto/dark_tower/internal/internal.proto`. Owner list `[protocol, auth-controller, security]` unchanged.
- **`scripts/guards/simple/validate-gsa-sync.sh`** — `INTERSECTION_SUBPATHS` array entry + adjacent prose comment updated to the new path. `CANON` array (`proto/**` glob) unchanged.
- **`.gitignore`** — `packages/sdk-core/src/proto/*_pb.ts` → `packages/sdk-core/src/proto/**/*_pb.ts` to recurse into the new nested codegen output paths.
- **`docs/specialist-knowledge/{client,media-handler,meeting-controller,protocol}/INDEX.md`** — 15 stale path references mass-updated from `proto/{signaling,internal}.proto` to `proto/dark_tower/{signaling,internal}/{signaling,internal}.proto`.

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS (`cargo check --workspace`, all crates Checking → Finished).

### Layer 2: cargo fmt
**Status**: PASS (cargo fmt, buf format, nx format all green).

### Layer 3: Simple Guards
**Status**: FAIL on `no-dev-trust-path-in-prod-bundle` only — pre-existing R-14 enforcement gap (tracked in `docs/TODO.md`), confirmed failing at HEAD baseline too via stash-and-run. Not introduced by this task. All other guards pass including `validate-gsa-sync`, `validate-cross-boundary-scope`, `validate-knowledge-index`.

### Layer 4: Unit Tests
**Status**: PASS (cargo test, nx test all green).

### Layer 5: All Tests (Integration)
**Status**: FAIL on 17 residual `buf lint` findings — the planned exclusive-window state per Revision 8 (#31 closes the window). Down from 21 at baseline; the 4 file-layout findings cleared as predicted.

### Layer 6: Audit (cargo audit / pnpm audit / buf breaking)
**Status**: FAIL on RUSTSEC-2023-0071 (rsa crate Marvin Attack via sqlx-mysql, no fixed upgrade available) — pre-existing, unrelated to this task. `pnpm audit` passes; `buf breaking` passes.

### Layer 7: Env-tests
**Status**: N/A (Wave 2 — not yet enabled in layer-all).

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Intersection-rule semantics preserved at new key; lockstep rename verified. |
| Test | RESOLVED-FIXED | 0 | 0 | 0 | All oracles green; 21→17 buf lint count confirmed; generated symbol set byte-identical. |
| Observability | CLEAR | 0 | 0 | 0 | No instrumentation/metric/trace surface touched; tags 20/21 (`trace_parent`/`trace_state`) preserved. |
| Code Quality | RESOLVED-DEFERRED | 2 | 1 | 1 | F1 (stale doc comments in already-edited GSA-sync mirrors) fixed in-flight; F2 (stale "real file" comment in `scripts/lang/proto/changed.test.sh:18-20`) accepted-deferral, tracked in `docs/TODO.md`. Code-reviewer self-labeled RESOLVED-FIXED after F1 fix; per review-protocol any accepted deferral forces RESOLVED-DEFERRED for the reviewer's verdict, recorded here per protocol. |
| DRY | CLEAR | 0 | 0 | 0 | No new logic / no extraction opportunities. |
| Operations | CLEAR | 0 | 0 | 0 | No manifests/Dockerfiles/CI workflows/runbooks touched; Layer 5 FAIL acceptance documented. |
| Semantic Guard | CLEAR | 0 | 0 | 0 | Credential-leak / actor-blocking / error-context / metrics-path all N/A. |

### Iteration history

7 reviewer verdicts received in one review iteration. @code-reviewer initially returned RESOLVED-DEFERRED with two findings:

**Finding 1 — fixed in-flight** (suspicious-deferral check: <5 LoC, inside existing changeset, no design ambiguity).

Stale example-text in two comment blocks of the GSA-sync mirror files I had already edited:
- `scripts/guards/simple/validate-gsa-sync.sh:21` — comment example `proto/internal.proto` updated to `proto/dark_tower/internal/internal.proto`.
- `scripts/guards/simple/cross-boundary-ownership.yaml:26` — same.

Not load-bearing (the guard parses bullet lists and YAML keys, not free-form comments), but inconsistent with the data-line updates landed in those same files. Guard re-passes (`exit 0`).

**Finding 2 — accepted deferral** (see Accepted Deferrals §1 below).

---

## Accepted Deferrals

- `docs/TODO.md` §Documentation Hygiene — `scripts/lang/proto/changed.test.sh:18-20` stale "real file" comment (code-reviewer F2)

---

## Rollback Procedure

1. Verify start commit: `8abe4b6cafaf5f78271231fc94260700736595e3`
2. Review changes: `git diff 8abe4b6cafaf5f78271231fc94260700736595e3..HEAD`
3. Hard reset: `git reset --hard 8abe4b6cafaf5f78271231fc94260700736595e3`
4. No migrations, no infra changes — `git reset` alone is sufficient.

---

## Issues Encountered & Resolutions

### Issue 1: Plan-revision churn at Gate 1 on maxdepth-fix scope

**Problem**: Team-lead issued conflicting plan approvals (option A vs B) during the verify-codegen.sh sub-edit discussion. ~4 flip cycles before locking option B.

**Resolution**: Implementer's chronology-and-options ("X/Y/P/Q/R") framing exposed the conflict cleanly; team-lead locked option B (current repo state at time of resolution) without forcing a revert of tested green code.

### Issue 2: Plan-table parser-format violation at Gate 1 classification-sanity guard

**Problem**: `proto/buf.gen.yaml` row's Classification column read `Mine (no edit expected — verification only)`; parser does exact-match `[[ classification == "Mine" ]]`, so the row was treated as non-Mine and tripped the GSA-path-missing-owner check.

**Resolution**: Implementer moved parenthetical context to the Path column; Classification became bare `Mine`. Guard re-passed. Later the row was dropped entirely since `buf.gen.yaml` was verify-only with no diff (tripping the planned-untouched check).

---

## Lessons Learned

1. **Cross-Boundary Classification table parser format is brittle**: bare `Mine` / bare `—` only; move all parenthetical context to the Path column or to prose. Worth a future improvement to the guard parser (allow trailing parens) but until then, keep the table parser-clean.
2. **Verify-only rows are anti-pattern**: a row in the Cross-Boundary Classification table for a file that won't be touched will trip the planned-untouched scope-drift check. Keep table rows = actual edits only; document verify-only files in prose.
3. **"Plan approved" is a one-shot commitment**: re-litigating after approval = silent flip-flop. Future devloops: lead resists the urge to flip on small scope items once approval is issued; if a reviewer surfaces a concern post-approval, route it to Gate 3 as a finding rather than re-opening Gate 1.
4. **Suspicious-deferral check is load-bearing**: a small (<5 LoC) in-file edit defaulted to "fix it" is almost always cheaper than tracking a deferral, especially when the same file is already being edited. Saved one TODO carry-forward by rolling the maxdepth fix into #30.
