# Devloop Output: rand 0.8 → 0.10 workspace migration

**Date**: 2026-10-09
**Task**: Move root `[workspace.dependencies]` `rand` from `"0.8"` to `"0.10"`, resolve the `small_rng` feature request, migrate every `rand`-crate call site (unblocks weekly root-cargo Dependabot).
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/rand-0-10-migration`
**Duration**: ~45m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5253aae058cd7e5fe81766746f78ed65d691f036` |
| Branch | `feature/rand-0-10-migration` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (spawned)` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |

---

## Task Overview

### Objective
Dependabot run 37724549263 (2026-10-08) failed `dependency_file_not_resolvable`: `mh-service depends on rand with feature small_rng but rand does not have that feature`. Migrate the workspace to rand 0.10 by hand (no `ignore:` for rand).

### Scope
- **Service(s)**: workspace `Cargo.toml`, common, mh-service, mc-service (test only)
- **Schema**: No
- **Cross-cutting**: Yes (workspace dependency)

### Lead pre-planning findings (re-grep at start commit)

- **`rand`-crate call sites are only four files**: `crates/common/src/types.rs` (`UserId::new`, `thread_rng().gen::<u64>()`), `crates/mh-service/src/media/sampler.rs` (`SmallRng::from_entropy()`, `gen_bool`), `crates/mh-service/src/webtransport/connection.rs` (`thread_rng().gen_range(0..=MAX)` close jitter), `crates/mc-service/src/media_routing/slots.rs` (test; `StdRng::seed_from_u64`, `gen_range`).
- **Every site the task brief flagged as security-relevant uses `ring::rand::SystemRandom`, not the `rand` crate** (`common/jwt.rs`, `ac-service/crypto`, `mc-service/media_admission/{kek,epoch}.rs`, `actors/{session,participant,meeting}.rs`, `gc-service/{meetings,meeting_assignments,mh_selection}`, `devloop-helper`, `ac-test-utils` comment). These are unaffected by this bump and are NOT edited. No GSA path is in the planned diff.
- Fuzz crates (`crates/ac-service/fuzz`, `crates/media-protocol/fuzz`) do not declare `rand`.
- rand 0.10.3 (source read from `$CARGO_HOME`): feature `small_rng` removed — `rngs::SmallRng` is unconditional (Xoshiro256++ on 64-bit, same as 0.8). `Rng`→`RngExt` (`gen`→`random`, `gen_range`→`random_range`, `gen_bool`→`random_bool`), `thread_rng()`→`rand::rng()` (ThreadRng = ChaCha12 reseeded from `SysRng`, still a CSPRNG), `OsRng`→`SysRng`, `from_entropy`/`from_os_rng` removed (use `try_from_rng(&mut SysRng)` or `rand::make_rng()`), `StdRng` = ChaCha12 ("output remains the same").

### Debate Decision
NOT NEEDED — dependency migration within existing design.

### Specialist choice
media-handler: the failing feature request and two of four call sites are in mh-service. No disambiguation prompt — the Dependabot error names mh-service.

### Process note
INDEX.md and review-protocol.md are not inlined in teammate prompts; each teammate is instructed to Read them as its first action (same content, avoids ~2.5k duplicated prompt lines).

---

## Cross-Boundary Classification

| Path | Owner | Classification | Why |
|------|-------|----------------|-----|
| `Cargo.toml` (`[workspace.dependencies] rand` + comment) | workspace (shared) | Mechanical | Version bump; comment rewritten to describe 0.10 reality (no `small_rng` feature). |
| `Cargo.lock` | workspace (shared) | Mechanical | Re-resolved by cargo (no `cargo update`): common / mh-service / mc-service dep lists `rand 0.8.5` → `rand 0.10.3` (already locked); no packages added/removed. |
| `crates/mh-service/Cargo.toml` | media-handler (own) | — | Drop `features = ["small_rng"]`; rewrite comment. |
| `crates/mh-service/src/media/sampler.rs` | media-handler (own) | — | API rename + reseed path (see Security Decisions). |
| `crates/mh-service/src/webtransport/connection.rs` | media-handler (own) | — | API rename only. |
| `crates/common/src/types.rs` (`UserId::new`) | common (not GSA — `ring::rand`/ADR-0027 sites untouched) | Mechanical | `thread_rng().gen()` → `rng().random()`; same generator (ThreadRng), same type/distribution (uniform u64). |
| `crates/mc-service/src/media_routing/slots.rs` (test only) | meeting-controller | Mechanical | `gen_range` → `random_range`, `Rng` → `RngExt`; same seed, same ranges. Generated cases change (0.9 uniform-int sampler) — test asserts properties, never specific values. |
| `docs/TODO.md` | — | Mechanical | Line 83 prose quotes `types.rs:79`'s `rand::thread_rng()`; updated to the new call so the TODO doesn't name a dead API. |

Not edited (historical records, not live API references): ADR-0003:623, ADR-0010:200, `docs/reviews/trust-but-verify-audit/01-security-self-audit.md`, `docs/debates/2025-01-22-*`.

---

## Planning

### Mechanism restatement
Dependabot cannot resolve the workspace because a consumer requests a feature (`small_rng`) the target version removed. The mechanism is wider than "rand 0.10 in Cargo.toml": rand 0.9 renamed the RNG-extension API (`Rng::gen*` → `RngExt::random*` in 0.10, where `rand::Rng` is now the rand_core base trait), removed `thread_rng()` (→ `rand::rng()`) and `SeedableRng::from_entropy()` (→ `rand::make_rng()` / `try_from_rng(&mut SysRng)`), and changed uniform-int sampling (seeded output changes). So the invariant to complete is: every `rand`-crate consumer compiles against one version, every replaced RNG is same-class, every distribution/bound is preserved, and no comment/doc names a removed API as live.

### Verification of the call-site set
Re-grep (`rand::`, `use rand`, `rand =`, `thread_rng|from_entropy|gen_bool|gen_range|OsRng|SmallRng|StdRng|SeedableRng|small_rng`) excluding `ring::rand`: exactly the four Lead-listed files, plus `rand` declarations in `common`, `mh-service`, `mc-service` Cargo.toml (all `workspace = true`). `mc-service/src/actors/session.rs` and `ac-service/src/crypto` are `ring::rand` — untouched. Neither fuzz crate declares `rand`.

### rand 0.10.3 facts (read from source)
- `[features]`: `std, std_rng, sys_rng, thread_rng` default; **no `small_rng`**; `rngs::SmallRng` exported unconditionally (Xoshiro256++ on 64-bit — same algorithm as 0.8).
- `rand::rng()` → `ThreadRng` (ChaCha12, reseeded from `SysRng`) — same construction as 0.8's `thread_rng()`.
- `rand::make_rng::<R>()` = `R::from_rng(&mut rand::rng())` under the default `thread_rng` feature.
- `expect_used = "deny"` workspace-wide, so `SmallRng::try_from_rng(&mut SysRng).expect(..)` is not available without a new `#[expect]`.

