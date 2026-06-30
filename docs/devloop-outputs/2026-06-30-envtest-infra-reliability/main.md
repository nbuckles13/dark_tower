# Devloop Output: Env-test cluster build reliability (.dockerignore + cleanup GC + disk guard)

**Date**: 2026-06-30
**Task**: Make `dev-cluster setup` / `layer7.sh` cluster rebuilds reliably succeed by eliminating the 27 GB build-context disk exhaustion, reclaiming accumulated image/cache cruft, and adding a fast-fail disk precondition.
**Specialist**: infrastructure (paired-with: operations)
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-57`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5b9bb307f6c12f3b25daaa920b1801582d4bf868` |
| Branch | `feature/browser-client-join-task-57` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `COMPLETE (committed d54143c via documented Gate-2 bypass; Prometheus gate → TODO item D)` |
| Implementer | `infra-implementer` (infrastructure) |
| Iteration | `1` |
| Security | `plan-confirmed` |
| Test | `plan-confirmed` |
| Observability | `plan-confirmed` |
| Code Quality | `plan-confirmed` |
| DRY | `plan-confirmed` |
| Operations (paired) | `plan-confirmed` |
| Semantic Guard | `plan-confirmed` |

### Gate 1 — Plan Approval (2026-06-30): all 7 confirmed

One revision round. Folds: observability ×2 (df-failure WARN; §6.7 "grep the relayed
stderr" note), DRY+code-reviewer (the `check_build_disk_space "$CONTAINER_CMD"`
arg-pass — fixes a real divergent-default graphroot-mismatch bug, flagged independently
by both), code-reviewer (`|| true` set-e hardening on the `avail_gb` command-sub),
test (forced-trip `scripts/setup.test.sh` for the `insufficient-disk` contract token +
`BASH_SOURCE==$0` guard on `setup.sh:918` + `layer3.sh` wiring), operations C1–C4
(`insufficient-disk` as a relayed SUB-CAUSE under `cluster-setup-failed`/`cluster-rebuild-failed`
not a peer §6.7 row; §4 third-emitter note; honest "per-run rebuild-all is unguarded"
framing + §0.5/`docs/TODO.md` follow-up; runbook row relabeled Domain-judgment/Owner=operations)
+ 3 minors (floor-not-guarantee wording; rootful graphroot fallback; podman-only cleanup comment).
Classification-sanity guard GREEN on the final 8-file table.

**Gate-3 carry-forward:** operations is the named owner-reviewer for
`docs/runbooks/devloop-validation.md` (Domain-judgment, satisfied via `--paired-with=operations`)
— it co-signs the §6.7 sub-cause + §4 third-emitter wording at Gate 3 (Ownership Lens).
Self-validation: Gate-2 Layer 7 must rebuild the cluster with the new `.dockerignore`
(proving the keystone) AND the `setup.test.sh` forced-trip must pass under Layer 3.

---

## Task Overview

### Origin
Blocking finding from the 2026-06-29 GC-telemetry-env-test devloop's Gate 2 (see
`docs/devloop-outputs/2026-06-29-gc-telemetry-env-tests/main.md` §Gate 2, and
`docs/TODO.md` §"Devloop Container Resource Hygiene & Build Isolation"). The
env-test cluster rebuild (`layer7.sh` → `dev-cluster setup` → `setup.sh`) failed
3/3, all rooted in **disk exhaustion**: the service Dockerfiles `COPY . .` with no
`.dockerignore`, copying the **27 GB host `target/`** into each of 4 image builds.
Confirmed root cause (host `layer-7.stderr.log`):
`processing tar file(write /build/target/debug/deps/...: no space left on device)`
at the GC builder-stage `COPY . .`. The telemetry devloop is PAUSED pending this fix.

