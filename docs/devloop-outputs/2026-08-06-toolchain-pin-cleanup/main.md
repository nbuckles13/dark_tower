# Devloop Output: Toolchain-Pin Cleanup (three-way Node pin drift)

**Date**: 2026-08-06
**Task**: Fix the `.nvmrc` / Dockerfile / lockfile-engines Node pin drift that caused the 2026-08-05 host dev-env failure (vite 8 / rolldown 1.2.1 raised the engines floor to >=22.12.0; `.nvmrc` still pinned 22.11.0; pnpm silently skipped the engines-mismatched optional native binding, so vite crashed at launch).
**Specialist**: infrastructure
**Mode**: Agent Teams (full) — HEADLESS (run-story task #61)
**Branch**: `feature/user-story-run-test`
**Duration**: ~1 session (headless); Gate 1: 2 plan-revision rounds (floor→22.13.0, drift-guard 4-member set); Gate 2: 1 layers-1–6 fix round (docs-formatting guard); Gate 3: all 7 reviewers CLEAR/RESOLVED-FIXED, 4 findings all fixed in-diff.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3ac76a431dbb669d8269e5d4254bb4dd74a47ece` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` / a2e43d9e771ed28d4 (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security` / a8799f2163fcfad7d — spawned |
| Test | `test` / a8abf7f036f6b7380 — spawned |
| Observability | `observability` / a42a21da60ad659d8 — spawned |
| Code Quality | `code-reviewer` / a9c4eea2eac047346 — spawned |
| DRY | `dry-reviewer` / a604f99dba25d6f0c — spawned |
| Operations | `operations` / a014952a14fe99e38 — spawned |
| Semantic Guard | `semantic-guard` / afd42650353b04ef1 — spawned |

---

## Task Overview

### Objective
Convert a whole class of dev-env failure (silent optional-binding skip → vite runtime crash) into a loud, actionable install-time error, and eliminate the three-way pin drift between `.nvmrc`, the devloop container Node, and the workspace lockfile engines floor.

### Scope
- **Service(s)**: none (toolchain/dev-env only)
- **Schema**: No
- **Cross-cutting**: dev tooling only; touches an operations-owned runbook and dev scripts

### Debate Decision
NOT NEEDED — bounded toolchain/config cleanup within existing conventions (fail-loudly, SSoT, config-over-hardcoding). No new architecture.

---

## Cross-Boundary Classification

<!-- Filled by implementer during planning; reviewed at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.nvmrc` | Mine · Mechanical | — (infrastructure; toolchain pin SSoT) |
| `.npmrc` (new, repo root) | Mine · Minor-judgment | — (infrastructure; single line `engine-strict=true`) |
| `package.json` (root, `engines.node`) | **Shared toolchain manifest** · Domain-judgment | infrastructure owns the pin; **flag code-reviewer + test + operations** — tightening `>=22 <23` → `>=22.13.0 <23` is a workspace-wide install gate, part of achieving fail-loudly (see Planning §crux) |
| `infra/devloop/Dockerfile` | Mine · Minor-judgment | — (infrastructure; Node pin via `ARG NODE_VERSION`) |
| `infra/devloop/devloop.sh` | Mine · Minor-judgment | — (infrastructure; read `.nvmrc`, pass `--build-arg` at both build sites) |
| `scripts/dev-web.sh` | Mine · Minor-judgment | — (infrastructure owns the executable preflight; runbook owns the *why*) |
| `scripts/generate-dev-certs.sh` | Mine · Mechanical (comment-only) | — (infrastructure; fix-don't-defer: corrected a stale comment (lines 27-31, 314-315) that mis-stated `fingerprints.env` as task-#18's reader — the root of a runbook error caught at review. No logic/crypto change) |
| `docs/runbooks/client-dev-local.md` | **Not mine** · Minor-judgment (prose) | **operations** — F11 add + §7/§6.5 drift fix; operations confirms Gate 1 **and** Gate 3 |
| `docs/TODO.md` | Mine · Mechanical | — (infrastructure; track the drift-guard, not implement it) |
| `docs/devloop-outputs/2026-08-06-toolchain-pin-cleanup/main.md` | Mine · Mechanical | — (this output doc) |

None of these paths are Guarded Shared Areas (no wire-format / auth / crypto / schema / forensics). Highest-attention rows: the root `package.json` `engines` tightening (shared toolchain contract) and the operations-owned runbook.

---

## Planning

### The crux — engine-strict alone does NOT bite; root `engines` must be tightened too

Verified against the lockfile (`pnpm-lock.yaml`): the **runtime-crash-class** engines floor for
`vite@8.2.0`, `rolldown@1.2.1`, and `@rolldown/binding-linux-x64-gnu@1.2.1` is
`{node: ^20.19.0 || >=22.12.0}` (lines 636–694, 2104, 2396) — i.e. `>=22.12.0` on the 22.x line.
But per DRY/code-reviewer finding, that is **not the workspace max floor**. `eslint@10.8.0` (a
direct root devDependency, `package.json:23`) and its non-optional `@eslint/*` ecosystem declare
`^20.19.0 || ^22.13.0 || >=24` — i.e. `>=22.13.0` on the 22.x line. The distinct 22.x floors in
the tree are `{22.9.0, 22.12.0, 22.13.0}`; the **max over packages that actually install on
linux-x64-gnu is `22.13.0`** (the other `22.13.0` hits — `@rolldown/binding-wasm32-wasi`,
`@napi-rs/wasm-runtime` — are optional WASM fallbacks, os/cpu-skipped here, so they do not bind).
**Therefore the true declared floor is `>=22.13.0 <23`.** `.nvmrc`=22.23.1 satisfies it, so no
runtime harm — this is about the declared floor being the true SSoT max, and it gives strictly
better coverage (a dev on 22.12.x, which rolldown tolerates but eslint does not, now fails loud).

pnpm's `engine-strict=true` turns an **engines check against the running Node** into a hard
`ERR_PNPM_UNSUPPORTED_ENGINE` failure — but the check it hard-fails is the **root project's**
`engines.node`, not the offending optional dependency's. The optional native binding is skipped
by pnpm's own os/cpu/libc/engines *optional-dependency* filter, which is silent regardless of
`engine-strict`. So:

- Root `engines.node = ">=22 <23"` + `engine-strict`: Node 22.11.0 **satisfies** the root range →
  install passes → the optional binding is still silently skipped → vite crashes at launch.
  **Goal defeated.**
- Root `engines.node = ">=22.13.0 <23"` + `engine-strict`: Node 22.11.0 (and 22.12.x) **violates**
  the root range → `pnpm install` fails loudly with `ERR_PNPM_UNSUPPORTED_ENGINE` naming Wanted/Got.
  **Goal achieved.**

**Therefore deliverable (2) is two edits, not one:** create `.npmrc` with `engine-strict=true`
AND tighten root `engines.node` from `>=22 <23` to `>=22.13.0 <23` (the SSoT **max** floor across
non-optional deps — eslint's `^22.13.0` dominates rolldown's `>=22.12.0`). This is the SSoT
reconciliation the tracked drift-guard (deliverable 6) will later
automate. `engine-strict` guards the Node *version* at install time; it does not guarantee an
existing `node_modules` physically contains the binding — that residual case is exactly what the
`dev-web.sh` bundler-load probe (deliverable 4) covers. The two are complementary, not redundant.

### Per-deliverable approach

1. **`.nvmrc`**: `22.11.0` → `22.23.1` (parity with the container's running Node + above floor).
2. **`.npmrc`** (new): single line `engine-strict=true` (+ a one-line comment). No registry/auth/ssl
   directives. **Plus** the root-`engines` tightening above.
3. **Dockerfile Node pin — approach (a), ARG plumbed from `.nvmrc`.** Chosen over (b) because a
   hardcoded apt version in the Dockerfile would be a *second copy* of the pin (drift-prone); (a)
   keeps `.nvmrc` the single source. `ARG NODE_VERSION` with **no default** + a `${NODE_VERSION:?}`
   loud guard so a direct `podman build` without the arg fails rather than floating. The NodeSource
   `setup_22.x | bash -` line stays (adds the GPG-signed NodeSource apt repo — unchanged trust
   posture) and the install becomes `apt-get install -y --no-install-recommends
   nodejs=${NODE_VERSION}-1nodesource1`. `devloop.sh` reads repo-root `.nvmrc` and passes
   `--build-arg NODE_VERSION=...` at **both** build sites (lines ~60 and ~437) via a shared helper.
   In-tree edit only; image **rebake is the remaining host-side action** (out of scope).
4. **`dev-web.sh` bundler-load probe**: a `probe_bundler()` that resolves `vite` from the web-app
   package, then does a real dynamic **`import()`** of `rolldown`'s entry relative to vite —
   triggering the native-binding load at module-init (test confirmed rolldown@1.2.1 `dist/index.mjs`
   loads the binding at import time; a bare `require.resolve` would false-pass, so it MUST be a true
   import, never degraded to resolve-only, and the error MUST propagate to `HARD_FAIL=1` — no
   `|| true`). **Guarded on `node_modules` presence**: run it when the tree exists (so `--check` and
   `--no-install` catch the silent-skip case), skip-with-note on a fresh clone (no false-fail before
   first install — the fresh case is then caught loudly by `engine-strict` at `pnpm install`). It
   **distinguishes two failure causes** (test point 3): "vite/rolldown not resolvable at all"
   (incomplete tree → remedy: reinstall) vs "binding load threw" (→ remedy: the engine/binding
   message); both hard-fail, but the printed remedy differs so the binding-mismatch message isn't
   emitted for an unresolvable-package cause. **The HARD-FAIL message carries NO literal version**
   (D2 / observability-A / test): it surfaces the actual pnpm/rolldown error text and DERIVES any
   floor reference by reading root `package.json` `engines.node` (the in-repo SSoT after the
   tightening), or stays symbolic ("see `.nvmrc` and `package.json` engines"). The remedy stays
   `nvm install` per `.nvmrc` + `rm -rf node_modules` + `pnpm install`, and the message **explicitly
   supersedes** the earlier "same major, likely fine" Node WARN. No hardcoded versions (SSoT
   preserved). I can smoke-test the no-false-positive direction here (container Node 22.23.1 +
   `node_modules` present → probe must PASS).
5. **Runbook (operations-owned)**: add **F11** (not F10 — taken by CORS); update §5 header `(F1–F10)`
   → `(F1–F11)`, ToC, Changelog + Last-Updated. F11 shape = Symptom/Discriminator/Cause/Fix/Why-it-
   recurs. Discriminator separates it from F2/F3/F4: F11's error names an *engines* violation /
   native-binding load failure, and F11 is precisely the mode the major-only Node check **WARNs**
   on rather than hard-fails. §7 rewrite states the browser E2E lane **accurately** (per
   `scripts/layer7.sh`): **diff-triggered** (not always-run), runs **after** the Rust env-tests and
   only if they passed, gated on operator-lane preconditions (fingerprints.json + Playwright
   Chromium). §6.5's "Browser E2E is not part of Layer 7 — see §7" corrected **in lockstep**.
6. **`docs/TODO.md`**: add a node-pin drift-guard entry under **## Developer Experience**, same shape
   as the Playwright-pin guard entry (`dt-guard` subcommand). **Tracked, not implemented.** Cross-
   references this cleanup as the motivating incident. Per DRY finding D1, the agreement set is
   enumerated as **four** members across **two distinct invariants** (the `≡` shorthand conflated
   them):
   - **(a) exact equality:** `.nvmrc` == the Dockerfile Node pin (`ARG NODE_VERSION`).
   - **(b) floor-match:** the **lockfile engines floor** == root `package.json` `engines.node` lower
     bound; AND `.nvmrc` satisfies (>=) that floor. "Lockfile engines floor" is defined precisely as
     the **max lower-bound (on the 22.x line) across packages that actually install on the target
     platform** — i.e. excluding os/cpu/libc-skipped optional deps. Today that max is **22.13.0**
     (eslint@10.8.0, non-optional), NOT rolldown's 22.12.0, and NOT the optional WASM fallbacks'
     22.13.0 — the guard's floor computation must apply the same optional-skip filter pnpm does, or
     it will mis-derive the floor.
   Root `engines.node` is explicitly a guarded member (its omission would let it silently keep an old
   floor after a future dep bump — the exact drift class this task kills). CI's
   `.github/workflows/*.yml` `setup-node` `node-version: '22'` is named as an additional,
   DELIBERATELY-unpinned Node-pin **site**: safe-by-construction (floating major always resolves ≥
   floor), so it is explicitly OUT of the (a) equality set — but named so a future guard author
   neither forgets it nor mis-flags it as drift. Suggested clean resolution (for the guard task, not
   this cleanup): migrate CI to `node-version-file: .nvmrc`, which deletes the site entirely by
   deriving from the SSoT.

### Reviewer questions answered up front

- **@security (1)** `.npmrc` verbatim: `engine-strict=true` only (+ comment). No tokens/registry/
  `strict-ssl`. **(2)** Dockerfile keeps `setup_22.x | bash -` (GPG-signed apt repo — **no** posture
  change vs today's floating install; exact-pin is a tightening). Honest gap note: nodejs arrives via
  signed apt, *not* raw-curl+sha256 like the buf/kubectl blocks — that asymmetry is **pre-existing**,
  not introduced here. **(3)** F11 surfaces **no** new credential/secret values.
- **@operations** §7/§6.5 wording will not overstate the lane and will not cite stale TODO:616 as
  open; I'll describe only what exists on this branch.
- **@observability (1)** probe message supersedes the "likely fine" WARN explicitly. **(2)** F11
  fingerprint = the mode the major-only check WARNs on. **(3)** F11 quotes/interprets
  `ERR_PNPM_UNSUPPORTED_ENGINE` (Wanted/Got). I will **not** duplicate the lockfile floor into
  `dev-web.sh`'s Node check — that would fork the SSoT the tracked drift-guard owns; the supersede
  message is the reconciliation instead.

### Gate 1 confirmation tracked below.

### Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed (after floor 22.12.0→22.13.0 revision) |
| DRY | confirmed (after D1 drift-guard 4-member revision) |
| Operations | confirmed |
| Semantic Guard | confirmed |

---

## Implementation Summary

All six deliverables implemented in-tree; verified via `bash -n` + live exercise (shellcheck is not
installed in this devloop image — see remaining host-side action).

1. **`.nvmrc`** `22.11.0` → `22.23.1` (container parity + above floor).
2. **`.npmrc`** (new, repo root) `engine-strict=true` **+** root `package.json` `engines.node`
   `>=22 <23` → `>=22.13.0 <23` (true SSoT max floor; eslint@10.8.0 `^22.13.0` dominates rolldown's
   `>=22.12.0`). Together these convert the below-floor case into a loud `ERR_PNPM_UNSUPPORTED_ENGINE`.
   **Floor paper-trail (per observability):** the Planning §crux cites `>=22.12.0` as the *rolldown
   crash-class* floor (the incident's proximate trigger); the *shipped* `>=22.13.0` is the true
   workspace **max** once eslint@10.8.0's non-optional `^22.13.0` is included. The §crux and the
   drift-guard TODO both carry the fuller derivation — 22.12.0 is explanatory context, 22.13.0 is
   the chosen value.
3. **`infra/devloop/Dockerfile`** — `ARG NODE_VERSION` (no default; `${NODE_VERSION:?}` loud guard),
   exact apt pin `nodejs=${NODE_VERSION}-1nodesource1` + a `node --version` build-time assertion.
   **`infra/devloop/devloop.sh`** — added `read_node_version()` helper; both `podman build` sites
   (~L60, ~L437) now pass `--build-arg NODE_VERSION="$(<.nvmrc)"`. Header build comment updated.
4. **`scripts/dev-web.sh`** — `probe_bundler()` after the version checks: node_modules-gated, real
   dynamic `import()` of rolldown (resolved from vite), exit-3 (tree unresolvable → reinstall) vs
   exit-4 (binding load → engine/binding remedy) split, `|| rc=$?` capture (not `|| true`) flipping
   `HARD_FAIL`, floor derived from `package.json engines.node` (no literal), supersedes the
   "same major, likely fine" WARN. **Verified live**: passes on Node 22.23.1 with the binding present.
5. **`docs/runbooks/client-dev-local.md`** (operations-owned) — added **F11** with the (a)/(b)
   sub-case split; updated §5 header (F1–F11), ToC anchor, Changelog, Last-Updated (2026-08-06).
   Reconciled §7 (stale "no Playwright" → corrected; the lane exists) and §6.5 cross-reference in
   lockstep, both stated from `scripts/layer7.sh` + on-branch files (playwright.config.ts, e2e/
   specs; `global-setup.ts` and the Layer-7 gate read `fingerprints.json` — corrected a first-pass
   claim that named `fingerprints.env`, per code-reviewer FINDING 1).
6. **`docs/TODO.md`** — node-pin drift-guard entry under §Developer Experience (tracked, NOT built):
   four-member agreement set across two invariants (exact-equality `.nvmrc`==Dockerfile pin;
   floor-match lockfile-max-floor==root engines lower bound, `.nvmrc` >= floor), floor defined as
   max-over-actually-installed deps (optional-skip filtered → 22.13.0), CI `node-version: '22'` named
   as excluded-but-tracked site + `node-version-file: .nvmrc` migration suggestion.

### Remaining host-side action (out of scope here)
- **Rebake the devloop image**: `./infra/devloop/devloop.sh --rebuild` (now reads `.nvmrc` and pins
  Node 22.23.1). In-tree edits are done; the image itself is unchanged until rebaked.

---

## Gate 2 — Validation (`scripts/layer-all.sh`)

Full pipeline run, then a targeted Layer-3 re-run after the Lead fixed a §Accepted Deferrals
formatting-policy violation (`inline_debt_body` — multi-line bullets rewritten as one-line pointers).

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | rust + ts typecheck (proto skipped — no diff) |
| 2 Format | OK | fmt-all-langs |
| 3 Guards | **FAIL → FIXED → OK** | pre-fix: `validate-todo-tracking` inline_debt_body @ main.md; post-fix `scripts/layer3.sh` = **35/35 guards, STATUS=OK** |
| 4 Test | OK | cargo-test + nx-test |
| 5 Lint | OK | clippy + nx-lint |
| 6 Audit | N/A(agg) | cargo-audit OK, pnpm-audit OK, buf-breaking OK (no dep-manifest change → aggregate N/A) |
| 7 Env-tests | OK (832s) | env-tests-passed **and browser-e2e-passed** (diff-triggered lane fully ran) |

The only failure was the docs-formatting guard, fixed in the Lead-owned §Accepted Deferrals section and
independently re-verified green. All other layers — including Layer 7 env-tests + browser E2E — passed on
the same tree; a markdown-only edit cannot affect them. **Gate 2: PASS.**

---

## Code Review Results (Gate 3)

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | CLEAR | 0 | 0 | 0 |
| Observability | CLEAR | 0 | 0 | 0 |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 |
| DRY | CLEAR | 0 | 0 | 0 |
| Operations | RESOLVED-FIXED (co-signed runbook cross-boundary edit) | 1 | 1 | 0 |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 |

**All CLEAR / RESOLVED-FIXED — zero deferred, zero spun-out, zero escalated.** Gate 3 approved.

Code Quality findings (all 3 fixed in-diff): (1) runbook §6.5/§7 mis-stated `global-setup.ts` reads `fingerprints.env` → corrected to `fingerprints.json` (`.env` is only the shell-sourceable sibling); (2) `dev-web.sh:124` `# e.g. 22.11.0` comment → version-agnostic; (3) root-cause SSoT: stale `generate-dev-certs.sh` comments (lines 27-31, 314-315) mis-attributing `.env` to task #18 — the upstream source that propagated Finding 1 — fixed at source (comment-only, fix-now, added as a classified Mine·Mechanical row). Operations finding = same fingerprints.env→.json inaccuracy in its owned runbook; fixed + co-signed. Test + Semantic-Guard re-confirmed CLEAR over the expanded 10-file diff.

---

## Accepted Deferrals

No reviewer *findings* were deferred (Gate 3 pending); per template this section is pointer-only, one physical line per entry. The items below are scope/host-side notes, not deferred findings:

- `docs/TODO.md` §Developer Experience — node-pin drift guard (out-of-scope by task; also Implementation Summary deliverable 6).
- Devloop image rebake — host-side action; see §Remaining host-side action above.
- `shellcheck` absent in this devloop image — shell edits validated via `bash -n` + live execution (run shellcheck host-side).

---

## Rollback Procedure

Start commit: `3ac76a431dbb669d8269e5d4254bb4dd74a47ece`. `git reset --hard` restores; no schema/migrations involved. The devloop image rebake is a separate host-side action (out of scope here) — reverting the tree reverts the Dockerfile pin.