### Edits
1. `Cargo.toml`: `rand = "0.10"`; comment keeps the "why hoisted" rationale, drops the stale `from_entropy`/`small_rng` description (0.10 has no `small_rng`; `SmallRng` is unconditional).
2. `mh-service/Cargo.toml`: `rand = { workspace = true }`; comment keeps the ADR-0036 §11 rationale, removes "small_rng is opted into here".
3. `sampler.rs`: `use rand::rngs::SmallRng; use rand::RngExt;` — `SmallRng::from_entropy()` → `rand::make_rng::<SmallRng>()` (infallible; panics internally only on SysRng init failure, as 0.8 `from_entropy` did; no fallback seed). `gen_bool` → `random_bool` (identical Bernoulli(p); p==1.0 always true; out-of-range panics — clamp guards it; one draw per call). `rng: SmallRng` stays per-`LatencySampler` (per-connection). Docs reworded to say what the code does: module doc (L21 "seeded from the operating system") and struct doc (L31-35 "`from_entropy()`, i.e. from the OS") → "seeded via `rand::make_rng()` from `rand::rng()` (ThreadRng, an OS-seeded CSPRNG); never from anything derived from stream identity". All three tests unchanged in assertion content.
4. `connection.rs`: `rand::rng().random_range(0..=MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS)` (inclusive bound preserved), `use rand::RngExt`.
5. `common/types.rs`: `use rand::RngExt; Self(rand::rng().random())` — uniform u64.
6. `slots.rs` test: `use rand::{RngExt, SeedableRng}`; `random_range` with identical range expressions; seed unchanged. **Positive control (@test):** `production_full_mesh_cases` counter incremented inside the `if all_connected && is_production` branch, `assert!(production_full_mesh_cases > 0, "generator never produced an all-connected roster for the production chooser")` after the loop — so the new 0.9+ sample stream can't silently make that branch dead.
7. `docs/TODO.md:83`: `rand::thread_rng()` → `rand::rng()`.
8. `Cargo.lock`: no `cargo update`; cargo re-resolves on build. rand 0.10.3 is already locked (via sqlx-postgres) and 0.8.5 stays (tokio-retry). Expected diff: only the `dependencies` lists of common / mh-service / mc-service switch `"rand 0.8.5"` → `"rand 0.10.3"`; zero packages added/removed.

