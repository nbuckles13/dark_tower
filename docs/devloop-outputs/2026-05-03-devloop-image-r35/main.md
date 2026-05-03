# Devloop Output: Devloop Image R-35 (pnpm + Playwright + Chromium)

**Date**: 2026-05-03
**Task**: Update `infra/devloop/Dockerfile` per R-35 — corepack enable pnpm@9.x, install Playwright system deps, pre-cache Chromium, version-pin Playwright; document image-size delta.
**Specialist**: infrastructure
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task3`
**Duration**: ~22 minutes (planning kick-off 03:28 UTC → final verdict 03:50 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1194236` |
| Branch | `feature/browser-client-join-task3` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-r35-image` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@devloop-r35-image` |
| Test | `test@devloop-r35-image` |
| Observability | `observability@devloop-r35-image` |
| Code Quality | `code-reviewer@devloop-r35-image` |
| DRY | `dry-reviewer@devloop-r35-image` |
| Operations | `operations@devloop-r35-image` |

---

## Task Overview

### Objective
Extend the devloop container image so it can host the browser-client toolchain required by the user story (`docs/user-stories/2026-05-02-browser-client-join.md`):

- Enable Corepack and prepare `pnpm@9.x` (Node 22 already present).
- Install Playwright system dependencies for Chromium (libnss3, libatk-bridge2.0-0, libdrm2, libxkbcommon0, libxcomposite1, libxdamage1, libxfixes3, libxrandr2, libgbm1, libpango-1.0-0, libcairo2, libasound2, fonts).
- Pre-cache the Chromium browser via `npx playwright install --with-deps chromium` at `/opt/ms-playwright`.
- Pin the Playwright version (build-time `ARG`).
- Document the image-size delta vs. the prior layer (story budget ≤600 MB additional; expected ~280 MB Chromium + ~50 MB deps).

### Scope
- **Service(s)**: none (devloop tooling image only)
- **Schema**: no
- **Cross-cutting**: no — single Dockerfile, no Rust/Cargo/proto/migrations/K8s-manifest changes

### Debate Decision
NOT NEEDED — task #3 in the story plan is a fully scoped infrastructure deliverable (T-INFRA-2/R-35); ADR-0025 already covers the devloop containerization model.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/devloop/Dockerfile` | Mine | — |
| `docs/devloop-outputs/2026-05-03-devloop-image-r35/main.md` | Mine | — |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine | — |

Scope note for `docs/user-stories/2026-05-02-browser-client-join.md`: edit limited to the Devloop Tracking row 3 (Status + Devloop Output cells only) — story-bookkeeping, not story-content rewrite.

No cross-boundary edits. No Guarded Shared Area paths touched.

---

## Planning

### Approach
Three new layers appended to `infra/devloop/Dockerfile` after the existing Node 22 + Claude Code CLI layer (line 62), keeping the existing layer order otherwise untouched.

#### Section header (above Layer A, per @code-reviewer)
```dockerfile
# === Browser-client toolchain (R-35) ===
# Image-size delta budget: ≤600 MB additional vs. prior image.
# Expected: ~280 MB Chromium binary + ~50 MB system deps + small fonts margin.
# Measure: podman image inspect darktower-dev:latest --format '{{.Size}}' before/after.
```

