# Devloop Output: Dependabot hygiene — consolidate PRs #60–64, #68, #69 + config grouping

**Date**: 2026-10-03
**Task**: Consolidate Dependabot PRs #60–64, #68, #69; regroup dependabot.yml; pnpm + workspace-dep SSoT; diagnose npm Dependabot job and Scheduled Audit failures (step 1 of 3)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/1-dependabot-hygiene`
**Duration**: ~14h wall-clock (2026-10-03 03:28–17:30 UTC, including a host-sleep gap ~05:30–16:13)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `cf0fcd5c2d389af514de98604dab18bf03c9d819` |
| Branch | `feature/1-dependabot-hygiene` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (infrastructure)` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Paired (GSA owner) | `paired-auth-controller` — base64 call sites in ac-service/src/crypto, common/src/jwt.rs (ADR-0024 §6.4) |
| Semantic Guard | `not spawned — diff is CI/dependency config, no checks.md surface` |

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
Consolidate Dependabot PRs #60–64, #68, #69; regroup `.github/dependabot.yml` so coupled crates/packages arrive together; make `package.json` `packageManager` the single pnpm-version source and `[workspace.dependencies]` the single Rust-dependency-version source (with a guard); diagnose the failing npm Dependabot job and the failing Scheduled Audit (step 1 of 3).

### Scope
- **Service(s)**: all four (Cargo.toml dependency declarations only; any metrics-util/base64 API churn in test code)
- **Schema**: No
- **Cross-cutting**: Yes (CI workflows, workspace manifests, devloop image)

### Debate Decision
NOT NEEDED - dependency and CI-config hygiene within existing ADR-0033/0034 machinery; no architectural choice.

---

## Cross-Boundary Classification

> Post-implementation: four conditional rows ("only if API churn" / "if needed") were removed because they were not needed. There was **no** source churn in `crates/ac-service/src/crypto/**`, any other ac/gc/mc/mh `src/`, or GSA code; the bumps compiled unchanged, and `crates/common/src/jwt.rs` gained only tests.

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
| `.github/dependabot.yml` | Mine | — |
| `.github/workflows/ci.yml` | Mine | — |
| `.github/workflows/ci-client.yml` | Mine | — |
| `.github/workflows/audit-scheduled.yml` | Mine | — |
| `.github/workflows/fuzz-nightly.yml` | Mine | — |
| `package.json` (`engines.node`, `packageManager` → pnpm 12.8.1 + hash, drop `pnpm` field) | Mine | — |
| `pnpm-workspace.yaml` (pnpm ≥11 settings: engineStrict, strictDepBuilds, overrides, allowBuilds, verifyDepsBeforeRun) | Mine | — |
| `.npmrc` (deleted; settings moved to pnpm-workspace.yaml) | Mine | — |
| `scripts/lang/ts/audit-remediation.md` (overrides location) | Mine | — |
| `docs/runbooks/client-dev-local.md` (engine-strict location) | Mine | — |
| `docs/specialist-knowledge/client/INDEX.md` (overrides pointer) | Not mine, Mechanical | client |
| `docs/specialist-knowledge/security/INDEX.md` (overrides/toolchain-pin pointers) | Not mine, Mechanical | security |
| `scripts/lang/ts/audit.sh` (fail closed on unrecognised JSON shape) | Mine | — |
| `scripts/lang/ts/audit.test.sh` (new) | Mine | — |
| `scripts/layer3.sh` (wire audit.test.sh, _pnpm.test.sh) | Mine | — |
| `infra/lib/package-manager.sh` (new) | Mine | — |
| `infra/devloop/Dockerfile` | Mine | — |
| `infra/devloop/devloop.sh` | Mine | — |
| `scripts/dev-web.sh` | Mine | — |
| `scripts/setup.test.sh` (package-manager lib tests; RUST_VERSION drift check extended) | Mine | — |
| `infra/docker/*/Dockerfile` (`RUST_VERSION` 1.91 → 1.95) | Mine | — |
| `pnpm-lock.yaml` (pnpm 12 env-lockfile document) | Mine | — |
| `scripts/lang/_pnpm.sh` (new: `pnpm_deps_fresh`, `pnpm_exec_unverified`) | Mine | — |
| `scripts/lang/_pnpm.test.sh` (new) | Mine | — |
| `scripts/lang/proto/_buf.sh` (verify-bypassed version probe + deps-fresh step) | Mine | — |
| `scripts/lang/proto/*.test.sh` (stale-tree token cases) | Mine | — |
| `scripts/lang/proto/compile.sh` (taxonomy comment: four → five tokens) | Mine | — |
| `scripts/lang/proto/lint.sh` (taxonomy comment: four → five tokens) | Mine | — |
| `scripts/lang/proto/breaking.sh` (taxonomy comment: four → five tokens) | Mine | — |
| `scripts/lang/proto/fmt.sh` (taxonomy comment: four → five tokens) | Mine | — |
| `scripts/dev-web.test.sh` (fixtures carry the hashed pin + the lib) | Mine | — |
| `scripts/lang/ts/compile.sh` | Mine | — |
| `scripts/lang/ts/lint.sh` | Mine | — |
| `scripts/lang/ts/test.sh` | Mine | — |
| `scripts/lang/ts/fmt.sh` | Mine | — |
| `scripts/lang/ts/fmt.test.sh` (fixture copies _pnpm.sh; disables verifyDepsBeforeRun in its borrowed-node_modules workspace) | Mine | — |
| `Cargo.toml` | Mine | — |
| `Cargo.lock` (regen) | Mine | — |
| `crates/devloop-helper/Cargo.toml` | Mine | — |
| `crates/dt-guard/Cargo.toml` | Mine | — |
| `crates/dt-story/Cargo.toml` | Mine | — |
| `crates/ac-service/Cargo.toml` | Not mine, Domain-judgment | auth-controller |
| `crates/common/Cargo.toml` | Not mine, Domain-judgment | auth-controller |
| `crates/common/src/jwt.rs` (negative base64 pinning tests) | Not mine, Domain-judgment | auth-controller |
| `crates/ac-service/src/config.rs` (STANDARD-decode negative tests) | Not mine, Domain-judgment | auth-controller |
| `crates/ac-service/fuzz/Cargo.toml` | Not mine, Domain-judgment | auth-controller |
| `crates/gc-service/Cargo.toml` | Not mine, Mechanical | global-controller |
| `crates/mc-service/Cargo.toml` | Not mine, Mechanical | meeting-controller |
| `crates/mh-service/Cargo.toml` | Not mine, Mechanical | media-handler |
| `crates/ac-test-utils/Cargo.toml` | Not mine, Mechanical | test |
| `crates/gc-test-utils/Cargo.toml` | Not mine, Mechanical | test |
| `crates/mc-test-utils/Cargo.toml` | Not mine, Mechanical | test |
| `crates/env-tests/Cargo.toml` | Not mine, Mechanical | test |
| `crates/mh-service/src/webtransport/connection.rs` (3 tests → MetricAssertion) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/src/observability/metrics.rs` (comments) | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/webtransport_integration.rs` (comment) | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/token_refresh_integration.rs` (comment) | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/gc_integration.rs` (comment) | Not mine, Mechanical | media-handler |
| `crates/mc-service/src/observability/metrics.rs` (comment) | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/src/media_routing/teardown.rs` (test: assert the MCPushQuiesceTimeouts series) | Not mine, Minor-judgment | meeting-controller |
| `crates/common/src/observability/mod.rs` (doc comment: metrics-util no longer gated) | Not mine, Mechanical | observability |
| `docs/decisions/adr-0032-metric-testability.md` | Not mine, Minor-judgment | observability |
| `crates/dt-guard/src/workspace_deps.rs` (new) | Mine | — |
| `crates/dt-guard/src/common/cargo_manifest.rs` (new) | Mine | — |
| `crates/dt-guard/src/common/mod.rs` | Mine | — |
| `crates/dt-guard/src/release_build_profile.rs` (use shared manifest parse) | Mine | — |
| `crates/dt-guard/src/lib.rs` | Mine | — |
| `crates/dt-guard/src/main.rs` | Mine | — |
| `crates/dt-guard/tests/workspace_deps_e2e.rs` (new) | Mine | — |
| `scripts/guards/simple/validate-workspace-deps.sh` (new) | Mine | — |
| `docs/contributor/audit-suppressions.md` | Mine | — |
| `docs/runbooks/devloop-validation.md` | Mine | — |
| `docs/TODO.md` (§Supply Chain Rust entry reworded; CI pnpm hash deferral; :257 pnpm-copies clause; :1882 Node-pin guard entry) | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |

---

## Planning

Revision 3. Rev 3 adds the operator decisions and Lead rulings: a merged `otel-web-stack` group, pnpm 12.8.1 now (§3d), `workflow_dispatch` on CI, and the Rust 1.95 unification restored (§3c). The Rev 3 delta below is authoritative. Revision 2 folded in the Gate-1 input from security, test, code-reviewer, operations, dry-reviewer, observability and paired-auth-controller, and the Lead rulings. Each reviewer item maps to a section, listed in §Reviewer item map at the end.

### Rev 3 delta (for re-confirmation)
1. **One `otel-web-stack` cargo group** (operator decision), replacing the separate otel and web-stack groups. The coupling evidence is in its comment (§1).
2. **pnpm 10.33.2 → 12.8.1** (operator decision + Lead ruling "latest"), §3d:
   - Settings migrate to `pnpm-workspace.yaml`. `.npmrc` is deleted and the `pnpm` field leaves `package.json`.
   - `verifyDepsBeforeRun: error`.
   - The lockfile gains an env document.
   - Proofs (a)–(c) and negative controls were run on 12 (and 11).
   - The `_buf.sh` stale-tree token is fixed, and a new `pnpm-deps-stale` preflight is added for the TS lane.
   - Runbook entries, and the `minimumReleaseAge` behaviour is documented.
3. **`workflow_dispatch:` added to ci.yml and ci-client.yml** (Lead ruling on the trigger gap).
4. **Rust toolchain unification restored** (Lead ruling, §3c). The TODO entry is reworded to "latest stable from one pin".
5. **Accepted deferral:** CI pnpm hash verification, filed in `docs/TODO.md` §Supply Chain.
6. **TS audit parser fails closed on unrecognised output shape** (security A2), §3d. pnpm ≥11 uses the bulk audit endpoint, so a reshaped `--json` would otherwise read as "0 high".
7. **Three more stale config pointers re-pointed** (DRY), §3d Doc and pointer moves.

### Mechanism restatement (instance → class)

1. **"A version is written in more than one place."** I swept every instance, and every one is fixed or guarded in this loop:
   - **pnpm 10.33.2.** It appears in 5 workflow `version:` inputs (ci.yml:72, ci-client.yml:98/180/221, audit-scheduled.yml:66) and in `infra/devloop/Dockerfile:137`. `scripts/dev-web.sh:283` has its own parser. SSoT = `package.json` `packageManager`.
   - **Node.** It appears in 5 workflow `node-version: '22'` inputs (ci.yml:78, ci-client.yml:104/186/227, audit-scheduled.yml:72). SSoT = `.nvmrc`.
   - **Rust toolchain.** 4 workflow steps say `1.95`, `infra/devloop/Dockerfile` says `rust:1.95`, and 5 image Dockerfiles have `ARG RUST_VERSION=1.91`, so the values already disagree. Unified on 1.95 with a drift check (§3c). Moving to latest stable stays in the TODO.
   - **Rust deps that are in `[workspace.dependencies]` but redeclared literally in a member:**
     - `base64` in ac-service (**0.21**), gc-service, ac-test-utils, mc-test-utils and env-tests
     - `metrics-util` in ac/gc/mc/mh-service
     - `hex` and `anyhow` in ac-service
     - `uuid` in env-tests
   - **Rust deps declared literally in 2 or more members and not in the workspace table:** `metrics`, `metrics-exporter-prometheus`, `reqwest`, `wiremock`, `tokio-stream`, `tempfile`, `http-body-util`, `regex`, `serde_norway`, `futures`, `clap`, `assert_cmd`, `rcgen`.
   - **Fuzz workspaces** (`crates/{ac-service,media-protocol}/fuzz`). They are independent `[workspace]`s, so they cannot inherit. They are covered by a version-match rule against root instead (guard R3).
2. **"Crates that must move together get split into separate PRs."**
   - Groups: otel, web-stack, metrics stack, npm `@opentelemetry/*`, npm protobuf-es, npm vitest.
   - **Operator decision:** otel and web-stack are MERGED into one `otel-web-stack` group, and the comment keeps the coupling evidence.
3. **"A global-registry facade duplicated in the graph silently drops telemetry"** (observability). This happens when the `metrics`, `tracing-core` or `opentelemetry` facade appears twice: the recorder is installed on one copy while the code emits to the other. The Cargo.lock single-version rule (guard R4) covers it.

### 1. `.github/dependabot.yml`
- **Header and suppression contract.**
  - The header contract (no `ignore:`) stays verbatim. The `@bufbuild/buf` exclude stays, with its rationale.
  - The npm protobuf group names `@bufbuild/protobuf` and `@bufbuild/protoc-gen-es` explicitly; `@bufbuild/*` would pick up buf.
  - `scripts/audit-suppressions-check.sh` (its `dependabot-ignore-present` check) must stay green.
- **Group ordering.**
  - First match wins. From dependabot-options-reference §groups: "If a dependency matches more than one rule, it's included in the first group that it matches." The pattern groups are therefore listed before the update-type group, and a comment says so.
  - No `tracing*` pattern exists, so `tracing-opentelemetry` cannot be captured ahead of the otel group.
- **Cooldown** is set on every ecosystem: `default-days: 5`. `semver-major-days: 14` applies to cargo and npm only, because the docs' cooldown table marks GitHub Actions SemVer-bump days "Not supported". A comment states that cooldown delays version updates only and never security updates (ADR-0033 §12 14-day MTTR), and that it is not a suppression surface.
- **cargo**
  - *(Superseded at Gate 2, operations F-OPS-1: Dependabot opens one PR per directory by default, and `group-by: dependency-name` would split the pattern groups. The cargo entry stays `directory: "/"`, and R3 makes a root bump that moves a shared requirement red until the fuzz literal is edited in the same PR.)* Originally planned: `directories: ["/", "/crates/ac-service/fuzz", "/crates/media-protocol/fuzz"]`, so fuzz manifests move in the same grouped PR as root. Dependabot groups create one PR per group across `directories`. Without this, guard R3 would fail any fuzz-only PR.
  - `versioning-strategy: increase-if-necessary`.
  - Groups:
    - `otel-web-stack` (operator decision): `opentelemetry*`, `tracing-opentelemetry`, `axum*`, `tower*`, `tonic*`, `prost*`. The comment records the coupling evidence:
      - `opentelemetry-otlp` takes our `tonic::transport::Channel` (`common/src/observability/otel.rs:240-243`).
      - gc encodes `opentelemetry-proto` messages with our `prost`.
      - opentelemetry-proto/otlp 0.33 require `prost ^0.14` and `tonic ^0.14.1`.
      - PR #75 (otel alone) went red.
      
      It also notes that tonic/prost bumps regenerate `proto-gen` (wire-format GSA), so they need protocol + security review.
    - `metrics-stack`: exact names `metrics`, `metrics-exporter-prometheus`. metrics-util is removed as a direct dep, see §4.
    - `cargo-minor-patch`.
  - Pattern-only groups have no `update-types`, so they include majors.
- **npm.** Same cooldown. Groups `npm-otel` (`@opentelemetry/*`), `npm-protobuf-es`, `npm-vitest` (`vitest`, `@vitest/*`, `vitest-browser-svelte`), then `npm-minor-patch`, which keeps the buf exclude. The root `pnpm.overrides` floors are not touched by Dependabot's versioning strategy. Overrides live in `pnpm.overrides`, not dependency ranges, and I'm leaving npm's strategy at its default.
- **Triggers.** `workflow_dispatch:` is added to `ci.yml` and `ci-client.yml` (Lead ruling).
- **github-actions.** One group with `patterns: ["*"]`, which covers all update types including majors. This removes the limit-5 starvation that hid the checkout/setup-node majors.
- **Validation.** Dependabot config cannot be exercised in CI. I'll check it against the options reference key-by-key and run `audit-suppressions-check.sh`. The real proof is the Lead's next "Check for updates" run.

### 2. GitHub Actions: every action to its latest release, SHA-pinned
Every `uses:` becomes `owner/repo@<40-hex sha> # vX.Y.Z`; Dependabot maintains SHA-plus-comment pins natively. That applies to first-party `actions/*` too, for uniformity. SHAs are resolved from the tags via `git ls-remote` (`^{}` peeled). The tables below record the release-notes and action.yml checks.

| Action | From → To | Breaking-change check |
|---|---|---|
| actions/checkout | v4 → v7.0.1 | v5 moved to node24 and needs runner ≥2.327.1. v6 persists credentials to a separate file. v7 is ESM and refuses fork checkout under `pull_request_target`/`workflow_run`, neither of which we use. |
| actions/setup-node | v4 → v7.0.0 | v5 added auto-cache for packageManager=npm; v6 narrowed it. We set `cache: 'pnpm'` explicitly. v7 is ESM. `node-version-file` input is present. |
| pnpm/action-setup | v4 → v6.1.0 | v5 moved to node24. v6 added pnpm 11 support. Inputs are unchanged. |
| actions/cache | v4 → v6.1.0 | v5 moved to node24. v6 is ESM. `key`/`path` are unchanged. |
| actions/upload-artifact | v4 → v7.0.1 | v5/v6 moved to node24. v7 is ESM, and `archive: false` is opt-in. `name`/`path` are unchanged. Hidden-file default: since v4.4, dotfiles are excluded unless `include-hidden-files: true`. Fuzz `corpus/` and `artifacts/` contain libFuzzer hash-named files (no leading dot), so nothing is lost. I'll recheck v5–v7 notes for any further default changes during implementation. |
| actions/download-artifact (new, for the S1 split) | — → v8.0.1 latest | Must be compatible with upload-artifact v7. I'll check the notes at implementation. |
| actions/github-script | v7 → v9.0.0 | v8 moved to node24. v9 breaks `require('@actions/github')` and injects `getOctokit`. Our script uses only `require('fs')` (still served by the require proxy per the v9 README §require), `github.rest.*`, `github.paginate` and `context`. |
| codecov/codecov-action | v4 → v7.1.1 | v5 switched to the CLI wrapper and renamed `file` to `files`. v6 moved to node24. v7 is the signing-key account move. The v7.1.1 action.yml still declares `files`, `flags`, `fail_ci_if_error`, `token`. **ci-client.yml passes no `files:`**, so I'll set `files:` explicitly to the vitest lcov path instead of relying on auto-discovery. That removes the silent-discovery question operations raised. |
| Swatinem/rust-cache | v2 → v2.9.2 | Same major, only SHA-pinned. |
| taiki-e/install-action | `@cargo-audit`/`@cargo-llvm-cov` (**branch refs**) → v2.87.22 + `tool:` input | Moves from a mutable branch to a pinned release. |
| dtolnay/rust-toolchain | `@master`/`@nightly` (**branch refs**) → **removed** | Replaced by `run: rustup toolchain install <v> --profile minimal --component … && rustup default <v>`. rustup is preinstalled on ubuntu-latest, and ci.yml already does `rustup toolchain install nightly` this way. Removing it leaves one fewer third-party action that runs with a token and nothing to pin. |

- **Runners.** All jobs run on GitHub-hosted `ubuntu-latest`, which already meets the node24 / runner ≥2.327.1 requirement (operations item 3). There are no self-hosted runners.
- **DRY.** After the change, no workflow is left on an old major.

### 2b. Workflow token hardening (security S1/S2)
- **audit-scheduled.yml** is split into two jobs:
  - `scan` job. Permissions: `contents: read` only. Checkout uses `persist-credentials: false`. It runs the toolchain, `pnpm install` and `scripts/audit.sh`, sets output `failed`, and uploads `audit-output.txt` as an artifact.
  - `report` job (`needs: scan`, `if: always()`). Permissions: `issues: write` only. No checkout and no install. It downloads the artifact and runs github-script.
  - Workflow top-level permissions: `contents: read`.
  - The artifact is used instead of a job output so arbitrary audit text never passes through `$GITHUB_OUTPUT` delimiters.
  - S2: `failed` reaches the script as `env: AUDIT_FAILED: ${{ needs.scan.outputs.failed }}`, and the scan result as `env: SCAN_RESULT: ${{ needs.scan.result }}`. Both are read via `process.env`; there is no `${{ }}` inside the JS.
  - **Fail-open fix (security NEW, operations F1).** The script decides from three states and never treats an empty or unknown output as clean:
    - **clean:** `SCAN_RESULT == 'success'` AND `AUDIT_FAILED == 'false'`. This is the ONLY branch that comments "Resolved" and closes the issue.
    - **drift:** `AUDIT_FAILED == 'true'`. Comments on or opens the issue with the audit output, as today.
    - **incomplete:** anything else (empty output, scan failed/cancelled/timed out, artifact missing). Comments on or opens the issue with "scan did not complete (result=<x>)" plus the run URL. Never closes it.
    - This bug is pre-existing (`'' === 'true'` is false today); it is fixed here because this block is being rewritten.
  - **Missing artifact (F2).** The scan job's upload step is `if: always()`. In the report job, download-artifact has `continue-on-error: true`, so a crashed scan still reaches the script, which falls back to "(no output captured)". The script itself still runs and goes red as below, so this is not a masked failure.
  - **Red run (F3).** The scan job keeps today's shape: it captures rc into the output and uploads the artifact, then its final step `Fail the run if audit failed` exits 1 when `failed == 'true'`. A scan that crashes earlier is red by itself. The report job also calls `core.setFailed` on both drift and incomplete. So the run is red on every non-clean state, and is red even if the scan job's own failure were ever masked.
  - **Verification.** The clean/drift/incomplete branching gets a reasoned read in main.md. The Lead's dispatch run exercises the clean path. Inducing a crash is not cheap without a workflow input, and I am not adding one.
- **ci.yml, ci-client.yml, fuzz-nightly.yml.** Every checkout gets `persist-credentials: false`. I confirmed no step needs authenticated git: a grep for `git fetch|push|pull|gh |GITHUB_TOKEN` hits nothing, and `buf breaking .git#branch=main` and `_get_base_ref.sh` read local refs. The `fetch-depth: 0` history is fetched by the checkout action itself.

### 3. Toolchain-version SSoT
**a. pnpm**
- Remove all 5 `version:` inputs. pnpm/action-setup v6.1.0 `src/install-pnpm/run.ts:readTargetVersion()` reads `packageManager` (or `devEngines.packageManager`) when `version` is absent. If both are set and differ it throws `Multiple versions of pnpm specified`. It strips the integrity suffix: `packageManager.slice('pnpm@'.length).split('+')[0]`.
- **Integrity (S7).** `packageManager` becomes `pnpm@12.8.1+sha512.f64ba907…2aabbe45` (§3d). The mechanism was first verified on 10.33.2's `a90faf6f…`.
  - The hash is the hex SHA-512 of `pnpm-10.33.2.tgz`. I verified that the downloaded tarball's `sha512sum` equals the registry `dist.integrity` decoded to hex.
  - Corepack enforces it. Verified: a hash with one changed nibble gives `Error: Mismatch hashes`.
  - **Honest limit:** action-setup strips the hash and does **not** verify it. CI's pnpm comes from action-setup's lockfile-pinned bootstrap and then `pnpm self-update 10.33.2` from the registry. Corepack (dev hosts, devloop image) does verify. Making CI verify would mean swapping action-setup for corepack, which interacts with setup-node's `cache: pnpm` ordering. Unless security wants it here, I'm leaving that as a finding for them to rule on.
- **One parser.** New sourced lib `infra/lib/package-manager.sh`, modelled on `infra/lib/cargo-lock-version.sh`:
  - `pnpm_package_manager_spec <package.json>` prints the full `pnpm@X.Y.Z+sha512.<128 hex>`.
  - `pnpm_version <package.json>` prints `X.Y.Z`.
  - Both validate shape with an anchored regex and fail loudly on anything malformed or missing. The hash is required.
  - Consumers: `devloop.sh` (new) and `scripts/dev-web.sh:283` (replaces its private grep).
- **Devloop image.** The "MUST match packageManager" comment at `infra/devloop/Dockerfile:132` is rewritten to say the value is "derived from" it. The Dockerfile's `ARG PNPM_VERSION=10.33.2` becomes `ARG PNPM_PACKAGE_MANAGER`. It has no default, fails with `:?` the way `NODE_VERSION` does, and runs `corepack prepare "${PNPM_PACKAGE_MANAGER}" --activate`, so the **image build verifies the hash**. devloop.sh passes `--build-arg PNPM_PACKAGE_MANAGER=` at both `podman build` sites.
- **Tests** in `scripts/setup.test.sh` group (F), next to the `cargo_lock_version` tests: exact match, missing field fails, missing hash fails, malformed fails, the **hashed form** `pnpm@X.Y.Z+sha512.<128 hex>` (spec printed verbatim, version printed bare), the real `package.json` parses, and devloop.sh/Dockerfile derive from the lib, not a literal.
- **One suffix-stripper** (DRY 2). `infra/lib/package-manager.sh:pnpm_version` is the only code that strips `+sha512…`. `dev-web.sh` and `devloop.sh` both call the lib, with no `${x%%+*}` or private grep anywhere else. A grep for this is recorded at Gate 2.
- `docs/TODO.md:257`: strike the "4 copies of the pnpm version" clause, which is now false.

**b. Node.** 5 sites change to `node-version-file: '.nvmrc'`. CI then runs exactly 22.23.1. Before Gate 2, `grep -rn "version: 10\|node-version:" .github/workflows` must return only `node-version-file` lines; the output is recorded.

**c. Rust toolchain (restored by Lead ruling, Rev 3 delta)**
- Unify on 1.95: `ARG RUST_VERSION=1.91` → `1.95` in the 5 `infra/docker/*/Dockerfile`s. The workspace already builds, lints and tests on 1.95 in CI and in devloop, so the images just do a release build on the same compiler.
- Workflows install it through a rustup `run:` step (§2).
- Drift check: extend the existing `scripts/setup.test.sh` block "RUST_VERSION: one checked value". The value must agree across:
  - the image ARGs
  - `infra/devloop/Dockerfile`'s `FROM …rust:<v>-`
  - every non-nightly workflow `rustup toolchain install <v>`
- The check carries a non-vacuity floor on the number of sites and a negative control.
- `docs/TODO.md` §Supply Chain is reworded to the remaining work: one pin → latest stable, plus the clippy-lint fixes.

**d. pnpm 10.33.2 → 12.8.1 (operator decision; Lead ruling "latest").**
- **Which version.** 12.8.1 is both npm `latest` and GitHub `releases/latest`. 12.8.2 is published (2026-09-30) but neither tag has been promoted to it, so I follow the maintainers' designation.
- **Settings-location audit: 11.0.0 notes.** These move **every** setting we depend on:
  - "pnpm no longer reads settings from the `pnpm` field of `package.json`" (#10086). This would have silently dropped `pnpm.overrides` and `pnpm.ignoredBuiltDependencies`.
  - "`.npmrc` is auth/registry only — all other settings must live in `pnpm-workspace.yaml`" (#11189). This would have silently dropped `engine-strict` and `strict-dep-builds`.
  - "`allowBuilds` replaces `onlyBuiltDependencies` … `ignoredBuiltDependencies`" (#11220). We have no `onlyBuiltDependencies`.
- **Settings-location audit: 11→12, all 49 v12.x release notes scanned.**
  - Nothing else we use moved or was renamed. `allowBuilds`, `engineStrict`, `strictDepBuilds`, `overrides` and `verifyDepsBeforeRun` are unchanged.
  - 12.0.0 now fails unknown `pnpm-workspace.yaml` keys with `ERR_PNPM_UNRECOGNIZED_WORKSPACE_SETTINGS` when the pinned pnpm is running, which catches typos loudly.
  - Under `engineStrict`, 12.0.0 also fails incompatible packages reached through a regular edge beneath an optional dep (11 warned). The frozen install passes.
- **Migration.**
  - `pnpm-workspace.yaml` gains:
    - `engineStrict: true`
    - `strictDepBuilds: true`, explicit although it is the default
    - `verifyDepsBeforeRun: error` (Lead-approved)
    - `overrides:` with the same 7 entries
    - `allowBuilds: {'@bufbuild/buf': false, msw: false, nx: false}`
  - The `.npmrc` rationale comments (F11, per-package build classification) move into the yaml.
  - `package.json` loses its `pnpm` field. `packageManager` becomes `pnpm@12.8.1+sha512.f64ba907…2aabbe45`; the hex is verified equal to the registry `dist.integrity`.
  - `.npmrc` is deleted. pnpm ≥11 reads only auth/registry from it, and we have none.
  - **`pnpm-lock.yaml` gains an env-lockfile document**, 158 lines. It records `packageManagerDependencies: pnpm 12.8.1` and the per-platform `@pnpm/exe.*` integrity hashes, and is written on the first install under 12. It is committed, and it pins the native binary's integrity per platform.
- **Proofs from the scratch trial, pnpm 12.8.1 via corepack.** They are re-run on the real tree at implementation and the output recorded:
  - (c) `pnpm install --frozen-lockfile` → rc 0. After the env-lockfile document is written once, a second fresh frozen install leaves the lockfile byte-identical (`cmp` passes).
  - (b) Fresh install after removing every `node_modules` → 0 lines matching "ignored build".
  - (a) `pnpm audit --audit-level high` → `Severity: 3 moderate`, 0 high. `DEVLOOP_AUDIT_FORCE_RUN=1 scripts/lang/ts/audit.sh` → `STATUS=OK REASON=pnpm-audit-passed`. The JSON still carries `advisories{}.github_advisory_id` and `.severity`, which the wrapper filter reads.
  - **Negative controls on 12**, each proving the setting is read from the yaml:
    - delete `overrides:` → `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH`
    - delete `msw` from `allowBuilds` → `ERR_PNPM_IGNORED_BUILDS`
    - engineStrict, below the floor: on the scratch trial the `<23` cap was still in place, so Node 24 → `ERR_PNPM_UNSUPPORTED_ENGINE`. **The real-tree controls after implementation, with the cap dropped (§5):**
      - Node **22.12.x** (below the `>=22.13.0` floor) must fail with `ERR_PNPM_UNSUPPORTED_ENGINE`. The error line is recorded.
      - Node **24.x** must install cleanly. This is the npm Dependabot fix itself, and the output is recorded.
    - a misspelled key → `ERR_PNPM_UNRECOGNIZED_WORKSPACE_SETTINGS`
  - These were also run on 11.28.2 with the same results.
- **`verifyDepsBeforeRun: error` and reason tokens (test, operations 1).**
  - **Reproduced:** with a stale tree (buf pin edited without install), `pnpm exec buf --version` fails with `ERR_PNPM_VERIFY_DEPS_BEFORE_RUN`. `_buf.sh` then reports `REASON=buf-not-installed`, the wrong token.
  - **Fix, fail-closed, never matching pnpm's wording.**
    - In `_buf.sh`, states (2) "is buf installed?" and (4) "version equality" probe with `pnpm --config.verify-deps-before-run=false exec buf --version`. 12.8.0 "applies every setting passed as `--config.<name>=<value>`", and I verified the probe returns 1.72.0 on a stale tree. A stale buf writer therefore still reports `buf-version-mismatch`.
    - A new shared preflight `pnpm_deps_fresh` runs `pnpm exec true` with the verify step on. Its exit status alone decides, and failure maps to `REASON=pnpm-deps-stale` with the remedy `pnpm install --frozen-lockfile`.
      - pnpm's stderr is captured and printed after the STATUS line so the operator sees which dependency is stale. It is displayed only, never matched on (code-reviewer 3).
      - It lives in a new sourced `scripts/lang/_pnpm.sh`.
    - **One home for the bypass** (code-reviewer 2, DRY 3). `_pnpm.sh` also defines `pnpm_exec_unverified() { pnpm --config.verify-deps-before-run=false exec "$@"; }`. Its comment explains why the bypass is safe there: the probe only reports the buf version, and `pnpm_deps_fresh` runs right after it, so staleness is still caught. Both `_buf.sh` probe states call it, and the flag literal appears nowhere else.
    - **`scripts/lang/_pnpm.test.sh`** (code-reviewer 1) is built on `_test_helpers.sh` with a stub `pnpm` that writes a marker file:
      - stub exits 0 → fresh, rc 0, and the marker is present (positive control that the stub was invoked);
      - stub exits non-zero → `REASON=pnpm-deps-stale`, the remedy text, and the stub's stderr echoed;
      - `pnpm_exec_unverified` passes the bypass flag.
      - It is wired into `scripts/layer3.sh`.
    - `_buf.sh` runs it after the version check, so precedence holds: buf mismatch first, then general staleness.
    - The TS wrappers (`scripts/lang/ts/{compile,lint,test,fmt}.sh`) run it before their `pnpm exec nx`, so a stale tree is a named token, not a misattributed `nx-*` failure.
  - **Tests.**
    - Where the `_buf.sh` state matrix is tested: a stub `pnpm` that fails unless `--config.verify-deps-before-run=false` is passed, asserting `buf-version-mismatch` (stale buf) and `pnpm-deps-stale` (stale non-buf dep).
    - A real-tree negative control in main.md: edit the buf pin without installing → `buf-version-mismatch`.
- **Tooling.**
  - pnpm/action-setup v6.1.0: README "supports pnpm v12 and earlier". The source takes the `targetsPnpm12` native-bootstrap path.
  - Dockerfile corepack path: corepack 0.34.6 (bundled with Node 22.23.1, the image's Node) runs `corepack prepare pnpm@12.8.1+sha512…`. This was verified locally ("Downloading the pnpm 12.8.1 binary for linux-x64…" → `12.8.1`).
  - The pnpm 12 npm package is a launcher; it fetches the native `@pnpm/exe.<platform>` binary and verifies npm registry signatures by default (`bin/pnpm.mjs:signaturePolicy`). The corepack hash covers the launcher, and the lockfile env document pins the native binary's integrity.
- **Dependabot (operations).**
  - `helpers.rb` activates the `packageManager` version with `corepack prepare pnpm@<v> --activate`, falling back to its own 11.25.0 only if activation fails. pnpm 12 is in `SUPPORTED_VERSIONS`.
  - The updater image sets `NODE_USE_ENV_PROXY=1` specifically so the pnpm 12 launcher's native download works through its proxy.
  - Confirmed: support exists. Hypothesized: that activation succeeds on their proxy. The real proof is the next npm Dependabot run, which is already in Verification.
  - On fallback, every key we set is also an 11.x key.
- **Other default changes.**
  - `minimumReleaseAge` defaults to 1440 (non-strict). Per 12.x (#13687), when nothing mature satisfies a range, a non-strict install takes the immature version and records it in `minimumReleaseAgeExclude` in `pnpm-workspace.yaml`. A same-day advisory fix therefore installs, and the exclusion shows up in the diff for review.
  - We do **not** set `minimumReleaseAge` explicitly; an explicit value makes it strict as of 12.x.
  - `docs/contributor/audit-suppressions.md` states this beside the cooldown note: neither slows the 14-day MTTR.
  - `blockExoticSubdeps: true`: the frozen install passes.
- **Doc and pointer moves.**
  - `scripts/lang/ts/audit-remediation.md:19` is functional: it tells the remediation agent where to add overrides, and now points to `pnpm-workspace.yaml` `overrides:`.
  - `.github/workflows/ci-client.yml:158` comment.
  - `scripts/dev-web.sh` engine-strict comments.
  - `docs/runbooks/client-dev-local.md:1272`.
  - `docs/specialist-knowledge/client/INDEX.md:74`.
  - The infra INDEX Node-pin line.
- **Audit parser fails closed (security A2).** `scripts/lang/ts/audit.sh` currently prints OK for any JSON object that has neither `advisories` nor `vulnerabilities`, which would silently pass a reshaped pnpm output.
  - **Fix.** In the Python decision, `data` that is not a dict, or that has neither recognised key, prints `FAIL unrecognised-shape`.
  - **New token.** That maps to `emit_status FAIL "pnpm-audit-unrecognised-output"`, distinct from `pnpm-audit-failed`, so it can never be "fixed" by suppressing an advisory.
  - **Unchanged.** An empty `advisories: {}` still counts as recognised and clean. The no-json and bad-json branches stay as they are.
  - **Tests.** A new hermetic `scripts/lang/ts/audit.test.sh`, wired into `scripts/layer3.sh` beside `audit-gate-test`, with a stub `pnpm` on PATH and `DEVLOOP_AUDIT_FORCE_RUN=1`:
    - `{"results":[]}` → `pnpm-audit-unrecognised-output`.
    - One high advisory under `advisories` → `pnpm-audit-failed`, with its GHSA in the `remaining=` line (positive control).
    - The same high advisory listed in the ignore file → OK plus `SUPPRESSED=`.
    - `{"advisories":{}}` → OK.
  - **Real-tree positive control (A1).** The keys of the 3 moderate entries from `pnpm audit --json` (`github_advisory_id`, `severity`, `module_name`) are recorded in main.md.
- **More stale pointers (DRY).**
  - `docs/specialist-knowledge/security/INDEX.md:67`: the overrides and toolchain-pin pointers move to `pnpm-workspace.yaml`.
  - `docs/specialist-knowledge/infrastructure/INDEX.md:53`: the Node-pin line drops `.npmrc`.
  - `docs/TODO.md:1882` (the Node-pin guard entry): `engine-strict` now lives in `pnpm-workspace.yaml`, and the `<23` cap clause is restated. That guard's invariants are the floor match and exact-pin agreement; neither keys on the cap, so the entry is otherwise unchanged.
  - **Confirmed:** the rationale comments (the F11 note for engineStrict, and the per-package reasons for buf, msw and nx) move verbatim with their settings into `pnpm-workspace.yaml`. `.npmrc` is deleted only after that.
- **Runbook entries (operations 2).**
  - `docs/runbooks/devloop-validation.md`: a symptom-catalogue row for `pnpm-deps-stale` and `ERR_PNPM_VERIFY_DEPS_BEFORE_RUN`, with fix `pnpm install --frozen-lockfile`.
  - `docs/runbooks/client-dev-local.md`: host migration. dev-web.sh hard-fails a host still on pnpm 10; the fix is `corepack enable` or installing pnpm 12. A `node_modules` built by pnpm 10 needs `rm -rf node_modules && pnpm install`.

### 4. Rust workspace-dependency SSoT + bumps
- **metrics-util is removed, not bumped** (Lead ruling).
  - Migrate the 3 `crates/mh-service/src/webtransport/connection.rs` tests to `common::observability::testing::MetricAssertion`. The timeout arm becomes `counter(METRIC_NAME).assert_delta(1)`; the cancel and registered arms become `assert_unobserved()`.
  - I confirmed they can be expressed this way. Default `#[tokio::test]` is current-thread, MetricAssertion is thread-local, and mh already uses it (`grpc/mh_service.rs:616`). `assert_unobserved` is stricter than the old `None | Some(0)`.
  - Then delete `metrics-util` from ac/gc/mc/mh dev-deps, from common's `[dependencies]`/`[dev-dependencies]` and the `test-utils` feature's `dep:metrics-util`, and from `[workspace.dependencies]`.
  - Dependabot #68 is resolved by the removal.
  - metrics-util 0.20 stays only as a transitive dependency of the exporter.