Workspace `rand = "0.10"` keeps default features (`rand::rng()` / `make_rng` need `thread_rng`). After the edit all three consumers read `rand = { workspace = true }`. No metric/label/config/wire/ConfigMap changes; whole change is revertable as one commit (files: root + 3 crate Cargo.tomls, Cargo.lock, 4 source files, docs/TODO.md).

Constructor per site: `types.rs` → `rand::rng().random::<u64>()`; `connection.rs` → `rand::rng().random_range(0..=MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS)` (expression temporary, dropped before `.await`, future stays `Send`); `sampler.rs` → `rand::make_rng::<SmallRng>()` + `random_bool`; `slots.rs` → `StdRng::seed_from_u64(0x5eed_0020)` + `random_range`. One idiom (`rand::rng()` + `RngExt`) at both ThreadRng sites.

### Security Decisions

| Call site | Old RNG (0.8) | New RNG (0.10) | Class |
|-----------|---------------|----------------|-------|
| `common/src/types.rs` `UserId::new` | `thread_rng()` = ThreadRng (ChaCha12 CSPRNG, OS-reseeded) | `rand::rng()` = ThreadRng (ChaCha12 CSPRNG, `SysRng`-reseeded) | Same generator |
| `mh-service/src/media/sampler.rs` `LatencySampler::new` | `SmallRng::from_entropy()` — Xoshiro256++ seeded directly from `OsRng` | `rand::make_rng::<SmallRng>()` — Xoshiro256++ seeded from ThreadRng (OS-seeded CSPRNG) | Same generator; seed source is OS-seeded CSPRNG output (computationally equivalent to OS bytes). Per-connection independence (§11) unchanged; never stream-derived. Alternative `try_from_rng(&mut SysRng)` rejected: needs a fallible constructor or a new `#[expect(clippy::expect_used)]`, for no security gain (0.8's `from_entropy` panicked on OS failure too). |
| `mh-service/src/webtransport/connection.rs` close jitter | `thread_rng().gen_range(0..=MAX)` | `rand::rng().random_range(0..=MAX)` | Same generator, same inclusive range |
| `mc-service/src/media_routing/slots.rs` (test) | `StdRng::seed_from_u64(0x5eed_0020)` | `StdRng::seed_from_u64(0x5eed_0020)` | Same (ChaCha12, deterministic). Generated cases differ under the 0.9+ uniform-int sampler; test asserts invariants only, no sampled values. |

No `SmallRng` introduced anywhere it wasn't; no CSPRNG downgraded; no `#[allow]`/`#[expect]` added.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

### Gate 3 — Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | All RNG classes same-or-stronger; no ring::rand/GSA path touched |
| Test | CLEAR | 0 | 0 | 0 | Seeded test deterministic; positive control added (planning) |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | sampler.rs module-doc rewrap |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | sampler doc rewrap; TODO.md UserId wording |
| DRY | CLEAR | 0 | 0 | 0 | Single workspace pin; no stale API refs |
| Operations | CLEAR | 0 | 0 | 0 | Deployment-neutral; single-commit revert; lock churn 3 dep lists |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | root Cargo.toml make_rng feature comment (Lead-directed drift scope; native SAFE) |

---

## Pre-Work

None

---

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK` (L1 6s, L2 4s, L3 148s, L4 173s, L5 1s, L6 5s, L7 1090s). Non-OK child lines: 2× proto `N/A not-applicable-to-this-lang` (registered placeholders), 1× TS audit `SKIPPED-NO-DIFF no-dep-changes`; `cargo audit` ran and passed. Attempt 1 of 3 (L1-6), 1 of 2 (L7).

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `5253aae058cd7e5fe81766746f78ed65d691f036`
2. `git revert <this commit>`; no schema, config, wire or manifest changes.
7. **Safe-revert unit**: the whole commit. A partial revert (e.g. Cargo.toml alone) leaves call sites on the wrong rand API and does not compile.

---

## Issues Encountered & Resolutions

### Issue 1: scope-drift guard on first fast check
**Problem**: Classification table lacked a `Cargo.lock` row and listed `docs/TODO.md:83` as path:line.
**Resolution**: Added the row; used the bare path.

### Issue 2: Task brief over-stated the call-site set
**Problem**: Brief listed ~15 sites as rand-crate users, incl. all key/token sites.
**Resolution**: Re-grep showed only 4 files use the `rand` crate; the rest use `ring::rand` and are untouched (security verified independently).

### Issue 3: SKILL.md names a semantic-guard check that does not exist
**Problem**: `.claude/skills/devloop/SKILL.md` §Conditional semantic guard lists "comment-vs-code meaning drift" as a check surface; `scripts/guards/semantic/checks.md` has no such check.
**Resolution**: Out of this devloop's scope; surfaced to the operator for a separate one-line fix.