#### Layer A — pnpm via Corepack
```dockerfile
ARG PNPM_VERSION=9.15.0
RUN corepack enable \
 && corepack prepare pnpm@${PNPM_VERSION} --activate
```
- `corepack` ships with Node 22; no apt install needed.
- `--activate` ensures the prepared shim is the default `pnpm` on PATH (avoids interactive prompt on first invocation under non-TTY devloop runs).
- Pinned via `ARG` so a future bump is a one-line change (story §464 says `pnpm@9.x` and the workspace setup task #1 will commit `packageManager: "pnpm@9.x"` — both Corepack-pinned, so versions stay in lockstep).

#### Layer B — Playwright Chromium system deps (single RUN, --no-install-recommends, list cleanup)
```dockerfile
# Story §139 / §464 verbatim list, in the order given there:
#   libnss3 libatk-bridge2.0-0 libdrm2 libxkbcommon0 libxcomposite1
#   libxdamage1 libxfixes3 libxrandr2 libgbm1 libpango-1.0-0 libcairo2
#   libasound2 fonts-liberation fonts-noto-color-emoji
# Plus two additions (libatk1.0-0, libcups2) that Playwright's `--with-deps`
# pulls in for headless Chromium 1.49 on Debian bookworm but the story list
# omits — without these, the build-time smoke (chrome --version) fails with
# missing-symbol errors. See @test's Gate-1 review note + Playwright's own
# `playwright install --with-deps` source for the canonical list.
RUN apt-get update && apt-get install -y --no-install-recommends \
    libnss3 libatk-bridge2.0-0 libatk1.0-0 libcups2 libdrm2 \
    libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 \
    libgbm1 libpango-1.0-0 libcairo2 libasound2 \
    fonts-liberation fonts-noto-color-emoji \
 && rm -rf /var/lib/apt/lists/*
```
- Story §139 / §464 verbatim list, plus `libatk1.0-0` and `libcups2` (Playwright's `--with-deps` step pulls these in for headless Chromium on Debian bookworm; without them the build-time smoke fails). Surfaced now per @test Gate-1 protocol so reviewers see the delta from the story list explicitly.
- Fonts: `fonts-liberation` is what Playwright's own deps list pulls in; `fonts-noto-color-emoji` covers emoji rendering in test assertions.

#### Combined ENV block (before Layer C, per @code-reviewer)
```dockerfile
# Build-time: Layer C's `playwright install` writes Chromium under PLAYWRIGHT_BROWSERS_PATH.
# Runtime:    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 stops downstream `pnpm install`
#             inside the running container from re-downloading Chromium —
#             it must use the image-baked browser at /opt/ms-playwright.
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright \
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
```
- One ENV layer instead of two; build-time vs. runtime intent conveyed via the comment above the block (per @code-reviewer's preference, adopted at Gate 1).
- `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` is harmless during the build because Layer C uses `npx playwright install` directly (not a pnpm-postinstall hook), so the skip flag is a no-op for that command but correct for any later layer/runtime.

#### Layer C — Pre-cache Chromium (separate RUN for cache discipline)
```dockerfile
# Load-bearing: PLAYWRIGHT_VERSION is the contract version between this image
# (which bakes Chromium at /opt/ms-playwright) and the packages/web-app
# `playwright` devDep (task #18). Bumping here REQUIRES bumping the consumer
# pin in lockstep — Playwright refuses to launch a Chromium revision that
# does not match its npm-package's recorded revision.
ARG PLAYWRIGHT_VERSION=1.49.0
RUN npx -y playwright@${PLAYWRIGHT_VERSION} install chromium \
 && chmod -R a+rX /opt/ms-playwright

# Build-time smoke (per @test, kept after @operations + @test confirmed option A):
# fail fast if Chromium can't dlopen its libs inside the image. This catches
# missing apt deps (e.g., libcups2, libatk1.0-0) at build time rather than as
# cryptic runtime failures in downstream tests. `playwright install`'s exit code
# does not catch this — it only verifies tarball extraction, not dynamic-linker
# resolution.
# TODO: revisit if a Playwright bump changes the on-disk path layout (currently
# `chromium-<revision>/chrome-linux/chrome` — historically Playwright has
# rearranged this; if the glob fails, that's the first place to look).
RUN /opt/ms-playwright/chromium-*/chrome-linux/chrome --version

# Rollback: rebuild from prior commit (no state migration; named volumes are toolchain-agnostic).
```
- `PLAYWRIGHT_VERSION=1.49.0` matches story §464 (the consumer side, task #18, will install the same version via `pnpm`); pinning at the image layer ensures parity.
- Drop `--with-deps` because we already installed the deps explicitly in Layer B (gives us a deterministic apt list under our own cache discipline; `--with-deps` re-runs apt which we don't want as a separate uncacheable layer).
- `chmod a+rX` so non-root mapped users (`--userns=keep-id`) can read the browser cache — mirrors the `/home/dev` and `/tmp/cargo-home` pattern at lines 67/73.

**Chromium trust chain (per @security):** Chromium browser blob is fetched via `playwright install` from Playwright's HTTPS CDN. Integrity is established transitively through the version-pinned `playwright` npm package (whose tarball is integrity-checked by npm) which carries the Chromium revision identifier and download manifest. No separate sha256 verification — Playwright does not publish per-version browser checksums. This is the same trust posture as the official `mcr.microsoft.com/playwright` images, and the devloop image already runs untrusted Claude-generated code in a container per ADR-0025, so the blast radius is the sandbox itself. Corepack-prepared pnpm verifies its tarball against integrity hashes baked into Node 22's corepack distribution — we rely on the default behavior and do not override `COREPACK_INTEGRITY_KEYS`.

### Cache discipline summary
- 1 RUN for apt deps (single update + install + list cleanup).
- 1 RUN for `corepack enable + prepare` (cheap; combined is fine — both pure node-side ops).
- 1 RUN for `npx playwright install chromium` (largest layer; isolated so other layers remain cacheable).
- No new ENVs interleaved between large layers (build-cache friendliness).

### Final RUN ordering (delta only)
1. (existing) Node 22 + Claude Code CLI (line 59-62).
2. **NEW** — Section header comment (size-budget block).
3. **NEW Layer A** — Corepack/pnpm.
4. **NEW Layer B** — Playwright apt deps.
5. **NEW** — Combined `ENV PLAYWRIGHT_BROWSERS_PATH + PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD` block.
6. **NEW Layer C** — `ARG PLAYWRIGHT_VERSION` + Chromium pre-cache + chrome-binary smoke.
7. (existing) HOME, cargo-home prep, entrypoint COPY, WORKDIR, ENTRYPOINT.

This ordering preserves the existing toolchain layer cache (Rust, kubectl, gh, Node) and adds new layers strictly above the userspace prep steps, so a typical Rust-only iteration hits the cache fully.

### What I'm NOT doing
- Not building the image (validation pipeline doesn't run image builds; size delta documented as expected).
- Not touching `entrypoint.sh`, `dev-cluster`, `devloop.sh`.
- Not adding any test fixtures, service code, or secrets.
- Not pinning Chromium revision separately — Playwright pins the matching browser revision per its version, so `PLAYWRIGHT_VERSION=1.49.0` is sufficient.

### Operations invariants (per @operations Gate 1)

1. **Image-size budget evidence**: option (a) — calculated estimate documented in the §Planning size header (~280 MB Chromium + ~50 MB deps + small fonts margin = ~330 MB, well under the 600 MB ceiling). The validation pipeline doesn't build container images per ADR-0024. If operations wants option (b) at Gate 2, I'll do an opportunistic local `podman build` and capture the actual delta in the Implementation Summary.
2. **WORKDIR / HOME / ENTRYPOINT invariance**: confirmed. `WORKDIR /work` (line 79), `ENV HOME=/home/dev` (line 68), `ENTRYPOINT` (line 81) — none touched. New layers inserted between line 62 (Node/Claude Code) and line 67 (`mkdir -p /home/dev`); the existing trailing block is unaffected.
3. **`--userns=keep-id` compatibility**:
   - **Chromium cache (`/opt/ms-playwright`)**: pre-populated at build time as root, then `chmod -R a+rX` (world-read + dir-traverse). Playwright at runtime only READS from `PLAYWRIGHT_BROWSERS_PATH` when launching a browser — it does not write a lockfile/manifest there during `playwright test` (writeable state lives under `$HOME` or each test's `outputDir`). On `pnpm install`, `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` short-circuits any write attempt. So `a+rX` is correct AND tighter than 777 — chosen for least-privilege.
   - **Edge case**: a user running `playwright install` from inside the container (e.g., trying to add Firefox) would fail with permission-denied — intentional guardrail; only image rebuilds mutate the baked browser cache.
   - **pnpm via Corepack**: `corepack prepare pnpm@9.15.0 --activate` runs at build time as root, writes shims to a Node-managed root-owned dir on PATH, and is invokable by anyone after that. First-run user state (`$HOME/.local/share/pnpm`) is covered by the existing `/home/dev` 777 pattern at line 67.
4. **Pinning explicit**: `ARG PNPM_VERSION=9.15.0` (concrete) and `ARG PLAYWRIGHT_VERSION=1.49.0` (concrete). Chromium revision implicit from Playwright pin per Playwright's own design. Both pins documented inline as load-bearing.
5. **Rebuild cadence**: keeping line-7 comment as-is for file-leanness — declined per operations' "OK to decline" framing.
6. **Rollback comment**: ADDING after the Chromium pre-cache layer:
   ```dockerfile
   # Rollback: rebuild from prior commit (no state migration; named volumes are toolchain-agnostic).
   ```
7. **GSA classification**: `infra/devloop/Dockerfile` not in ADR-0024 §6.4 GSA list. Classification = **Mine** (infrastructure-owned per ADR-0025), already in the Cross-Boundary Classification table. No `Approved-Cross-Boundary:` trailer needed.

### Reviewer questions
- **@security**: pulling the Chromium binary at build time via `npx playwright@1.49.0 install` (HTTPS, signed by Playwright's CDN). Acceptable, or do we want sha256 verification akin to the kubectl block (lines 50-56)? Note: Playwright doesn't publish per-version sha256 checksums for the browser blob, so equivalent verification would mean wrapping the install in a custom step.
- **@operations**: Image-size budget is ≤600 MB additional. I expect ~330 MB. If you want me to actually build locally to confirm before flipping the gate, say so — the validation pipeline doesn't, but I can.
- **@code-reviewer**: Splitting `ENV PLAYWRIGHT_BROWSERS_PATH=…` before Layer C and `ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` after — preference? Could be one combined `ENV` block at the top of the Playwright section; I lean separate so the build-time vs. runtime intent reads clearly.
- **@dry-reviewer**: No code duplication risk — Dockerfile is the only source of devloop-image config. ADR-0025 is the spec.
- **@test**: Confirming task #18 is the consumer of the pinned `PLAYWRIGHT_VERSION=1.49.0`. Task #18 will install Playwright as a `pnpm` devDep at the same version, and `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` will route it to `/opt/ms-playwright` from this image. Any concerns about that contract?
- **@observability**: No metric/log surface affected.

---

## Pre-Work

None.

---

## Implementation Summary

Extended `infra/devloop/Dockerfile` with the browser-client toolchain per R-35, applied between the existing Node 22 + Claude Code CLI layer (line 62) and the existing `mkdir -p /home/dev` block (now line 134). All upstream and downstream layers are unchanged, so the Rust toolchain, kubectl, gh, and Node cache layers stay valid for unrelated rebuilds.

Five new `RUN`/`ENV`/`ARG` constructs added, in the planned order:

1. **Section header comment** (lines 64-67) — image-size delta budget (≤600 MB, ~330 MB expected) + how-to-measure command.
2. **`ARG PNPM_VERSION=9.15.0` + Corepack `RUN`** (lines 70-72) — single `RUN` enabling Corepack and preparing pnpm 9.15.0 with `--activate`.
3. **Apt deps `RUN`** (lines 85-90) — story §139/§464 list verbatim plus `libatk1.0-0` + `libcups2` (Playwright `--with-deps` canonical additions for Debian bookworm), with `--no-install-recommends` and `rm -rf /var/lib/apt/lists/*` matching the existing apt cache-discipline pattern.
4. **`ENV PLAYWRIGHT_BROWSERS_PATH + PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD`** (lines 96-97) — combined ENV block, build-time/runtime intent in the comment above per @code-reviewer.
5. **`ARG PLAYWRIGHT_VERSION=1.49.0` + Chromium pre-cache `RUN` + chrome-binary smoke `RUN`** (lines 114-125) — `npx -y playwright@1.49.0 install chromium` (no `--with-deps`), then `chmod -R a+rX /opt/ms-playwright`, then a smoke RUN that resolves the Chromium binary path via `playwright install --dry-run chromium | awk '/Install location:/ {print $3}'`, asserts `test -x` on the binary, and runs `--version` to dlopen-check the shared libraries. Path resolution via `--dry-run` (rather than a glob) per @test's Gate-1 follow-up — invariant under future Playwright on-disk layout changes.

All comments per the Gate-1 review consensus (security trust-chain, test contract-version + smoke-rationale, operations rollback-note + path-layout TODO, code-reviewer build-vs-runtime ENV intent, dry-reviewer audit-list rationale).

**Did NOT change**: `WORKDIR /work` (line 146), `ENV HOME=/home/dev` (line 135), `ENTRYPOINT` (line 148), or any pre-existing layer. `entrypoint.sh`, `dev-cluster`, `devloop.sh` untouched per scope.

**Image size**: not measured locally (validation pipeline doesn't build container images per ADR-0024). Documented expected delta ~330 MB inline; opportunistic measurement deferred per @operations confirmation.

**Story tracking**: row 3 in `docs/user-stories/2026-05-02-browser-client-join.md` updated to "In Review" with the devloop-output path filled in.

---

## Files Modified

| Path | Lines | Classification |
|------|-------|----------------|
| `infra/devloop/Dockerfile` | +66 / -0 (new layers between line 62 and line 67 of pre-edit file) | Mine |
| `docs/devloop-outputs/2026-05-03-devloop-image-r35/main.md` | this file (loop output) | Mine |
| `docs/user-stories/2026-05-02-browser-client-join.md` | row 3 of Devloop Tracking only (Status + Devloop Output cells) | Mine (story-bookkeeping) |

### Key Changes by File

**`infra/devloop/Dockerfile`**
- New section header (lines 64-67): R-35 banner + image-size budget comment.
- Layer A — `ARG PNPM_VERSION=9.15.0` + `RUN corepack enable && corepack prepare pnpm@${PNPM_VERSION} --activate` (lines 70-72).
- Layer B — apt deps for Playwright Chromium (lines 74-90); story-list-verbatim with `libatk1.0-0`/`libcups2` additions documented inline.
- Combined ENV — `PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright` + `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` (lines 92-97).
- Layer C — `ARG PLAYWRIGHT_VERSION=1.49.0` + `RUN npx -y playwright@${PLAYWRIGHT_VERSION} install chromium && chmod -R a+rX /opt/ms-playwright` (lines 99-116).
- Build-time smoke (lines 118-125) — resolves Chromium binary path via `playwright install --dry-run` (no glob), asserts `test -x`, then `chrome --version` to dlopen-check shared libs. Per @test's post-approval refinement to address @operations' fragility concern.
- Rollback comment (lines 128-129).

**`docs/user-stories/2026-05-02-browser-client-join.md`**
- Devloop Tracking row 3: Status `Pending` → `In Review`; Devloop Output filled with `docs/devloop-outputs/2026-05-03-devloop-image-r35/main.md`. No other cells touched.

---

## Devloop Verification Steps

| Layer | Status | Notes |
|-------|--------|-------|
| 1. `cargo check --workspace` | PASS | No Rust changes in diff; ran clean. |
| 2. `cargo fmt --all -- --check` | PASS | No Rust changes; format check vacuous. |
| 3. `./scripts/guards/run-guards.sh` | PASS | 22/22 guards green (after iter-1 Cross-Boundary table fix). |
| 4. `./scripts/test.sh --workspace` | SKIPPED | Per ADR-0024 §line 135 — no Rust files in diff. |
| 5. `cargo clippy --workspace -- -D warnings` | SKIPPED | Per ADR-0024 §line 135. |
| 6. `cargo audit` | SKIPPED | Per ADR-0024 §line 135. (Note: 6 pre-existing wtransport-stack CVEs predate this devloop; Cargo.toml/lock unchanged.) |
| 7. semantic-guard agent | PASS — SAFE | Diff matches plan; no credentials, public version pins, HTTPS fetches with documented trust chain, least-privilege perms, no orphan TODOs. |
| 8. `dev-cluster rebuild-all` + env-tests | SKIPPED | Per ADR-0024 §line 135 — no service code changed; live-cluster binaries unchanged. |
| Artifact: `hadolint infra/devloop/Dockerfile` | 2 new warnings, deferred | Same class as 8 pre-existing accepted warnings on this Dockerfile. Decision and justification in §Tech Debt References below. |

Gate 2 took 2 iterations — iter 1 failed on `validate-cross-boundary-scope` (Cross-Boundary table cell carried a parenthetical qualifier the parser couldn't bind to a bare path). Iter 2 cleared after the implementer moved the qualifier into prose below the table.

---

## Code Review Results

| Reviewer | Verdict | Findings |
|----------|---------|----------|
| Security | CLEAR | 0 |
| Test | CLEAR | 0 |
| Observability | CLEAR | 0 |
| Code Quality | CLEAR | 0 (2 non-blocking observations recorded in reviewer's verdict, no iteration required) |
| DRY | CLEAR | 0 (no true duplication, no extraction opportunities flagged for `docs/TODO.md`) |
| Operations | CLEAR | 0 |

All reviewers verified: ADR-0024 §6.4 Guarded Shared Areas not touched; Cross-Boundary classification "Mine" is correct; no `Approved-Cross-Boundary:` trailers required. Hadolint deferral (recorded in §Tech Debt References) explicitly endorsed by code-reviewer as consistent with the file's established accepted-warning convention.

---

## Tech Debt References

### Hadolint warnings — DEFERRED (decision recorded at Gate 2 iter 2)

Two new hadolint warnings introduced by this devloop are accepted as consistent with the established `infra/devloop/Dockerfile` convention:

| Warning | Location | Class | Pre-existing instances on this image |
|---------|----------|-------|---------------------------------------|
| DL3008 — Pin versions in apt get install | line 85 (Playwright apt deps) | Same as 3 pre-existing unpinned `apt-get install` blocks (lines 14, 40, 59) | 3 |
| DL4006 — Set the SHELL option -o pipefail before RUN with a pipe | line 123 (smoke `awk` pipe) | Same as 3 pre-existing piped RUNs | 3 |

**Justification (defer)**:
- **DL3008**: Pinning bookworm package versions (`libnss3=2:3.87.1-1+deb12u1`, etc.) creates known pinning-rot maintenance churn — the image's line-7 comment specifies "Rebuild weekly or when toolchain/dependencies change," and bookworm security patches would force lockstep version updates without semantic gain. The image's existing `apt-get install` blocks all decline pinning by convention; introducing it for only the new layer would be inconsistent. If the project decides to pin globally, that's a separate broader-scope cleanup, not a R-35 task.
- **DL4006**: Adding `SHELL ["/bin/bash", "-o", "pipefail", "-c"]` would change shell semantics for ALL subsequent RUN steps in the file, which is a non-local refactor outside this devloop's scope. The local alternative `set -o pipefail` inside the RUN fights the `&&`-chain idiom used everywhere else and would be an idiomatic departure. The smoke step's pipe (`npx ... | awk`) is benign — failure of `npx` will be observed via the empty `BROWSER_DIR` triggering `test -x` failure on the next line.

**Total hadolint warnings on `infra/devloop/Dockerfile` after this devloop**: 10 (8 pre-existing + 2 new). All warnings remain in the same idiomatic class as the file's existing accepted convention.

If/when a future devloop standardizes hadolint compliance for the devloop image, these two warnings should be addressed alongside the 8 pre-existing ones in a single sweep — not piecemeal here.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `1194236`
2. Review all changes: `git diff 1194236..HEAD`
3. Soft reset (preserves changes): `git reset --soft 1194236`
4. Hard reset (clean revert): `git reset --hard 1194236`
5. After revert, rebuild devloop image to drop the new layers: `podman build -t darktower-dev:latest infra/devloop/`

---

## Issues Encountered & Resolutions

### Issue 1: Cross-Boundary table parser couldn't bind a parenthetical-qualified path
**Problem**: The Cross-Boundary Classification table used a Path cell of `` `docs/user-stories/2026-05-02-browser-client-join.md` (Devloop Tracking row 3 only) ``. The `validate-cross-boundary-scope` guard parses the entire cell content as the path, so the bare path that's actually in the diff (`docs/user-stories/2026-05-02-browser-client-join.md`) was reported as scope-drift-inbound, and the qualified path was simultaneously reported as scope-drift-planned-untouched. Failed Gate 2 iter 1.
**Resolution**: Moved the "(Devloop Tracking row 3 only)" qualifier into a sentence below the table; left the Path cell as a bare backtick-wrapped path. Guard cleared, Gate 2 iter 2 PASS.
**Lesson**: Path cells in the Cross-Boundary Classification table must be bare paths (verbatim against `git diff --name-only`); scope qualifiers belong in prose below the table.

### Issue 2: Hadolint introduces 2 new warnings consistent with the file's accepted-warning convention
**Problem**: `hadolint infra/devloop/Dockerfile` reported 10 warnings on the post-edit file vs. 8 baseline — 2 new (DL3008 unpinned-apt at line 85, DL4006 missing-pipefail at line 123). The image's existing `apt-get install` blocks all decline pinning, and the file has 3 pre-existing piped RUNs in the same DL4006 class.
**Resolution**: Implementer chose **defer** with a justification recorded in §Tech Debt References — both new warnings are same-class as established convention; pinning bookworm packages creates pinning-rot churn, and a SHELL-level pipefail change would alter shell semantics for all subsequent RUN steps (non-local refactor outside scope). Code-reviewer at Gate 3 explicitly endorsed the deferral.
**Lesson**: When introducing a new same-class warning to a file with an established accepted-warning convention, defer with a justification is appropriate; piecemeal fixes drift from the file's idiom.

---

## Lessons Learned

1. **ADR-0024 §line 135 gives Layer 4-6 + Layer 8 skip authority for non-Rust diffs.** This devloop benefited directly: cargo-audit reports 6 pre-existing upstream-wtransport CVEs that aren't actionable in a Dockerfile-only change. Strict pipeline reading would have blocked merge for unrelated background; the §135 carve-out is the right escape hatch.
2. **Build-time smoke checks pay for themselves.** The `chrome --version` smoke at Dockerfile:118-125 catches missing apt deps at image-build time, where the failure is fast and diagnostic, instead of as cryptic test-runner errors hours later. Implementer also adopted `playwright install --dry-run | awk` for path resolution (vs. a glob), aligning with Playwright's documented tooling-contract output.
3. **Lockstep version contracts deserve inline call-outs.** `PLAYWRIGHT_VERSION=1.49.0` is a contract with `packages/web-app` `playwright` devDep (story task #18). The inline comment at Dockerfile:99-103 makes this dependency visible to anyone bumping the image — better than burying the contract in a README.
4. **Cross-Boundary table cells must be bare paths.** Surfaced via Gate 2 iter 1 failure — worth memorizing: scope qualifiers go in prose, not in cells.

---

## Appendix: Verification Commands

```bash
# Layers 1-2 (Rust): no Rust changes — likely NO-OP, but the pipeline runs them anyway.
cargo check --workspace
cargo fmt --all --check

# Layer 3: simple guards (incl. hadolint via run-guards.sh detection of Dockerfile changes)
./scripts/guards/run-guards.sh

# Layer 4-5: tests
./scripts/test.sh --workspace

# Layer 6: clippy
cargo clippy --workspace -- -D warnings

# Layer 7: semantic guards
# Spawned via semantic-guard agent.

# Layer 8: env-tests (runs against live Kind cluster)
dev-cluster rebuild-all
cargo test -p env-tests --features all

# Artifact-specific (mandatory because Dockerfile is in changeset):
hadolint infra/devloop/Dockerfile

# Image build smoke (not part of the pipeline; documented for reproducibility):
podman build -t darktower-dev:r35-smoke infra/devloop/
podman image inspect darktower-dev:r35-smoke --format '{{.Size}}'
```