- **Metrics family moves as one set:** `metrics` 0.24 → 0.24.6 and `metrics-exporter-prometheus` 0.16 → 0.18.3, both hoisted.
  - Evidence recorded in main.md: `cargo tree -i metrics --workspace -e normal,dev` shows **one** `metrics` version.
  - **Render diff (operations B, observability 3):** a scratch test (not committed) uses **mc-service**, which has the widest counter/gauge/histogram mix; the render sites are ac `routes/mod.rs:245`, gc `handlers/metrics.rs:29`, mc `main.rs:383` and mh `main.rs:362`. It renders the registry via `PrometheusBuilder` + `handle.render()` before and after the bump. I diff the output and paste it under Implementation Summary. Explicit checks: no sample-name change (no `_total` added or doubled on any counter sample); no change to histogram `_bucket`/`_sum`/`_count` names or `le` formatting; no unit suffix appearing.
  - If counter naming changes, I stop and raise it with observability before going further.
- **base64 → 0.23.1 (Dependabot #69).**
  - Workspace entry: `base64 = { version = "0.23", default-features = false, features = ["std"] }` (security and auth-controller decision). Every direct consumer inherits it via `workspace = true`. **ac-service moves 0.21 → 0.23.**
  - `crates/ac-service/fuzz/Cargo.toml` gets the same literal, `{ version = "0.23", default-features = false, features = ["std"] }`, with a comment pointing at root. Guard R3 enforces the match.
  - Expected source churn is none: we never match `DecodeError` variants (auth-controller grep), and the `general_purpose` presets are byte-identical across 0.21/0.22/0.23. If churn does appear in a GSA file, that row is already Domain-judgment.
  - **Feature evidence, recorded in main.md:** `cargo tree --workspace --target all -e features,normal,dev,build -i base64@0.23.1` must show no `simd-unsafe` and no `default` edge. The same check runs in `crates/ac-service/fuzz`. Any third-party crate that enables it gets named.
  - jsonwebtoken keeps its own base64 0.22.1, so signature verification is untouched.
- **Negative pinning tests.** These are written and run **before** the bump so they pin behaviour rather than describe the change.
  - `crates/common/src/jwt.rs` tests, `decode_ed25519_public_key_jwk`. Positive control: the valid 43-char x decodes to 32 bytes. Each of these must be Err:
    1. `=` appended
    2. `_` changed to `/`
    3. last `o` changed to `p` (trailing bits, `InvalidLastSymbol`)
    4. truncated to 41 chars (`InvalidLength`)
    5. `!` at index 5 (mid-quad)
  - `decode_ed25519_public_key_pem`: padding stripped → Err; `+`/`/` changed to `-`/`_` → Err.
  - `extract_kid`: a padded header → `MalformedToken`.
  - `crates/ac-service/src/config.rs` tests, STANDARD path (AC_MASTER_KEY / AC_HASH_SECRET decode): stripped padding, URL-safe alphabet chars, non-canonical trailing bits and len%4==1 each reject.
  - Matching on specific `DecodeError` variants is allowed where it keeps the test fail-closed.
- **Suites covering base64 call sites** (test item 2):
  - `common` unit tests (`jwt.rs` `extract_kid`/JWK/PEM)
  - ac-service unit tests (`config.rs` key decode, `crypto/mod.rs`, `key_management_service.rs`)
  - ac-service integration tests (Basic-auth `auth_handler.rs`, JWKS)
  - `ac-test-utils` (token builders)
  - gc-service tests (JWT fixtures)
  - These run in Layers 4/5.
- **Hoist.** The 13 multi-member literals are hoisted at the highest existing lower bound: `futures 0.3.31`, `regex 1.10`, `uuid 1.11`, `bytes 1.9` (matches media-protocol/fuzz). `reqwest` is hoisted as `default-features = false, features = ["json","rustls-tls"]`, exactly what every member uses. The hoist itself moves no version. `cargo update -p` runs only for the moved crates; there is no blanket update.
- **Lock** evidence, recorded in main.md:
  - `cargo tree -d` before and after.
  - `cargo tree -i metrics` (single version).
  - `cargo tree -i metrics-util` (single, transitive 0.20.x).
  - The mh test migration is checked by reading the tests, not only by compiling them: no double-read and no vacuous `None` assertions remain. `assert_unobserved` replaces the `None | Some(0)` match.

### 4b. Guard: `dt-guard workspace-deps` (`scripts/guards/simple/validate-workspace-deps.sh`)
- **Machinery.**
  - Checked first: no existing guard covers this.
  - New `crates/dt-guard/src/workspace_deps.rs`, plus a clap arm and a one-line `_dt_guard_wrapper.sh` shim (ADR-0034).
  - **Parsing is reused, not re-implemented.** The root-manifest read+parse and its fail-loud handling move out of `release_build_profile.rs` into `crates/dt-guard/src/common/cargo_manifest.rs`, as one typed struct carrying `members`, `exclude`, `dependencies` and `profile`. `release_build_profile` switches to it with no behaviour change; its tests stay green.
- **Rules.** Each rule has a distinct REASON token:
  - **R1 `workspace-dep-redeclared`.** A member declares a dep whose package name (with `package =` renames honoured) is in `[workspace.dependencies]` without `workspace = true`. Checked in `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` and `[target.*.…]`.
  - **R2 `dep-pinned-in-multiple-members`.** A registry dep is declared with a literal version in 2 or more members and is absent from the workspace table.
  - **R3 `excluded-workspace-version-drift`.** For each root `[workspace] exclude` crate that has its own manifest (the fuzz workspaces, derived from the root, not hardcoded), any dep also in `[workspace.dependencies]` must have an identical version requirement. If root sets `default-features = false`, the excluded crate must too, which is the base64 simd-unsafe case.
  - **R4 `facade-crate-duplicated`.** `Cargo.lock` holds more than one version of a crate in `SINGLE_VERSION_CRATES = ["metrics", "tracing-core", "opentelemetry"]`. That const carries a rationale doc and is observability-owned policy content, like `SECURITY_CONTEXT_KINDS`. Today each of these crates has exactly one version in the lock.
  - **R5 `member-default-features-ignored`** (code-reviewer 4). A member writes `{ workspace = true, default-features = false }` while the workspace entry keeps default features. Cargo ignores the member flag with only a warning, so features silently turn on. Also recorded: `cargo check --workspace 2>&1 | grep -c "default-features is ignored"` returns 0 after the hoist.
  - **R2 counts DISTINCT members.** A crate that declares a dep in both `[dependencies]` and `[dev-dependencies]` (or a target table) counts once. There is a unit test for this.
  - **R2 still applies when `[workspace.dependencies]` is absent.** R1 then has nothing to check; R2 must still fire, and there is a unit test for this.
  - `SINGLE_VERSION_CRATES` gets a one-line inclusion criterion (observability): the crate holds a process-global (recorder, dispatcher, provider), so a duplicate copy yields an empty global with no error.
  - `path`/`git` deps are exempt.
  - Preconditions: unreadable or unparseable manifests or lock, glob members, or **zero members / empty `[workspace.dependencies]`** all give PRECONDITION, with REASON tokens distinct from R1–R5 (e.g. `workspace-manifest-unreadable`, `workspace-members-empty`, `cargo-lock-unreadable`), so a parse failure can never be triaged as a content finding. A wrong root is never a vacuous pass. The PASS STATUS line carries `members=N excluded=M`.
- **Tests**, consistent with siblings:
  - In-module unit tests: positive and negative for each rule; rename; target/dev/build tables; `features` alongside `workspace = true`; path/git exempt; the R3 default-features direction; R4 with a two-version lock fixture for a listed crate that must fire, and a two-version crate NOT in the list that must not fire; an R3 default-features fixture (root sets `default-features = false`, the excluded crate omits it, the rule must fire).
  - `crates/dt-guard/tests/workspace_deps_e2e.rs` (assert_cmd against tempdir workspaces): a positive-control fixture per rule that must trip; a clean fixture; the precondition cases; the STATUS/REASON surface; and a run against the real repo root (resolved from `env!("CARGO_MANIFEST_DIR")` up two levels, never cwd) that asserts clean AND that `members=N` on the STATUS line equals the length of the root `[workspace] members` array, which the test reads from the real root Cargo.toml.
  - Wiring: `run-guards.sh` globs `simple/*.sh`, so the wrapper runs in Layer 3. I'll also satisfy the `test-registration` guard and `binary_status_surface.rs` the way siblings do.

### 5. npm Dependabot job: diagnosis and fix
- **Confirmed:**
  - Every `npm_and_yarn in /.` run has failed, from 2026-08-10 through 2026-10-02 (run 37076911083). The updater log requires write access, and the API returns 403.
  - The updater runs Node 24: `dependabot-core/npm_and_yarn/Dockerfile` `ARG NODEJS_VERSION=24`, since 2025-12-12 #13576.
  - `engines.node: ">=22.13.0 <23"` and `.npmrc` `engine-strict=true` were both present at the first failing SHA (8ebce6b1).
- **Reproduced:** in a clean clone on Node v24.21.0, `pnpm install --lockfile-only --ignore-scripts` fails with `Expected version: >=22.13.0 <23 Got: v24.21.0`. Without the cap it succeeds.
- **Hypothesized:** that this is the only error in the updater log.
- **Pinning tests run before the bump** (auth-controller Gate 3): the run is recorded in main.md with its log.
- **Fix:** `engines.node` → `">=22.13.0"`.
  - The floor, which is the F11 property engine-strict exists for, is unchanged. `.nvmrc` still pins exactly 22.23.1.
  - The only thing lost is a refusal of Node ≥23 by a raw `pnpm install` run outside `dev-web.sh`. `scripts/dev-web.sh` itself **hard-fails** on a Node major different from `.nvmrc` (operations correction).
  - Operations and security accept this.
  - The example in the `scripts/dev-web.sh:286` comment is updated.
- Verification is the Lead's.

### 6. Scheduled Audit: diagnosis and fix
- **Confirmed (public issue #70, which carries every run's full output):**
  - The persistent failure was `STATUS=FAIL REASON=buf-binary-missing`. The workflow never installed a bare `buf`, and `breaking.sh` gated on `command -v buf` (verified at 6462bcc, the head of the 09-28 run).
  - cargo-audit passed every week. pnpm-audit failed on 09-07 and 09-14 only.
  - No new advisory was involved, so there is no suppression change and no audit loosening (S8).
- **Already fixed on main** by f3fb8d34 (2026-10-02), which postdates the last scheduled run. Local reproduction on this tree (depth-1 clone, `GITHUB_ACTIONS=true GITHUB_EVENT_NAME=schedule DEVLOOP_AUDIT_FORCE_RUN=1 ./scripts/audit.sh`) gives rc=0 with cargo-audit, pnpm-audit and buf-breaking all passed.
- **Fixed here:**
  - `fetch-depth: 2` on the scan checkout. Today the depth-1 checkout gives `BASE_SOURCE=ci-push-first-commit`, so buf compares HEAD to HEAD and the breaking check is vacuous. **Evidence:** `GITHUB_EVENT_NAME=schedule` takes the non-PR branch of `_get_base_ref.sh`. In a depth-2 clone, `GITHUB_ACTIONS=true GITHUB_EVENT_NAME=schedule ./scripts/lang/_get_base_ref.sh` gives `BASE_REF=3ee98d55dd407ef6d780acd2330bc9f1bec59af6 BASE_SOURCE=ci-push-main DIFF_MODE=two-dot FILES_CHANGED=20`, i.e. the parent commit. This is re-recorded on the final tree at Gate 2.
  - The `::error::` and issue wording claimed "unsuppressed advisories" for any failure; it now points at the `STATUS=FAIL REASON=` lines.
  - The S1/S2 job split (§2b).
- **Docs sweep (operations A):**
  - `docs/contributor/audit-suppressions.md` §Dependabot: grouping, cooldown, and that cooldown is version-updates-only.
  - `docs/runbooks/devloop-validation.md` (lines around 539 and 610): the scan/report job split and issue wording.
  - The issue title becomes neutral ("Scheduled audit failing — see STATUS=FAIL REASON= in the latest comment"). The dedup finds the open issue by label plus the current or old title, and renames #70 rather than orphaning it (Gate 2, operations F-OPS-2 / code-reviewer F3).

### 7. Stale-reference fixes (observability 4, Lead ruling)
- `crates/mc-service/src/observability/metrics.rs:2446`: the comment says "DebuggingRecorder"; it should be MetricAssertion's own recorder.
- `crates/mh-service/src/observability/metrics.rs:2446/2461/2499` and `crates/mh-service/tests/{webtransport,token_refresh,gc}_integration.rs`: `Snapshotter::snapshot` → `MetricAssertion` snapshot.
- `docs/decisions/adr-0032-metric-testability.md:58,181`: the "metrics-util DebuggingRecorder"-backed wording.
- `docs/TODO.md:878`: the DebuggingRecorder reference; I'll recheck whether the entry is still accurate.

### Verification still owed (host-side, the Lead's). Each item is NOT DONE until its result or URL is recorded here.
- `workflow_dispatch` audit-scheduled.yml on the pushed branch. This covers the github-script v9 issue path end to end, plus download-artifact and the job split.
- `workflow_dispatch` fuzz-nightly.yml on the pushed branch (upload-artifact v7, checkout v7, rustup step). Owner: Lead. Its corpus-upload step must go green.
- The next Dependabot "Check for updates" for npm and cargo (grouping).
- ci.yml / ci-client.yml: checkout v7, setup-node v7 with `node-version-file`, action-setup v6 without `version:`, cache v6, codecov v7 `files:`, rustup steps. The PR triggers are `branches: [main, develop]` and neither workflow has `workflow_dispatch`, so a PR into `update_deps_fix_dev_flow` runs no CI. **Lead ruling: add `workflow_dispatch:` to both** (done in this diff). The Lead dispatches both on the pushed branch. Layers 1–6 are **not** evidence for any YAML change in this diff.
- `devloop.sh --rebuild` on the host: every existing devloop image is stale once `PNPM_PACKAGE_MANAGER` is required. A build without it fails loudly, which is correct. Existing images also bake pnpm 10, so anything run inside a stale container fails the `packageManager` check until it is rebuilt (operations 4).

### Rollback units
- **Independent:** dependabot.yml, the engines change, base64 (workspace + fuzz + tests), and the workflow action bumps with SHA pins.
- **Must revert together:**
  - `metrics` 0.24.6 + exporter 0.18 + metrics-util removal + the mh test migration.
  - `packageManager` hash + `infra/lib/package-manager.sh` + devloop.sh + Dockerfile + dev-web.sh + the workflow `version:` removal. Restoring a `version:` with a different value errors.
  - The pnpm 12 migration (`pnpm-workspace.yaml` + `package.json` + `.npmrc` deletion + `packageManager` + lockfile env document + `scripts/lang/_pnpm.sh` and its callers). Reverting only part of it makes pnpm silently drop overrides or build classification.
  - Rust 1.95 image ARGs + the setup.test.sh drift check.
  - The guard + workspace hoist. Reverting the hoist alone reds R1/R2.
  - audit-scheduled job split + the artifact actions.

### Out of scope / surfaced
- **CI pnpm binary not hash-verified.** action-setup strips the `packageManager` integrity suffix. Security ruled this a deferral: corepack is being unbundled from Node 25+, and the swap would reorder setup-node caching at 5 sites. It is filed in `docs/TODO.md` §Supply Chain and recorded under Accepted Deferrals.
- pnpm 10 → 11: this will arrive through the npm job once §5 lands.
- Rust bump to latest stable from the one pin: existing TODO, reworded.
- CI verification of the pnpm integrity hash: a security ruling, see §3a.

### Reviewer item map
- **security:** S1 → §2b. S2 → §2b. S3 → §2 + Verification. S4 → §2. S5 → §4 + table. S6 → §1. S7 → §3a. S8 → §6.
- **test:** 1 → §4 (single `metrics`). 2 → §4 suites. 3 → §1 directories + R3. 4 → §4b. 5 → §2 + Verification. 6 → §1 Validation.
- **code-reviewer:** 1/2 → §4b. 3 → §4 (no `#[allow]`; `#[expect]` with a reason only if unavoidable). 4 → §3a. 5 → §1. 6 → table.
- **operations:** 1/2 → §4. 3 → §2 Runners. 4 → §2/§3a. 5 → Verification. 6 → §1. 7 → §6 docs sweep. A–E: A → §1/§6, B → §4 render diff, C/D → Rollback units/Verification, E → noted (Lead ruled separate groups).
- **dry-reviewer:** 1–7 → Mechanism 1 + §3/§4/§4b. The Rust toolchain is unified and checked (§3c).
- **observability:** 1a → §1 metrics-stack. 1b → §4 hoist. 1c → R4. 2 → §4 removal. 3 → §1 otel comment. 4 → §7.
- **paired-auth-controller:** 1 → table. 2 → §4. 3 → §4 tests. 4 → §4 fuzz. 5 → §4 + R1. 6 → §4 feature evidence.

---

## Pre-Work

None.

---

## Implementation Summary

### Dependabot config (`.github/dependabot.yml`)
| Item | Before | After |
|------|--------|-------|
| cargo groups | `cargo-minor-patch` only | `otel-web-stack` (opentelemetry*, tracing-opentelemetry, axum*, tower*, tonic*, prost*), `metrics-stack` (metrics, metrics-exporter-prometheus), then `cargo-minor-patch` |
| cargo scope | `/` | root block `directory: "/"` (unchanged) plus a **second cargo block** for the two fuzz crates: `directories: [/crates/ac-service/fuzz, /crates/media-protocol/fuzz]`, `group-by: dependency-name`, `allow: [libfuzzer-sys]` (the fuzz-only crates), same cooldown and strategy |
| cargo strategy | default | `versioning-strategy: increase-if-necessary` |
| npm groups | `npm-minor-patch` (buf excluded) | `npm-otel`, `npm-protobuf-es` (explicit names, not `@bufbuild/*`), `npm-vitest`, then `npm-minor-patch` (buf exclude + rationale kept) |
| actions | `actions-minor-patch` | `github-actions` with `patterns: ["*"]` (majors included) |
| cooldown | none | `default-days: 5` everywhere; `semver-major-days: 14` for cargo/npm (actions supports default-days only) |
- The header contract is kept: no `ignore:`; `audit-suppressions.toml` is the SSoT.
- New comments cover first-match group ordering, the otel coupling evidence (PR #75), and that cooldown delays version updates only, never security updates (ADR-0033 §12).
- `scripts/audit-suppressions-check.sh` → `STATUS=OK REASON=suppressions-clean`.
- YAML syntax: parsed clean with `yaml@2.9.0`. Schema fidelity was checked against the options reference by hand; the proof is the next Dependabot run.

**Why two cargo blocks (Gate 2, F-OPS-1; Lead ruling).** Evidence:
- **Docs.** `dependabot-options-reference.md` §`group-by`: "When set to `dependency-name`, Dependabot will create a single pull request for each dependency update across all specified directories, rather than separate pull requests per directory." So the default is one PR per directory, and the cross-directory merge is per dependency. That rules out listing the fuzz crates as root `directories`: it would either split PRs per directory or break the pattern groups.
- **dependabot-core.** `cargo/lib/dependabot/cargo/file_parser.rb:163-170` skips any `[workspace.dependencies]` entry the root `Cargo.lock` does not resolve (`next if lockfile && !version_from_lockfile(name, requirement)`). So a fuzz-only crate (`libfuzzer-sys`), even when hoisted to the root table, never gets a PR from the root block.
- **Resulting shape.**
  - Fuzz-only crates get the fuzz block, guarded by R6.
  - Shared crates move with the root PR, with R3 forcing the fuzz literal edit in that same PR.
  - R6 has distinct tokens for: the block missing or overlapping; a fuzz-only crate missing from `allow:`; an extra `allow:` name.
  - A root `exclude` entry with no manifest is the precondition `workspace-deps-excluded-manifest-missing`.

### GitHub Actions (all four workflows)
- **Pinning.** Every `uses:` is SHA-pinned with a `# vX.Y.Z` comment (SHAs from `git ls-remote` on the tag):
  - checkout v7.0.1
  - setup-node v7.0.0
  - pnpm/action-setup v6.1.0
  - cache v6.1.0
  - upload-artifact v7.0.1
  - download-artifact v8.0.1 (new)
  - github-script v9.0.0
  - codecov-action v7.1.1
  - rust-cache v2.9.2
  - taiki-e/install-action v2.87.22, moved from branch refs `@cargo-audit`/`@cargo-llvm-cov` to `with: tool:`
- **Removed actions.** `dtolnay/rust-toolchain` (`@master`, `@nightly`) is replaced by `rustup toolchain install … && rustup default …` run steps.
- **Checkout.** `persist-credentials: false` on every checkout; no step needs authenticated git.
- **Version sources.** pnpm/action-setup has no `version:` (it reads `packageManager`), and setup-node uses `node-version-file: '.nvmrc'`.
- **Triggers.** ci.yml and ci-client.yml gain `workflow_dispatch:` (Lead ruling). ci-client.yml also gains `.nvmrc` in its PR path filter.
- **Codecov (ci-client).** `files: packages/sdk-core/coverage/coverage-final.json`, the only coverage file `pnpm test:unit` writes (verified by running it).
- **audit-scheduled.yml** is split into two jobs:
  - `scan`: `contents: read`, `fetch-depth: 2`, uploads the output with `if: always()`, and keeps its final `exit 1` on failure.
  - `report`: `needs: scan`, `if: always()`, `issues: write`, no checkout or install. Download has `continue-on-error`. github-script reads `AUDIT_FAILED`/`SCAN_RESULT` from env and decides clean / drift / incomplete. Only clean closes the issue. Drift and incomplete both comment (or open the issue) and call `core.setFailed`.
  - The issue wording now points at the `STATUS=FAIL REASON=` lines.
- **Lint.** `actionlint` 1.7.12 is clean on all four workflows (no shellcheck on this host); the baseline was also clean.

### pnpm 10.33.2 → 12.8.1 and the config move
- `pnpm-workspace.yaml` now holds every pnpm setting (`engineStrict`, `strictDepBuilds`, `allowBuilds`, `verifyDepsBeforeRun: error`, `overrides`), with the `.npmrc` rationale comments moved alongside.
- `.npmrc` is deleted.
- `package.json` loses its `pnpm` field; `packageManager` is `pnpm@12.8.1+sha512.f64ba907…2aabbe45`; `engines.node` is `>=22.13.0`.
- `pnpm-lock.yaml` gains the env document: 158 lines recording `packageManagerDependencies: pnpm 12.8.1` and the per-platform `@pnpm/exe.*` integrity.
- **New `scripts/lang/_pnpm.sh`:**
  - `pnpm_deps_fresh` → `REASON=pnpm-deps-stale`, decided on the exit status of `pnpm exec true`, with pnpm's stderr shown afterwards.
  - `pnpm_exec_unverified` is the one place the verify step is bypassed.
  - Wired into `_buf.sh` (the version probe goes through the bypass, then a 5th state `pnpm_deps_fresh`) and the TS compile/lint/test/fmt wrappers.
- **New `infra/lib/package-manager.sh`.** It is the one `packageManager` reader and the one place the `+sha512` suffix is stripped. Consumers:
  - `devloop.sh` → Dockerfile `ARG PNPM_PACKAGE_MANAGER` (no default, `:?` fail-loud; `corepack prepare "<spec>"` verifies the hash).
  - `scripts/dev-web.sh`.
- **TS audit (`scripts/lang/ts/audit.sh`).**
  - It fails closed on an unrecognised JSON shape (`pnpm-audit-unrecognised-output`).
  - It **fixes a pre-existing bug** (since ac6bc96e, 2026-06-06): the JSON was read from the stdin that `python3 -` had already consumed for its program. Every scan was therefore decided on pnpm's exit code alone, and the GHSA suppression filter never saw an advisory. The program now goes in via `python3 -c` and the JSON stays on stdin, which has no size cap.
  - Pass requires an explicit OK decision, otherwise `pnpm-audit-no-decision`.
  - Gate 2 (test F3): the never-exercised npm-v2 `vulnerabilities` parser is removed, so that shape now fails as unrecognised. A `cves: null` crash in the v1 id fallback is fixed. Empty output is `pnpm-audit-unrecognised-output` whatever pnpm's rc (security).
  - Gate 2 (code-reviewer F1 / test F2): `pnpm_deps_fresh` names a missing pnpm `pnpm-unavailable`, never `pnpm-deps-stale`.

### Rust dependencies
- **Workspace SSoT.**
  - Every literal redeclaration of a workspace dep is converted: base64 ×5, hex, anyhow, uuid.
  - 13 multi-member literals are hoisted at the highest existing lower bound: metrics, metrics-exporter-prometheus, reqwest (defaults off, json+rustls-tls), http-body-util, futures 0.3.31, tokio-stream, clap, regex 1.10, serde_norway, wiremock, tempfile, assert_cmd, rcgen.
  - `bytes` → 1.9 and `uuid` → 1.11, matching existing member and fuzz requirements.
- **base64 0.22 → 0.23.1** (`default-features = false, features = ["std"]`). ac-service moves 0.21.7 → 0.23.1. The ac fuzz crate repeats the same entry. No source churn.
- **metrics-util removed.** The direct dependency is gone from ac/gc/mc/mh, from common (deps, dev-deps, `test-utils` feature) and from the workspace. The 3 mh `connection.rs` tests use `MetricAssertion`: `assert_delta(1)` on the timeout arm, `assert_unobserved()` on cancel/registered. It remains transitively via the exporter, at 0.20.4 only.
- **Metrics stack.** `metrics` 0.24.3 → 0.24.6 and `metrics-exporter-prometheus` 0.16.2 → 0.18.3. No API churn.
- **Rust version.** The 5 image Dockerfiles go `ARG RUST_VERSION=1.91` → `1.95`. `scripts/setup.test.sh` `rust_versions()` now requires one value across the image ARGs, the devloop image `FROM`, and every workflow `rustup toolchain install` and `rustup default`. Floor: ≥14 sites. Negative controls: one drifted site, and install/default disagreeing within one step.

### Guard `dt-guard workspace-deps` (R1–R6)
- New module `crates/dt-guard/src/workspace_deps.rs`.
- Shared root-manifest parse in `crates/dt-guard/src/common/cargo_manifest.rs`, used by both this guard and `release_build_profile`.
- Wrapper `scripts/guards/simple/validate-workspace-deps.sh`; e2e `crates/dt-guard/tests/workspace_deps_e2e.rs`.
- **Deviation:** status lines must be kebab-case with no spaces, so the member counts are carried in the reason token, `REASON=workspace-deps-clean-members-16-excluded-2`. The real-repo e2e parses N and compares it with the length of root `[workspace] members`.
- Empty `[workspace.dependencies]` raises the precondition `workspace-deps-workspace-dependencies-empty`, and R2 is still evaluated.

### Docs
- **Re-pointed to the new pnpm config:**
  - `docs/contributor/audit-suppressions.md` §Dependabot: grouping, cooldown, `minimumReleaseAge`.
  - `scripts/lang/ts/audit-remediation.md`: overrides location.
  - The ci-client.yml comment.
  - `docs/specialist-knowledge/{security,client}/INDEX.md`.
- **`docs/runbooks/devloop-validation.md`:**
  - token rows for `pnpm-deps-stale` and `pnpm-audit-unrecognised-output`;
  - the scheduled-scan job split and issue wording;
  - `buf-version-mismatch` noted as kept under a stale tree.
- **`docs/runbooks/client-dev-local.md`:** `engineStrict` location, plus a new F20 (pnpm 12 migration, `verifyDepsBeforeRun`). The F4 command now uses the hash-carrying spec.
- **`docs/TODO.md`:**
  - Rust entry narrowed to "latest stable from the one pin".
  - New §Supply Chain entry "CI pnpm binary is not hash-verified" (accepted deferral).
  - SHA-pin entry marked done.
  - :257 pnpm-copies clause corrected.
  - :878 metrics-util references corrected.
  - :1882 `.npmrc` / `<23` / `node-version` clauses corrected.
- **Stale recorder comments:**
  - ADR-0032 lines 58 and 181.
  - mc `metrics.rs:2446`.
  - mh `metrics.rs` and `tests/{webtransport,token_refresh,gc}_integration.rs`, Snapshotter → MetricAssertion.
- **`docs/specialist-knowledge/infrastructure/INDEX.md`:** the new pointers.

### Evidence (real tree)

**pnpm 12.8.1 proofs.** Run in this container with `COREPACK_HOME` pointed at a writable cache, because the stale image's `/opt/corepack` is read-only and still bakes pnpm 10 (see Remaining host-side actions).
- (c) `pnpm install --frozen-lockfile` → rc 0 (first run writes the 158-line env document). After `rm -rf` of every `node_modules`, a second `pnpm install --frozen-lockfile` → rc 0, and `cmp pnpm-lock.yaml` is unchanged.
- (b) That fresh install has 0 lines matching "ignored build". Its only WARN is the container's store location: `The store at /home/dev/.local/share/pnpm/store/v11 is not used because packages cannot be hard linked…`.
- (a) `pnpm audit --audit-level high` → `3 vulnerabilities found / Severity: 3 moderate` (0 high).
  - A1 positive control: `pnpm audit --json` keys are `top: ["advisories","metadata"]`, with entries `{github_advisory_id:"GHSA-82fw-gwwq-j7x9",severity:"moderate",module_name:"vitest"}`, `{…"GHSA-82fw-gwwq-j7x9",…"@vitest/mocker"}` and `{…"GHSA-hrr3-gc8f-f4qj",…"fast-uri"}`. These are the fields the wrapper reads.
- **engineStrict, below the floor.** Node v22.12.0 → `Error: ERR_PNPM_UNSUPPORTED_ENGINE … Unsupported engine for /work: wanted: {"node":">=22.13.0"} (current: {"node":"22.12.0"})`.
- **Node 24, the Dependabot fix.** Node v24.21.0 → `Lockfile is up to date, resolution step is skipped / Done in 25ms using pnpm v12.8.1`, rc 0.
- **Negative controls (scratch trial on 12.8.1).**
  - Deleting `overrides:` → `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH`.
  - Deleting `msw` from `allowBuilds` → `ERR_PNPM_IGNORED_BUILDS`.
  - A misspelled key → `ERR_PNPM_UNRECOGNIZED_WORKSPACE_SETTINGS`.
- **Stale-tree tokens (real tree, package.json edited without install, then restored).**
  - buf pin → 1.71.0: `proto/lint.sh` → `STATUS=FAIL REASON=buf-version-mismatch` ("buf 1.72.0 != pinned @bufbuild/buf 1.71.0").
  - prettier pin changed: `proto/lint.sh` → `STATUS=FAIL REASON=pnpm-deps-stale` + `Error: ERR_PNPM_VERIFY_DEPS_BEFORE_RUN`; `ts/lint.sh` → `STATUS=FAIL REASON=pnpm-deps-stale`.

**base64.**
- The pinning tests ran on the pre-bump versions (ac-service base64 0.21.7, common 0.22.1) **before** the bump, and again after it (both 0.23.1). Both runs: `config::tests::test_from_vars_master_key_base64_strictness_pins ... ok` (1 passed) and `jwt::tests::{test_decode_jwk_strictness_pins,test_decode_pem_strictness_pins,test_extract_kid_rejects_padded_header} ... ok` (3 passed).
- Full lib suites after the bump: common 229 passed, ac-service 395 passed.
- Feature tree, workspace (`cargo tree --workspace --target all -e features,normal,dev,build -i base64@0.23.1`). No `simd-unsafe` and no `base64 feature "default"` edge anywhere (grep count 0):
  ```
  base64 v0.23.1
  ├── base64 feature "alloc"
  └── base64 feature "std" (*)
  ```
- The same command in `crates/ac-service/fuzz` gives identical root edges (`alloc`, `std` only). The fuzz crate passes `cargo check`.
- jsonwebtoken keeps its own base64 0.22.1, so 0.22.1 and 0.23.1 are the only versions in the lock.

**metrics.**
- `cargo tree --workspace -e normal,dev,build -i metrics --depth 0` → `metrics v0.24.6`. That is a single version; a duplicate makes `-i metrics` fail as ambiguous.
- `cargo tree … -i metrics-util --depth 1` → `metrics-util v0.20.4 └── metrics-exporter-prometheus v0.18.3`. It is transitive only, with one version; 0.18.0 and 0.19.1 left the lock.
- **/metrics render diff** (mc-service, `configured_prometheus_builder()` + `handle.render()` over 30 `record_*`/`set_*` calls covering counters, gauges and histograms with buckets; scratch test since deleted).
  - The 0.16.2 and 0.18.3 renders each have 121 non-empty lines, and sorted they are **byte-identical** (`diff <(sort before) <(sort after)` rc 0). Only the series order differs, and that order is hash-based in both versions.
  - No `_total` was added or doubled (`grep -c _total_total` = 0). `_bucket`/`_sum`/`_count` names and `le` formatting are unchanged, and no unit suffix appears.
  - No HELP lines exist in either render: no service calls `describe_*!`, so 0.18's HELP/TYPE `_total` fix has no surface here.
- `cargo check --workspace 2>&1 | grep -c "default-features is ignored"` → 0.

**Guard on the real repo.** `SCOPE: 16 members, 2 excluded crates, 53 workspace deps, 509 lock packages, 0 hits` / `STATUS=OK REASON=workspace-deps-clean-members-16-excluded-2`. Before the hoist it reported 26 hits (R1 12, R2 13, R3 1). After R2 started counting excluded crates (DRY Gate-2 F2), it flagged `libfuzzer-sys` pinned in both fuzz crates. That is now hoisted to root as a member-unused entry that R3 binds both fuzz crates to. A listed facade crate absent from the lock is now the precondition `workspace-deps-facade-absent-from-lock` (observability Gate-2 F1). `cargo test -p dt-guard`: 847 passed, 0 failed (`workspace_deps_e2e` 9). The real-repo e2e asserts the exact `members-16-excluded-2` token, with both counts read from the root manifest.

**Scheduled-audit base ref.** `GITHUB_ACTIONS=true GITHUB_EVENT_NAME=schedule scripts/lang/_get_base_ref.sh`:
- Depth-2 clone: `BASE_REF=3ee98d55dd407ef6d780acd2330bc9f1bec59af6 BASE_SOURCE=ci-push-main`.
- Depth-1 clone (old workflow): `BASE_REF=cf0fcd5c… BASE_SOURCE=ci-push-first-commit` (HEAD vs HEAD).

**Self-tests.**
- `scripts/lang/_pnpm.test.sh` 16/16
- `scripts/lang/ts/audit.test.sh` 21/21
- `scripts/lang/proto/fmt.test.sh` 30/30 (was 25)
- `scripts/lang/proto/golden-format.test.sh` 4/4
- `scripts/setup.test.sh` 685/685
- `scripts/dev-web.test.sh` 183/183

### Remaining host-side actions (NOT DONE until recorded)
- [ ] `workflow_dispatch` audit-scheduled.yml on the pushed branch (github-script v9, the job split, download-artifact). Run URL: —
- [ ] `workflow_dispatch` fuzz-nightly.yml on the pushed branch (upload-artifact v7, checkout v7, the rustup step). The corpus upload must go green. Run URL: —
- [ ] ci.yml and ci-client.yml via a **draft PR against `main`** (their new `workflow_dispatch:` is only dispatchable once on the default branch). Record: Codecov upload step green in both; ci-client actions/cache save step no error. Covers checkout v7, setup-node v7 + `.nvmrc`, action-setup v6 without `version:` on pnpm 12, cache v6, codecov v7 `files:`, rustup steps). Run URLs: —
- [ ] Post-merge: Dependabot "Check for updates" for npm (expected to run for the first time), both cargo blocks (groups) and actions. Confirm Insights → Dependency graph → Dependabot shows **no config error** for `.github/dependabot.yml` (a rejected file stops ALL updates, not just the fuzz block), and that the fuzz cargo block's run lists `libfuzzer-sys` (F-OPS-4).
- [ ] Close issue #70 after a clean dispatch.
- [ ] `devloop.sh --rebuild` on the host. Existing devloop images bake pnpm 10 and a read-only `/opt/corepack`, so pnpm fails inside a stale container until it is rebuilt.

---

## Files Modified

```
 .github/dependabot.yml                             |  100 +-
 .github/workflows/audit-scheduled.yml              |  212 +++-
 .github/workflows/ci-client.yml                    |   59 +-
 .github/workflows/ci.yml                           |   54 +-
 .github/workflows/fuzz-nightly.yml                 |   13 +-
 .npmrc                                             |   21 -
 Cargo.lock                                         |  213 ++--
 Cargo.toml                                         |   36 +-
 crates/ac-service/Cargo.toml                       |   21 +-
 crates/ac-service/fuzz/Cargo.toml                  |    2 +-
 crates/ac-service/src/config.rs                    |   63 +
 crates/ac-test-utils/Cargo.toml                    |    6 +-
 crates/common/Cargo.toml                           |   14 +-
 crates/common/src/jwt.rs                           |  125 ++
 crates/common/src/observability/mod.rs             |    4 +-
 crates/devloop-helper/Cargo.toml                   |    4 +-
 crates/dt-guard/Cargo.toml                         |   10 +-
 crates/dt-guard/src/common/cargo_manifest.rs       |  297 +++++
 crates/dt-guard/src/common/mod.rs                  |    1 +
 crates/dt-guard/src/lib.rs                         |    1 +
 crates/dt-guard/src/main.rs                        |   13 +
 crates/dt-guard/src/release_build_profile.rs       |  108 +-
 crates/dt-guard/src/workspace_deps.rs              | 1204 ++++++++++++++++++++
 crates/dt-guard/tests/workspace_deps_e2e.rs        |  258 +++++
 crates/dt-story/Cargo.toml                         |    8 +-
 crates/env-tests/Cargo.toml                        |   10 +-
 crates/gc-service/Cargo.toml                       |   19 +-
 crates/gc-test-utils/Cargo.toml                    |    4 +-
 crates/mc-service/Cargo.toml                       |   16 +-
 crates/mc-service/src/media_routing/teardown.rs    |    5 +
 crates/mc-service/src/observability/metrics.rs     |    2 +-
 crates/mc-test-utils/Cargo.toml                    |    6 +-
 crates/mh-service/Cargo.toml                       |   14 +-
 crates/mh-service/src/observability/metrics.rs     |   10 +-
 crates/mh-service/src/webtransport/connection.rs   |   62 +-
 crates/mh-service/tests/gc_integration.rs          |    2 +-
 .../mh-service/tests/token_refresh_integration.rs  |    2 +-
 .../mh-service/tests/webtransport_integration.rs   |    2 +-
 docs/TODO.md                                       |   24 +-
 docs/contributor/audit-suppressions.md             |   17 +
 docs/decisions/adr-0032-metric-testability.md      |    6 +-
 .../2026-10-03-dependabot-hygiene/main.md          |  880 ++++++++++++++
 docs/runbooks/client-dev-local.md                  |   35 +-
 docs/runbooks/devloop-validation.md                |   11 +-
 docs/specialist-knowledge/client/INDEX.md          |    2 +-
 docs/specialist-knowledge/infrastructure/INDEX.md  |    4 +-
 docs/specialist-knowledge/security/INDEX.md        |    2 +-
 infra/devloop/Dockerfile                           |   23 +-
 infra/devloop/devloop.sh                           |   19 +-
 infra/docker/ac-service/Dockerfile                 |    2 +-
 infra/docker/db-migrate/Dockerfile                 |    2 +-
 infra/docker/gc-service/Dockerfile                 |    2 +-
 infra/docker/mc-service/Dockerfile                 |    2 +-
 infra/docker/mh-service/Dockerfile                 |    2 +-
 infra/lib/package-manager.sh                       |   45 +
 package.json                                       |   20 +-
 pnpm-lock.yaml                                     |  158 +++
 pnpm-workspace.yaml                                |   49 +
 scripts/dev-web.sh                                 |   24 +-
 scripts/dev-web.test.sh                            |    7 +-
 scripts/guards/simple/validate-workspace-deps.sh   |    5 +
 scripts/lang/_pnpm.sh                              |   42 +
 scripts/lang/_pnpm.test.sh                         |   74 ++
 scripts/lang/proto/_buf.sh                         |   15 +-
 scripts/lang/proto/breaking.sh                     |    2 +-
 scripts/lang/proto/compile.sh                      |    2 +-
 scripts/lang/proto/fmt.sh                          |    2 +-
 scripts/lang/proto/fmt.test.sh                     |   26 +-
 scripts/lang/proto/lint.sh                         |    2 +-
 scripts/lang/ts/audit-remediation.md               |    3 +-
 scripts/lang/ts/audit.sh                           |   61 +-
 scripts/lang/ts/audit.test.sh                      |  106 ++
 scripts/lang/ts/compile.sh                         |    2 +
 scripts/lang/ts/fmt.sh                             |    3 +
 scripts/lang/ts/fmt.test.sh                        |    6 +
 scripts/lang/ts/lint.sh                            |    2 +
 scripts/lang/ts/test.sh                            |    2 +
 scripts/layer3.sh                                  |    2 +
 scripts/setup.test.sh                              |   91 +-
 79 files changed, 4232 insertions(+), 553 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| (see §Planning per-section file lists and §Cross-Boundary Classification) | |

---

## Devloop Verification Steps

### Gate 2 — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (Lead, final tree)
Run with `COREPACK_HOME=<writable scratch>` (container image still bakes pnpm 10 until `devloop.sh --rebuild`). rc=0, no `FMT_APPLIED`.

| Layer | Result | Duration | Children |
|-------|--------|----------|----------|
| 1 Compile | OK | 8s | |
| 2 Format | OK | 4s | |
| 3 Guards | OK | 159s | guards-passed (incl. new validate-workspace-deps R1–R6, _pnpm.test.sh, ts/audit.test.sh) |
| 4 Test | N/A | 264s | cargo-test-passed, nx-test-passed; N/A from proto placeholder only |
| 5 Lint | OK | 1s | cargo-clippy, nx-lint, buf-lint passed |
| 6 Audit | N/A | 4s | cargo-audit, pnpm-audit, buf-breaking passed; N/A from proto placeholder only |
| 7 Env-tests | OK | 1280s | env-tests-passed, browser-e2e-passed (service images rebuilt on Rust 1.95, exporter 0.18.3) |

`TOTAL_RESULT=N/A` (worst child = intentional proto placeholders; no FAIL/PRECONDITION).

Layer-fast iterations (implementer): runs 1–4 red (guard table/INDEX cap, pnpm-deps-stale from fmt.test.sh fixture, scope-table race), run 5 green; run 6 red = run-story containment check tripped by concurrent edits (not reproduced on quiet tree), runs 7–8 green.

---

## Code Review Results

See §Gate 3 — Verdicts (end of file) for the verdict table; per-reviewer findings are summarised there and in §Issues Encountered.

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

- `docs/TODO.md` §Supply Chain — CI pnpm binary is not hash-verified (action-setup strips it)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `cf0fcd5c`
2. Review all changes: `git diff cf0fcd5c..HEAD`
3. Soft reset (preserves changes): `git reset --soft cf0fcd5c`
4. Hard reset (clean revert): `git reset --hard cf0fcd5c`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: these units revert together, never in part:
  - (1) `metrics` 0.24.6 + exporter 0.18.3 + metrics-util removal + the mh test migration.
  - (2) The pnpm 12 migration: `pnpm-workspace.yaml`, `package.json`, `.npmrc` deletion, the lockfile env document, `infra/lib/package-manager.sh` and its callers (devloop.sh, Dockerfile, dev-web.sh), `scripts/lang/_pnpm.sh` and its callers, and the workflow `version:` removal. A partial revert makes pnpm silently drop overrides or build classification, or makes action-setup error on two pnpm versions.
  - (3) Rust 1.95 image ARGs + the setup.test.sh drift check.
  - (4) The workspace hoist + the dt-guard workspace-deps guard. Reverting only the hoist reds R1/R2.
  - (5) The audit-scheduled job split + the download/upload artifact steps.

  These revert independently: dependabot.yml, `engines.node`, base64 (workspace + fuzz + pinning tests; the pinning tests are safe to keep on a revert), the TS audit fix + its test, and the action SHA pins. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: TS audit parser never saw the audit JSON (pre-existing, since 2026-06-06)
**Problem**: `scripts/lang/ts/audit.sh` ran its decision as `python3 - … <<'PY'` and then read `sys.stdin`. Python had already consumed stdin as its program, so `raw` was always empty and every scan was decided on pnpm's exit code alone. The GHSA suppression filter never saw an advisory. Issue #70's "remaining: <see pnpm audit output>" (09-07, 09-14) is this bug.
**Resolution**:
- The program moves to `python3 -c "$__audit_decide_py"` and the JSON stays on stdin. A first fix used an env var, but @security caught that an env/argv string is capped at 128 KiB (`MAX_ARG_STRLEN`): a large report would hit E2BIG and abort with no STATUS. Verified: a 210 000-byte env var gives `Argument list too long`, while stdin carries it.
- Pass now requires an explicit `^OK` decision; an empty or garbled one gives `pnpm-audit-no-decision`.
- `scripts/lang/ts/audit.test.sh` (21/21) includes the suppressed-high and names-its-GHSA cases (both fail on the old code), a >200 KiB report with one high, and a silent-python case.
- Impact check: the only TS suppression ever tracked, GHSA-h67p-54hq-rp68 (js-yaml, 2026-06-20 → 2026-09-20), was moderate, below the high gate, so the bug never mattered for it. `.pnpm-audit-ignore.json` is empty today. Reported to @security.

### Issue 2: `verifyDepsBeforeRun: error` misattributed a stale tree to `buf-not-installed`
**Problem**: a stale `node_modules` made `pnpm exec buf --version` refuse, which landed in `_buf.sh` state (2).
**Resolution**: the version probe now goes through `pnpm_exec_unverified`, and a new state 5 `pnpm_deps_fresh` follows it (`pnpm-deps-stale`). Both are verified on the real tree and in `fmt.test.sh`.

### Issue 4: A self-test re-installed into the repo's node_modules under pnpm 12
**Problem**: `scripts/lang/ts/fmt.test.sh` symlinks the repo `node_modules` into a synthetic nx workspace that has no pnpm config. pnpm >= 11 defaults `verifyDepsBeforeRun: install`, so its `pnpm exec nx …` re-installed into the REAL `node_modules` for the synthetic manifest. The next layer then failed `pnpm-deps-stale` ("The value of the allowBuilds setting has changed"), so the new preflight caught it loudly.
**Resolution**: the fixture writes `verifyDepsBeforeRun: false` into its own `pnpm-workspace.yaml` and copies `_pnpm.sh` beside its stub `_common.sh`. I checked that `pnpm exec true` stays fresh in the repo after `ts/fmt.test.sh`, `proto/fmt.test.sh`, `proto/golden-format.test.sh` and `dev-web.test.sh`.

### Issue 3: Stale devloop image cannot run the new pnpm
**Problem**: this container's corepack home `/opt/corepack` is read-only and bakes pnpm 10, so `pnpm` fails once `packageManager` names 12.8.1.
**Resolution**: none in-tree; it is expected. Validation here ran with `COREPACK_HOME` pointed at a writable cache. The host action `devloop.sh --rebuild` is listed above.

## Lessons Learned

1. Version bumps of pnpm (10→12) silently change where config is read from; every migrated setting needs a negative control proving it is read from the new home.
2. Dependabot cargo planning skips root `[workspace.dependencies]` entries absent from Cargo.lock and opens one PR per directory without `group-by`; verify Dependabot semantics from docs/dependabot-core source, not belief.
3. Guards and parsers must fail closed on "found nothing" (empty lock, unrecognised audit JSON, missing manifest) — three such vacuous passes were found in this loop.
4. Editing the tree during a validation run trips run-story's containment self-test; quiesce before running.

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

---

## Lead Notes (setup)

- Precondition: codec_decode fuzz fix is on `origin/main` as `b696b6b8` (patch-equivalent of `7df2ef1c`, per `git cherry`). Nightly run *status* on main NOT verified — `gh` unauthenticated and no `ssh` in container at setup.
- Remote-dependent scope (item 5/6 log diagnosis, push, workflow_dispatch verification) blocked on `gh auth`; requested from operator.
- Deviation: INDEX.md / review-protocol.md contents are loaded by each teammate reading the file as its first action, rather than inlined into the spawn prompt (7×~15KB inline would be pure retyping). Same content, same source files.

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed rev 3 |
| Test | confirmed rev 3 |
| Observability | confirmed rev 3 |
| Code Quality | confirmed rev 3 |
| DRY | confirmed rev 3 |
| Operations | confirmed rev 3 |
| Paired auth-controller (GSA) | confirmed rev 3 |

Gate 1 PASSED 2026-10-03: all 7 confirmed rev 3; validate-cross-boundary-classification STATUS=OK. Rev 3 delta (operator decisions 2026-10-03, not in rev 2): merged `otel-web-stack` cargo group; pnpm 10 → 11 with settings-location audit + 3 proofs; Lead ruling: `workflow_dispatch:` on ci.yml / ci-client.yml.

## Lead Notes (verification ownership, 2026-10-03)

- No `gh`/push from this environment (operator confirmed). Operator pushes and runs remote verification; Lead records URLs.
- Pre-merge: `gh workflow run fuzz-nightly.yml --ref feature/1-dependabot-hygiene`, same for `audit-scheduled.yml` (branch YAML runs; dispatch already enabled on main). ci.yml / ci-client.yml exercised via a draft PR against `main` (new `workflow_dispatch:` only becomes usable post-merge).
- Post-merge only: dependabot.yml config and the npm-job engines fix (Dependabot reads default branch) — verify via "Check for updates".

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 1 (+planning) | 1 | 1 | S-R1 fence injection fixed; deferral: CI pnpm hash (TODO §Supply Chain) |
| Test | RESOLVED-FIXED | 5 | 5 | 0 | excluded-count raw exclude len + manifest-missing precondition; pnpm-unavailable; npm-v2 branch deleted; version: needle; rust comment/check |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | R4 vacuous-pass precondition; stale DebuggingRecorder refs; MCPushQuiesceTimeouts label assertion added |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | pnpm-unavailable precheck; pnpm version-input check; neutral audit issue title; evaluate() split |
| DRY | RESOLVED-FIXED | 3 | 2 | 0 | F1 rustup default guarded; F2 libfuzzer-sys hoisted + R2 counts excluded; F3 withdrawn (metrics-util no longer direct) |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | fuzz Dependabot block + R6; neutral audit title; runbook rows; F-OPS-4 verification line (Lead edit) |
| Paired auth-controller (GSA) | CLEAR | 0 | 0 | 0 | base64 GSA: tests-only edits in jwt.rs/config.rs; simd-unsafe absent in workspace + fuzz feature trees |
