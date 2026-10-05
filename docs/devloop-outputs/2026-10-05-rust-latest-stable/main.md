# Devloop Output: Move workspace to latest stable Rust (1.99) from one pinned version

**Date**: 2026-10-05
**Task**: Bump the single pinned Rust toolchain to latest stable (1.99.0), keep/justify its single source of truth, fix the lints the new toolchain flags. Closes docs/TODO.md §Supply Chain "Move CI and every image to the latest stable Rust, from the one pinned version".
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/4-rust-latest-stable`
**Duration**: ~1h50m (17:45–19:35 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `70e447c105efe760d401ab18255baacc4994b5f1` |
| Branch | `feature/4-rust-latest-stable` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (no check surface expected: toolchain pins + lint fixes; re-evaluate if the diff grows) |
| Paired (GSA owners) | `paired-protocol` (proto-gen/**), `paired-auth-controller` (common jwt.rs / token_manager.rs) |

---

## Task Overview

### Objective
One pinned Rust version everywhere, moved to latest stable 1.99.0, with every new rustc/clippy/rustfmt finding fixed (not blanket-allowed).

### Setup findings (Lead)
- Task prompt's site list is partly stale: df79abd4 already unified every site on 1.95 (4 workflow rustup steps, devloop Dockerfile, 5 image Dockerfiles at `ARG RUST_VERSION=1.95`) and added a drift check in `scripts/setup.test.sh` (`rust_versions()`). `dtolnay/rust-toolchain` is gone (replaced by rustup run steps).
- No workspace `rust-version` (MSRV) is declared.
- Latest stable per static.rust-lang.org channel: `1.99.0 (b940084d7 2026-09-28)`.

### Scope
- **Service(s)**: all (toolchain); lint fixes in common, proto-gen, mh-service, possibly more
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — SSoT mechanism choice is within one devloop; justified in Planning.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `rust-toolchain.toml` (new — the ONE Rust version) | Mine | — |
| `infra/lib/rust-toolchain.sh` (new — the ONE reader) | Mine | — |
| `infra/devloop/Dockerfile` | Mine | — |
| `infra/devloop/devloop.sh` | Mine | — |
| `infra/docker/*/Dockerfile` (5: ac, gc, mc, mh, db-migrate) | Mine | — |
| `infra/kind/scripts/deploy.sh` (`image_build_args`) | Mine | — |
| `.dockerignore` | Mine | — |
| `.github/workflows/ci.yml` | Mine | — |
| `.github/workflows/ci-client.yml` | Mine | — |
| `.github/workflows/audit-scheduled.yml` | Mine | — |
| `scripts/setup.test.sh` (`rust_versions()` block rewritten) | Mine | — |
| `Cargo.lock` (async-trait 0.1.89 → 0.1.92) | Mine | — |
| `Cargo.toml` (`async-trait = "0.1.92"` declared floor, per @paired-protocol) | Mine | — |
| `crates/common/src/jwt.rs` (tests only, `assert_is_empty` x3) | Not mine, Domain-judgment (GSA) | auth-controller |
| `crates/common/src/token_manager.rs` (tests only, `assert_is_empty` x2 on a secret) | Not mine, Domain-judgment (GSA) | auth-controller |
| `crates/common/src/observability/otel.rs` (test-only exporter, `unused_async_trait_impl`) | Not mine, Minor-judgment | observability |
| `crates/mh-service/src/observability/metrics.rs` (`#[must_use]` x4) | Not mine, Minor-judgment | media-handler, observability |
| `crates/mh-service/src/routing/mod.rs` (tests only, `assert_is_empty` x3) | Not mine, Minor-judgment | media-handler |
| `crates/mh-test-utils/src/transport_shim.rs` (deprecated `fetch_update` x2) | Not mine, Minor-judgment | media-handler, test |
| `crates/env-tests/tests/33_alert_rules_loaded.rs` (`&` in `panic!` arg) | Not mine, Minor-judgment | test |
| `docs/TODO.md` (close §Supply Chain entry) | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/runbooks/devloop-validation.md` (stale-container row, per @operations) | Mine | — |
| `crates/dt-guard/src/cross_boundary_classification.rs` (owner_not_in_manifest message: say Owner must be exactly ONE specialist — Lead finding at Gate 1) | Mine | — |

No `proto-gen/**` edit: see Planning §3 (the generated-code finding is fixed by the async-trait bump, so there is nothing to allow).

---

## Planning

### 1. Mechanism restated
"Bump 14 literals" is the instance. The mechanism is: **the declared Rust version must be the compiler that actually runs**, in CI, in the devloop container, on the host (devloop-helper) and in every image build. The current guard only proves the *declared sites* agree. It cannot see the compiler that runs. That is the situation today: every site says 1.95, but nothing forces a stale devloop container or a host developer onto it. Requirement 4 of this task (layer-fast.sh runs under "whatever is active") is that same gap.

### 2. SSoT decision: `rust-toolchain.toml`, every other site derives from it

```toml
[toolchain]
channel = "1.99.0"
profile = "minimal"
components = ["rustfmt", "clippy"]
```

Why this file and not "N literals + drift guard":
- rustup reads it on **every** `cargo`/`rustc` invocation under the repo. The pin governs the compiler that runs, not just the declared sites. A container whose image predates the bump runs 1.99 anyway: rustup auto-installs it, and it fails loudly if it cannot. No `rustup default` hack is needed, and that closes requirement 4 by construction.
- It follows the repo's existing pin pattern (`.nvmrc` → ONE reader → Dockerfile `ARG` with no default + CI reads the file; pnpm `packageManager` → `infra/lib/package-manager.sh`). One literal, with derivation instead of comparison.
- A bump re-runs every Rust layer with no wiring change: `scripts/lang/_dispatch.sh` is always-run (ADR-0033 §3; the per-language `changed.sh` classifier was retired 2026-08-20).

Sites and how each derives:
| Site | Today | After |
|---|---|---|
| CI `ci.yml` lint/test job, `ci-client.yml`, `audit-scheduled.yml` | `rustup toolchain install 1.95 …` + `rustup default 1.95` | `rustup toolchain install` with **no argument** (rustup ≥ 1.28 installs the toolchain the file names, components included) + `rustup show active-toolchain` for the log. `rustup default` is removed: the file overrides it inside the repo, so a leftover `default` is a misleading second value. |
| CI `ci.yml` coverage job | same + `--component llvm-tools-preview` | same no-arg install + `rustup component add llvm-tools-preview` (applies to the file's toolchain) |
| `infra/devloop/Dockerfile` | `FROM rust:1.95-slim-bookworm` | `ARG RUST_VERSION` (no default) + `FROM docker.io/library/rust:${RUST_VERSION}-slim-bookworm`. Its build context is `infra/devloop/`, so it cannot read the file. `devloop.sh` passes `--build-arg RUST_VERSION=$(rust_toolchain_version …)` at both `podman build` sites, the same way it passes `NODE_VERSION`. |
| `infra/docker/{ac,gc,mc,mh}-service`, `db-migrate` Dockerfiles | `ARG RUST_VERSION=1.95` | `ARG RUST_VERSION` with **no default**. `deploy.sh:image_build_args()` emits `--build-arg RUST_VERSION=…` for every first-party image (from the reader), and db-migrate keeps its `SQLX_CLI_VERSION` on top. An unset arg yields `FROM rust:-slim-bookworm`, which is an invalid reference and fails loudly. The header `docker build` examples are updated. |
| Host `devloop.sh:build_helper` | host default toolchain | runs from `$REPO_ROOT` (rustup discovers the file by **cwd**, not `--manifest-path`), so the security-relevant helper is built on the pinned compiler wherever `devloop.sh` is invoked from. The host's rustup installs 1.99.0 once. (Called out for @operations.) |
| `.dockerignore` | — | excludes `rust-toolchain.toml`. Image builds `COPY . .`. The official `rust:` images are `--profile minimal` (no rustfmt/clippy), and rustup would try to fetch the file's components over the network mid-release-build. The image's toolchain is already the file's version (via the derived `FROM`), so the file adds nothing there except a network dependency. |

Version string: **`1.99.0` exactly**, one string used verbatim as the toolchain channel and the image tag (`rust:1.99.0-slim-bookworm`: verified 200 on Docker Hub, as is `1.99-slim-bookworm`). Not `1.99`: that tag floats to 1.99.x. The image installs its toolchain as `$RUST_VERSION` (`1.99.0-x86_64-…`), so a floated image would hold `1.99.1` while the file says `1.99.0`, and rustup would download a second toolchain inside every container. The reader **rejects** anything but `X.Y.Z` (`stable`, `1.99`, `nightly`), so the exact-pin property is enforced, not just conventional.

Nightly sites are unaffected because each selects nightly explicitly. `fuzz-nightly.yml` uses `cargo +nightly fuzz`, `scripts/guards/{common,strip-test-code}.sh` use `rustup run nightly`, and `ci.yml`'s `rustup toolchain install nightly` names nightly. `+toolchain`/`rustup run` outrank the file. Neither nightly step is edited.

Guard (`scripts/setup.test.sh`, replaces `rust_versions()`):
- The reader handles exact `X.Y.Z` (passes), plus missing file, missing `channel`, duplicate `channel`, `stable`, `1.99` and `nightly` (each fails, with the reason named). It also reads the real file.
- No literal Rust version remains anywhere it could drift: no `ARG RUST_VERSION=` default in any `infra/**/Dockerfile`, no `rust:<digit>` in any `FROM`, and no `rustup (toolchain install|default) <digit>` / `rustup default` in any workflow. Every `FROM …/rust:` uses `${RUST_VERSION}`.
- Every consumer takes the version from the reader. `devloop.sh` calls `rust_toolchain_version`, and its `--build-arg "RUST_VERSION=` count equals its `podman build` count. `deploy.sh:image_build_args` (executed via `src_run`) emits `RUST_VERSION=<file value>` for a service repo AND for `db-migrate` (alongside `SQLX_CLI_VERSION`).
- Workflows: at least 4 no-arg `rustup toolchain install` steps (non-vacuity). Ordering after checkout is not guarded: a no-arg install with no toolchain file fails loudly on its own.
- `.dockerignore` lists `rust-toolchain.toml`.
- No `rust-version` key in any `Cargo.toml` (see MSRV below).
- Negative controls in a temp root: a literal `ARG RUST_VERSION=1.99.0`, a workflow `rustup toolchain install 1.99.0`, and a stray `rustup default 1.99.0` each trip.

