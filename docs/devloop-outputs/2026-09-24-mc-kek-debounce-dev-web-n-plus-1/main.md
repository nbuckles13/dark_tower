# Devloop Output: MC KEK rotation debounce W config + dev-web N+1 bring-up

**Date**: 2026-09-24
**Task**: Add `MC_KEK_ROTATION_DEBOUNCE_SECONDS` (W=60, required, no Rust default) to the MC ConfigMap + both MC deployments; update `scripts/dev-web.sh` / `scripts/dev-web.test.sh` for the N+1-browser-profile bring-up (story 2026-09-21-hear-each-other task 4; R-12, R-23, R-25, R-1, R-7)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present (tier=light ESCALATED to full: plan reaches K8s deployment manifests, a `--light` exclusion surface)
**Branch**: `feature/hear-each-other`
**Duration**: ~2h40m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `78bf4959af5af4fdb20c29042a13a970deb77c28` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full — escalated from light: task edits infra/services/mc-service/{configmap,mc-0-deployment,mc-1-deployment}.yaml (K8s deployment manifests = --light exclusion surface; SKILL.md Step 5 escalation backstop)` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — diff (YAML config + bash dev script) touches no scripts/guards/semantic/checks.md surface` |

---

## Task Overview

### Objective
See task description (verbatim in run-story task-4.prompt). Summary: W config key for the MC KEK-lifecycle task (dependent task), plus dev-web.sh N+1-profile bring-up docs/preflight (VITE_DT_RECEIVE_SLOTS, MC_MAX_RECEIVE_SLOTS cap echo, test-tone mode, second-machine non-option contract).

### Scope
- **Service(s)**: mc-service deployment config; dev tooling scripts
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED — values/contract fixed by the story plan.

---

## Cross-Boundary Classification

| File | Owner | Classification |
|------|-------|----------------|
| `infra/services/mc-service/configmap.yaml` | infrastructure | Mine |
| `infra/services/mc-service/mc-0-deployment.yaml` | infrastructure | Mine |
| `infra/services/mc-service/mc-1-deployment.yaml` | infrastructure | Mine |
| `scripts/dev-web.sh` | infrastructure | Mine |
| `scripts/dev-web.test.sh` | infrastructure | Mine |
| `docs/TODO.md` | infrastructure (entries edited) | Mine |
| `docs/user-stories/2026-09-21-hear-each-other.md` | Lead (story planning doc) | Mine-by-Lead-authorization |

Row detail:
- `docs/TODO.md`: extend the on-disk-ConfigMap entry (~1564) with the cap read as a second call site; tick the counter-message entry (~1009), whose defer trigger fired; add a Media Path Obligations entry: task 9 adds the mc-deployment.md W row and updates the "fifteen Required rows" count (operations C1b).
- Story file: remove the false "a profile's tabs share the auth session" rationale at ~20 (R-1), ~54 (R-25), ~125, ~180 (decision 5), ~219, ~354 (task 4), ~600 (manual test plan). Lead authorized this in-loop at Gate 1.
- WITHDRAWN: the `docs/runbooks/client-dev-local.md` "tab or profile" edit (D-1). D-7 showed the premise false, so the runbook is correct as written and was not edited.
- This devloop's own main.md is the record, not a planned change.

No Guarded Shared Area touched. No Rust, proto, client or guard-crate edit. `crates/mc-service/src/config.rs` (the required read of W) is task 9's, not this task's.

---

## Planning

### Mechanism restatement
Two instances of one class: *a value whose divergence or invisibility silently changes behaviour must be (a) sourced once and (b) visible where the operator stands.* W: one ConfigMap key, both workloads, no Rust default -> divergence impossible without a CrashLoop. Client N: one exported value per dev server, validated against the cap READ from the MC ConfigMap (not copied), echoed in the preflight.

### Part A — W (R-12, R-23)
1. `configmap.yaml`: add `MC_KEK_ROTATION_DEBOUNCE_SECONDS: "60"` in a new "KEK lifecycle (ADR-0036 §4)" block after the media-signalling block. Comment: REQUIRED, no Rust default (task 9 reads via `ConfigError::MissingEnvVar`); security consequence of divergence (R-12 states the leaver exposure bound AS W; two MCs with different W = bound depends on which pod owns the meeting, invisible from outside); pointer back to the existing ROLLBACK ORDERING / three-artifact block rather than a copy, plus the one-line standing warning inline (`resources:` not `configMapGenerator:` -> editing does not roll pods, wrong W latent until next restart); where an operator sees it (`mc_meeting_kek_rotation_window_seconds`, task 9, named in the story plan); bounds validated by task 9 at load (not restated as figures here — task 9 owns them).
2. `mc-0-deployment.yaml`, `mc-1-deployment.yaml`: identical `configMapKeyRef` env entry (key == env name, rule 4), under its own short comment.
3. Ordering: this lands before the task-9 image (story §Deploy ordering). Until task 9 lands the key is referenced but unread by Rust; env-config rule 3 (referenced by a workload) holds, rule 1 coverage arrives with task 9's `MissingEnvVar`.