### Scope (3 changes; per-slug tagging deferred — see below)
1. **`.dockerignore` (keystone)** — NEW at repo root, excluding `target/`, `.git/`,
   `node_modules/`, `worktrees/`, and other build-irrelevant trees. Shrinks every
   `COPY . .` from ~27 GB to ~MB. **Provably safe**: the Dockerfiles are cargo-chef
   multi-stage (`cargo chef cook` builds deps; the final `cargo build` uses the
   container's own `/build/target`) — the host `target/` is never read. Affects only
   the Docker build context, not host cargo (Layers 1–6 untouched). The portable
   filename is `.dockerignore` (honored by both podman and docker; podman also reads
   `.containerignore`). Implementer confirms which the build path honors.
2. **`cleanup()` GC** — `infra/devloop/devloop.sh:cleanup()` (`:126-167`) tears down
   containers + network + Kind cluster + clone but never reclaims the host service
   images / build cache it produced (the ~2 TB leak). Add reclaim after the Kind
   delete. Conservative default `podman image prune -f` + `podman builder prune -f`
   (dangling/unused only — cross-slug-safe; never `-af`, which could yank a parallel
   slug's images while per-slug tagging (B) is not yet in place).
3. **Pre-build disk guard** — a Phase-1 precondition (in `layer7.sh`, and/or at the
   top of `setup.sh`) that checks free space on the container-storage filesystem and
   emits `PRECONDITION_FAILURE REASON=insufficient-disk` (with a prune remediation
   hint) BEFORE a doomed multi-minute rebuild, rather than dying mid-`COPY`. Runbook
   `docs/runbooks/devloop-validation.md:192` already anticipates this token.

### Deferred — NOT in this devloop
**Per-slug image tags (finding B, the build/load concurrency correctness race)** —
`localhost/{ac,gc,mc,mh}-service:latest` is shared across slugs (no lock), so
concurrent same-host devloops can load the wrong code into a cluster. This is a
build-isolation *contract* change (touches `setup.sh` build+load, the kind kustomize
overlay, the service manifests, and `cleanup()`), warranting its own **ADR + devloop**.
It is NOT what blocks the telemetry devloop. Tracked in `docs/TODO.md` §"Devloop
Container Resource Hygiene & Build Isolation" item B.

### Debate Decision
NOT NEEDED for these 3 — standard, low-risk, no contract change. (The deferred
per-slug tagging DOES warrant a debate/ADR; out of scope here.)

---

## Cross-Boundary Classification

<!-- Implementer fills/finalizes at planning; reviewers verify at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.dockerignore` (NEW) | Mine | — (infrastructure owns the container build) |
| `.gitignore` (MODIFY: un-ignore `.dockerignore` — line 78 — so the committed file reaches the `git clone --local` build context) | Mine | — |
| `infra/devloop/devloop.sh` (MODIFY: `cleanup()` dangling-only prune GC, after the Kind delete ~:153) | Mine | — |
| `infra/kind/scripts/setup.sh` (MODIFY: disk precondition in build_image + sourceable-guard; see Planning) | Mine | — |
| `scripts/setup.test.sh` (NEW: forced-trip self-test for the disk-guard `insufficient-disk` contract token) | Mine | — |
| `scripts/layer3.sh` (MODIFY: one `run_and_emit` line wiring the new self-test into the Layer-3 meta-runner, :28-38) | Mine | — |
| `docs/runbooks/devloop-validation.md` (MODIFY: `insufficient-disk` sub-cause in the `cluster-setup-failed` + `cluster-rebuild-failed` §6.7 Fix columns; §4 third-emitter note) | **Not mine — Domain-judgment** | operations (Owner: Operations Team per ADR-0024) — satisfied via `--paired-with=operations`; editing operator-lane triage content (REASON tokens, fix steps, lane mapping) is a cross-boundary domain edit, not "Mine". Runbook is not GSA; guard stays green. |
| `docs/TODO.md` (MODIFY: append C3 follow-up — gate `cmd_rebuild` per-run build path on disk — under §Devloop Container Resource Hygiene) | Mine | — (infra tech-debt ledger entry) |
| `scripts/layer-all.test.sh` (MODE 100644→100755) | Mine — Mechanical | — (test-runner meta-test execute-bit; pre-existing base-commit `f34f3a6` bug — committed non-executable while all sibling `*.test.sh` are 100755, so the Layer-3 meta-test `Permission denied`s. Squarely this devloop's test-runner-reliability theme; infra Layer 3 needs it executable to run. Added at impl-time — see §Gate-2 note. Supersedes the telemetry devloop's orphan 5th-row claim on this file.) |

(No Guarded Shared Areas — no `proto/**`, `crates/common/src/jwt.rs`, `migrations/**`. Additions beyond the scaffolded three: `.gitignore` (keystone prereq — §0); `docs/TODO.md` (C3 follow-up — §0.5); the self-test pair `scripts/setup.test.sh` + the `scripts/layer3.sh` wiring line (@infra-test fix-it — §Tests). The runbook row is **Domain-judgment / operations** (C4 relabel — ledger accuracy per ADR-0024 §6.2; routing already correct via the paired-with). Everything else `Mine`/infrastructure.)

**Gate-2 coexistence note (2026-06-30, Lead).** This devloop runs in the same working tree as the PAUSED 2026-06-29 GC-telemetry devloop, whose WIP coexists. To give infra Gate 2 a clean single-devloop changeset, the Lead stashes the telemetry WIP (its 4 deliverable files + untracked `31_gc_telemetry.rs` + the 2026-06-29 output dir) for the duration of infra validation, then restores it to resume telemetry. The 9th row (`scripts/layer-all.test.sh` mode-fix) was MOVED here from the telemetry devloop's 5th-row claim: it's a pre-existing base-commit bug, thematically this devloop's (test-runner reliability), and infra Layer 3 needs it executable — so committing it here (first to land) is correct, and telemetry drops the row on resume. Reviewers see the 9th row in the Gate-2 diff (Mechanical, mode-only).

---

## Planning

### Topology grounding (why the placements below are correct)

- **Build CONTEXT is the repo root.** `setup.sh:build_image` (`:288`) runs `${CONTAINER_CMD} build -t … -f <Dockerfile> "${PROJECT_ROOT}"`, and the helper's `cmd_rebuild` (`crates/devloop-helper/src/commands.rs:1094-1100`) runs `<runtime> build -t … -f <Dockerfile> <ctx.project_root>`. Both pass the **repo root** as the context. So a repo-root `.dockerignore` governs every service build.
- **The four Dockerfiles each `COPY . .` twice** (planner stage + builder stage) with no `.dockerignore`: `gc:39,56` / `mc:39,56` / `mh:39,56` / `ac:38,55`. With the 27 GB host `target/` in context, that is what writes `…no space left on device` mid-COPY (confirmed root cause).
- **`podman build` runs on the HOST, not in the dev container.** The host-side helper (ADR-0030) executes both `setup.sh` (`commands.rs:706`) and the direct `cmd_rebuild` build (`commands.rs:1094`). `scripts/layer7.sh` runs **inside the dev container** and only talks to the helper over a unix socket — it has **no view of the host container-storage filesystem**. ⇒ The disk guard must be **host-side (`setup.sh`)**; a layer7 `df`/`podman info` would measure the wrong filesystem. This is why the guard is NOT in `layer7.sh` Phase-1.
- **cargo-chef never reads the host `target/`.** Builder stage runs `cargo chef cook` then `cargo build --release` into the container's own `/build/target` (`gc:53,59`). The host/clone `target/` is dead weight in the context — excluding it is provably safe.

### §0 — KEYSTONE PREREQUISITE: `.gitignore` un-ignore (mechanically required)

`.gitignore:78` (under `# Docker`) currently ignores `.dockerignore`, and the file is untracked (`git check-ignore` confirms; `git ls-files` shows it absent). The devloop builds from a `git clone --local` (committed files only — `devloop.sh:380-383`). **A gitignored, uncommitted `.dockerignore` would never reach the clone's build context, so the keystone would silently do nothing.** Fix: delete the `# Docker` + `.dockerignore` lines from `.gitignore` and commit the new `.dockerignore`. (Note: `Cargo.lock` is also gitignored+untracked — out of scope; cargo regenerates it in-container, and `.dockerignore` does not affect it.)

### §0.5 — `docs/TODO.md` follow-up: gate the per-run `rebuild-all` build path (C3)

Append a follow-up entry under `docs/TODO.md` §"Devloop Container Resource Hygiene & Build Isolation" (alongside findings A/B): **"Disk guard does NOT cover the steady-state per-run `rebuild-all` path — `cmd_rebuild` (`crates/devloop-helper/src/commands.rs:1084`) builds via a direct `podman build` not routed through `setup.sh`. Add a host-side disk precondition inside `cmd_rebuild` before its build (Rust). Bundle with finding B (per-slug tagging) since both edit `cmd_rebuild`, or track as its own item."** This ensures the C3 coverage gap is a tracked ledger item, not an inline aside that rots. (@infra-operations C3.)

### §1 — `.dockerignore` (NEW, repo root) — the keystone

**Filename choice:** a single repo-root `.dockerignore`. docker reads `.dockerignore`; podman/buildah read `.containerignore` *first* then fall back to `.dockerignore`. One `.dockerignore` therefore covers both engines (the runtime is podman-or-docker per the `KIND_EXPERIMENTAL_PROVIDER` detection in `setup.sh:281-285` / the helper's `ctx.container_runtime`). Adding `.containerignore` too would force podman onto a second file we'd have to keep in sync — rejected. Engines read the ignore file from the **context root** (= repo root here), not next to the `-f` Dockerfile, so repo-root placement is correct.

**MUST remain in context (verified build inputs):** `Cargo.toml`, `Cargo.lock` (if present), `crates/**` (all 13 workspace members + each `build.rs`), `proto/**` (`crates/proto-gen/build.rs:18-20` compiles `../../proto/…`), `migrations/**` (runtime stage `COPY --from=builder /build/migrations`, e.g. `gc:81`), `.cargo/`, `.sqlx/`. None of these are excluded below.

**Excluded trees (build-irrelevant; rationale per group):**
| Pattern(s) | Why safe to exclude |
|---|---|
| `target/`, `**/target/` | The 27 GB keystone; cargo-chef compiles fresh in-container. `**/target/` also drops the excluded `crates/*/fuzz` target dirs. |
| `.git/` | 47 MB VCS history; never a build input. |
| `node_modules/`, `**/node_modules/`, `.pnpm-store/`, `**/.pnpm-store/`, `.nx/`, `packages/`, `dist/`, `build/`, `out/` | JS/TS toolchain (353 MB `node_modules`) + TS workspace (`packages/`, not a Cargo member) + JS build outputs. The Rust services read none of these. |
| `coverage/`, `*.profraw`, `*.profdata`, `lcov.info` | Coverage/profiling artifacts. |
| `docs/`, `.terraform/`, `*.tfstate*`, `.idea/`, `.vscode/`, `*.swp`, `.DS_Store` | Docs, IaC state, editor/OS cruft — not build inputs. |
| `*.log`, `logs/` | Logs. |
| `.env`, `.env.*` | Local env/secrets — security: don't ship them into the build context. |
| `worktrees/` | Sibling clones live at `${REPO_ROOT}/../worktrees` (outside this context) — listed defensively; a no-op in-context. |

No negation (`!`) rules are needed: every excluded pattern is disjoint from every required path (e.g. `target/` excluded, `crates/` kept). The file gets a header comment explaining the cargo-chef safety argument and the dual-engine filename rationale.

### §2 — `cleanup()` GC (`infra/devloop/devloop.sh`)

Insertion point: **after the Kind-cluster delete (`:150-153`), before `rm -rf "$HELPER_RUNTIME_DIR"` (`:155`)**. `cleanup()` runs on the host (devloop.sh is the host wrapper), so the prune targets the host storage where the leak lives. It fires only on explicit destroy (`--destroy` / menu `d`), which is the right teardown moment.

```bash
    # Reclaim the host build cruft this devloop produced — the service images and
    # build cache nothing else GCs (the multi-hundred-GB leak that exhausts
    # container storage over time). CONSERVATIVE, dangling/unused-ONLY prune — NOT
    # -af: per-slug image tagging is DEFERRED (docs/TODO.md finding B), so the
    # service images are still the shared `localhost/{ac,gc,mc,mh}-service:latest`
    # tags. `-af` here would yank a *parallel* slug's :latest images mid-run.
    # Dangling-only is cross-slug-safe: orphaned layers + unused build cache only,
    # never a tagged image a concurrent devloop still references. Revisit (-> -af /
    # per-slug filtered prune) once finding B lands per-slug tags.
    # podman-only by design — consistent with every other container op in this script
    # (devloop.sh assumes podman throughout). On a docker-built host
    # (KIND_EXPERIMENTAL_PROVIDER=docker) these images wouldn't be reclaimed here;
    # acceptable given the podman-primary assumption (@infra-operations minor).
    if command -v podman &>/dev/null; then
        echo "Reclaiming dangling images + unused build cache..."
        podman image prune -f 2>/dev/null || true
        podman builder prune -f 2>/dev/null || true
    fi
```

### §3 — Disk precondition guard (`infra/kind/scripts/setup.sh`)

**Placement: a run-once `check_build_disk_space "$CONTAINER_CMD"` called at the top of `build_image()` (`:278`), AFTER build_image's existing `CONTAINER_CMD` resolution (`:280-285`).** This is the single chokepoint that fires before the *first* `podman build` on every setup.sh build entrypoint (full `main()` → `deploy_*_service`, and the `--only` path → `deploy_only_service`), and is naturally skipped under `--skip-build` (build_image isn't called). It runs host-side where `podman build` writes, so it measures the correct filesystem. The guard takes the **already-resolved runtime as `$1`** — it does NOT re-derive it (see correctness note below).

```bash
_DT_DISK_CHECKED=false
# Fail fast BEFORE the first cold image build if the host container-storage
# filesystem lacks headroom for a 4-service release build — rather than dying
# mid-COPY with "no space left on device" after several minutes. Run-once.
# $1 = the resolved container runtime (build_image's $CONTAINER_CMD). MUST be
# passed, NOT re-derived: build_image defaults to `docker` when
# KIND_EXPERIMENTAL_PROVIDER is unset, so a re-derived `podman` default here would
# `df` the WRONG runtime's graphroot on an unset-provider host — measuring the
# wrong filesystem exactly when the guard matters. @infra-dry-reviewer Gate-1 fix.
check_build_disk_space() {
    local cmd="$1"
    [[ "${_DT_DISK_CHECKED}" == "true" ]] && return 0
    _DT_DISK_CHECKED=true
    # ${min_gb} is a fast-fail FLOOR, NOT a success guarantee: a fully-cold build cache
    # can exceed it and still pass-then-die, so a pass means "not obviously doomed", not
    # "will succeed". (@infra-operations Gate-1 minor.)
    local min_gb="${DEVLOOP_MIN_DISK_GB:-15}" graphroot avail_gb
    # ${cmd}'s container-storage root is where build layers land; fall back to the
    # rootless default, then the rootful default, then PROJECT_ROOT's fs (always resolvable).
    graphroot="$(${cmd} info --format '{{.Store.GraphRoot}}' 2>/dev/null || true)"
    [[ -n "${graphroot}" && -d "${graphroot}" ]] || graphroot="${HOME}/.local/share/containers/storage"
    [[ -d "${graphroot}" ]] || graphroot="/var/lib/containers/storage"
    [[ -d "${graphroot}" ]] || graphroot="${PROJECT_ROOT}"
    # `|| true`: setup.sh is `set -euo pipefail`, so a non-zero df under pipefail would
    # abort the script on this bare assignment — match the `graphroot=…|| true` line above.
    # The downstream `[[ -n … ]] && (( … ))` guard is already set-e-safe (if-condition).
    avail_gb="$(df -Pk "${graphroot}" 2>/dev/null | awk 'NR==2{printf "%d", $4/1024/1024}' || true)"
    if [[ -z "${avail_gb}" ]]; then
        # Unmeasurable (df/podman info failed). Fail OPEN — never false-positive-block a
        # healthy build — but say so, so the case stays self-attributing instead of
        # silently regressing to the old buried mid-COPY error (@infra-observability /
        # @infra-semantic-guard Gate-1 advisory). The build's own no-space error is the backstop.
        echo "WARN: could not determine free space on ${graphroot}; skipping disk precondition (the build's own 'no space left on device' error remains the backstop)." >&2
        return 0
    fi
    if (( avail_gb < min_gb )); then
        echo "PRECONDITION_FAILURE: container-storage filesystem (${graphroot}) has ${avail_gb}GB free, below the ${min_gb}GB floor for a 4-service image build — a cold build would die mid-COPY ('no space left on device'). REASON=insufficient-disk" >&2
        echo "  Fix: reclaim space — 'podman image prune -f && podman builder prune -f' (add -af only if no parallel devloops are running). See docs/runbooks/devloop-validation.md §6.7." >&2
        exit 2
    fi
}
```

**Gate-1 advisory folded in (non-blocking):** the `df`/`podman info`-unmeasurable branch fails OPEN (right call — don't block healthy builds) but now emits a one-line `WARN: …` to stderr so the unmeasurable case is self-attributing rather than a silent regression to the buried mid-COPY error. Closes @infra-semantic-guard's fallback note.

**Threshold = 15 GB (override `DEVLOOP_MIN_DISK_GB`).** Reasoning: post-`.dockerignore` the context is ~MB, but each of the 4 services still materializes a release builder layer (LTO, full workspace deps) on the order of a few GB, plus the cargo-registry/base-image layers — a cold 4-service build comfortably needs low-double-digit GB of headroom. 15 GB fails fast on a near-full disk without false-positiving a healthy incremental rebuild (which reuses cached layers). It is a **fast-fail FLOOR, not a success guarantee** (@infra-operations minor): a fully-cold cache can exceed 15 GB and still pass-then-die, so a pass means "not obviously doomed", not "will succeed" — the §6.7 text + the in-code comment say so explicitly. Pragmatic, env-overridable.

**Filesystem detection:** `${cmd} info --format '{{.Store.GraphRoot}}'` is the authoritative graphroot; fall back to the rootless default `~/.local/share/containers/storage`, then the rootful default `/var/lib/containers/storage` (@infra-operations minor — otherwise a rootful host with a failed `podman info` would measure PROJECT_ROOT's fs), then to `PROJECT_ROOT` (so detection failure degrades to a still-meaningful `df`, never a crash). `df -Pk` is POSIX-portable.

**Exit-code contract & runbook wiring (C1/C2 — `insufficient-disk` is NOT a first-class layer7 enum):** `exit 2` from setup.sh → the helper's `run_command_streaming` returns error → `dev-cluster setup` non-zero → `layer7.sh` Phase-1a `__dev_cluster_setup` maps it to `precondition_fail cluster-setup-failed`. layer7's STATUS enum is therefore `cluster-setup-failed` (or `cluster-rebuild-failed` on the 1b/1c path); **`insufficient-disk` is a RELAYED setup.sh banner, not a layer7 STATUS token.** So per @infra-operations C1 it does **NOT** get a peer §6.7 row — instead the runbook edit adds it as a **SUB-CAUSE in the Fix column of BOTH the existing `cluster-setup-failed` and `cluster-rebuild-failed` rows**: *"if the relayed setup.sh stderr (`${DEVLOOP_TMP:-/tmp/devloop}/layer-7.stderr.log`) carries `PRECONDITION_FAILURE: … REASON=insufficient-disk`, the root cause is host disk exhaustion — reclaim per §4."* This keeps the ADR-0033 wrapper-contract honest (layer7 owns its enum set; setup.sh's token is a relayed sub-cause). Per C2, the §4 two-token convention gets a one-line note that **`setup.sh` is now a THIRD `PRECONDITION_FAILURE:` emitter** (host-side, relayed through the helper into layer7's stderr) alongside `layer-all.sh` + `layer7.sh`; banner stays line-anchored `^PRECONDITION_FAILURE:`. **Path confirmed:** the relayed banner lands in `${DEVLOOP_TMP}/layer-7.stderr.log` (`layer-all.sh:110` redirects each layer's stderr to `layer-${n}.stderr.log`; matches the user's host paste of `layer-7.stderr.log` showing `=== setup FAILED ===` + the no-space error) — NOT `layer-7-env-test.log` (that is only the Phase-2 suite tee).

**Coverage scope — corrected framing (C3):** `layer7.sh` invokes `dev-cluster rebuild-all` on **every** run (Phase 1c, `layer7.sh:340`), and `rebuild-all` → `cmd_rebuild` (`commands.rs:1084`) builds via a **direct `podman build`** that does NOT route through `setup.sh`. So `cmd_rebuild` is the **steady-state per-run** build path, not a rare standalone command. Consequence, stated honestly: **the disk guard fires ONLY on the cold `dev-cluster setup` path (Phase 1a — first cluster bring-up / eager-setup); the steady-state per-run `rebuild-all` is UNGUARDED by the fast-fail**, and is covered only by the **`.dockerignore` keystone** (which `cmd_rebuild` honors via the same `ctx.project_root` context — it removes the 27 GB×4 blow-up entirely, the actual fix for every path) **plus the `cleanup()` prune GC**. A fast-fail disk check on the per-run path would have to live inside `cmd_rebuild` (Rust helper — the same surface as deferred finding B / per-slug tagging), so it is out of scope here and **tracked as an explicit `docs/TODO.md` follow-up entry** (gate `cmd_rebuild` on disk before its `podman build`), not just an inline aside — see §0.5.

### Tests / self-validation

**Disk-guard TRIP-branch coverage (@infra-test Gate-1 fix-it — the guard's contract token is its reason to exist; Gate-2's rebuild only exercises the PASS path, so the trip branch needs an explicit test):**
1. **Sourceability:** add a `BASH_SOURCE[0]==$0` guard around the bottom `main "$@"` (`setup.sh:918`) so the script can be sourced without running `main` — the same pattern `layer7.sh:423` uses and which the existing self-tests rely on. (setup.sh's top-level code — cluster-name validation, optional `DT_PORT_MAP`/`DT_HOST_GATEWAY_IP` validation, the `KUBECTL=` string assignment — is side-effect-free to source with the defaults; only `main` must be gated.)
2. **`scripts/setup.test.sh` (NEW)** — forced-trip case with REAL inputs (ADR-0034): source setup.sh (guard suppresses `main`), then in a subshell (to isolate the `exit 2`) call `check_build_disk_space podman` with `DEVLOOP_MIN_DISK_GB=999999999` → real `df` on the real graphroot (or its PROJECT_ROOT fallback) → comparison trips → assert **exit 2** AND a greppable **`^PRECONDITION_FAILURE:.*REASON=insufficient-disk`** on stderr. One case exercises override-read + graphroot-fallback + df + arithmetic + comparison + banner + exit (the whole contract). Robust even where `podman` is absent: the fallback chain ends at `PROJECT_ROOT` (always `df`-able), so `avail_gb` is always populated and the huge threshold always trips.
3. **Wire into the Layer-3 meta-runner:** `scripts/layer3.sh` has NO `*.test.sh` auto-discovery (`:26` comment) — each self-test is explicitly `run_and_emit`-wired (`:28-38`). Add one line alongside `layer7-selftest`: `run_and_emit "setup-disk-guard-selftest" "${__here}/setup.test.sh" || true`.

**Untouched suites stay green:** I am NOT touching `layer7.sh`, so `scripts/layer7.test.sh` + `scripts/layer-all.test.sh` assertions are unaffected. I will run the full Layer-3 self-test set to confirm (including the new one).

**Keystone self-validation:** the `.dockerignore` fix is exercised by this devloop's own Gate-2 Layer 7 — `rebuild-all` builds all four images with the tiny context, proving the fix and confirming the rebuild no longer exhausts disk.

---

## Validation / Review / Deferrals

### Gate 2 — Validation (2026-06-30)

**Layers 1–6: GREEN** (two runs, `gate2-verdict` RUN_AT 05:48 + 06:17). L1 compile, L2
fmt, **L3 guards GREEN incl. the new `setup.test.sh` forced-trip disk-guard self-test
(`setup-disk-guard-selftest`) + the now-executable `layer-all.test.sh` meta-test** +
cross-boundary classification/scope on the 9-file table, L4 3065 unit tests / 0 fail,
L5 clippy, L6 audit.

**KEYSTONE VALIDATED.** The `.dockerignore` fix works: the cluster rebuild now
**succeeds** (2/2 runs, ~18 min, no disk exhaustion — the prior `cluster-rebuild-failed`
"no space left on device" mode is GONE). Layer 7 got *past* the rebuild to the Phase-1
readiness gate.

**Layer 7 BLOCKED by a SEPARATE, pre-existing env-test-infra bug (NOT this diff, NOT disk).**
`STATUS=PRECONDITION_FAILURE REASON=observability-prometheus-not-ready` — `layer7.sh`'s
Phase-1f Prometheus `/-/ready` gate fails deterministically *in-run* (2/2, even with
`DEVLOOP_HEALTH_BUDGET=900`), although Prometheus is stably `/-/ready`→200 from the
container right after, with a correct probe (`curl -fsS … 20300/-/ready`) and a correct
ports.json URL. Root cause UNKNOWN — needs in-run host observation (pod state /
container→NodePort routability at poll time) unavailable from the sandbox. The disk fix
*uncovered* this: `layer7.sh`'s real-Prometheus poll path was never exercised before
(`layer7.test.sh` uses fake `dev-cluster` stubs; the disk failures always died before
Phase-1f). It blocks `GATE2=PASS` for BOTH this devloop and the paused telemetry devloop
(same Phase-1f gates the suite). Tracked: `docs/TODO.md` §Devloop Container Resource
Hygiene → "Phase-1f Prometheus readiness gate".

**DECISION (user, 2026-06-30):** commit the proven fixes now (disk keystone + cleanup GC
+ disk guard + setup.test.sh — all validated), via a documented **Gate-2-verdict-gate
bypass** (`git commit --no-verify`), since the FAIL is an unrelated operator-lane env
transient and Layers 1–6 are green. Then investigate the Prometheus readiness gate as a
separate effort. Bypass justification recorded in the commit message.

### Gate 3 — Review (diff review of the 9-file changeset): all 7 cleared

| Reviewer | Verdict |
|----------|---------|
| Security | CLEAR (`.dockerignore` keeps all build inputs; `.env.test` key removed from layers; prune injection-safe) |
| Test | RESOLVED-FIXED (forced-trip `setup.test.sh` landed as specified; no self-test regression; `layer-all.test.sh` mode-only) |
| Observability | CLEAR (df-WARN + §6.7 grep note folded; no obs regression; test locks the `insufficient-disk` contract) |
| Code Quality | CLEAR (ADR-0033/0030/0025 PASS; arg-pass + set-e fixes confirmed; keep-list re-verified) |
| DRY | CLEAR (arg-pass fix landed; prune complements `rmi`; extraction-opportunity appended to TODO §DRY) |
| Operations (paired) | CLEAR + **runbook co-signed** (`Approved-Cross-Boundary: operations`); C1–C4 + minors confirmed |
| Semantic Guard | CLEAR (best-effort prune masking appropriate; disk guard fails loud; no credential leak) |

Zero deferred, zero escalated. Committed `d54143c` (`--no-verify`, documented Gate-2-verdict-gate
bypass — the Layer-7 FAIL is the unrelated Phase-1f Prometheus gate, TODO item D, not the disk fix).

### Outcome
**KEYSTONE PROVEN + LANDED.** The disk exhaustion that blocked the env-test cluster rebuild is
fixed: rebuilds now succeed (no `cluster-rebuild-failed`). `cleanup()` GC + the pre-build disk
guard + the `setup.test.sh` contract lock landed alongside. Per-slug image tags (B), the per-run
`cmd_rebuild` guard gap (C-followup), and the **Phase-1f Prometheus readiness gate (D — the next
blocker for both this and the telemetry devloop)** are tracked in `docs/TODO.md`.
