# Devloop Output: mh.sh MH pool tool + hermetic self-test (Story 4 I-5)

**Date**: 2026-11-07
**Task**: Story 4 task I-5 — infra/kind/scripts/mh.sh per mh-sh-contract.txt + mh.test.sh wired into Layer 3
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present; --paired-with=test (paired-test replaces the test slot)
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c771cc8f06f0bc871d64ff1fd37a0b69f10ede7c` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

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
| Test | `paired-test (--paired-with=test)` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — shell-only diff, no checks.md surface` |

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
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

---

## Cross-Boundary Classification

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
| `infra/kind/scripts/mh.sh` (new) | Mine | — |
| `infra/kind/scripts/mh.test.sh` (new) | Mine | — |
| `scripts/layer3.sh` | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt` (§3 tokens, §4 pod_ip, I-2 interface) | Mine | — |
| `docs/user-stories/2026-10-31-survive-handler-loss.md` (one-line T-1 note) | Not mine, Minor-judgment | test |

---

## Planning

mh.sh (I-5) plan v1, from infrastructure. It answers every reviewer's planning input, and the full text is in docs/devloop-outputs/2026-11-07-mh-sh-pool-tool/main.md §Planning. Files, all Mine: infra/kind/scripts/mh.sh (new), infra/kind/scripts/mh.test.sh (new), scripts/layer3.sh (one parallel_queue_add), the infrastructure INDEX.md. lib/common.sh is NOT edited.

INTERFACE (script header; I-2 must produce it)
- ConfigMap mh-template in dark-tower with data keys deployment.json (the rendered Deployment as JSON, so the dependency stays jq-only), revision, baseline.
- Slot ConfigMaps labelled dark-tower.io/mh-slot=<k>, keys MH_WEBTRANSPORT_ADVERTISE_ADDRESS and MH_HOST_PORT.
  - Exactly one ConfigMap per slot value. A duplicate gives no-template DETAIL=duplicate-slot-configmap. That matters if I-2 keeps a previous hash-named generation labelled.
  - K = the discovered set. There is no count literal.
- Template preconditions, else no-template: a non-empty spec.selector.matchLabels that the pod-template labels contain; a container port named webtransport; an env MH_WEBTRANSPORT_ADVERTISE_ADDRESS with a configMapKeyRef. mh.sh does not hardcode app=mh-service; it derives from the template. A template missing its selector or app labels therefore fails as no-template (B16, obs 1).

INSTANTIATION (jq only, never sed or eval)
- Name mh-s<k>, replicas 1.
- dark-tower.io/mh-{slot,revision,owner} are MERGED into Deployment metadata.labels, spec.selector.matchLabels and the pod-template labels. Every template label survives.
- The webtransport hostPort = MH_HOST_PORT, and the configMapKeyRef name = the slot ConfigMap's name.
- Annotation dark-tower.io/mh-replaces=<k>, added only by upgrade.
- No other labels or annotations. The address appears in no label or annotation (obs 3/5).

STATES (per slot)
- free: no Deployment and no pod.
- starting: live Deployment, pod absent or not Ready.
- serving: live Deployment and pod Ready, nothing deleting.
- draining: deletion requested (Deployment or pod deletionTimestamp) and the pod exists within grace. Past grace = now > pod.deletionTimestamp, which the API sets to request time + grace.
- terminating: the pod is past grace (terminating_past_grace=true), or the pod is gone and the Deployment is still finalizing (false).
- The slot's pod is the oldest by creationTimestamp, i.e. the hostPort holder. restart_count comes from the webtransport container.

TOKENS
- ONE array, MH_TOKENS=( "ok:0" ... ), one "token:exit" per line. Exit codes are derived from it only.
- mh.sh is sourceable: main runs only when BASH_SOURCE==$0. mh.test.sh sources it in a subshell and prints the array, so the token list is never retyped. The test also parses mh-sh-contract.txt §3 and asserts contract ⊆ array with matching codes, with a distinct failure label for an extraction failure. T-1's Rust can extract the same stable format (dry 3, test A4/B4).
- On STATUS=OK, REASON is always `ok` (exit 0), and DETAIL says what happened (created / existing / drained / empty / joined / noop).
- Contract tokens: mh-rollout-failed, drain-timeout and not-available-timeout (1); usage, no-kubeconfig and no-template (2); mh-no-free-slot (3); last-serving-mh, stuck-terminating and foreign-owner (4); mh-pool-busy (5).
- PROPOSED ADDITIVE TOKENS, which need a @team-lead ruling (exit groups unchanged):
  - kubectl-failed (1): an unexpected API or kubectl error mid-operation. It never masquerades as rollout or timeout (ops 6).
  - internal-error (1): the EXIT-trap fallback for SIGINT/SIGTERM or any abort before the emitter ran. The self-test reaches it by signalling mh.sh mid-wait.
  - prerequisite-missing (2): kubectl or jq is not on PATH. Same word as classify_env_failure.
- The apiserver being unreachable or failing at startup (the first get) gives no-kubeconfig.
- A bad DT_CLUSTER_NAME, DT_PORT_MAP or gateway IP: dt_init_cluster_env is probed in a subshell first (its text relayed to stderr), then called for real. A probe failure gives no-kubeconfig, exit 2. The context must also exist in the kubeconfig (else no-kubeconfig).
- One emitter function plus an EXIT trap gives exactly one final MH_RESULT line on every path (obs 4, ops 6, test A5/A6).
- DETAIL is always the LAST field, single-line, and built only from validated values. It may contain spaces, e.g. `DETAIL=timeout=45s; pod=mh-s2-xxx; run: mh.sh drain 2 --owner manual`. It never carries raw kubectl stderr or the kubeconfig path; those go on earlier human lines.

VERBS + JSON (stdout only with --json; every object carries "ok")
- list: {ok, schema_version:1, template_revision, baseline, slots:[{slot, host_port, address, pod_uid, state, terminating_past_grace, deployment, pod, revision, stale, owner, restart_count}]}.
  - Every slot appears; a free slot has nulls, stale=false and past_grace=false.
  - No lock. Exit 0 on an empty or full pool, and with the Lease busy.
  - pod_ip only if the Lead OKs it; it then joins the golden.
- up [--slot k] [--owner ID] [--timeout S]: output is the slot's list entry + created.
  - Next-fit: after the highest non-free slot, wrapping. An empty pool picks the lowest slot.
  - Creation is atomic; AlreadyExists ("(AlreadyExists)" in stderr) moves to the next slot. Every candidate failing, or none free, gives mh-no-free-slot exit 3.
  - --slot k: a live same-owner current-revision MH gives ok, created:false (it waits for Available if still starting). Occupied by anything else (another owner, stale, draining) gives mh-no-free-slot DETAIL=slot-occupied (test A8).
  - The wait for Available is OUTSIDE the Lease. The default timeout = the max over the webtransport container's startup/readiness/liveness probes of (initialDelay + period × failureThreshold + timeoutSeconds), + MH_UP_MARGIN_S (one named constant, 30). DETAIL carries timeout=<n>s.
  - Never Available gives not-available-timeout: the pod is left in place, and DETAIL has the drain command.
- drain <k> | --all, with [--owner ID] [--no-wait] [--timeout S] [--force]:
  - Decision under the Lease, on a fresh read taken after acquire (sec 7, ops 4), in this order:
    1. Empty slot: ok, drained:false, no delete.
    2. Any slot pod past grace: stuck-terminating exit 4. NOT forceable (sec 8, ops 2).
    3. Already deleting: join the wait, with no second delete.
    4. Owner != --owner (default manual): foreign-owner unless --force.
    5. Target serving and serving_count==1: last-serving-mh unless --force. serving counts only state=serving, across all owners; serving + starting still refuses (ops 4, A9).
  - The delete is `delete deployment mh-s<k> --cascade=foreground --wait=false`. That is the only delete mh.sh issues. --force never reaches kubectl.
  - The wait for gone (pod and Deployment) is bounded by the pod's terminationGracePeriodSeconds + MH_DRAIN_MARGIN_S (named, 30), or --timeout. Timeout gives drain-timeout, with DETAIL listing bound, pod, `kubectl describe pod` and `mh.sh list` (terminating_past_grace). No escalation, and no --force hint (ops 5).
  - --no-wait returns once the pod carries a deletionTimestamp (or there is no pod).
  - Output: the pre-drain entry + drained.
  - --all needs --owner (else usage) and selects only owner=<validated ID>. It deletes every one it may. If one would be the last serving MH, it refuses that one only, waits for the rest, and ends with STATUS=FAIL REASON=last-serving-mh SLOT=<k> exit 4, JSON {ok:false, reason, slot, detail, drained:[...]}. On success: {ok:true, drained:[k...]}.
  - I am NOT adding "--all with baseline needs --force" (sec 11). last-serving-mh already protects the floor, and full teardown is teardown.sh, not this.
- upgrade:
  - Prechecks before the first create (ops 3): Lease, stuck-terminating, a free slot (mh-no-free-slot). Each refusal arm asserts zero create and zero delete.
  - For each live stale MH (lowest slot first), serially:
    1. up into a next-fit slot, inheriting the old owner, annotated replaces=<old>.
    2. Wait Available. Not Available gives mh-rollout-failed DETAIL=old=k new=j (+ drain command for j); the old MH is untouched and still serving.
    3. Drain old (the owner check is skipped, because upgrade is owner-preserving).
    4. Wait gone, else drain-timeout.
  - Resume: a Ready current-revision replacement with replaces=k, where k is still stale and live, only triggers drain k. A NOT-Ready one is waited on (same derived bound), then gives mh-rollout-failed. It never stacks a second replacement (ops 8).
  - Output {ok:true, replaced:[{old:k,new:j}]}. Nothing stale gives ok DETAIL=noop.

LEASE mh-pool (coordination.k8s.io/v1, dark-tower)
- holderIdentity is `<owner>@<host>:<pid>`, unique per invocation. The host is sanitized to [A-Za-z0-9.-].
- leaseDurationSeconds is MH_POOL_LEASE_S=30, stated in the header. The critical section is a few kubectl calls.
- Acquire = `create -f -`. On AlreadyExists: get. If the holder is empty or expired (renewTime + duration < host `date -u` now; the header notes host-vs-apiserver clock skew, and the 30 s covers ordinary skew), reclaim via `replace -f -` carrying the observed resourceVersion. A Conflict means busy.
- Busy: exit 5 at once, DETAIL=holder=<holderIdentity>. A holder read back that doesn't match ^owner-regex@[A-Za-z0-9.-]+:[0-9]+$ prints holder=malformed (sec 3).
- Release (EXIT trap, every path) = get, and only if the holderIdentity is ours, `replace` with the holder cleared, carrying the resourceVersion. A Conflict means it is no longer ours: warn, never delete.
- The Lease is released BEFORE any Ready/gone wait, and the test asserts that ordering from the argv log.

SECURITY / OBS
- Owner regex ^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$. --slot is a decimal integer in the discovered set. --timeout is an integer in 1..3600. All of these are checked before any mutation (a slot-set check needs the ConfigMap read); a bad value gives usage.
- Cluster-read slot labels, replaces= and holder are validated, failing loudly with no-template or kubectl-failed, never skipped.
- Every call goes through one kc() wrapper with `--context ${KUBE_CONTEXT} -n dark-tower`. common.sh's log_info/log_step (stdout) are not used.
- mktemp + trap cleanup. No dumps of ConfigMaps or kubeconfig. No curl, prometheus or /metrics.

SELF-TEST (mh.test.sh, _test_helpers.sh only, no local asserts)
- A stateful stub kubectl first on PATH (asserted with command -v) over a per-case JSON state dir: configmaps, deployments, pods, lease.
  - It supports get / create / replace / delete deployment / config get-contexts. Anything else is loud: rc 97 + a violation file.
  - It rejects any call without `--context kind-$DT_CLUSTER_NAME` (rc 98 + marker), with a positive arm for DT_CLUSTER_NAME=foo.
  - It refuses and records `--grace-period=0`, `--force`, and any `delete pod`.
  - It logs argv append-only and captures each created object.
  - Knobs flip pods Ready or gone after N reads, or stuck. AlreadyExists uses the verbatim "Error from server (AlreadyExists)" text, exit 1.
- The poll interval is MH_POLL_INTERVAL_S (production config, default 2, validated). The tests use 0.01, and the targets are timeouts of ~1 s.
- One run_mh helper used by every case asserts: exactly one MH_RESULT line on stderr, and it is last; with --json, one jq-parseable stdout object whose ok matches STATUS; without --json, empty stdout. It records REASON and rc to a collector.
- Cases: B1-B16 as paired-test listed (next-fit {0,1}->2, {1,3}->4, {5}->0, {0,5}->1, empty->lowest, K=4 unsorted, the AlreadyExists race and all-AlreadyExists; idempotent up + negative arms; drain empty / join / --no-wait paired with drain-timeout on the same state; derived bounds checked against the fixture's own numbers; every refusal with its positive arm and no delete in argv; stuck-terminating + --force still 4; Lease busy / expired-reclaim / release on failure / ordering; list with lease busy; upgrade forward / resume / not-ready-resume / no-slot / noop / two-stale serial; list exact golden + every state reached; the usage / no-kubeconfig / no-template / prerequisite-missing / kubectl-failed / internal-error cases).
- Also the obs 1 label-merge positive and negative arms; the aggregate force ban with a positive `delete deployment` control; a static grep secondary check (no curl/prometheus/:9090//metrics, positive control MH_RESULT); and the token set in both directions.

OPEN for @team-lead: (1) the three additive tokens; (2) pod_ip in list (paired-test's ask); (3) the slot ConfigMap key name MH_HOST_PORT, which I-2 inherits.

PLAN v1 ADDENDA (from reviewer input)
- The owner regex lives in ONE named variable, MH_OWNER_RE, which is readable when mh.sh is sourced (dry b). T-1 derives from it.
- kc() takes --context "${KUBE_CONTEXT}" as a separate argv word. It does not word-split ${KUBECTL} (dry a).
- spec.selector.matchLabels gains only dark-tower.io/mh-slot. The metadata and pod-template labels get all three (obs).
- AlreadyExists is matched on kubectl's wording. This fails closed: if the wording changes, the error surfaces loudly as kubectl-failed. A comment at the matching site says so (obs).
- drain --all: inside the one critical section, serving_remaining is decremented as each delete is issued. If an owner holds every serving MH, exactly one is refused, so there are N-1 deletes. There is a case for this, plus a mixed case where the baseline MH survives (security A).
- upgrade is owner-preserving and runs across all owners. This is stated in the header next to the owner semantics (security B). The header's clock-skew note names the consequence: skew greater than 30 s could reclaim a live Lease.
- MH_UP_MARGIN_S and MH_DRAIN_MARGIN_S are validated config, like MH_POLL_INTERVAL_S: default 30, integer >= 0, a bad value gives usage. Derived-bound cases run with margin 0 against tiny literal probes and grace, and use two probe fixtures (startup-max and readiness-max). The suite budget is about 15 s, measured at review (paired-test P1).
- Cases P2-P9 adopted:
  - the --no-wait Nth-read return point, plus its never-marked arm that gives drain-timeout;
  - a create failing with foreign wording gives kubectl-failed, with no next-slot attempt;
  - the full set of Lease arms, including Conflict on reclaim giving busy, and a foreign holder on release (not cleared, no delete, warns);
  - a signal raised deterministically on a stub marker, giving internal-error, with tempdir cleanup;
  - upgrade of two stale MHs with one free slot, plus a resume whose annotation is orphaned giving noop;
  - --all partial refusal, and --all with nothing owned giving drained:[];
  - a non-contiguous slot set, plus duplicate and non-decimal slot labels;
  - stub timestamps taken from real `date -u` with per-case offsets.
- From code-reviewer, all adopted:
  - kc() builds a quoted argv, `kubectl --context "${KUBE_CONTEXT}" -n "${MH_NAMESPACE}" "$@"`, and `readonly MH_NAMESPACE=dark-tower` is the only home of the namespace.
  - ONE jq snapshot filter, taking (configmaps, deployments, pods, now), derives every slot entry. Every verb and the serving-count check consume it. decide() is a pure function of that snapshot.
  - The token lookup maps an unknown token to internal-error, with a case. The tests grep that no hand-written `exit <n>` appears outside the emitter.
  - Any create or replace failure whose stderr is not recognised maps to kubectl-failed, never to "next slot" or "busy".
  - The EXIT trap runs in this order: release the Lease, clean up tmp, then the final emitter. MH_RESULT stays last even when the release warns.
  - All time comparisons are done in jq with `now` passed in (`date -u +%s`), with no GNU `date -d`.
  - Sourcing has no side effects. There is one cmd_* function per verb, each under about 50 lines.
  - A header comment explains why dt_init_cluster_env is called twice.
- From operations, all adopted:
  - The upgrade free-slot precheck counts pending resume pairs as needing no slot. Case: a full pool with a Ready replacement gives drain k, exit 0. A resume whose k is already draining joins the wait and issues no second delete.
  - stuck-terminating is pool-wide. Case: the target is serving while another slot is stuck, giving exit 4 and no delete. For --all, the stuck and Lease checks are all-or-nothing, before the first delete.
  - Lease reclaim prints a loud warning with the prior holder and the age. If renewTime is more than MH_LEASE_FUTURE_SKEW_S (5) seconds ahead of host now, the result is busy plus a skew warning, never a reclaim. The residual is recorded in the header, and there is a case for it.
  - If the Lead approves the tokens, contract.txt §3 is amended in this commit, and the check becomes array == contract (bidirectional).
  - Each upgrade drain step re-evaluates stuck-terminating and last-serving-mh under the Lease on a fresh read. Only the owner check is skipped.
- From paired-test P10: the layer7-shaped owner `e2e-<16 hex>` is accepted. `E2E-X`, a trailing hyphen and a 64-character owner each give usage, with no create or delete.
- LEAD RULINGS (approved): the tokens kubectl-failed (1), internal-error (1) and prerequisite-missing (2); `pod_ip` (from `.status.podIP`, null when there is no pod, in the golden); the I-2 interface as proposed. contract.txt §3/§4 and the interface are updated in this commit, and the token check is array == contract in both directions.
- T-1 notes: T-1 pins MH series by pod IP (`metrics.rs:instance_for_pod_ip`, because the mh-service scrape job has no `pod` target label), and it consumes `stale`, not `current`. One line is added to the story's T-1 section.
- P10 refinement: the accepted-owner case sources scripts/layer7.sh in a subshell and uses `__generate_org_subdomain`'s output as `--owner`. A separately labelled assertion checks that the generated token is non-empty. The literal negative arms stay.
- Story-file placement (agreed with the test owner): fix the T-1 manifest prompt (id 18) in place. In INPUTS, "current" becomes `stale` and `pod_ip` is added. The Prometheus wait becomes `instance=<pod_ip>:8083` via `metrics.rs:instance_for_pod_ip`, because the scrape job has no `pod` label. Any ~line-600 note must stay consistent with that prompt. Then run `scripts/guards/simple/validate-story-manifest.sh`.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### mh.sh (new, `infra/kind/scripts/mh.sh`)
The script implements `list|up|drain|upgrade` per the plan and its addenda.
- **Contract and validation.** The header restates the contract and the I-2 interface. `MH_TOKENS` is the only home of the token-to-exit-code mapping, and `MH_OWNER_RE` defines a valid owner. Every argument is parsed before the first kubectl call.
- **Reads.** `kc()` is the only kubectl entry and pins `--context`. One jq snapshot (`snapshot_jq`) produces the state of every slot.
- **Lease.** The `mh-pool` Lease is acquired with create, reclaimed with a replace that carries the resourceVersion, and released by clearing the holder.
- **Result line.** One emitter (`mh_finish`) prints the final line. On a signal, it first restores the original stdout/stderr (fds 3/4), so the line is not lost in a redirected temp file.

### mh.test.sh (new)
- **Stub.** A stateful stub kubectl whose behaviour is a single jq program over per-case JSON state. It is loud on unknown calls, on calls without `--context`, and on any force or pod delete.
- **Stream contract.** It is asserted on every run.
- **Groups.** 17 independent groups run concurrently: about 14 s wall time when run alone, and 797 assertions.
- **Global checks:**
  - tokens: array == contract §3 == emitted;
  - the aggregate force/zero-grace ban, with a positive control;
  - a static check that the script is Prometheus-free, with a positive control;
  - exactly one `exit` in the script.
- **Mutation spot-checks.** Each of these mutations turns the suite red: lowest-free instead of next-fit; stuck-terminating forceable; a broken last-serving guard; labels replaced instead of merged; the `--all` count not decremented.

### Other edits
- `scripts/layer3.sh`: one `parallel_queue_add` entry.
- `mh-sh-contract.txt`: §3 gains `ok` and the three approved tokens, §4 gains `pod_ip` and `ok`, and the I-2 interface is added.
- INDEX pointer added. `validate-knowledge-index.sh` is OK.
- Story file: the T-1 manifest prompt is corrected (`stale` and `pod_ip`; the pod-IP Prometheus pin). `validate-story-manifest.sh` exits 0.

---

## Files Modified

```
{Output of: git diff --stat HEAD}
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `{the whole commit | <unit + safe direction>}` (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

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

## Gate 1 — Lead Rulings & Confirmations

Lead rulings (2026-11-07):
1. Additive tokens APPROVED: `kubectl-failed` (1), `internal-error` (1), `prerequisite-missing` (2); contract.txt §3 updated in same commit.
2. `pod_ip` APPROVED (additive, schema_version 1, null when free). T-1 pins MH series by pod IP (`instance_for_pod_ip`) — the mh-service scrape job has no `pod` label. T-1 consumes `stale`, not `current`.
3. I-2 interface APPROVED: `mh-template` keys `deployment.json`/`revision`/`baseline`; slot CM keys `MH_WEBTRANSPORT_ADVERTISE_ADDRESS` + `MH_HOST_PORT`, exactly one CM per slot label.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 3 | 3 | 0 | request-timeout + lease_guard (fail-closed); injection-shaped usage cases. Residual: host-clock skew >30 s on Lease reclaim (header) |
| Test (paired) | RESOLVED-FIXED | 6 | 6 | 0 | concurrency flake (margins), Lease-ordering on drain/upgrade, JSON goldens, next-fit over non-serving, idempotent starting up, --no-wait tightening |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | label-survival derived from template labels |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | signal re-entry, malformed Lease, single snapshot, vanish-mid-wait, poll>0, errexit comment, entry_field false |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | in-file SSoT constants; no extraction opportunities |
| Operations | RESOLVED-FIXED | 5 | 5 | 0 | lease overrun, test margins, signal hint, upgrade progress, minReadySeconds. Layer-3 budget breach pre-existing (TODO.md) |
| Semantic Guard | — | — | — | — | not spawned (shell-only diff, no checks.md surface) |

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (attempt 1 killed by the Lead's 600 s tool timeout before reaching a verdict; this does not count as a pipeline attempt). Attempt 2: `TOTAL_RESULT=OK`.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 | OK | 6 |
| 2 | OK | 5 |
| 3 | OK | 50 |
| 4 | OK | 159 |
| 5 | OK | 1 |
| 6 | OK | 4 |
| 7 | OK | 1265 |

Layer-3 BUDGET_BREACH warning (pre-existing, tracked in docs/TODO.md; operations accepted the self-test's ~5 s share).

## Final Summary

mh.sh (list/up/drain/upgrade) implements mh-sh-contract.txt with Lead-approved additive tokens (kubectl-failed, internal-error, prerequisite-missing) and `pod_ip`; the contract text was amended in the same change and the self-test enforces token array == contract §3 in both directions. The hermetic self-test (mh.test.sh, ~998 assertions) is wired into Layer 3. Ships uncalled; I-2 produces the cluster-side interface stated in the script header. Live-cluster half of R-29 is proven at I-2.

**Safe-revert unit**: the whole commit (script, self-test, Layer-3 wiring and contract amendment move together).