### Part B — dev-web.sh (R-25, R-1, R-7, R-23)
1. Header contract: new "N+1 PARTICIPANTS" section — profiles not tabs (tabs in one profile share the auth session: one cookie jar/storage partition per profile); test-tone via `__DT_TEST_TONE__` (client task 13 owns the define and its enabling mechanism; this script names it and does not invent its switch) as the answer to one mic feeding back across N participants, not headphones (headphones only for the separate human-audibility pass); N via `VITE_DT_RECEIVE_SLOTS`.
2. Header contract: "WHY NOT A SECOND MACHINE — verified, not cautionary", each claim cited:
   - secure context: `http://demo.localhost:5173` is potentially trustworthy (`.localhost`), `http://<lan-ip>:5173` is not -> getUserMedia and WebTransport unavailable (existing SECURE CONTEXT block + runbook).
   - leaves carry DNS-only SANs: `scripts/generate-dev-certs.sh:generate_service_cert()` emits `DNS:` entries only (localhost, mc-/mh-service[.dark-tower[.svc.cluster.local]]); no IP SAN. STATED PRECISELY: the browser path pins by `serverCertificateHashes`, which does not name-check (setup.sh's devloop patch comment already relies on this), so the SAN fact bars name-verifying clients, it is not the browser-side blocker. I will not overstate it.
   - advertise addresses are loopback literals: `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-configmap.yaml` = `https://127.0.0.1:443{3..6}` -> a second machine's browser dials its OWN loopback.
3. New preflight section "Receive slots (client N vs MC cap)":
   - cap: parsed from `infra/services/mc-service/configmap.yaml` `MC_MAX_RECEIVE_SLOTS` (SSoT; not copied). Unparseable/absent/non-integer -> HARD FAIL "CANNOT VERIFY ... REPO/CONFIG DRIFT" (same lane as the advertise-address drift branch).
   - N: `VITE_DT_RECEIVE_SLOTS` if the operator set it, else the launcher's demo default `3` (named `DEMO_RECEIVE_SLOTS`, the story's manual-plan N; A..D hear three, E is the fewer-sources case). This is a LAUNCHER parameter, same precedent as `VITE_TELEMETRY_ENDPOINT` in this script, and it is always exported, so the echoed N is by construction the N every profile's client sees — no dependency on (or copy of) whatever default task 13 gives the SDK.
   - N not an integer >= 1 -> HARD FAIL. N > cap -> HARD FAIL: MC rejects the whole declaration and the participant presents as one silent participant — exactly the contract's "silently produces no audio".
   - pass line echoes `effective N=<n> (VITE_DT_RECEIVE_SLOTS, <set|launcher default>) <= MC_MAX_RECEIVE_SLOTS=<cap> (from <file>)` and "with N=3, four participants each hear the other three; a fifth finds some receivers' slots already full — that participant is the fewer-sources case, not a bug".
   - Known stale-on-devloop caveat mirrors the existing WT block: print the live `kubectl get cm mc-service-config -n dark-tower -o jsonpath='{.data.MC_MAX_RECEIVE_SLOTS}'` command (SHARED ConfigMap, consumed by both mc-0 and mc-1).
   - "per profile": one Vite dev server serves every profile on one origin and `import.meta.env` is fixed at server start, so the exported N is per dev-server = identical for every profile; changing N means restarting the script. Stated in header and launch banner.
4. Launch banner: print the N+1 profile bring-up steps (distinct accounts, one profile each, `chrome --profile-directory`-free: "Chrome profile menu -> Add"), N and cap. No browser flag.
5. `--help` sentinel (header last line) kept last; existing hard fails untouched.

### Part C — dev-web.test.sh
New cases (hermetic; fixture writes a synthetic `infra/services/mc-service/configmap.yaml` in `make_root`; `run_check` gets `env -u VITE_DT_RECEIVE_SLOTS` default + explicit override):
- default N: pass line contains `N=3` and `MC_MAX_RECEIVE_SLOTS=8` (fixture cap), positive control.
- N > cap (e.g. 9 vs 8): hard-fail branch (`✗` present, `!` absent), mentions "one silent participant".
- N=0 and N=abc: hard fail.
- cap key missing from fixture: CANNOT VERIFY hard fail, drift lane.
- cap echoed from the file (fixture cap 5, N=5 -> pass with 5): proves read not hardcoded.
- `--help` contains profiles/`__DT_TEST_TONE__`/second-machine sentences; header-last-line sentinel still asserted.
- Existing assertions unchanged (every existing hard fail still asserted).


