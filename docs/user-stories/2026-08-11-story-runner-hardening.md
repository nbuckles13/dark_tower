# User Story: Story-Runner Hardening (ADR-0035 follow-on)

**Date**: 2026-08-11
**Status**: Planning
**Participants**: auth-controller, global-controller, meeting-controller, media-handler, client, database, protocol, infrastructure, security, test, observability, operations

## Story

As a **developer running the Dark Tower story workflow**, I want to **hand a planned story to `run-story` and have it execute unattended**, so that I **come back to it done — or to a small queue of genuine decisions — rather than to a run that stopped for a reason nobody recorded.**

Run #1 proved the runner works: 10 tasks, $415.97, driven to ALL-TASKS-COMPLETE including an unattended advisory remediation. It has **zero tests** and a handful of real defects. This story fixes those and makes the script testable — nothing more.

## Scope

Planning produced 23 requirements across 8 tasks. That was disproportionate to the request, and it was cut on 2026-08-12 to the set below: the defects that misfire in practice, the containment needed to test a script that runs `git reset --hard`, and the cluster-state bug that makes repeated runs fail. Everything else is recorded under **Deferred** with its evidence, so re-opening it is a decision rather than a rediscovery.

## Requirements

- **R-1 — A runner test run cannot touch the operator's repository**, invoke a real `claude`, sleep in real time, or run the real validation pipeline.

  Containment must be *verified rather than assumed*, because setting the redirect is not the same as being redirected. Git resolves upward from the working directory, so a root pointing inside the real clone — `/work/fixtures/story-a`, the obvious place to put a fixture — provides none: measured, `git reset --hard` run from such a directory reverts the real tree, and `git clean -fdq` deletes the fixture's own contents. The runner reaches exactly that pair at `:433-434`. Containment holds only when the redirected root **is its own git top-level and differs from the real one**; the second half matters because a root set to the real repository is a no-op redirect that would otherwise pass. *(test, infrastructure, security)*

- **R-2 — Each known runner defect gets a test that fails against today's code and passes after the fix.** Five, all reachable:
  1. Canary classification misfires on a typo in prose matching.
  2. It also misfires on a single byte of stderr — the probe's stderr is merged into the file its stdout is parsed from, so one warning line makes the JSON unreadable (see R-3).
  3. Every operator-class gate failure is recorded against the implementer (see R-4).
  4. Any transient git error is misclassified.
  5. The task prompt is interpolated into `/devloop "%s" --specialist=%s`, so a prompt containing a double quote truncates the devloop's instruction and silently changes what the task is told to do. Latent while manifests are hand-authored; live once `/user-story` emits them. *(infrastructure, test, security)*

