# Devloop Output: MH pool fixture, process injector, resilience hygiene, browser mhPool.ts (T-1)

**Date**: 2026-11-10
**Task**: Story 4 task T-1 (run-story task #18) — env-tests MH pool fixture (`mh_pool.rs`), process injector, participant MediaHandlerSet exposure, hoisted address-invariant helper, browser `e2e/mhPool.ts`, resilience-binary `#[serial]` hygiene
**Specialist**: test
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~3h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `cd1fb42cee9b40866611a7c2d4faade8897e58fd` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `test` |
| Tier | `full` |
| Iteration | `2` |
| Paired | `paired-infrastructure`, `paired-client` |
| Security | `CLEAR` |
| Test | `RESOLVED-FIXED` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `RESOLVED-FIXED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |

---

## Task Overview

### Objective
See run-story task #18 prompt (story `docs/user-stories/2026-10-31-survive-handler-loss.md`, ADR-0039 §7/§9, mh-sh-contract §8).

### Scope
- **Service(s)**: env-tests, web-app e2e harness, Layer 7 script (run-id export)
- **Schema**: No
- **Cross-cutting**: Yes (test infra shared by MC/MH/client resilience legs)

### Debate Decision
NOT NEEDED - governed by ADR-0039 and the 2026-10-28 debate artifacts.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/env-tests/src/fixtures/mh_pool.rs` | Mine (test) |  |
| `crates/env-tests/src/fixtures/mh_address.rs` | Mine (test) |  |
| `crates/env-tests/src/fixtures/gc_fleet.rs` | Mine (test): hoist `removal_settle_budget` |  |
| `crates/env-tests/src/fixtures/participant.rs` | Mine (test) |  |
| `crates/env-tests/src/fixtures/mc_session.rs` | Mine (test) |  |
| `crates/env-tests/src/fixtures/auth_client.rs` | Mine (test): doc rename only |  |
| `crates/env-tests/src/eventual.rs` | Mine (test): `poll_within` |  |
| `crates/env-tests/src/canary.rs` | Mine (test): image derived from mh.sh `MH_DEBUG_IMAGE` (paired-infra SSoT ask) |  |
| `crates/env-tests/src/resilience_hygiene.rs` | Mine (test): new |  |
| `crates/env-tests/src/lib.rs` | Mine (test): mod line |  |
| `crates/env-tests/Cargo.toml` | Mine (test): dev-dep `syn` |  |
| `Cargo.lock` | Mechanical: `syn` edge for env-tests (already locked) |  |
| `crates/env-tests/tests/40_resilience.rs` | Mine (test) |  |
| `crates/env-tests/tests/42_mh_address_invariant.rs` | Mine (test) |  |
| `crates/env-tests/tests/44_gc_mh_registration.rs` | Mine (test) |  |
| `crates/env-tests/tests/26_mh_quic.rs` | Mine (test) |  |
| `crates/env-tests/tests/27_mc_slot_placement.rs` | Mine (test) |  |
| `crates/env-tests/tests/28_mh_egress_admission.rs` | Mine (test) |  |
| `crates/env-tests/README.md` | Mine (test) |  |
| `docs/specialist-knowledge/test/INDEX.md` | Mine (test) |  |
| `infra/kind/scripts/mh.sh` | Domain-judgment: additive `signal` verb, `MH_DEBUG_IMAGE`, new tokens | infrastructure (paired) |
| `infra/kind/scripts/mh.test.sh` | Domain-judgment: signal cases, suite-wide argv ban | infrastructure (paired) |
| `infra/kind/scripts/deploy.sh` | Minor-judgment: FATAL `preload_debug_image` step | infrastructure (paired) |
| `scripts/setup.test.sh` | Minor-judgment: preload cases | infrastructure (paired) |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mechanical: navigation entry | infrastructure (paired) |
| `docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt` | Domain-judgment: §3/§5/§9 signal amendment, §8(a) pod_ip pin | infrastructure + test |
| `packages/web-app/e2e/mhPool.ts` | Domain-judgment: new | client (paired) |
| `packages/web-app/tests/mhPool.test.ts` | Domain-judgment: new | client (paired) |
| `packages/web-app/e2e/README.md` | Mechanical: harness bullet | client (paired) |
| `packages/web-app/project.json` (Lead: nx `lint` target now `eslint src/ e2e/`, matching the package script; was ungating e2e/) | Mechanical | client (paired) |
| `scripts/layer7.sh` | Minor-judgment: `ENV_TEST_TIMEOUT` default 900 -> 1800 (Gate-2 rc-124 re-measure, runbook §6.7) | infrastructure (paired) |
| `docs/runbooks/devloop-validation.md` | Minor-judgment: §6.7 restated env-test default | infrastructure (paired) |
| `docs/TODO.md` | Mechanical: third copy of the timeout replaced by a pointer to layer7.sh | infrastructure (paired) |
| `docs/specialist-knowledge/client/INDEX.md` | Mechanical: navigation entry (paired-client review F6) | client (paired) |

Not touched beyond the timeout: `scripts/layer7.sh` exports (run id already exported as `ENV_TEST_ORG_SUBDOMAIN` / `E2E_ORG_SUBDOMAIN`; `DT_MH_SH` + `DT_CLUSTER_NAME` in Phase 1h); `packages/web-app/e2e/env.ts` (`e2eEnv.orgSubdomain` already exposed); `crates/env-tests/src/fixtures/kube.rs`.

---

## Planning

### Mechanism restatement

The instance is "a fixture for the MH pool plus an injector". The mechanism is: **every operation a test performs on a shared, live MH pool has exactly one implementation, gated on ownership, with a machine-checked verdict and an asserted cleanup, and the serialization that makes this safe is enforced, not assumed.** Two places are wider than the task names:

1. **The privileged recipe would get two homes.** Rust and Playwright both need kill/freeze/resume. Writing the `kubectl debug` dance twice (image pin, uid, pid lookup, PID-1 refusal, preflight, budget) creates two privileged recipes for security to review and two copies to drift. **Resolution (paired-infra 4g, paired-client 6):** add an additive `mh.sh signal <k> KILL|STOP|CONT --owner ID --json` verb as the ONE home. Both fixtures become thin parsers that keep only outcome assertions (restart count, pod_uid, elapsed vs budget). This needs an additive contract amendment, like the `pod_ip` one.
2. **The serialization invariant is wider than 40_resilience.rs.** `44_gc_mh_registration.rs` already mutates the pool (`up` + teardown) in its own binary, and it is unmarked. The real hazard is a pool mutation running concurrently with another test in the SAME binary, because cargo runs binaries one at a time. **Completed here:** the check covers 40_resilience.rs (every test `#[serial]`, as the task says) AND every `tests/*.rs` that calls a pool-mutating fixture API (`MhPool::up`, `MhSlot::drain*`, `MhSlot::kill/freeze`, `MhPool::teardown*`). 44 gets `#[serial]`. New legs go in 40 (module-doc convention). I'm not moving 44 into 40: it is a `flows` single-test binary cited by name in test-gates.md, and both features run under Layer 7's `--features all`.

### 1. `mh_pool.rs`: one API (extend, never parallel)

- **Rename `RunOwner` -> `MhPool`** and migrate 44, gc_fleet and the auth_client doc in the same commit, so only one surface remains. `MhPool::for_run()` resolves the `DT_MH_SH` path and the owner ONCE at construction and stores both. This fixes the existing defect where `Drop -> reclaim_command() -> mh_sh_path()` can panic, which during unwind is a double panic and an abort (code-reviewer 2). The owner is `auth_client::org_subdomain()` (`ENV_TEST_ORG_SUBDOMAIN`), already validated against the subdomain rule that is sync-guarded to mh.sh's `MH_OWNER_RE`. No new layer7.sh export and no new generator.
- `MhPool::snapshot()` delegates to the existing `list()`/`parse_list_value`, so there is no second parser. Add one test: an extra additive key PASSES. The missing-key test loops over every `SLOT_KEYS` entry; the `SLOT_KEYS` <-> `MH_JQ_PUBLIC` pin already exists.
- `MhPool::up() -> MhSlot {slot, deployment, pod, pod_uid, pod_ip, address, restart_count}`. `pod_ip` is added because the scrape wait needs it (observability 2). Owner mismatch is a PRECONDITION. `MhPool::up_scraped(prom)` = `up()` + `MhSlot::wait_scraped(prom)`: `poll_until_pinned_instance(prom, "up{job=\\"mh-service\\"}", pod_ip, k x service_job_scrape_settle(prom), ..., "mh-scrape-up", "== 1", |v| v == 1.0)`. It pins through `instance_for_pod_ip`, with no `pod=` matcher and no literal `:8083`.
- `MhSlot::drain()` / `drain_no_wait()`: success is taken only from exit code + `MH_RESULT` (+ `drained` in JSON), never from MH metrics (observability 4).
- **Token/exit SSoT.** `MH_TOKENS: &[(&str, u8)]` in Rust is pinned to mh.sh's `MH_TOKENS=( "tok:code" ... )` in BOTH directions, including codes. This replaces the one-direction `branched_reasons_are_in_mh_sh_tokens` and fixes its stale "17 tokens" comment (mh.sh has 15). `run_mh_sh` now fails loudly on an unknown REASON, on an exit code that differs from the token's group (e.g. rc 0 + STATUS=FAIL), on `STATUS=OK` with a REASON other than `ok`, and on more than one `MH_RESULT` line (contract §2: exactly one). Errors surface `STATUS/REASON/SLOT/DETAIL` only, never raw stdout/stderr.
- **BUSY.** `run_mh_sh` becomes a thin wrapper over a pure `run_with_busy_retry(runner, bound, interval, clock/sleep seam)`. Hermetic tests cover: BUSY->OK on attempt N (positive control); exhaustion fails loudly naming `mh-pool-busy`, the attempt count and `DETAIL=holder=<owner>`; and no other exit group is retried. The bound stays `BUSY_RETRY_BOUND` (60 s / 2 s).
- **Fixture-side tokens** each get one named const in mh_pool.rs, distinct from mh.sh's: `foreign-mh-activity`, `mh-run-leak`, `freeze-budget-exceeded`, `restart-count-changed` (freeze), `restart-not-observed` (kill), `mh-scrape-up-timeout`.
- **`assert_baseline(prom)`** wraps the pure `baseline_verdict(&PoolSnapshot, run) -> Result<(), String>`. It fails if any occupied slot has an owner outside {`baseline`, run}, or if the baseline-owned **serving** count != `snapshot.baseline` (B from `list --json`, never a constant). It is unit-tested for: clean, foreign owner, short pool, over-count, a baseline MH that is `starting`/`terminating` (does not count), and `stale` (it still counts: an upgrade-pending MH still serves). The message names the offending owner(s) and slot(s), B vs observed, and a reclaim hint (`$DT_MH_SH drain --all --owner <foreign>` / `$DT_MH_SH list`) (operations 2/3). It is an eventual poll (`eventual::assert_eventually`). Its ceiling is the budget hoisted out of `gc_fleet::wait_removed_mh_unselectable` into `pub fn removal_settle_budget(prom)`: B_gc from `gc_mh_staleness_threshold_seconds` + GC health tick + scrape (dry 4), with no re-derivation. `assert_no_run_leak()` covers nothing with owner = run, nothing `terminating_past_grace`.
- **Teardown / Drop.** `MhPool::teardown()` / `teardown_and_settle()` are kept and asserted (rc 0 + `STATUS=OK`). The command string has ONE home, `fn reclaim_command(path, owner)` (pure), used by teardown's error, Drop and the baseline hint. Drop writes through a writer seam (`fn drop_notice(&self, w: &mut dyn Write)`). Tests: the exact printed command, and that a drop inside a panicking `catch_unwind` does not abort (it does not panic). **Failed leg:** the test body runs teardown on every exit path that it controls. A panic in the middle of a leg leaves only Drop's print. A leaked run-owned MH then makes the NEXT run's `assert_baseline` fail with `foreign-mh-activity`, naming the old owner and the exact reclaim command (operations 2).

### 2. Injector (thin; recipe lives in `mh.sh signal`)

**`mh.sh signal <k> KILL|STOP|CONT --owner ID --json`** is paired-infra's implementation, with test reviewing the contract:
- Refuses `foreign-owner` unless the slot's owner == `--owner`; there is no `--force` for signal (security 7 / ops 7). Baseline MHs are therefore structurally untouchable.
- Preflight `.spec.shareProcessNamespace` on the LIVE pod. If absent, it fails with a new PRECONDITION token and the exact message "pod <name> lacks shareProcessNamespace: mh-template is stale or not Kind-rendered; re-run deploy (infra/kind/scripts/deploy.sh)".
- `kubectl debug <pod> --target=mh-service --profile=restricted --custom=<{securityContext: runAsUser/runAsGroup 65532, runAsNonRoot}> --image=$MH_DEBUG_IMAGE --image-pull-policy=IfNotPresent --container=dt-inject-<sig>-<n>`, non-interactive, always `--context kind-$DT_CLUSTER_NAME` via `dt_init_cluster_env`. The in-container script is a CONSTANT string; the signal name is passed as `$1` from a closed set. It finds the pid as the single `/proc/*/comm == mh-service`; zero or more than one match fails, and pid 1 fails. It never targets `-1`, `0` or a group. It polls `ephemeralContainerStatuses[name].state.terminated` (bounded; ImagePullBackOff/waiting is a loud failure), asserts exitCode 0, and reads output via `kubectl logs -c <name>`.
- `MH_DEBUG_IMAGE`: a digest-pinned readonly const in mh.sh is the one home. deploy.sh preload adds it, and Rust and TS read it only through mh.sh behaviour, never re-encoding it. Ephemeral containers persist on the pod (one per signal), but only run-owned pods get them and teardown drains those pods. That bounds spec growth to one leg's handful (operations 6).
- The JSON carries the slot entry + `pid` + `process_state` (from `/proc/<pid>/stat` after signalling: `T` after STOP, not `T` after CONT) and, on STOP, `liveness_budget_seconds` = `(failureThreshold-1) x periodSeconds + timeoutSeconds` from the live pod's mh-service livenessProbe (paired-infra 5: a STOP right after a probe tick restarts at ~25 s, not 30 s). Absent fields fail loudly; there are no k8s defaults, since the API server fills them on a live pod, so absence means a bad read (code-reviewer 6, test 5). The derivation and every refusal are hermetically tested in mh.test.sh. NOTE: the task's `mh-0-deployment.yaml:429` pointer is stale; the probe is `infra/services/mh-service/deployment.yaml:457`.
- Contract doc: the §5 amendment (signal verb, new tokens, output keys) and §8(a) aligned to `instance=<pod_ip>:8083`.

**Rust** (`MhSlot`, blocking calls via `spawn_blocking`):
- `kill()` -> after signal OK, an eventual poll of `snapshot()` until the same `pod_uid`, `restart_count == before+1` and `serving` (ceiling `kube::MH_POD_SETTLE`). Then it re-runs `wait_scraped` (observability 3: counters reset, so callers re-baseline; this is documented on the fn).
- `freeze() -> FreezeGuard` asserts `process_state == T` (the freeze took effect) and records `Instant` + budget. `FreezeGuard::resume()` checks the pure `freeze_overrun(elapsed, budget)` (unit-tested without sleeping) BEFORE sending CONT, then asserts `process_state != T`, `restart_count` unchanged and the same `pod_uid`, then re-waits the scrape. `FreezeGuard::drop` while still frozen does a best-effort CONT through the same runner (errors are not unwrapped) plus an eprintln of the exact resume command, and never panics (code-reviewer 7, operations 6).

### 3. Participant MediaHandlerSet exposure

- `Participant` gains a `HandlerSets` tracker fed from `JoinResponse.media_handler_set` and EVERY `ServerMessage::MediaHandlerSet` read anywhere in the fixture (`converge` currently drops it into `_`). Plus `await_handler_set(phase, bound, pred)`, because MC may release a parked join with an EMPTY set and send the set later (`actors/join_park.rs`).
- `current() -> (version, Vec<(ref, url)>)`; `history() -> &[(version, Vec<(ref, url)>)]`. The history holds NO tokens (semantic-guard 2).
- `dial_token(ref) -> Option<DialToken>`: the latest token per IN-SET ref. A ref that left the set is DROPPED from the token map, not hidden. An empty token is `None`. `DialToken` is a newtype with a hand-written redacted Debug, no Display/Serialize, and `expose()` for building the MH connect request (MH-1's commit uses it). Unit tests cover: empty -> None, latest-per-ref after a token-only refresh, removed ref -> None, and `format!("{:?}")` of the tracker/token not containing the token text (semantic-guard 1, test 10). No assert message formats a set/entry except through the generated redacted Debug.
- `offered()` / `two_offered_handlers()` read the CURRENT set's urls (sorted), and `mc_session::mc_join_with`'s positive control moves to `media_handler_set.handlers` (non-empty urls). It is equivalent today, since MC double-populates from one projection (`connection.rs:2461`). 26 and 28's direct `media_servers` reads move to the set. **27's double-population arm** (`p.offered() == set`) becomes a tautology and is removed. Its property is MC's, covered by MC's unit test (`webtransport/connection.rs` ~3019), and P3 deletes the field. Result: `grep media_servers crates/env-tests` = 0 hits, so P3 needs no env-test edit.
- Map url -> slot: `MhPool::slot_for_address(&snapshot, url)` (exact `address` match; none or more than one is loud).

### 4. Address helper hoist

`mh_address::assert_addresses_unique()` (async): the whole gather that 42 currently does inline (settle, list, cross-check, CMs, policy, logs) plus `check_address_invariant` and the positive controls, moved verbatim. 42 becomes one call. Resilience legs call it after each leg.

### 5. `#[serial]` hygiene check (always-run lane)

`crates/env-tests/src/resilience_hygiene.rs`, `#[cfg(test)]` lib unit tests (Layer 4, not feature-gated). It parses with `syn` 2 (`full`; already in Cargo.lock, so this adds a dev-dep only; dt-guard has no Rust attribute walker to reuse). It collects every fn (including nested mods) carrying a test attribute (last path segment `test`: `#[test]`, `#[tokio::test(...)]`) and requires an attribute whose last segment is `serial` (`serial`, `serial(group)`, `serial_test::serial`), in any order and with doc comments between. It runs on 40_resilience.rs plus every `tests/*.rs` that names a pool-mutating API. Positive controls: an unmarked `#[tokio::test]` fixture string is flagged, reordered or grouped forms pass, and the real 40 must yield at least 1 test (vacuity). The module doc of 40 states the convention, including that concurrent runs on one cluster fail loudly with `foreign-mh-activity`.

### 6. Smoke env-tests (in `40_resilience.rs`, `#[serial]`, fixture-owned MHs only)

- `mh_pool_fixture_end_to_end`: assert_baseline -> up_scraped -> drain (asserted) -> teardown (asserted) -> assert_baseline + assert_no_run_leak. There are no assertions on the exiting MH's series.
- `mh_injector_freeze_resume_kill_in_place`: assert_baseline -> up_scraped -> freeze (state T) -> resume (restart count unchanged, same uid) -> kill (restart+1, same uid, eventual) -> scrape re-wait -> teardown -> assert_baseline + assert_no_run_leak + `assert_addresses_unique()`.
Both run teardown in the body on every controlled path; Drop is the backstop.

### 7. `packages/web-app/e2e/mhPool.ts` (paired-client's shape, accepted)

- Async `execFile`, argv only, no shell. `DT_MH_SH` is read at CALL time (unset, blank, relative or not a file -> PRECONDITION naming layer7.sh). The owner comes from `e2eEnv`'s org subdomain (`E2E_ORG_SUBDOMAIN`), with no `process.env` re-read.
- The parse mirrors Rust: last stderr line regex; exactly one `MH_RESULT`; known token; exit code == group; OK => `ok` + rc 0 + `json.ok`; typed `MhShError {verb, reason, exitCode, detail}`.
- Verbs: `list/up/drain/drainNoWait/teardown/kill/freeze/resume`. The latter three call `mh.sh signal` (no kubectl in TS at all, so no context question there) and carry the same restart-count, pod_uid, `process_state` and budget assertions. Rust twin sites are cross-referenced in comments (dry 7).
- `tests/mhPool.test.ts` (one file): a stub `mh.sh` in mkdtemp records argv as JSON lines and replays scenarios. It covers: unset/relative path; list golden / unknown version / missing key (each key) / extra key passes; up owner + mismatch; BUSY->OK and BUSY exhausted (injectable clock); unknown token; rc/group mismatch; missing or duplicate MH_RESULT; teardown argv; freeze guard overrun; restart-count assertions both ways; and no `delete` argv ever. Drift pins read mh.sh (`MH_TOKENS` incl. codes, `MH_JQ_PUBLIC`) and mh_pool.rs (`BUSY_RETRY_BOUND`/interval) with positive controls. Stubs carry no real tokens. The header notes that C3 owns the post-teardown GC settle wait and that callers must `test.setTimeout` above mh.sh's derived `up` bound.

### Observability / ops constraints honoured

There are no new series, labels, service config keys, dashboards or alerts. No new env var: README documents the existing `DT_MH_SH`, `DT_CLUSTER_NAME` and `ENV_TEST_ORG_SUBDOMAIN`-as-owner, each with its unset behaviour. A static check for `delete pod` / `--grace-period=0` / `--force` (in signal) across `crates/env-tests/` and `packages/web-app/e2e/` goes in resilience_hygiene.rs (security 4: no existing guard covers it; the grep found none).

### Gate-1 refinements (agreed with reviewers)

- **signal verb** (paired-infra): new tokens `mh-no-share-process-namespace:2`, `signal-failed:1`, `debug-container-timeout:1`, `mh-not-serving:4`. `process_state` is always present: after STOP and CONT, mh.sh polls until the state is T (or not T), with a bound; after KILL it is `gone`. DETAIL carries validated fields only. Marked test-only in usage/help. Takes NO mh-pool Lease (stated in §5). mh.test.sh pins: the image matches `@sha256:[0-9a-f]{64}$`; the emitted securityContext; the signal set checked at argv and in-container; pid > 1; an argv-recording case that never contains `delete` / `--grace-period` / `--force` (a positive control does call `kubectl debug`). That case covers mh.sh, which can't be grepped because its comments contain the needles.
- **FreezeGuard** clock starts BEFORE the STOP call. In TS the guard implements `[Symbol.asyncDispose]` (best-effort CONT), falling back to `dispose()` + try/finally if the tsconfig lib lacks `esnext.disposable`.
- **mhPool.ts** is the factory `createMhPool({owner, mhSh, clock, sleep})`, with the owner read lazily from e2eEnv. Drift pins against mh_pool.rs cover `BUSY_RETRY_BOUND`, `MH_POD_SETTLE` (kube.rs) and the fixture-side tokens. Rust and TS MH_RESULT parsers carry identically named rejection cases.
- **BUSY vs Lease**: a hermetic test reads `MH_LEASE_S` and `MH_LEASE_FUTURE_SKEW_S` from mh.sh and asserts `BUSY_RETRY_BOUND > lease + skew + interval`; the exhaustion message names the lease.
- **Hygiene scan**: needles are built with `concat!` plus a self-path exclusion, with a positive control. The pool-mutating API set is derived from a `Pool-mutating.` doc marker in mh_pool.rs via syn. Vacuity: the set includes up/drain/kill/freeze/teardown, and discovery finds 40 and 44.
- **slot_for_address**: exact match, byte-identical by construction (set URL = MC projection of MH's RegisterMH endpoint = slot CM `MH_WEBTRANSPORT_ADVERTISE_ADDRESS`, proven by R-28; mh.sh `address` = the same CM value). 0 or >1 matches fail loudly.

### Open items for @team-lead

- (a) Adopting the `mh.sh signal` verb moves the liveness-budget derivation and the PID recipe into mh.sh (tested in mh.test.sh) rather than a Rust pure fn. The Rust/TS side tests only the overrun check. This deviates from the task's "fixtures/mh_inject.rs" wording on SSoT grounds.
- (b) The serial invariant is widened to 44 (+ any future pool-mutating binary).
- (c) 27's double-population arm is removed (it becomes a tautology; MC unit-tested).

---

## Gate 1

Lead rulings (2026-11-10): (a) `mh.sh signal` verb as the single injector home — APPROVED (completion of the one-home invariant; additive contract §5 amendment, infra owner paired). (b) widened #[serial] rule to any pool-mutating test binary (44) — APPROVED. (c) env-tests fully off `JoinResponse.media_servers`, 27's tautological arm removed — APPROVED. Classification guard: `STATUS=OK`. Note: team-lead address resolves as `main` in this session.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired infrastructure | confirmed |
| Paired client | confirmed |

---

## Pre-Work

None

---

## Implementation Summary

- **mh_pool.rs**: `RunOwner` renamed to `MhPool`, which resolves the path, cluster and owner once, so Drop cannot panic.
  - `MH_TOKENS` table is pinned to mh.sh in both directions, codes included.
  - Strict `parse_mh_sh_result` with named cases: one-line, fields, known-token, exit-group, ok-reason, json-ok-agrees.
  - `run_with_busy_retry` with runner and sleep seams; count-based, with the holder and lease named on exhaustion.
  - `MhSlot` provides drain, drain_no_wait, kill (in-place restart poll), freeze, wait_scraped (pinned by pod IP).
  - `FreezeGuard`: the clock starts before STOP; resume checks the budget, the state and the incarnation; Drop sends a best-effort CONT.
  - Pure verdicts: `baseline_verdict`, `run_leak_verdict`, `slot_for_address`, `parse_signal`, `freeze_overrun`.
  - The fixture-side tokens are consts.
  - teardown is asserted and includes the leak check.
- **gc_fleet.rs**: the budget is hoisted into `removal_settle_budget`, and `assert_baseline` polls with it through `eventual::poll_within`.
- **mh_address.rs**: `assert_addresses_unique()` is the hoisted live gather; 42 is now one call.
- **participant.rs**:
  - `HandlerSets` tracker (history holds membership only) and `DialToken` with a redacted Debug.
  - `offered()` reads the current set; `await_handler_set` added.
  - `converge` now absorbs set messages.
- **media_servers**: env-tests no longer reference it (mc_session, 26, 27, 28); 27's tautological arm is removed.
- **resilience_hygiene.rs** (Layer 4, syn):
  - Every test in a pool-mutating binary is `#[serial]`. The API set is derived from the `Pool-mutating.` doc marker, and discovery must find 40 and 44.
  - Scans string literals for MH pod-delete and force paths. canary.rs is allowlisted for its own graceful canary deletes.
- **40_resilience.rs**:
  - The module doc states the isolation convention.
  - `with_pool` asserts the baseline before and after, runs teardown on every path, and re-raises the leg's panic.
  - Two smoke legs: pool end to end; freeze, resume, then kill in place, with R-28 afterwards.
  - **44** gets `#[serial]` and migrates to `MhPool`.
- **canary.rs**: the image is derived from mh.sh `MH_DEBUG_IMAGE` (one busybox, pinned and preloaded).
- **mhPool.ts + tests/mhPool.test.ts**: the TS twin, with identically named rejection cases.
  - The FreezeGuard has `dispose()` because the tsconfig lib has no `esnext.disposable`.
  - The Node test runs against a stub mh.sh: 50 tests, including drift pins against mh.sh, mh_pool.rs, kube.rs and eventual.rs.
- **Infra (paired-infrastructure)**:
  - `mh.sh signal` and mh.test.sh (1359/0).
  - The FATAL `preload_debug_image` step and its setup.test.sh cases (1121/0).
  - Contract §3/§5/§8(a)/§9.

The live smoke legs in 40 run only at Layer 7 (Lead's `layer-all.sh`).

---

## Files Modified

```
 Cargo.lock                                         |    1 +
 crates/env-tests/Cargo.toml                        |    6 +
 crates/env-tests/README.md                         |   31 +-
 crates/env-tests/src/canary.rs                     |   49 +-
 crates/env-tests/src/eventual.rs                   |   77 +-
 crates/env-tests/src/fixtures/auth_client.rs       |    2 +-
 crates/env-tests/src/fixtures/gc_fleet.rs          |   76 +-
 crates/env-tests/src/fixtures/mc_session.rs        |   17 +-
 crates/env-tests/src/fixtures/mh_address.rs        |  103 +-
 crates/env-tests/src/fixtures/mh_pool.rs           | 1955 ++++++++++++++++++--
 crates/env-tests/src/fixtures/participant.rs       |  289 ++-
 crates/env-tests/src/lib.rs                        |    2 +
 crates/env-tests/src/resilience_hygiene.rs         |  517 ++++++
 crates/env-tests/tests/26_mh_quic.rs               |   36 +-
 crates/env-tests/tests/27_mc_slot_placement.rs     |   27 +-
 crates/env-tests/tests/28_mh_egress_admission.rs   |    7 +-
 crates/env-tests/tests/40_resilience.rs            |  108 +-
 crates/env-tests/tests/42_mh_address_invariant.rs  |   98 +-
 crates/env-tests/tests/44_gc_mh_registration.rs    |   49 +-
 docs/TODO.md                                       |    2 +-
 .../mh-sh-contract.txt                             |   17 +-
 .../2026-11-10-mh-pool-fixture/main.md             |  301 +++
 docs/runbooks/devloop-validation.md                |    2 +-
 docs/specialist-knowledge/client/INDEX.md          |    2 +-
 docs/specialist-knowledge/infrastructure/INDEX.md  |    2 +-
 docs/specialist-knowledge/test/INDEX.md            |    4 +-
 infra/kind/scripts/deploy.sh                       |   40 +
 infra/kind/scripts/mh.sh                           |  222 ++-
 infra/kind/scripts/mh.test.sh                      |  281 ++-
 packages/web-app/e2e/README.md                     |    7 +
 packages/web-app/e2e/mhPool.ts                     |  765 ++++++++
 packages/web-app/project.json                      |    2 +-
 packages/web-app/tests/mhPool.test.ts              |  693 +++++++
 scripts/layer7.sh                                  |   13 +-
 scripts/setup.test.sh                              |   29 +-
 35 files changed, 5356 insertions(+), 476 deletions(-)
```


---

## Devloop Verification Steps

### Gate 2 attempt 2 (L7 attempt 2/2) — PASS
`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`: L1-L6 OK; L7 OK (1586s; Rust env-tests per-binary sum 914s incl. 40_resilience smoke legs, under the new 1800s budget; browser E2E OK). `TOTAL_RESULT=OK`.

### Gate 2 attempt 1 (L7 attempt 1/2)
Layers 1-6 OK. Layer 7 FAIL `env-tests-failed`: suite hit `DEVLOOP_ENV_TEST_TIMEOUT=900s` (rc 124) during 44; per-binary sum 866s (task #17: 812s). No test failed (40_resilience 5/5 incl. both smoke legs, 42, 43 OK). Diff-caused budget outgrowth → implementer lane: raise default per runbook §6.7.


---

## Code Review Results

### Gate 3 verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | injector: owner-gated, uid/gid 65532 restricted, digest-pinned image, pid>1, constant script; DialToken redacted |
| Test | RESOLVED-FIXED | 5 | 5 | 0 | keyed #[serial] rejected; KILL ceiling from mh.sh restart_budget_seconds; R-28 inside kill leg; top-level key loop; bounded poll in mh.test.sh |
| Observability | CLEAR | 0 | 0 | 0 | up==1 pinned by pod IP via instance_for_pod_ip; drain outcomes from mh.sh only |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | FreezeGuard resumed-after-CONT; real post-STOP check; MhSlot::refresh; one argv builder; MhPool::run_leg |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | committed_mh_sh_source(); poll_kernel; signal_args/teardown_args; schema/baseline drift pins |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | DT_CLUSTER_NAME PRECONDITION; freeze span through CONT; KILL budget derived; README isolation |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |
| Paired infrastructure | RESOLVED-FIXED | 2 | 2 | 0 | freeze overrun after CONT; cluster threaded into child env |
| Paired client | RESOLVED-FIXED | 8 | 8 | 0 | freeze leak paths; reclaim hint; drain argv tests; INDEX; e2e lint errors; cluster env |

Lead post-Gate-3 edit (pre-Gate-2): `packages/web-app/project.json` nx `lint` target widened to `eslint src/ e2e/` to match the package script (both reviewers noted e2e/ was ungated; `eslint e2e/` verified clean). Fix-don't-defer, single-source-of-truth drift.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `cd1fb42cee9b40866611a7c2d4faade8897e58fd`
2. `git diff cd1fb42c..HEAD`
7. **Safe-revert unit**: the whole commit. The fixtures (Rust + TS) consume the additive `mh.sh signal` verb and `restart_budget_seconds`; reverting mh.sh/contract alone would break both fixtures' smoke legs, and reverting the layer7.sh 1800s budget alone would re-time-out Layer 7 with the new legs present. Safe direction: revert all together.

---

## Issues Encountered & Resolutions

- Gate 2 attempt 1: Layer 7 env-tests hit `DEVLOOP_ENV_TEST_TIMEOUT=900` (rc 124) inside 44; no test failed (40's legs 5/5 in 132s). The per-binary sum through 43 was 866s.
  - Measured live: 44 takes 41.3s (task 17: 40.4s), so `run_leg`'s before/after `assert_baseline` costs about 1s. The poll returns on the first clean read. 45 takes 23.8s.
  - The suite is about 945s with compile. paired-infra raised the default to 1800s (~52%) per runbook §6.7, with no coverage trimmed.
  - The pool was verified clean after the killed run: baseline 2, no run-owned MH.