### Security planning input folded in (security, pre-plan)
- W range: the bound VALUES are task 9's (`crates/mc-service/src/config.rs` is their single home, "bounded both sides at load" per story task 9). The comment states that the value is bounded both sides and that enforcement ARRIVES WITH task 9's read. Until then nothing in the tree reads or validates the key, and the comment says so plainly: no claimed enforcement that does not exist. Security's input for task 9 (REVISED by security, superseding its earlier `1..=300`) is **`30..=300`**, not written as figures in the ConfigMap. Floor: the client's derived retention must exceed its transmit-key re-wrap latency (proto `JoinResponse.kek_rotation_debounce_seconds` ~509-511), so a tiny W is a media-path availability failure. Ceiling: MC's timer is unclamped and R-12's exposure bound is the MC-side number; the client-side ceiling clamp protects a different property (client key minimisation) and does not make the server bound optional. Task 9 inherits this reasoning; the bound's home is `crates/mc-service/src/config.rs`.
- The comment says W reaches clients on the wire (`JoinResponse`/`MeetingKekUpdate.kek_rotation_debounce_seconds`) and drives their previous-generation retention, so tuning W moves client key hygiene too. The derivation's home is that field's doc; no formula is restated.
- Divergence consequence worded concretely: "the leaver exposure bound is whichever pod owns the meeting, and nobody outside can tell which."
- Profile dirs: the script creates none. The operator adds profiles through Chrome's own profile manager, which lives in Chrome's user-data dir outside the repo. The script echoes no token.
- Test tone: documented only. There is no runtime `VITE_DT_TEST_TONE`-style var and no `.env` is written. The script never implies tone is on.
- Cap read from the in-repo ConfigMap. Empty or non-integer triggers a distinct HARD FAIL. There is no fallback, and N > cap hard-fails with no clamp.
- The existing "Never a browser flag" sentence stays verbatim. The second-machine rationale sits adjacent to it.

### Observability planning input folded in (observability, pre-plan)
- N > cap and an unreadable cap are both HARD FAIL. An unreadable cap is CANNOT VERIFY, in the drift lane. There is never an empty echo.
- The echoed N comes from the exported `VITE_DT_RECEIVE_SLOTS` variable itself, not from a second literal.
- The live-value command is spelled for the SHARED `mc-service-config` in its own helper. It does NOT reuse `wt_live_value_cmd()`, which derives per-instance names.
- The remedy names `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}` (docs/observability/metrics/mc-service.md) as the in-cluster ground truth.
- Every new check gets a row in the header "What it verifies" severity block.
- The W key carries `Enforced by:` / `Where an operator sees it:` trailers. The first names mc-service at config load, from task 9. The second says "nothing today", then names `mc_meeting_kek_rotation_window_seconds` beside `mc_meeting_kek_generated_total{trigger}` and `mc_meeting_kek_rotation_pending_age_seconds`. An explicit "no observable effect until task 9" NOTE follows the MC_AUDIO_* note shape. The comment also states that a changed W reaches connected clients only on their next `MeetingKekUpdate`.
- No account identifiers anywhere. Profiles are keyed by index (profile 1..N+1).
- Test positive control: the fixture writes the cap and the assertion derives the expected number from the fixture variable.