### MSRV (`rust-version`): not declared, and its absence is guarded
This is an application workspace (no published library), so the minimum supported Rust is by definition the pinned toolchain. Declaring `rust-version` would be a second copy of the same value with no consumer that needs it. The workspace is `resolver = "2"` and `edition = 2021`, so the MSRV-aware resolver (v3) is not in use and `rust-version` would not steer dependency resolution. Its only effect would be clippy's `msrv` lint gating, which is correct by default when MSRV = the running toolchain. A guard asserts that no `Cargo.toml` (members + fuzz) declares one, with the reason in the message. If a published crate ever needs an MSRV, that decision removes the assertion.

### 3. Lint survey on 1.99.0 (`cargo +1.99.0 clippy --workspace --all-targets --all-features --keep-going`, warnings not denied so nothing hides behind a first failure)
| Finding | Sites | Fix (no blanket allow) |
|---|---|---|
| `double_must_use` | 11 in proto-gen's generated tonic code + **1 not in the task's list**: `gc-service/src/services/mc_client.rs:252` (`McClientTrait`) | **Root cause is `async-trait` 0.1.89**: its desugared methods carry a bare `#[must_use]` on a pinned boxed `Future`. tonic's generated servers use the same macro. **0.1.92 no longer emits it.** I verified this on a scratch crate under 1.99.0: 0.1.89 warns, 0.1.92 is clean. Fix = `cargo update -p async-trait` (lockfile only, and a patch-level bump within the declared `0.1`). **No `#[allow]` at all, and no proto-gen or gc source edit.** |
| `assert_is_empty` (pedantic) | `common/jwt.rs` x3, `common/token_manager.rs` x2, `mh-service/src/routing/mod.rs` x3 (tests) | Non-secret values use `assert_eq!(v, <empty>)`, which shows the value on failure as the lint intends. `token_manager.rs` asserts on `token.expose_secret()`: clippy's suggestion `assert_ne!(secret, "")` puts the secret in assertion-formatting position, and changing it later to `assert_eq!` would print a live token. Use `assert_ne!(token.expose_secret().len(), 0)` instead: the failure message carries only a length. **@paired-auth-controller / @security, please confirm this shape.** |
| `unused_async_trait_impl` (pedantic, new) | `common/observability/otel.rs:760` (test-only `CapturingExporter::export`) | `fn export(…) -> impl Future<Output = OTelSdkResult> { std::future::ready(Ok(())) }` (clippy's suggestion; `SpanExporter::export` is an RPITIT method) |
| `must_use_candidate` (pedantic) | `mh-service/src/observability/metrics.rs` x4 (`forwarded`, `dropped`, `latency`, `egress_queue_depth`) | Add `#[must_use]`, matching the sibling `codec_dropped` that already has it. These are pure handle lookups. |
| `needless_borrows_for_generic_args`-family ("redundant reference in `panic!` argument") | `env-tests/tests/33_alert_rules_loaded.rs:142` | drop the `&` |
| rustc `deprecated`: `Atomic::fetch_update` renamed `try_update` | `mh-test-utils/src/transport_shim.rs:206,721` | rename to `try_update` (same signature and semantics) |

Also checked:
- `cargo +1.99.0 fmt --all --check`: clean.
- Fuzz workspaces (`crates/{ac-service,media-protocol}/fuzz`): clippy clean on 1.99.0.
- The devloop Dockerfile's pinned `cargo install --locked` tools, `cargo-llvm-cov 0.8.7` and `cargo-audit 0.22.2`, both **build on 1.99.0**, so the pins stay unchanged.
- `--all-features` was included in the survey.
- dt-guard, dt-story and devloop-helper are workspace members, so they are covered.

During implementation I re-run the survey with `-D warnings` after the fixes, and also run `cargo +1.99.0 llvm-cov` on one crate to confirm llvm-tools/profdata compatibility.

### 4. Validation exercises 1.99
Once `rust-toolchain.toml` lands, every `cargo` under `/work` in this container resolves to 1.99.0 (installed here during planning with `rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy,llvm-tools-preview`; the container default remains 1.95). That is ordinary rustup behaviour driven by the committed file, not an environment hack. I will confirm it with `rustc --version` from `/work` before `layer-fast.sh`.
- Side finding: in this container, the post-install step of `rustup toolchain install` exits non-zero with `rustup is not installed at '/tmp/cargo-home'` (`CARGO_HOME` is redirected away from the image's `/usr/local/cargo`). The toolchain itself installs fine. That matters only for stale containers. The rebuilt image ships 1.99.0, so nothing is auto-installed. I will check whether rustup's *auto-install* path hits the same error. If it does, it is a loud failure that the host-side rebuild already resolves.
- **Host-side operator steps (not attempted here):** `./infra/devloop/devloop.sh --rebuild --recreate` (image moves to `rust:1.99.0-slim-bookworm`). Host rustup will install 1.99.0 the first time `devloop.sh` builds the helper.

### 6. Gate 1 input folded in
- **@security, reader key allow-list:** the reader fails unless the file's keys are exactly `[toolchain]` with `{channel, profile, components}`. It is an allow-list, so `path = "…"` (which makes rustup run binaries from a local directory and would bypass the X.Y.Z check) and any unknown key fail. The guard also asserts that no legacy plain-text `rust-toolchain` file exists. Negative controls cover a `path =` line, an unknown key, a second table and a legacy file.
- **@paired-protocol, declared floor:** `Cargo.toml` `async-trait = "0.1.92"`, with a comment explaining why. A lockfile regen or `-Z minimal-versions` cannot bring the lint back. The diff touches nothing under `proto/**`, `crates/proto-gen/**` or `build.rs`, and the `Cargo.lock` change is that one package.
- **@paired-auth-controller / @security, secret asserts:** `assert_ne!(token.expose_secret().len(), 0)`, with a one-line comment at each site explaining why it isn't clippy's `assert_ne!(secret, "")`. I will verify under `-D warnings` that it trips neither `len_zero` nor `assert_is_empty`, and escalate before any allow if it does. token_manager.rs:1743 (`!captured.is_empty()` with a message) is not flagged by 1.99 (it is absent from the survey), so it is untouched. No non-test edits in any auth GSA file.
- **@dry-reviewer, toml inside image builds:** the toml is excluded from the image build context (`.dockerignore`), so rustup inside the image has no file to honour. It can neither silently download a different toolchain nor fetch components, and the image's toolchain is the derived `FROM`. `RUSTUP_AUTO_INSTALL=0` would guard a path that can no longer occur, so I do not add it. The `.dockerignore` entry is asserted. Stale prose is updated too: TODO.md "this workspace is on 1.95", the workflow "checked by setup.test.sh" comments, and the INDEX pointer. I will confirm empirically that `cargo +nightly` overrides the root file in the fuzz crates.
- **@observability:** if a test or bench call site warns on a discarded handle after `#[must_use]`, I fix it by using the handle, never `let _ =` or allow.

- **@dry-reviewer, items 1/3/4:**
  - (1) Rationale corrected above. `docs/TODO.md:2029` stands as written: it is the historical "Half A" text, and the entry's own `UPDATE 2026-08-20` line directly above it already records that `changed.sh` was retired and Half A is closed.
  - (3) Every header comment that describes the old ARG default or the setup.test.sh comparison now points at `rust-toolchain.toml` + `infra/lib/rust-toolchain.sh`: `db-migrate/Dockerfile:15`, the 4 service Dockerfile headers, the 4 workflow step comments and the devloop Dockerfile header.
  - (4) The `.dockerignore` entry carries its exception reason: the FROM is already derived from this file, and the image would otherwise fetch the file's components mid-build.
  - TODO.md:2182 is reworded to point at the pin and no longer restates a number.
- **@code-reviewer:** the async-trait floor is in (above). The "length, not value" comment goes at both token_manager sites.
- **@test:**
  - (1) Derivation is proven with a sentinel: the reader and `deploy.sh:image_build_args` run against a temp root whose toml says `9.8.7`, and the sentinel must come out. devloop.sh's `--build-arg "RUST_VERSION=` values must all be `${…}` (no digit after `=`), and their count must equal the `podman build` count.
  - (2) The D8 deploy harness asserts that ALL recorded `build` lines carry `RUST_VERSION=<reader value>`, with count == the `$ALL_DOCKERFILES` count (5).
  - (3) Positive controls: at least 6 Dockerfiles scanned, and exactly 6 `FROM …rust:${RUST_VERSION}-` matches.
  - (4) The reader is strict: a full anchored line match `^channel = "X.Y.Z"$`. A trailing `# comment` and single quotes are **rejected**, each asserted. A `channel` under another table is rejected by the key/table allow-list.
  - (5) The llvm-cov spot-check records covered lines > 0 and `show-env` carrying `instrument-coverage` + `cfg=coverage`. Numbers go in the Implementation Summary.
- **@operations:**
  - (1) **Stale-container outcome, observed:** (a) it installs and cargo proceeds. In a scratch dir with a toml naming an uninstalled `1.98.0`, `cargo --version` auto-installed the toolchain (5 components), printed `warn: the missing active toolchain … has been auto-installed`, and returned rc 0. The post-install `/tmp/cargo-home` error is specific to an explicit `rustup toolchain install` and does not affect the auto-install path. A row goes into `docs/runbooks/devloop-validation.md` §8 covering the symptom (that warn, or an offline download failure), the cause (the image predates a `rust-toolchain.toml` bump) and the fix (`./infra/devloop/devloop.sh --rebuild --recreate`).
  - (2) **cargo-chef pinned:** each of the 4 service Dockerfiles gets `ARG CARGO_CHEF_VERSION` (no default) and `cargo install cargo-chef --locked --version "=${CARGO_CHEF_VERSION}"`. The ONE value lives in `deploy.sh` beside `image_build_args`, which passes it to the 4 service images (derivation, not 4 literals). Value: `0.1.78` (current), verified to build `--locked` on 1.99.0 during implementation. The guard asserts no `ARG CARGO_CHEF_VERSION=` default, no unpinned `cargo install cargo-chef`, and that all 4 service build lines in the D8 harness carry it.
  - (3) Operator rollout/rollback note: see the section below.
  - **Other builders:** confirmed that `deploy.sh:build_content_tagged_image` is the only build site for `infra/docker/*/Dockerfile`. The compose `build:` stanzas are commented out, CI has no image job, and `release-feature-gate.test.sh` runs `cargo check` with no image build.

### 7. Operator rollout / rollback
- Post-merge host step: `./infra/devloop/devloop.sh --rebuild --recreate`. A container that is not rebuilt still works: rustup auto-installs 1.99.0 on its first cargo call (warn line, then proceeds).
- The host's first `devloop.sh` run downloads 1.99.0 (helper build).
- The first CI run on main and on each PR rebuilds the Rust cache cold (new rustc in the Swatinem key).
- The first `dev-cluster deploy` after merge rebuilds all 5 images from scratch (new base, so every cargo-chef layer misses).
- Rollback: revert the commit and do the same rebuild.

### 5. Out of scope / unchanged
`fuzz-nightly.yml` and the ci.yml nightly step (explicit nightly; see above). The commented-out compose `build:` stanzas in `docker-compose.yml` are commented out and do not build anything.

---

## Gate 1

All 8 confirmed 2026-10-05. Classification-sanity guard: `STATUS=OK` after Lead normalized the two GSA Owner cells to a single owner (`auth-controller`; security co-sign is via the GSA intersection rule, not the Owner field). The first run failed `owner_not_in_manifest` with a self-contradictory message (`lists Owner="auth-controller, security" but manifest allows only {auth-controller, security}`) — message fix added to scope.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Paired Protocol | confirmed |
| Paired Auth-controller | confirmed |

---

## Implementation Summary

**Toolchain:** `rustc --version` inside `/work` → `rustc 1.99.0 (b940084d7 2026-09-28)`. Selected by the committed `rust-toolchain.toml`. The container default is still 1.95, and I changed no `rustup default` or override.

### SSoT
- New `rust-toolchain.toml`: `channel = "1.99.0"`, `profile = "minimal"`, `components = ["rustfmt", "clippy"]`.
- New reader `infra/lib/rust-toolchain.sh:rust_toolchain_version`:
  - only one `[toolchain]` table is allowed;
  - keys are allow-listed to {channel, profile, components}, each at most once;
  - `channel = "X.Y.Z"` must match the whole line.
- Consumers:
  - `infra/devloop/devloop.sh:read_rust_version()` passes `--build-arg RUST_VERSION` at both `podman build` sites.
  - `infra/kind/scripts/deploy.sh:image_build_args()` gives `RUST_VERSION` to every image, `SQLX_CLI_VERSION` to db-migrate, and `CARGO_CHEF_VERSION` (`0.1.78`, the ONE value, in deploy.sh) to the 4 service images.
- Dockerfiles:
  - The devloop image and all 5 `infra/docker/*/Dockerfile`s use `ARG RUST_VERSION` with no default.
  - The 4 service images also take `ARG CARGO_CHEF_VERSION` (no default; `test -n` fails loud) and run `cargo install cargo-chef --locked --version "=…"`.
  - Header comments now point at the SSoT.
- CI (`ci.yml` x2, `ci-client.yml`, `audit-scheduled.yml`):
  - The 4 stable steps run a no-arg `rustup toolchain install` + `rustup show active-toolchain`. The coverage job also adds `llvm-tools-preview`.
  - `rustup default` is gone. The nightly steps are untouched.
- `.dockerignore` excludes `rust-toolchain.toml`, and the RULE header carries the exception and its reason.
- `devloop.sh:build_helper` builds from `$REPO_ROOT`.

### Guard (`scripts/setup.test.sh`, replaces `rust_versions()`)
- **Reader:**
  - Accepts an exact `9.8.7` sentinel and a file with comments and blank lines.
  - Rejects each of these, with an `ERROR:` reason: missing channel, duplicate key, `stable`, `nightly`, `1.99`, a trailing `# comment`, single quotes, `path =` (the message names the allow-list), an unknown key (`targets`), a second table, a key outside any table, and an unreadable file.
  - Reads the real file.
  - Asserts that the ONLY toolchain file in the tree is the root `rust-toolchain.toml` (nothing nested, no legacy plain-text `rust-toolchain`), with negative controls for each (Gate 3, @security F1).
- **Derivation:** `image_build_args` runs against a temp root whose file says `9.8.7`. The sentinel comes out for a service repo and for db-migrate, a missing file fails, and service repos carry an X.Y.Z `CARGO_CHEF_VERSION`.
- **devloop.sh:**
  - Derives the version through the reader.
  - Every `RUST_VERSION` build-arg is `${RUST_VERSION}`, and the count equals the `podman build` count (≥ 2).
  - The helper is built from the repo root.
- **D8 harness:**
  - Every recorded `build` line carries `RUST_VERSION=<reader value>`, and the count equals the number of Dockerfiles (5).
  - All 4 service builds carry `CARGO_CHEF_VERSION`.
- **No literal Rust version:** no `ARG RUST_VERSION=`, no `FROM …rust:<digit>`, and no workflow `rustup (toolchain install|default) <digit>` or `rustup default`.
- **Positive controls:**
  - 6 Dockerfiles scanned, with exactly 6 `FROM …rust:${RUST_VERSION}-` lines and 6 bare `ARG RUST_VERSION` lines.
  - At least 4 no-arg workflow installs.
  - The `.dockerignore` entry is present.
- **Cargo-chef:** pinned in all 4 service images, with no Dockerfile default and no unpinned install.
- **MSRV:** no `rust-version` in any `Cargo.toml` (members + fuzz).
- **Negative controls:** an ARG default, a FROM literal, a workflow install literal and a workflow `rustup default` each trip the scan. A clean root passes.
- The three tree copies the suite makes (`jobcopy`, `migcopy`, `repocopy`) now copy `rust-toolchain.toml` too (deploy.sh input).
- Gate 3 fixes:
  - @dry-reviewer: one `deploy_tree_copy` helper is the ONE list of root inputs deploy.sh reads, used at all 3 copy sites.
  - @code-reviewer: the ci-client step is renamed "Install Rust toolchain". The Dockerfile and cargo-chef counts are derived from the file set, with floors of 6 and 4.
  - @test F1: devloop.sh must assign `RUST_VERSION="$(read_rust_version …)"` once per build site, and no `RUST_VERSION=<digit>` literal is allowed.
  - @test F2: the FROM scan is case-insensitive and handles flags and stages, with a `from --platform=… AS extra` negative control.
  - Both of test's mutations (a literal `RUST_VERSION="1.99.0"` in devloop.sh, and an extra `FROM --platform … rust:1.99.0 … AS extra`) now fail the suite. I verified this on a scratch copy of the tree.
- Result: `scripts/setup.test.sh: 735 passed, 0 failed`.

### Lint fixes (all on 1.99.0; `cargo clippy --workspace --all-targets --all-features --keep-going -- -D warnings` is clean; no `#[allow]` added)
- **`double_must_use`** (12): `Cargo.toml` `async-trait = "0.1.92"` (declared floor, with the reason) and `Cargo.lock` 0.1.89 → 0.1.92 (that one package; 3 lines). There is **no diff under `proto/**`, `crates/proto-gen/**` or `build.rs`**.
- **`assert_is_empty`:**
  - `common/jwt.rs` x3 → `assert_eq!(…, Vec::<&str>::new())` / `Vec::<String>::new()`.
  - `common/token_manager.rs` x2 → `assert_ne!(token.expose_secret().len(), 0)` with a "Length, not value" comment. Clean under `-D warnings`: it trips neither `len_zero` nor `assert_is_empty`.
  - `mh-service/src/routing/mod.rs` x3 → `assert_eq!(…, Vec::<…EgressEdge>::new())`.
- **`unused_async_trait_impl`:** the `otel.rs` test exporter → `fn export(…) -> impl Future<…> + Send { std::future::ready(Ok(())) }`.
- **`must_use_candidate`:** `#[must_use]` on `forwarded`, `dropped`, `latency` and `egress_queue_depth`. No call site warned afterwards, so no `let _ =`.
- **Redundant `&` in a `panic!` arg:** `env-tests/tests/33_alert_rules_loaded.rs`.
- **rustc `deprecated`:** `fetch_update` → `try_update` x2 in `mh-test-utils/src/transport_shim.rs` (plus the comment).

### Lead-added item
`crates/dt-guard/src/cross_boundary_classification.rs`: `owner_not_in_manifest` now reads "lists Owner=… ; Owner must be exactly ONE of {…}". The semantics are unchanged. New unit test `case_3b_comma_list_owner_rejected_with_exactly_one_wording` asserts that `Owner="auth-controller, security"` is rejected with that wording.

### Docs
- `docs/TODO.md`: the §Supply Chain entry is closed (`[x]`, pointing to this devloop). The `#[expect]`/`reason` entry now refers to the pin instead of "1.95".
- `docs/runbooks/devloop-validation.md` §8: a stale-container row (symptom / cause / `--rebuild --recreate`).
- `docs/specialist-knowledge/infrastructure/INDEX.md`: the Rust-pin pointer.

### Verification evidence
- **Fuzz:** `cargo +nightly --version` inside `crates/ac-service/fuzz` → `cargo 1.101.0-nightly`, while a bare `rustc --version` there → 1.99.0. So `+nightly` outranks the root file. Both fuzz workspaces are clippy-clean on 1.99.0.
- **Stale container:** a toml naming an uninstalled toolchain auto-installs on the first cargo call (warn line) and returns rc 0.
- **`--locked` builds on 1.99.0:** cargo-llvm-cov 0.8.7, cargo-audit 0.22.2 and cargo-chef 0.1.78.
- **llvm-cov on 1.99.0:**
  - `show-env` carries `instrument-coverage` and `cfg=coverage`.
  - `cargo llvm-cov -p media-protocol --summary-only` → lines 684/812 covered (84.24%), regions 958/1091 (87.81%), functions 71/83.
- **`cargo fmt --all --check`:** clean.
- **`./scripts/layer-fast.sh`:** rc 0. L1 OK, L2 OK, L3 OK, L4 N/A (`cargo-test-passed`, `nx-test-passed`; N/A comes from the proto lang having no test verb), L5 OK (`cargo-clippy-passed`), L6 N/A (`cargo-audit-passed` on the new lock, `pnpm-audit-passed`; proto lang N/A).

### Host-side operator steps (not performed here)
`./infra/devloop/devloop.sh --rebuild --recreate`. See Planning §7 for rollout and rollback.

---

## Gate 3

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | F1 nested toolchain files → whole-tree single-toolchain-file guard |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | mutation gaps: devloop.sh literal assignment; FROM --platform form |
| Observability | CLEAR | 0 | 0 | 0 | must_use additive; otel test exporter equivalent |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | step rename; derived counts; stale comment |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | 3 inline tree copies → deploy_tree_copy() helper |
| Operations | CLEAR | 0 | 0 | 0 | rollout/rollback documented; host rebuild + cold image rebuild noted |
| Paired Protocol | CLEAR | 0 | 0 | 0 | no proto/proto-gen/build.rs diff; lock: async-trait only (syn 3.0.6 already present) |
| Paired Auth-controller | CLEAR | 0 | 0 | 0 | 5 hunks all #[cfg(test)]; len-only secret asserts; clippy 1.99 clean, 142/142 |

---

## Gate 2

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` under `rustc 1.99.0 (b940084d7 2026-09-28)` (selected by rust-toolchain.toml): exit 0, TOTAL_RESULT=N/A (N/A = proto has no test/audit verb only).
L1 OK, L2 OK (no FMT_APPLIED), L3 OK, L4 N/A (cargo-test-passed, nx-test-passed), L5 OK (cargo-clippy-passed), L6 N/A (cargo-audit-passed on new lock, pnpm-audit-passed, buf-breaking-passed), L7 OK (env-tests-passed, browser-e2e-passed). 1 attempt.

Service-image verification: `dev-cluster deploy` after Gate 2: all 5 images (ac/gc/mc/mh/db-migrate) built `FROM docker.io/library/rust:1.99.0-slim-bookworm` with `cargo install cargo-chef --locked --version "=0.1.78"`; all 15 workloads rolled out (exit 0).

## Remaining operator actions (host)

1. `./infra/devloop/devloop.sh --rebuild --recreate`. Until then, the old container auto-installs 1.99.0 on first cargo call (warning, rc 0); see runbook §8.
2. Expect a cold CI Rust cache on the first run, and cold image rebuilds on the first deploy elsewhere.

## Accepted Deferrals

None. All 7 Gate-3 findings fixed in-loop.