- **R-3 — Failure classification must survive the output it actually parses.** Before blaming a task, the runner runs a cheap probe (`--model haiku`) whose result separates "quota, auth or network is broken" from "the task genuinely failed" — the only thing standing between an outage and a recorded implementer bug. It must be pinned against every `api_error_status` value production emits (present-with-429, present-with-null, absent — production emits the middle one, and `jq`'s `//` collapses null and missing today) and against output that is not valid JSON at all. *(test, security)*

- **R-4 — An environment failure is never recorded as an implementer failure.** The pipeline already draws this line and names it: `layer7.test.sh:8-16` labels exit 2 `PRECONDITION_FAILURE` "operator" and exit 1 `FAIL` "implementer". `run-story.sh:489-492` collapses both into one `escalate` call, so a broken cluster is recorded against the task and the next session hunts a bug in a diff that was fine. rc 1 stays a task escalation; rc 2 routes to the infra lane with the manifest untouched — a lane change, not a second flavour of escalation. *(test, operations, global-controller)*

- **R-5 — `--stop-after <id>` halts when told to, and refuses loudly when it cannot.** It exits cleanly after a named task so a run can be staged around a human step. Today the id is validated only as a number, so one that never runs — a typo, or a task the story never reaches — is silently ignored and the runner continues to the end. A flag that silently does nothing is the same failure class as a gate that passes without running. *(infrastructure, security, operations)*

- **R-6 — When meeting creation is refused, the cause is distinguishable from the response alone.** GC's query returns an empty result for three unrelated reasons — the org's concurrent-meeting cap is full, the org does not exist, the org is inactive — and the handler turns all three into one `403 "Organization meeting limit exceeded"`. Harmless while one org is seeded once; live with per-run organizations (R-7), where an organization whose state changes underneath a live token becomes indistinguishable from a full cap.

  *Premise corrected 2026-08-14 during implementation.* This originally read "a provisioning bug becomes indistinguishable from a full cap and the runner would retry a real failure forever as resource exhaustion." It never could: AC fails closed on a missing or inactive organization, so no token is minted and the request never reaches GC (see Task 3 notes). The reachable confusion is narrower — an org row changing state inside the ~1h token TTL — and the fix is unchanged. Corrected rather than deleted, because the original sentence would otherwise be citable as evidence that a provisioning-detection gap was closed here. *(global-controller)*

- **R-8 — A story is runnable without hand-written YAML, task status has one home, and the manifest declares nothing nobody reads.** `/user-story` does not emit a manifest today (zero mentions of it in the skill), so every story must be hand-authored before `run-story` can drive it — this one was. `/close-story` likewise reads the markdown checkbox table rather than the manifest, so the same fact lives in two places and drifts. Planning must emit the manifest it validates, and closing must read the manifest rather than the table.

  The manifest must also stop requiring `branch`, which is a required field (`pub branch: String`, not `Option`) that **nothing reads** — zero references in `dt-story`'s engine or either workflow script. Nothing creates that branch, nothing checks the run is on it, and a story driven from another branch carries a manifest that quietly says something false. A required field that looks like a contract and isn't one will eventually mislead someone, and it hardcodes topology into a planning document.

  **This depends on R-2's fifth defect being fixed first**: hand-written prompts are safe because a human avoids stray quotes, and machine-generated ones are not. *(operations, protocol)*

- **R-7 — Layer 7's Nth consecutive run against the same dev cluster reaches the same verdict as its first**, for both suites. It does not today: no production code ever marks a meeting ended, so each organization's count of live meetings only ever climbs toward its cap. The browser suite creates ~7 meetings per run against a cap of 10, so a second run fails partway with a 403 the pipeline attributes to the code under test. This bites *within* a single devloop, which iterates until the gates are green, and it makes a story uncloseable, since rerunning is the documented recovery for several lanes. *(database, test, client)*

## Implementation Plan

| # | Task | Specialist | Depends on | Covers | Status | Devloop Output |
|---|------|-----------|------------|--------|--------|----------------|
| 1 | Seams, hermetic test suite, and the five defect fixes in `run-story.sh` | `test` (paired with `infrastructure`) | — | R-1..R-5 | Completed | `docs/devloop-outputs/2026-08-13-run-story-seams-and-hermetic-tests/` |
| 2 | Disambiguate GC's three meeting-refusal causes | `global-controller` | — | R-6 | Completed | `docs/devloop-outputs/2026-08-14-gc-meeting-refusal-causes/` |
| 3 | Provision a fresh organization per layer-7 run | `test` (paired with `database`) | 2 | R-7 | Completed | `docs/devloop-outputs/2026-08-15-layer7-per-run-org/` |
| 4 | `/user-story` emits the manifest; `/close-story` reads it | `operations` | 1 | R-8 | Pending | — |

**Task 1 notes.** Two `DEVLOOP_TEST`-gated seams — `STORY_REPO_ROOT` and `DT_STORY` — using the existing exact-match sentinel idiom at `audit-suppressions-check.sh:36-60`, including its fail-loud-when-set-without-sentinel half. `DEVLOOP_TMP` already redirects the run dir; no third seam. Fixture is `mktemp -d` + `git init`, with stub layers and the real `dt-story`; `claude`, `sleep` and `date` are PATH-injected under `env -i`; `git` stays real. Wire the test file into `layer3.sh` at creation. Every stub records its invocation and every case asserts the stubs it depends on ran — promote `assert_marker`/`assert_no_marker` out of `layer-all.test.sh:89-95` rather than copying them. Also carries the ADR §6 one-liner: the canary must run with `--allowedTools ""`, keeping `--dangerously-skip-permissions` (dropping it reintroduces a permission-prompt hang that gets misclassified as `infra`).

**Task 3 notes.** Provision in `layer7.sh` Phase 1, which drives the cluster solely through the `dev-cluster` helper's closed verb allowlist (ADR-0030) — no `psql`, no `kubectl`. Set `max_concurrent_meetings` explicitly (1000); do **not** touch `max_participants_per_meeting`, which an env-test asserts equals 100 and which `LEAST()` would silently cap. Subdomains must be lowercase. Provisioning failure must report on the operator lane (`PRECONDITION_FAILURE`, exit 2), never as a suite failure — which is why task 2 precedes it.

  *Premise corrected 2026-08-15 during implementation.* The clause above asserting that Phase 1 has **no `psql`, no `kubectl`**, and the R-7 mechanism built on it (that provisioning must therefore go through a new `dev-cluster` helper verb), rest on a premise verified **FALSE** in the devloop container on 2026-08-15. Four facts, each checked directly:

  1. `command -v psql` → `/usr/bin/psql`; `infra/devloop/Dockerfile:33` installs `postgresql-client`.
  2. `command -v kubectl` → `/usr/local/bin/kubectl`; `infra/devloop/Dockerfile:69-78` installs it, sha256-verified.
  3. `infra/devloop/devloop.sh:519-520` exports `KUBECONFIG=/tmp/devloop/kubeconfig` in the same conditional block that bind-mounts the helper runtime dir, so helper-socket-present ⟺ kubeconfig-present — and ADR-0030 §"Container-Side Test Execution" grants the container kubectl plus a cluster-admin kubeconfig as an explicit, risk-accepted decision.
  4. `infra/devloop/devloop.sh:235-239` `build_helper()` compiles from `$REPO_ROOT/Cargo.toml` — the **host** checkout, never `CLONE_DIR`. A verb added on this branch therefore cannot exist in the helper serving this devloop, and nothing container-side can rebuild or restart it. Since task 3 runs with `env_tests: true`, the helper-verb route was a *certain* Layer-7 operator-lane red rather than a risk to mitigate, which in headless mode terminates the story.

  **Ruling: no helper verb.** `layer7.sh` Phase 1h invokes `infra/kind/scripts/setup.sh --provision-org <sub>` directly, container-side. That satisfies every substantive requirement stated above (Phase 1; `max_concurrent_meetings` explicitly 1000; `max_participants_per_meeting` untouched; lowercase by construction; `PRECONDITION_FAILURE` exit 2 on any failure) and adds no verb at all, so it cannot undo ADR-0030's injection-impossibility property — which the ADR scopes to the socket surface. `crates/devloop-helper/**` and `infra/devloop/**` are untouched. The corollary now recorded in ADR-0030 is the general form: **no devloop can validate a change to `crates/devloop-helper/` within its own run.**

  Corrected in place rather than deleted, for the same reason this file states for its own 2026-08-14 correction: otherwise the original sentence remains citable as evidence that a helper verb was required, and the next reader re-derives the same dead end. *(test, infrastructure)*

**Where a provisioning failure actually surfaces (corrected 2026-08-14, task 2).** It surfaces at **AC token acquisition**, not at GC meeting creation. AC's `org_extraction` middleware resolves the Host subdomain through `organizations::get_by_subdomain`, which filters `WHERE subdomain = $1 AND is_active = true`, and **fails closed** with `AcError::NotFound`. A missing or inactive organization therefore never yields a token, so the request never reaches `POST /api/v1/meetings` at all. Phase 1 must consequently verify provisioning **directly** — org exists, `is_active`, token obtainable — and treat that check failing as the `PRECONDITION_FAILURE`. A detector keyed on a GC meeting-creation status would be permanently dead code. What task 2 does deliver is the removal of a false positive: `403 ORGANIZATION_MEETING_LIMIT_EXCEEDED` now means a genuine cap **and nothing else**, so a cap signal can be trusted rather than treated as possibly-a-provisioning-fault.

## Architecture Validation

**PASS — 12 of 12 specialists.** No new services, transports, service-to-service paths, or infrastructure components. Bounded to existing scripts, the `dt-story` crate, existing tables, and existing instrumentation.

Three specialists opted out with checkable reasons:

- **auth-controller** — the run-1 `AUTH-EXPIRED` classification came from the Claude CLI's own OAuth credential, not from AC. AC accumulates no state across runs: signing-key init is DB-state-gated so restarts mint nothing, rate limits are 60-second sliding windows, and user uniqueness is scoped `(org_id, email)`, which per-run orgs make airtight by construction.
- **media-handler** and **meeting-controller** — cluster reuse is safe for both because `layer7.sh` calls `rebuild-all` unconditionally every run, so no MC or MH process survives a task boundary.

Two corrections to the lead's briefing, both load-bearing:

- `layer7.sh` Phase 1 has **neither `psql` nor `kubectl`** — see task 3 notes.
- The near-term failure is the **browser** suite (~7 meetings/run against a cap of 10), not the Rust one (~22 against 1000).

## Deferred

Real, verified, and out of scope for this story. Each is recorded with the evidence that established it so re-opening is a decision, not a rediscovery.

**Runner testability beyond the known defects**
- *Full branch enumeration* — pinning every runner branch that can end or stall a story, rather than only the branches the five defects touch.
- *Test-seam CI unreachability* — the runner's seams need no `GITHUB_ACTIONS` rejection because CI invokes neither workflow script (verified: zero references across all four workflows) and a redirected repo root makes the runner do *no work* rather than forge a green verdict. That rests on a current fact, not an invariant; a standing layer-3 guard would keep it from going stale silently.
- *Unrun and vacuous self-tests* — 10 of 16 `scripts/**/*.test.sh` files have no invocation site, so they pass by never executing. Separately, `scripts/layer7.test.sh` never asserts that the stubs it installs were consulted across its 469 lines, so a case cannot distinguish "the path I meant to test ran" from "another path produced the same status" — a structural gap, not a demonstrated failure.
- *A sixth defect in the same pipeline — SIGPIPE unreachable, `find`-exit reachable.* The SIGPIPE path cannot fire today: input is scoped by `-newer "$start_marker"` and the marker is re-stamped before every read, so the window never spans more than one task's execution, and serial execution bounds what it can contain. Both bounds are emergent from resume-detection code that was not written to provide them. **A different mechanism in the same pipeline was reachable, and is fixed in task 1**: `find` exits non-zero when `docs/devloop-outputs` is absent, `pipefail` propagates it, and the enclosing `d="$(…)"` assignment kills the runner under `set -e` with exit 1 and no lane — while the runner's exit-code header documents 1 as "task escalated", i.e. an environment fault recorded as an implementer failure (R-4's own defect class). It is reachable because the session-limit lane's own `git clean -fdq` removes that directory, since git does not track empty ones. Extracted to `newest_devloop_output`, which answers absence explicitly and keeps SIGPIPE tolerance without masking a genuine enumeration failure. *The original entry reasoned only about SIGPIPE and read as clearing the whole pipeline — which is why it is corrected here rather than annotated: a Deferred entry exists so re-opening is a decision rather than a rediscovery, and this one would have stopped the next reader looking.* Found by task 1's test suite, not by review.

**The run record**
- *One durable outcome record per task, and a story-level terminal record.* A per-task-only scheme structurally cannot represent a run that exits through no lane, since every per-task record is then present and green. Detection can key on neither the exit code nor any per-task signal — measured: a runner truncated after its main loop completes every task, commits each one, and exits **0** with the story-close gate never having run. The task loop (`:270-521`) is one compound command and is immune once entered; what is lost is `:523-539`, which for any story whose tasks are not `env_tests`-tagged is the **only** layer-7 execution in the story. The `trap` at `:105` still fires in that case, so a `run` record carrying `lane: none` would make it positively recordable rather than only detectable by absence.
- *Cost and time attribution* — the ledger records what a task spent, not what the money bought; elapsed time is not decomposable into work, gate, quota-wait and unaccounted. Cost attribution wants `auth_mode ∈ subscription | api-key`, not an account identifier, which would itself be the hazard below.
- *No uncontrolled text in records* — CLI stderr, probe output and model prose must be referenced by path, and those files must live outside the work tree, where `git add` cannot sweep them (a `.gitignore` entry is not a control; `git add -f` bypasses it).
- *The `STORY_RUN` console stream is captured nowhere* (`grep -c tee run-story.sh` → 0).
- *A declared journal schema*, asserted bidirectionally — every declared field present *and* no undeclared field emitted, since presence-only lets an added field pass while the declaration stops describing reality.

**Evidence durability**
- Run evidence lives under `/tmp/devloop`, host-mounted only when `devloop.sh` sets up the cluster helper — the mount exists for the helper's socket, so durability is incidental. `/work` is always durable. Whether evidence will outlive the container should be settled at run start. The home should also be private by construction: the runner's `mkdir -p` calls run at the default umask (verified `0755`), and today's containment is inherited entirely from `devloop.sh` and `_common.sh` creating the parent `0700`.

**Gates**
- *Suppression hard-gate* — `audit-suppressions.toml` is how an advisory gets waived instead of fixed; today the runner logs a NOTE after the fact. Any change to that file should stop the run. Note the detector must scan the task's **commit range**: `:510` uses `git show HEAD`, but the runner only requires that HEAD moved, so a suppression in any earlier commit of a multi-commit task is invisible.
- *Probe cache integrity* — preflight runs an expensive substrate probe (spawns agents, 420s timeout) and caches a pass as a marker keyed on CLI version. The cache is on a host mount, so it survives container destruction: re-entering the container re-probes nothing if the version string matches. A marker dated 2026-08-02 is still on this host and would suppress the probe today. It is also written when a control the probe depends on has failed *open*. **If evidence durability above is ever built, this must land with or before it** — a durable evidence home is also a durable marker home.
- *Verdict monotonicity* — `gate2_signature` is an unkeyed `sort | sha256sum`; despite the name it is a change-detector, and after a commit the worktree it reads is empty, so it collapses to the SHA-256 of no input (`e3b0c442…`). The runner reads it **nowhere** today (ADR-0035 §3 cited a freshness check at `run-story.sh:254` that was never built — since corrected in the ADR), so the runner is trivially monotone. Any read ever built must only add to the failure set, never subtract.

**Manifest contract**
- Schema changes must be safe three ways at once: an existing story file still validates unedited (the deserializer rejects unknown fields today), required-ness is enforced by status-aware checks rather than the deserializer, and the runner never writes a manifest it would itself reject.
- Task status lives in two places — the markdown checkbox and the manifest `status` field — and two copies drift.
- Manifest discovery greps for a version-suffixed marker, so a version bump would select zero files and exit 0. *(Fixed in-tree during planning.)*

**Other**
- *Schema drift on a reused cluster* — a layer-7 run may execute against a database predating the tree's migrations.
- *Lane recovery documentation* — no lane the runner can exit through has a documented recovery.

## Considered and deliberately excluded

Not deferred — decided against.

- **`--keep-going` (ADR-0035 §2).** An escalation stops the whole story, including unrelated tasks. Excluded 2026-08-12 on sequencing, not size: `dt-story` already models `deps` and selects dep-satisfied tasks, so the missing pieces are a skip set plus a distinct outcome for "candidates remain but all are skipped" (today that returns `AllDone`, which would run the story-close gate on an incomplete story). It is premature because continuing is only ever correct for *implementer*-class escalations — `auth-expired`, `infra` and substrate-change are environment failures where every subsequent task fails identically. That classification is R-4's. Cheap and well-founded once R-4 lands. *(ADR amended.)*
- **Cumulative spend ceiling.** Run #1 cost $415.97 unattended and nothing would have stopped it. Excluded 2026-08-12: runs continue until rate-limited. The per-task ledger remains, so the data to reconsider is still being collected. *(ADR amended.)*
- **Mechanical per-task acceptance criteria (ADR-0035 §13).** §13 had each task declare an acceptance test at planning time, committed red and cleared as the behaviour landed. The mechanism does not exist — bash has no expected-fail marker, and Playwright's `test.fail()` does not hold when a test reds by timing out rather than throwing — and committing a red test is actively harmful, since a red env-test reds every *subsequent* task's gates. A weaker variant (name the test at planning, make it green within its own task) was also rejected: a task's own test, written by the agent that wrote the code, does not catch a partially-broken implementation, which was the failure it was meant to address. **Env-tests remain the integration check.** *(ADR amended.)*

## Notes

**The runner has no env-test surface.** With §13 cut, the integration backstop that covers product code does not cover runner code, so task 1's suite is the only independent verification for `run-story.sh`. Task 1 therefore lands before any later runner work, later runner tasks extend its suite rather than inventing a harness, and runner tasks are paired `--paired-with=test`.

**Editing `run-story.sh` while it runs is safe in practice.** The Edit tool replaces the inode (verified three times), so a running shell keeps its descriptor on the orphaned original; `sed -i` also renames. Git does **not** — `reset --hard` and `checkout` rewrite in place — but it is still not a vector, because an unmodified file is never rewritten and a file already inode-swapped leaves the shell on the orphan. The single trigger for corruption is a direct `>` redirect onto the live script. No `--stop-after` segmenting is required for this story.

## Running this story

Either drive it with `run-story` (the manifest below is hand-written, since `/user-story` emitting one is task 4's job):

```
# unattended, from the host — sets up the worktree, containers and cluster,
# then runs the story instead of attaching an interactive session
./infra/devloop/devloop.sh story-runner-hardening -- scripts/workflow/run-story.sh story-runner-hardening

# or stop after the first task to inspect the result before committing to the rest
./infra/devloop/devloop.sh story-runner-hardening -- scripts/workflow/run-story.sh story-runner-hardening --stop-after=1
```

It exits with the runner's own code (0 all tasks complete, 1 implementer-class stop, 2 operator-class) and leaves the containers and cluster up — teardown deletes the clone, and rerunning is the documented recovery for several lanes.

…or run the devloops individually, in order:

```
/devloop "Add DEVLOOP_TEST-gated seams STORY_REPO_ROOT and DT_STORY to run-story.sh using the exact-match sentinel idiom from audit-suppressions-check.sh:36-60 including its fail-loud-when-set-without-sentinel half; validate --stop-after against the manifest rather than as a bare integer; build a hermetic test suite at scripts/workflow/run-story.test.sh wired into layer3.sh, with an mktemp fixture repo, PATH-injected claude/sleep/date under env -i, real git, and a containment assertion that the redirected root is its own git top-level and differs from the real one; every stub records its invocation and every case asserts the stubs it depends on ran, promoting assert_marker and assert_no_marker out of layer-all.test.sh:89-95 rather than copying them; fix the five defects in R-2 with a failing test each; and give the canary --allowedTools with an empty value while keeping --dangerously-skip-permissions. See docs/user-stories/2026-08-11-story-runner-hardening.md R-1 to R-5." --specialist=test --paired-with=infrastructure

/devloop "Make GC distinguish the three causes of a refused meeting creation. create_meeting_with_limit_check returns an empty result for cap-exhausted, organization-missing and organization-inactive alike, and the handler maps all three to one 403 saying the limit was exceeded. Give each cause a distinct observable outcome so a provisioning failure cannot be mistaken for a full cap. See docs/user-stories/2026-08-11-story-runner-hardening.md R-6." --specialist=global-controller

/devloop "Provision a freshly generated organization per layer-7 run so repeated runs against the same dev cluster reach the same verdict. Provision in layer7.sh Phase 1 through the dev-cluster helper verb allowlist only, per ADR-0030 — Phase 1 has neither psql nor kubectl, and adding a SQL verb would undo that ADR's injection-impossibility property. Set max_concurrent_meetings explicitly to 1000; do not touch max_participants_per_meeting, which an env-test asserts equals 100 and which LEAST would silently cap. Subdomains must be lowercase. A provisioning failure must report on the operator lane as PRECONDITION_FAILURE exit 2, never as a suite failure. See docs/user-stories/2026-08-11-story-runner-hardening.md R-7." --specialist=test --paired-with=database
```

Then `/close-story story-runner-hardening`.

## Revisions

**2026-08-12 — scope cut.** 23 requirements / 8 tasks → 7 requirements / 3 tasks. Reason: disproportionate to the request. Cut items moved to Deferred with their evidence intact.

```yaml
# task-metadata (dt-story manifest v1)
story: story-runner-hardening
branch: feature/story-runner-hardening
tasks:
- id: 1
  status: completed
  specialist: test
  env_tests: false
  prompt: Add DEVLOOP_TEST-gated seams STORY_REPO_ROOT and DT_STORY to run-story.sh using the exact-match sentinel idiom from audit-suppressions-check.sh:36-60 including its fail-loud-when-set-without-sentinel half; validate --stop-after against the manifest rather than as a bare integer; build a hermetic test suite at scripts/workflow/run-story.test.sh wired into layer3.sh, with an mktemp fixture repo, PATH-injected claude/sleep/date under env -i, real git, and a containment assertion that the redirected root is its own git top-level and differs from the real one; every stub records its invocation and every case asserts the stubs it depends on ran, promoting assert_marker and assert_no_marker out of layer-all.test.sh:89-95 rather than copying them; fix the five defects in R-2 with a failing test each; and give the canary --allowedTools with an empty value while keeping --dangerously-skip-permissions. See docs/user-stories/2026-08-11-story-runner-hardening.md R-1 to R-5.
- id: 2
  status: completed
  specialist: global-controller
  env_tests: false
  prompt: Make GC distinguish the three causes of a refused meeting creation. create_meeting_with_limit_check returns an empty result for cap-exhausted, organization-missing and organization-inactive alike, and the handler maps all three to one 403 saying the limit was exceeded. Give each cause a distinct observable outcome so a provisioning failure cannot be mistaken for a full cap. See docs/user-stories/2026-08-11-story-runner-hardening.md R-6.
- id: 3
  status: completed
  specialist: test
  env_tests: true
  deps:
  - 2
  prompt: Provision a freshly generated organization per layer-7 run so repeated runs against the same dev cluster reach the same verdict. Provision in layer7.sh Phase 1 through the dev-cluster helper verb allowlist only, per ADR-0030 — Phase 1 has neither psql nor kubectl, and adding a SQL verb would undo that ADR's injection-impossibility property. Set max_concurrent_meetings explicitly to 1000; do not touch max_participants_per_meeting, which an env-test asserts equals 100 and which LEAST would silently cap. Subdomains must be lowercase. A provisioning failure must report on the operator lane as PRECONDITION_FAILURE exit 2, never as a suite failure. See docs/user-stories/2026-08-11-story-runner-hardening.md R-7.
- id: 4
  status: pending
  specialist: operations
  env_tests: false
  deps:
  - 1
  prompt: Make the dt-story manifest the single home for task status across the story workflow. The /user-story skill must emit the manifest block it validates — today it has zero mentions of manifests or dt-story, so every story must be hand-authored before run-story can drive it. The /close-story skill must read task status from the manifest rather than from the markdown checkbox table, so the same fact does not live in two places and drift. Also remove the manifest branch field — it is declared required in crates/dt-story/src/manifest.rs as a bare String, yet nothing reads it anywhere in dt-story or either workflow script, so it hardcodes topology into a planning document and lets a manifest assert a branch the run is not on. Removing it is a manifest schema change, so keep it compatible in the three directions ADR-0035 describes and update every existing story file that carries the field. Prompts written into the manifest must survive the runner interpolating them into a shell command; this task depends on task 1 having fixed that, and must not reintroduce the hazard by emitting unescaped prompt text. Do not change the devloop skill — its Headless Mode section at SKILL.md:608 already covers run-story. Pair with protocol for the crate change. See docs/user-stories/2026-08-11-story-runner-hardening.md R-8.
```