### Operations planning input folded in (operations, pre-plan)
- Ordering conclusion, stated for Gate 2. Rule 1 of `dt-guard env-config` does not demand the key until task 9 adds `MissingEnvVar("MC_KEK_ROTATION_DEBOUNCE_SECONDS")`. Rules 2, 3 and 4 are satisfied by the two refs, so the key without a Rust read is guard-green and runtime-inert, and this manifest lands first. The reverse order (task-9 image without the key) would CrashLoop.
- Roll asymmetry is worded precisely. This task's own apply DOES roll mc-0/mc-1 once, because the pod template gains an env ref. It is a later VALUE edit of W in configmap.yaml that does not roll and stays latent.
- The rollback essay is cited by name, not copied. The W block adds only what is W-specific.
- "Nothing reads it yet" NOTE: see the observability section. `Enforced by:` reads "nothing yet — task 9 adds the required read and its two-sided load-time bound".
- N is per dev server, not per profile. It is stated in the header and in the echoed line, with a restart-to-change note mirroring the fingerprints branch.
- Cap extraction is anchored on the data-key line (`^[[:space:]]+MC_MAX_RECEIVE_SLOTS:[[:space:]]*"?[0-9]+"?`), so prose mentions of the name in comments never match. The fixture includes a decoy comment line mentioning `MC_MAX_RECEIVE_SLOTS: "99"`-style prose to prove it. Drift lane plus a live kubectl line.
- N > cap: HARD FAIL, confirmed.
- Header mechanics: new prose goes BEFORE the `Usage:` block, and the last line (`AC_PORT / GC_PORT env if your overlay differs.`) stays last, so its sentinel is unchanged. The header stays contiguous and no `SKIP_`/`ALLOW_` bypass is added.
- Procedure placement: the script keeps a short numbered bring-up of 6 lines or fewer. The full human procedure (the N=3 A..E walkthrough, dashboards, triage) is the story's manual-test-plan deliverable (story line ~600), so it is not duplicated in `client-dev-local.md` by this task.
- docs/runbooks/mc-deployment.md env-table row for W: deferred to task 9, which is the task that makes the key required (the row's "Required" column would be false today). This is not a one-line fix that is being parked: the row's truth depends on task 9's code.

### DRY planning input folded in (dry-reviewer, pre-plan)
- D-1: both runbook "tab or profile" lines are corrected to profiles (see Cross-Boundary table; Minor-judgment into operations; operations asked to confirm).
- D-2: two readers, with a shared comment anchor, not one helper. The WT read extracts an `https://` URL from a per-instance ConfigMap by an unanchored service-prefixed key (it is deliberately unanchored, see its comment). The cap read extracts a bare integer from the SHARED ConfigMap by an ANCHORED data-key line, because the key name also appears in comment prose there. One helper would have to branch on both the anchoring policy and the value shape, so per the collapse test it would be the wrong shape.
- D-3: distinct CANNOT VERIFY drift branch, driven by a test case (see test section).
- D-4: live-value command printed for `mc-service-config`, and the existing TODO ~1564 entry is extended (no second entry).
- D-5: one home for the DEV value of N: dev-web.sh always exports, and the echo reads the exported variable. The SDK's own default (task 13) is the non-dev home. It is never consulted by the echo, because the launcher always exports.
- D-6: header rows with severity tags; contiguity is kept.
- TODO ~1009 trigger fires (this diff adds failure output). Fix, in the entry's prescribed shape: on the hard-fail exit path only, print the retained secure-context / never-a-browser-flag line, naming the CLASS and no flag literal. It is test-driven: every hermetic `--check` run hard-fails on AC/GC, so the path is exercised. A new assertion pins it, and the entry is ticked.

### Observability round 2 + code-reviewer Q1-Q4 folded in
- The W comment names exactly `mc_meeting_kek_rotation_window_seconds`, `mc_meeting_kek_generated_total{trigger}` and (optionally) `mc_meeting_kek_rotation_pending_age_seconds`, and NOT the overdue-threshold gauge (it is a separate knob).
- **`min(W/2, ceiling)` dropped** from the ConfigMap comment. The comment cites `JoinResponse.kek_rotation_debounce_seconds` as the derivation's home and carries no formula.
- The live kubectl command carries `-n dark-tower` and names `mc-service-config` as the SHARED ConfigMap consumed by both mc-0 and mc-1.
- New header rows, enumerated: N valid integer >=1 and <= cap [HARD FAIL]; cap cannot be read [HARD FAIL] with its causes (file absent, key absent, non-integer).
- The over-cap remedy says the counter `slot_count_over_cap` is the AUTHORITY, while the echo is a pre-flight read of an on-disk file. It points at docs/observability/metrics/mc-service.md.
- The fewer-sources clause is phrased in participants: "with N=3, four participants each hear the other three; a fifth finds some receivers' slots full — that is the fewer-sources case, not a bug".
- The cap-5/N-5 clean run also asserts the ABSENCE of the over-cap text.
- CR-Q1: both refs are plain `configMapKeyRef` with no `optional:`, so a missing key gives CreateContainerConfigError (loud) today, before any Rust read exists.
- CR-Q2: env-config has no "reader must exist" rule. Rule 3 (referenced by a workload naming the CM) and rule 4 (key==name) are satisfied, and there is no allowlist/manifest registration. I will run it and record the STATUS line.
- CR-Q3: the second-machine facts live in the dev-web.sh header, reachable via `--help` and pinned by a test.
- CR-Q4: (a) the echo labels the source as "(set by you)" or "(launcher demo default)". (b) 3 has no upstream SSoT in the tree. Grep of packages/scripts/crates/env-tests finds no receive-slot count, and the story's "suite locked at N=3" belongs to a later test task. So it is a launcher-local named constant `DEMO_RECEIVE_SLOTS`, with a comment that it is the demo topology, not the SDK default.

### Security round 2
- Profile hygiene sentence: use profiles created for the demo, not your daily profile, and remove them afterwards, because each retains a live signed-in dev session.
- The proto is canonical for the retention rule (the story's "W/2" prose is looser). The ConfigMap cites the field and restates no formula.

### D-7 (dry-reviewer): the "tabs share the auth session" premise is FALSE, verified
- `packages/web-app/src/App.svelte:22`: `let auth = $state<AuthSession | undefined>(undefined)`. This is "the in-memory authenticated session (never persisted — R-23)", and it is the token's "only home" (l.26-27).
- There are no `localStorage`/`sessionStorage`/`indexedDB`/`BroadcastChannel`/`document.cookie` hits in `packages/{web-app,sdk-core,sdk-svelte}/src`. The SDK comments (`media/setup/identity.ts:79`, `media/frame/ed25519.ts:184`) state no web storage by design. No cookie is set by AC/GC (`grep -i cookie crates/ac-service/src` finds nothing, and the GC hit is otel only).
- Result: each tab of ONE profile loads a fresh SPA with `auth === undefined` and signs in independently. Tabs do not share the auth session.
- What the script says instead (narrowed per D-7a): N+1 participants need N+1 browsing contexts, each a DISTINCT account. Separate profiles are RECOMMENDED as the known-good shape. The auth session is NOT the reason: it is per-context in-memory state (App.svelte:22) with nothing persisted, so tabs of one profile are already independently authenticated. Whether N+1 simultaneously-capturing tabs in ONE profile behave as well is UNVERIFIED and is marked so. The script asserts neither "tabs fail" nor "tabs suffice", and no test pins either positive; only the cited negative is asserted.
- D-1 collapses: the runbook's "tab or profile" is correct, so there is NO runbook edit.
- Security's profile-hygiene sentence is kept, conditioned: "if you do use extra profiles".
- The N wording is "one value per dev server: every tab and profile served by it gets the same N".
- Recorded as deviation #2 in §Issues, so the Lead can amend story R-25 / the manual-test-plan prose (story ~600), which carry the false premise.

### Operations round 2 (conditions 1-4)
1. Breadcrumbs. (a) The W ConfigMap comment says the `docs/runbooks/mc-deployment.md` §Configuration Reference row lands with task 9, because that table's Required column is `config.rs`-derived. (b) A `docs/TODO.md` entry: task 9 adds the W row AND updates the "fifteen Required rows" literal count. The TODO entry is chosen over the story file, which is not this task's to edit. It is genuinely task-9-sized, because the row's truth depends on task 9's code.
2. Real-file positive control. A test case copies the REAL `infra/services/mc-service/configmap.yaml` into a fixture root and runs `--check`. It asserts (own reason token) that the cap line carries a non-empty integer >= 1 and that no CANNOT-VERIFY cap branch fired. There is no literal-8 expectation. The comment-decoy fixture is kept.
3. §Rollback Procedure gains the two non-generic facts (Deployment revert rolls pods, ConfigMap revert does not; after task 9, reverting this task is a CrashLoop, not a rollback).
4. The existing TODO ~1564 entry is extended to name the cap read as the second on-disk consumer (already committed under D-4).
- Check: the N/cap line prints in the PREFLIGHT (before the `Preflight OK`/`--check` exit), not the launch banner.

### Operations round 3 (conditions 5-7) + D-7a reconciled
- C7: the emphasized rule is **N+1 DISTINCT REGISTERED ACCOUNTS, one per browsing context**. The container is the mechanics beneath it: profiles recommended as known-good; tabs-in-one-profile under N+1 simultaneous capture UNVERIFIED (D-7a). The script does not say "tabs suffice".
- The not-persisted claim cites the standing control `scripts/guards/semantic/checks.md` §Client Credential Lifetime (a persisted session is a named finding on every client diff), plus `App.svelte:22`.
- C6: symptom sentence: if contexts ever shared a session, every context would show as the SAME participant (a roster of 1 where you expect N+1). Look at the client, not MC routing. A roster short by one usually means two contexts used the same account.
- C5: story sites ~354 and ~600 are corrected in place IF the Lead authorizes an edit to the story file (Lead-owned plan; dry-reviewer raised it to main as a Gate-1 condition). The correction is flagged for @test / the manual-test-plan task either way.

### Validation
`dt-guard env-config` (both mc workloads), `bash scripts/dev-web.test.sh`, `shellcheck`, `./scripts/layer-fast.sh`.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

---

## Implementation Summary

- **W key**: `MC_KEK_ROTATION_DEBOUNCE_SECONDS: "60"` at the end of `infra/services/mc-service/configmap.yaml`, in its own "Meeting KEK lifecycle" block. The comment covers:
  - required with no Rust default, and why;
  - the security consequence of divergence;
  - W reaching clients via `JoinResponse.kek_rotation_debounce_seconds`, cited with no formula;
  - the no-auto-roll warning for later value edits, with a cite of the existing three-artifact block;
  - `Enforced by: NOTHING in the tree today` (task 9);
  - the operator observables from task 9;
  - a NOTE that there is no effect until task 9, and a runbook-row breadcrumb.

  Identical non-optional `configMapKeyRef` entries (key == env name) are in `mc-0-deployment.yaml` and `mc-1-deployment.yaml`. The two files still differ only in their six instance lines.
- **dev-web.sh**:
  - Two new severity rows in "What it verifies".
  - A "WHY NOT A SECOND MACHINE" header block with three cited facts, the SAN fact scoped as not a browser blocker. It sits adjacent to the unchanged "Never a browser flag" sentence.
  - An "N+1 PARTICIPANTS" block. It leads with the distinct-accounts rule, recommends profiles, cites the in-memory session, marks tabs as UNVERIFIED, and gives the symptom and profile hygiene. It covers test tone via `__DT_TEST_TONE__` (documented, not switched) and one N per dev server.
  - New preflight `check_receive_slots`:
    - It always exports `VITE_DT_RECEIVE_SLOTS`, taking the operator's value or `DEMO_RECEIVE_SLOTS=3`, and labels the source.
    - It reads the cap from the shared ConfigMap with an anchored data-line match.
    - Hard fails: an unreadable cap (CANNOT VERIFY, drift lane), an invalid N, and N > cap. The N > cap comparison is length-first, so it cannot overflow. The message names the authority counter and prints the live kubectl command.
    - The pass line echoes N, its source, and the cap with its file, in participant terms.
  - The hard-fail exit prints the never-a-browser-flag counter-message (TODO ~1009).
  - The launch banner gains the N+1 steps.
  - All existing hard fails are unchanged.
- **dev-web.test.sh**: the suite went from 79 to 136 assertions.
  - The fixture writes the shared MC ConfigMap with a comment decoy. `run_check` takes an optional N and otherwise `env -u`.
  - Cases (10a-j): default positive control; cap read from the file (5/5) with over-cap text absent; over-cap hard fail; a huge-N overflow; invalid N ×4; unreadable cap ×3 causes; a real-committed-ConfigMap control; the hard-fail counter-message; `--help` contract pins; tree-fact pins (loopback advertise addresses, DNS-only SAN emission).
  - `CANNOT_RUN_CAUSES` gained the cap cause.
- **docs/TODO.md**: the ~1564 entry is extended to a second call site; ~1009 is ticked as resolved; a new task-9 runbook-row entry is added, carrying security's `30..=300` input.
- **Story file**: the false tabs rationale and "per-profile" wording are corrected at ~20, ~54, ~125, ~180, ~219, ~354 and ~600 (Lead-authorized).

---

## Devloop Verification Steps

- `bash scripts/guards/simple/validate-env-config.sh` gives `STATUS=OK REASON=env-config-clean-4-services-6-workloads`, covering both mc workloads.
  - Negative check: removing the ref from both Deployments gives `orphan_configmap_key` FAIL.
  - Stated limitation: until task 9 adds `MissingEnvVar`, a ref dropped from ONE Deployment is not caught by the guard (rule 3 is "at least one workload"). It is caught at pod start only if the ConfigMap key is also missing. Task 9's rule-1 coverage closes the per-workload gap.
- `bash scripts/dev-web.test.sh` gives 152 passed, 0 failed (79 before this task).
- Mutation checks, each applied, measured and reverted; every one red:
  - unanchored cap grep (reads the comment decoy): 14 failures;
  - `fail` changed to `warn` on over-cap: 4;
  - demo default changed to 30: 3;
  - counter-message removed: 1;
  - `DEMO_RECEIVE_SLOTS` requoted (the post-review F1 case, GREEN before the fix): 2;
  - authority note dropped from the pass branch: 3; from the over-cap branch: 1;
  - key-absent, file-absent and header-loopback wordings each collapsed: 1 each.
- `validate-no-insecure-browser-flags` is OK; `validate-cross-boundary-scope` and `-classification` are OK.
- `./scripts/layer-fast.sh`: rc=0. L1 OK, L2 OK, L3 OK, L4 N/A, L5 OK, L6 N/A.
- shellcheck is not installed in this container, so `bash -n` was the only local syntax check.

---

## Code Review Results

All findings FIXED; none deferred. Six reviewers, nine distinct findings (five reviewers independently found the same vacuous assertion).

| ID | Reviewer(s) | Finding | Resolution |
|----|-------------|---------|------------|
| F-DRY-1 / OPS-1 / S-2 / test-1 / obs-F1 / CR-1 | dry, operations, security, test, observability, code-reviewer | `slots-default-n-constant-found` could not fail: `assert_status "default=" "default=${default_n:+…}"` is `contains("")` wearing a prefix, and its `if [[ -n ]]` sibling then SKIPPED the only assertion binding the echoed N to the script's own constant | FIXED — explicit `^[1-9][0-9]*$` test with its own FAIL + reason token saying EXTRACTION, not value; the echo assertion now runs unconditionally. Mutation (`DEMO_RECEIVE_SLOTS="3"`) now reds 2 assertions; it was green before |
| obs-F2 | observability | The authority framing sat only on the over-cap branch. The demo-breaking case (on-disk cap 8, live cap 2) lands on the GREEN branch holding a number the cluster does not enforce | FIXED — `cap_authority_note()`, one home, called from BOTH branches; both call sites pinned so it cannot be inlined back into one |
| obs-F3 | observability | The CANNOT VERIFY branch printed no live-value command, though it is the branch where the live value DISCRIMINATES the cause | FIXED — every cap branch prints it, with the discrimination stated |
| obs-F4 | observability | The CANNOT VERIFY message named a path that may not exist, collapsing file-absent into key-absent; (10f) PINNED that collapse by asserting one shared needle | FIXED — three causes, three branches, three messages (file absent / key absent / value malformed); each test asserts its own wording AND the absence of the other two |
| F-DRY-2 | dry-reviewer | The header restated four ConfigMap ports (`4433..4436`); `help-second-machine-loopback` checked the header against itself and (10j) asserted only a prefix, so a moved port left the claim false with both green | FIXED — duplication REMOVED rather than guarded: the header claims loopback literals (127.0.0.1) and cites the files, no port set. Follows the `MC_AUDIO_FRAME_RATE_HZ` precedent in the same ConfigMap |
| F-DRY-3 | dry-reviewer | `N=3` had two homes: `DEMO_RECEIVE_SLOTS` and task 19's manual-plan walkthrough, a document authored AFTER this loop | FIXED — task 19 now takes N from the launcher's echoed value and records it with the run, instead of restating a literal |
| S-1 | security | The profile-hygiene line said each profile "keeps a live signed-in dev session" — asserting credential persistence eight lines after proving R-23 forbids it | FIXED — advice kept, reason corrected to browser-level site state, and it now says explicitly that the auth session is NOT among them, reinforcing R-23 |
| OPS-2 | operations | No bespoke divergence guard wanted, but the residual should be recorded: nothing asserts mc-0/mc-1 pod-template parity, so any non-required key can diverge undetected | FIXED (as recorded) — new `docs/TODO.md` entry under Infrastructure Validation with operations' three-part reasoning and the general fix shape (per-service instance parity, not a per-key guard) |
| obs-F5 / F-DRY-4 | observability, dry-reviewer | The malformed-cap branch restated `1..=64`, a `crates/mc-service/src/config.rs` figure — the rule this same diff states for W ("the bound's figures live in config.rs, not here") holding everywhere except one line, and a sixth prose home for that number | FIXED — the figure is gone; the branch now says MC refuses to start outside its own configured bound, cites config.rs as the home, and names the rule. The five pre-existing homes are out of scope and deliberately untouched (cross-owner: meeting-controller + operations) |
| F-DRY-3a | dry-reviewer | Task 19's walkthrough still said "E hears the earliest three" one clause after its two siblings became N-derived — self-inconsistent in the one step whose subject IS slot exhaustion | FIXED — "the earliest N" |
| OPS-3 | operations | Non-blocking: two browser-flag warnings back to back on a fingerprints failure, with the cert-validation half near-duplicated | FIXED — the exit line no longer restates the cert remedy the fingerprints branch carries; it keeps the origin class and points at each item's own remedy |

Knock-on kept rather than weakened: splitting the cap check into three branches broke the suite's `header-cannot-run-count-matches-body` 1:1 cause/branch invariant (7 causes, 6 branches). Resolved by splitting key-absent from value-malformed into their own branches — they have different first moves — so the invariant holds at 7:7 rather than being relaxed to a declared count.

---

## Accepted Deferrals

None. Every review finding was fixed in-changeset.

---

## Rollback Procedure

1. Start commit: `78bf4959af5af4fdb20c29042a13a970deb77c28`
2. `git diff 78bf4959..HEAD`; `git reset --soft|--hard 78bf4959`
3. ConfigMap applied to a cluster: re-apply prior configmap and restart MC pods (resources:, not configMapGenerator: — no auto-roll)
4. Asymmetry: reverting the ConfigMap ALONE does not roll pods. Reverting the two Deployments DOES (pod template changes), so a full revert of this task self-applies on the next `kubectl apply -k` — the safe direction. Before task 9's image, reverting is inert (nothing reads the key).
5. **After task 9's image is deployed, reverting this task is a CrashLoop, not a rollback**: the image requires `MC_KEK_ROTATION_DEBOUNCE_SECONDS` (MissingEnvVar). Never remove the key in the same deploy that removes its read; roll the image back first.

---

## Issues Encountered & Resolutions

1. **Task brief's "three verified facts" became two blockers plus a caveat, by verification.** The brief named DNS-only SANs as a reason a second machine cannot work. Verified, `scripts/generate-dev-certs.sh:generate_service_cert()` emits only `DNS:` SAN entries, and the MC/MH leaves (~253-275) carry no IP SAN. But the browser trusts these leaves via `serverCertificateHashes` pinning, which matches by hash and does not name-check. `infra/kind/scripts/setup.sh`'s devloop advertise-address patch already relies on this ("TLS SAN coverage is not required"). So DNS-only SANs block NAME-VERIFYING clients, not the browser. The header states the two real browser-side blockers (non-secure LAN origin; loopback-literal advertise addresses) as facts, and the SAN point as a precisely-scoped caveat. Overstating it would be fail-open documentation in the file read just before someone reaches for a flag (security, Gate 1).

---

## Lead Notes

- SendMessage name `team-lead` is not reachable in this session runtime; teammates redirected to address the Lead as `main` (2026-09-24).
2. **Task brief's "profiles rather than tabs, because one profile's tabs share the auth session" is false for this app** (dry-reviewer D-7, verified). The auth session is in-memory per document (`packages/web-app/src/App.svelte:22`, never persisted per R-23; no web storage or cookies anywhere in the client or set by AC/GC). So each tab signs in independently. dev-web.sh documents what is verified: N+1 contexts, each a distinct account; profiles recommended as known-good; auth-session sharing is not the reason (cited); tabs-in-one-profile under N+1 simultaneous capture is marked UNVERIFIED. The story sites carrying the false reason are ~354 (task 4) and ~600 (manual test plan). **Lead action**: story R-25's wording and the manual test plan's bring-up prose (story ~line 600) carry the false premise and should be amended. The story file is not this task's to edit.

### Gate 2 (Lead) — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`
rc=0, TOTAL_RESULT=N/A (no FAIL). L1 OK, L2 OK, L3 OK (98s), L4 N/A (proto intentional-gap placeholder; rust+ts OK), L5 OK, L6 N/A (audit dep-manifest gate / proto placeholder), L7 OK (1033s, env-tests + browser E2E). Attempt 1/3.

### Gate 2 re-run (Lead, after review fixes)
rc=0, TOTAL_RESULT=N/A (no FAIL): L1 OK, L2 OK, L3 OK, L4 N/A (proto placeholder), L5 OK, L6 N/A, L7 OK (1004s).

### Gate 3 — Final Verdicts (Lead)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | S-1 profile-hygiene reason contradicted R-23; S-2 vacuous 10a assertion |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | vacuous 10a assertion |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | authority note on pass branch; kubectl on CANNOT VERIFY; 3-way cap diagnosis; `1..=64` restatement removed |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | vacuous 10a; re-confirmed after `1..=64` removal |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | plus Gate-1 D-7 (false tabs-share-auth premise) |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | `docs/TODO.md` mc-0/mc-1 pod-template parity entry is a pre-existing repo-wide gap surfaced while answering a Lead question, not a deferral of this diff — Lead accepts RESOLVED-FIXED |
| Semantic Guard | — | — | — | — | not spawned (no checks.md surface) |

Lead decisions: story file correction authorized in-loop (Gate 1 condition from operations + dry-reviewer); DRY's pre-existing `1..=64` homes not filed (not caused by this diff, cross-owner).
