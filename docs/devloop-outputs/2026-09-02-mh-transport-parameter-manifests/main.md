# Devloop Output: MH Transport-Parameter Deployment Configuration

**Date**: 2026-09-02
**Task**: Land the MH transport-parameter deployment configuration for the ADR-0036 media path (story task #9 / I-2)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d756ed66457b41ac13fa34d5ffe6dccf281fa9de` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Headless | `DEVLOOP_HEADLESS=1` (run-story task #9) |
| Resumed | 2026-09-02 — session interrupted during `planning`; roster respawned, plan preserved verbatim |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (infrastructure, respawned) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `respawned` |
| Test | `respawned` |
| Observability | `respawned` |
| Code Quality | `respawned` |
| DRY | `respawned` |
| Operations | `respawned` |
| Semantic Guard | `respawned` |

| Media-Handler Pairing | `media-handler-consult` (pairing partner, not a Gate-3 reviewer) |

---

## Task Overview

### Objective
Land the deployment-side half of ADR-0036 §1's explicit QUIC transport parameters for mh-service, so
the media-handler code task (#12) that makes them required via `ConfigError::MissingEnvVar` lands into
a manifest tree that already carries them (env-config guard green in both directions).

### Scope
- **Service(s)**: mh-service (manifests only), env-tests (new assertion)
- **Schema**: No
- **Cross-cutting**: Yes — key names paired with media-handler; drain window is an operations concern

### Debate Decision
NOT NEEDED — the design is settled in `docs/user-stories/2026-08-27-hear-yourself-through-handler.md`
§Design/infrastructure (I-2) and ADR-0036 §1.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/services/mh-service/configmap.yaml` | Mine | — |
| `infra/services/mh-service/mh-0-deployment.yaml` | Mine | — |
| `infra/services/mh-service/mh-1-deployment.yaml` | Mine | — |
| `infra/services/mh-service/kustomization.yaml` | Mine | — |
| `crates/env-tests/tests/01_mh_deployment_config.rs` (new) | Mine | — |
| `crates/env-tests/src/lib.rs` | **Minor-judgment** | @security (co-signed at Gate 3; same by-test rule and trailer as `00_cluster_health.rs`) |
| `crates/env-tests/tests/00_cluster_health.rs` | **Minor-judgment** | @security (ACKed, trailer) |
| `docs/observability/dashboard-conventions.md` | **Minor-judgment** | @observability (ACKed, trailer) |
| `docs/runbooks/mh-incident-response.md` | **Minor-judgment** | @operations (ACKed verbatim, re-ACKed after correction) |
| `docs/TODO.md` | Mine | — (DRY-exception + Rulings 3/4) |
| `docs/devloop-outputs/2026-09-02-mh-transport-parameter-manifests/main.md` | Mine | — |

**Why the table grew 10 → 11 at Gate 3.** `crates/env-tests/src/lib.rs` is the eleventh file and was
not in the closed changeset. @dry-reviewer's F1 required collapsing a `NAMESPACE` const that this
commit had defined **twice** — once in `00_cluster_health.rs`, once in `01_mh_deployment_config.rs`.
Rust integration-test files are separate crates and cannot import one another, so the crate root is
the only possible shared home; there is no way to satisfy the finding without touching it. The change
is a 7-line `pub const` plus doc comment — no behaviour, no new module, no API beyond the constant.
Classified **Minor-judgment, owner @security**, on their own Ownership Lens ruling: I had proposed
*Mine*, but the const determines whether the credential-leak probes sample real pods, so the same
by-test rule that makes `00_cluster_health.rs` theirs makes this theirs. They verified the value is
unchanged and nothing else in `lib.rs` moved, and co-signed under the existing trailer. Not a
Guarded Shared Area — ADR-0024 §6.4 is not enumerated for it, and a test-harness constant is not a
detection/forensics *contract* in the §6.4 sense. Flagged to @dry-reviewer, @security and
@test as new shared surface, and disclosed to @main as a scope change rather than absorbed silently —
the scope-drift guard caught it before I signalled, which is the guard working as intended.

**Why the table grew 6 → 10 during Gate 1** (@main Ruling 1 ratified this as intentional, not accretion):
none of the four additions widens the task's ambition. `dashboard-conventions.md:201` is a sentence
**this commit makes false**. `00_cluster_health.rs` is a sub-5-LoC fix to a dead security control in a
crate the diff already opens. `mh-incident-response.md` Scenario 5 goes wrong **when this task lands**
(it advises scaling out in response to a bound this commit makes explicitly-never-capacity).
`docs/TODO.md` is the condition @dry-reviewer attached to an accepted deferral, plus @security's
Ruling-3 condition and @main's Ruling-4 flags. Three of the four are reviewer findings fixed *in-diff*
rather than spun out.


**Why `crates/env-tests/**` is Mine, not cross-boundary.** `crates/env-tests/` is the shared
env-test crate, and ADR-0024's ownership rule for it is by *test*, not by directory: the story's
Test Plan assigns each env-test to the specialist who owns the artifact under assertion
(A/B → client, C → media-handler, D → meeting-controller, E → observability, **F → infrastructure**).
Item F asserts a Kubernetes deployment artifact (a pod's env value against its own pod spec's
`terminationGracePeriodSeconds`). It reads no mh-service Rust type, calls no mh-service API, and
encodes no mh-service runtime semantics — it is `kubectl` against the API server. The semantics of
`MH_MAX_STREAMS` / `MH_MAX_CONNECTIONS` *are* media-handler's, which is why this test asserts
neither of them and why the ConfigMap comments were paired with @media-handler-consult rather than
written unilaterally.

`crates/env-tests/tests/00_cluster_health.rs` is **the exception**, and is classified cross-boundary
(Minor-judgment, owner @security) in the table above precisely because it is *not* my test — it is a
pre-existing credential-leak control, and the by-test rule assigns it to whoever owns the control, not
to whoever happens to be editing the crate.

No Guarded Shared Area path is touched. No mh-service Rust file is touched (task #12 owns
`crates/mh-service/src/config.rs`; this task deliberately leaves the `unwrap_or(DEFAULT_MAX_CONNECTIONS)`
alone).

---

## Planning

### Problem, restated as a mechanism

The task names an instance ("MH's transport parameters and drain window"). The mechanism is:

> **A value a service reads from its environment must be declared in the manifest that supplies it;
> and where that same quantity is already encoded elsewhere in the same manifest, the second
> encoding must be *derived* from the first with a build-time hard-fail on drift, not retyped.**

Two halves, each with its own enforcement:
- *Is it declared?* — `dt-guard env-config` (`missing_in_manifest` / `orphan_configmap_key`), static.
- *Does it apply?* — the kustomize replacement (build-time) plus the env-test (deployed artifact).

**The wider class this mechanism produces, and its same-owner siblings.** Restating it this way
makes visible that MH is not the only instance in this tree, and the siblings are mine:

| Sibling | Evidence | In scope here? |
|---|---|---|
| GC drain window | `crates/gc-service/src/main.rs:441-445` reads `GC_DRAIN_SECONDS` with `unwrap_or(30)`; `GC_DRAIN_SECONDS` appears in **no** manifest, and `gc-service/deployment.yaml:26` sets grace `35`. Same defect shape as MH's: an unchosen default drains inside a grace period nobody tied it to. | **No** — needs global-controller. Surfacing, not fixing. |
| MC drain window | `crates/mc-service/src/main.rs:456` hardcodes `sleep(Duration::from_secs(2))`; `mc-{0,1}-deployment.yaml:26` grace `35`. Identical to the MH literal task #12 is removing. | **No** — needs meeting-controller. |
| AC drain window | `crates/ac-service/src/main.rs:171,188` "30s drain" per ADR-0012; `statefulset.yaml:22` grace `35`. | **No** — needs auth-controller. |
| Port literals | `4434` / `8083` / `50053` each appear twice per MH deployment (ConfigMap bind address + `containerPort`), with no drift guard. A second instance of the same "two encodings, no derivation" mechanism. | **No** — a separate mechanism task; the replacement idiom this task lands is the tool that would fix it. |

I am **not** widening this task to cover them: three of the four need a Rust change in another
specialist's crate, and the fourth is its own mechanism task. I am recording them here so the class
is visible rather than rediscovered, and I will raise the three drain-window siblings for the story's
follow-up list. What this task *does* contribute to the class is a working, verified instance of the
derivation idiom that the siblings can copy.

### Answers to the two questions the Lead asked to be settled in the plan

**Q1 — the kustomize-version premise. The task's "verified against kustomize v5.7.1" claim is NOT
what this tree runs, so I verified it myself against what the tree actually invokes.**

- Tooling in this container: **no standalone `kustomize` binary**. `kubectl` is `v1.32.3` with
  **embedded Kustomize `v5.5.0`**.
- What the tree invokes: `crates/dt-guard/src/kustomize.rs` → `detect_kustomize_tool()` /
  `run_kustomize_build()`, over `infra/services/mh-service` (base) **and**
  `infra/kubernetes/overlays/kind/services/mh-service` (overlay), via
  `scripts/guards/simple/validate-kustomize.sh`. The deploy path is
  `infra/kind/scripts/setup.sh:1208` → `kubectl apply -k .../overlays/kind/services/mh-service/`.
  Both are `kubectl`, i.e. **v5.5.0** — so v5.5.0 is the version that matters here, not v5.7.1.
- **Verified empirically, on a scratch copy of the real manifests, before writing the plan:**

| Case | Command | Result |
|---|---|---|
| Happy path | `kubectl kustomize <base>` | exit 0; both deployments render `MH_TERMINATION_GRACE_SECONDS: "35"` (kustomize coerces the int source to a quoted string — valid for an env `value:`) |
| **Source field deleted** (`terminationGracePeriodSeconds` removed from `mh-0-deployment.yaml`) | same | **exit 1** — ``error: fieldPath `spec.template.spec.terminationGracePeriodSeconds` is missing for replacement source Deployment.[noVer].[noGrp]/mh-0.[noNs]`` |
| **Target env entry deleted** (`MH_TERMINATION_GRACE_SECONDS` removed from `mh-1-deployment.yaml`) | same | **exit 1** — `error: unable to find field "spec.template.spec.containers.[name=mh-service].env.[name=MH_TERMINATION_GRACE_SECONDS].value" in replacement target` |

  So on **Kustomize v5.5.0 (kubectl v1.32.3)** the single-source-of-truth guarantee holds in *both*
  directions, and it holds under the guard that actually runs. I will re-run all three cases at Gate
  2 against the landed files (not a scratch copy) and paste the raw output for @code-reviewer.

**Q2 — can the env-test assert the real deployed artifact here, and what does it do with no cluster?**

Yes. A live Kind cluster is present now (`mh-0-674d5d56cf-w66k8`, `mh-1-687b9fb885-flv5w` Running in
namespace **`dark-tower`** — note: *not* `default`; `00_cluster_health.rs`'s `-n default` is a legacy
detail I am not copying). Probed live: `.spec.terminationGracePeriodSeconds` reads `35` off the real
pod object.

- **Both sides come off one live pod object** fetched from the API server: side (a) is
  `.spec.containers[name=mh-service].env[name=MH_TERMINATION_GRACE_SECONDS].value`, side (b) is
  `.spec.terminationGracePeriodSeconds` **of that same pod**. Nothing in the test runs
  `kubectl kustomize`. It cannot be a tautology, because the two sides are only equal if the
  replacement (a) was written correctly, (b) survived the overlay build, and (c) was applied to the
  cluster — the "does it apply" half.
- **Honest limitation, stated rather than hidden:** the strongest possible reading would be the
  process's own environment (`printenv` / `/proc/1/environ`), but the MH image is distroless with
  `readOnlyRootFilesystem` and no shell, so `kubectl exec` has no binary to run. The pod spec as
  stored by the API server is the strongest reading available, and it is still the *deployed
  artifact*, not a render. I will say exactly this in the test's module doc so nobody later reads
  the test as asserting more than it does.
- **No self-skip.** Missing `kubectl`, missing pod, missing env entry, unparseable value → `panic!`
  with a diagnostic message. The legitimate no-cluster case is a *lane* decision in
  `scripts/layer7.sh` (`SKIPPED-NO-CLUSTER`, exit 0, CI-only), never a green test.

### Design

**1. Four keys into the shared ConfigMap `mh-service-config`, referenced by both deployments.**

`MH_MAX_CONCURRENT_UNI_STREAMS`, `MH_DATAGRAM_BUFFER_AUDIO_FRAMES`, `MH_KEEPALIVE_INTERVAL_MS`,
`MH_MAX_CONNECTIONS` — same shape as every existing MH key: `data:` entry in `configmap.yaml`,
`configMapKeyRef` in `mh-0-deployment.yaml` **and** `mh-1-deployment.yaml`. Per-workload, not union
— `dt-guard env-config` check 1 is per-workload by design, and a var in `mh-0` but not `mh-1` is a
CrashLooping pod once task #12 lands.

**2. `MH_TERMINATION_GRACE_SECONDS` is deliberately NOT a ConfigMap key.**

It is a *per-instance* quantity derived from *that instance's own* pod spec. Putting it in the
shared `mh-service-config` would create exactly the second literal R-21 forbids, and would make the
two instances share one value they do not necessarily share. So it lands as a literal `env` entry on
each deployment, overwritten by the kustomize replacement. Guard consequence: `env_names` contains it
in both workloads (check 1 green), and it is in no ConfigMap (no `orphan_configmap_key`).

Placeholder value: **`"0"`**, not `"35"`. `"35"` would be the second literal. `"0"` is a sentinel
that is *invalid* — task #12's startup validation requires `grace > MARGIN`, so a `0` reaching a
running pod is a loud refusal to start, not a silently-wrong drain. And per the table above, v5.5.0
hard-fails the build if the entry is deleted, so the sentinel cannot be reached by deletion either.

**3. The replacement, in `infra/services/mh-service/kustomization.yaml`.**

Two entries (one per instance), each `source: {kind: Deployment, name: mh-N, fieldPath:
spec.template.spec.terminationGracePeriodSeconds}` →
`target: spec.template.spec.containers.[name=mh-service].env.[name=MH_TERMINATION_GRACE_SECONDS].value`.
Placed in the **base**, so both `kubectl kustomize infra/services/mh-service` (guard) and the Kind
overlay (deploy) get it. `options: create:` is deliberately NOT used — requiring the target entry to
pre-exist is what makes deletion of *either* side a build failure, and it is what keeps the env name
visible to `dt-guard env-config`, which reads base YAML files and never runs kustomize.

**4. Chosen values, each with a rationale tying it to §1 or the loopback demo.**

| Key | Value | Rationale (goes in the comment, in meaning-language not magnitude-language) |
|---|---|---|
| `MH_MAX_CONCURRENT_UNI_STREAMS` | `64` | §1's working figure is ~5 concurrent (five video slots at a one-second keyframe cadence, one group per stream). 64 is ~12× that, enough headroom for the §1 failure mode (a slow subscriber accumulating unfinished groups) while sitting **below** quinn's unchosen default of 100 — so the ceiling is demonstrably ours and the application bound still trips first. Story 1 is audio-only, so the operative count is 0; the bound is declared because a bound must be declared to be assertable. |
| `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` | `32` | A **frame count** (20 ms of Opus each), converted once to bytes in code. 32 frames = 640 ms ≈ 7.2 KB at the ~225 B on-wire frame, against quinn's 1 MiB default ≈ 4660 frames ≈ **93 s** of queued audio — the exact anti-pattern §1 names. It is a *backstop*, not the operative queue: §1 requires MH's application-level egress queue to trip first, so this must sit above that bound and below anything a realtime path would call a delay. Inside @media-handler-consult's 16–64 sane band. |
| `MH_KEEPALIVE_INTERVAL_MS` | `15000` | Milliseconds. What refreshes the NAT binding of a **muted** participant so unmute is instantaneous (§1, §5) — directly load-bearing for the story's mute/unmute demo. 15 s refreshes at least twice inside the ~30 s reap window of an aggressive NAT. |
| `MH_MAX_CONNECTIONS` | `500` | Accept-time resource-exhaustion guard, **never** capacity, **never** advertised. Sized to this pod: 1 Gi limit / 1000m CPU, expected peak in this deployment is single digits, so 500 is ~100× headroom and still refuses a connection flood at accept long before memory pressure. Replaces a `DEFAULT_MAX_CONNECTIONS = 10_000` (`crates/mh-service/src/config.rs:46`) that nobody chose. |


> **Correction notice (added at Gate 3; the plan text above is preserved verbatim per the devloop
> contract and is NOT edited).** The `MH_MAX_CONNECTIONS` row's claim that 500 *"still refuses a
> connection flood at accept long before memory pressure"* was **falsified during review** and does
> not describe the landed artifact. @security and @operations independently showed the bound is not
> memory-derived — quinn's receive-side flow control is undeclared, giving a ~206 MB per-connection
> adversarial ceiling against 1 Gi. The clause was deleted from the ConfigMap comment; see
> §Implementation Summary and `docs/TODO.md` § Port Constant Scattering. `crates/mh-service/src/config.rs:46`
> in that row is likewise frozen plan text — the live reference is the named `DEFAULT_MAX_CONNECTIONS`
> const, since task #12 deletes the `unwrap_or` that gives the line number meaning.

**5. Comments for the three MAX-named keys** — the confusion-prevention requirement. Each states
what the key *controls* and who enforces it; none uses magnitude language. `MH_MAX_STREAMS` gets an
explicit *"`100` is not a chosen value; it is known-wrong (wrong unit) and its correction is story
2's egress-budget chain — do not fix it here"* so nobody reads it as chosen or repairs it in scope.
`MH_MAX_STREAMS`'s value and consumers are untouched.

**6. Env-test — `crates/env-tests/tests/01_mh_deployment_config.rs` (new).**

- One `kubectl get pod -n dark-tower -l instance=mh-N -o json` per instance, parsed with `serde_json`
  (already a dependency). Not `-o jsonpath`: the filter expression needed here is quoting-fragile in
  bash and silently returns empty on a typo, which would make the test pass vacuously — a probe that
  can return "nothing" and be read as "equal" is the failure mode this test exists to prevent.
- **Both instances** asserted, `mh-0` and `mh-1`, as separate `#[tokio::test]` fns over a shared
  helper — they are independently replaced deployments and covering one leaves the other unguarded.
- Assertion message prints pod name, the env value, and the pod-spec grace, so a divergence is
  diagnosable without a second `kubectl` round trip.
- Feature gate: **`smoke`**. Its dependency set is exactly `00_cluster_health.rs`'s — `kubectl` plus
  deployed pods — and none of `flows`' AC→GC→MC→MH chain or port-forwards. `flows` would overstate
  the dependency. Both run under layer 7's `--features all`, so this is a truthfulness question, not
  a coverage one. **Raised to @test explicitly; I will take `flows` if @test prefers consistency with
  `26_mh_quic.rs` over dependency-accuracy — their call, and I'll say which we chose in main.md.**
- Registration: `dt-guard test-registration` only governs `tests/<subdir>/` files behind a
  `<subdir>_tests.rs` entry point. A flat numbered file at `tests/` root is discovered by cargo
  directly, exactly like the twelve existing ones, so the guard stays green. Verified by reading
  `crates/dt-guard/src/test_registration.rs`, not assumed.

### Verification

1. `scripts/guards/simple/validate-env-config.sh` — green both directions (keys declared; nothing orphaned).
2. `scripts/guards/simple/validate-kustomize.sh` — base + Kind overlay build.
3. The three-case kustomize matrix above, re-run on the landed files, output pasted at Gate 2.
4. `kubectl apply -k infra/kubernetes/overlays/kind/services/mh-service/` — **manifest-only, no image
   rebuild** (no Rust changed), so the running pods can be rolled from here; then
   `cargo test -p env-tests --features smoke` for the new test against the real cluster.
5. `scripts/layer-all.sh` per the devloop gate.

### Risks / open items for reviewers

- **R1 (for @media-handler-consult).** §1 requires the application egress-queue bound to trip
  *before* the transport datagram-buffer ceiling, and task #12 adds a startup validation for exactly
  that. My `32` frames must therefore sit strictly **above** whatever bound task #12 picks. If your
  app-level egress queue is ≥ 32 frames, MH will refuse to start when #12 lands. **Please confirm
  your intended bound** — this is a value dependency, not just a name dependency, and it is the one
  way this task can break task #12.
- **R2 (for @operations).** The drain window becomes `min(SETTLE_TARGET, 35 − MARGIN)` in task #12.
  Changing `terminationGracePeriodSeconds` now silently changes MH's drain. That is the intended
  coupling, but it is new operational behaviour and belongs in `mh-deployment.md`'s
  "Rollout With Media Flowing" section (operations' task, not mine) — flagging so it is not missed.
- **R3.** Rolling the MH pods to pick up the new env vars is a live-cluster action inside this
  devloop. Harmless today (task #12 hasn't landed, so MH ignores the new vars), but it does restart
  both MH pods — noting it rather than doing it silently.

---

## Gate 1 — Plan Approval (Lead record)

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files` (exit 0).

Lead-verified environment premises (independently of the implementer):

| Premise | Verified value |
|---|---|
| Kustomize tooling | `kubectl v1.32.3`, **embedded Kustomize v5.5.0**; no standalone `kustomize` binary in this container |
| Story text claim | `docs/user-stories/...:104` says "verified against kustomize v5.7.1" — **not what this tree runs**. Doc defect, flagged not propagated. |
| Cluster | Kind up; `mh-0-674d5d56cf-w66k8`, `mh-1-687b9fb885-flv5w` Running in ns `dark-tower` |

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (after P8 ruling) |
| Semantic Guard | confirmed |
| Media-Handler (pairing, not a Gate-3 reviewer) | pairing confirmed |

### Lead rulings

**Ruling 1 — scope growth 6 → 9 files: RATIFIED as intentional, not accretion.**
Raised by @dry-reviewer for Lead ratification rather than decided by a reviewer. None of the three
additions widens the task's ambition:
- `docs/observability/dashboard-conventions.md` — a sentence *this commit makes false*; shipping
  without it would knowingly land a lie in a doc. Owner @observability, Minor-judgment, ACKed.
- `crates/env-tests/tests/00_cluster_health.rs` — sub-5-LoC fix, inside a crate the diff already
  opens, no design ambiguity. The protocol's suspicious-deferral check says *deferring* would be the
  defect. Owner @security, Minor-judgment, ACKed.
- `docs/TODO.md` — the condition @dry-reviewer attached to an accepted deferral. Mine.

**Ruling 2 — @security S5 vs @test's spin-out: do both; the namespace fix is not a half-fix.**
A genuine reviewer disagreement neither could settle alone. @test was right that a namespace-only
patch re-arms the same trap on the next label drift, and wrong that this makes deferral better.
Resolution: land the `-n default` → `-n dark-tower` correction **and, in the same hunk, a
non-empty-result-set guard** that panics when the probe returns no pods. That converts "assert
against `""`" into "assert against a real pod set" and closes @test's objection inside this diff.
The wider class (`26_mh_quic.rs`, `31_gc_telemetry.rs`, shared-helper shape) spins out, filed by
@test at verdict time, named as a class so the next instance is greppable.

**Ruling 3 — (C) accepted, with @security's condition intact.**
(C) is better than either option escalated to the Lead. The reasoning that a hand-typed
receive-window key would be a second encoding of a memory budget whose real source is
`resources.limits.memory` **in the same file** is decisive — that is the R-21 defect this task
exists to remove, and it is why (A) was rejected.

Precision on what (C) is: **500 is memory-derived *conditional on task #12 landing those
constants*.** Today it is not. That conditionality is carried entirely by a commitment made in this
session's transcript — precisely the fragility @security named. Therefore the residual goes in
`docs/TODO.md` (naming the three undeclared terms and the ~206 MB current ceiling), **not** a
ConfigMap comment alone: a comment records a decision, only a TODO entry is tracked work. The
cross-task preconditions are recorded in main.md **and** in `docs/TODO.md` — main.md for task #9 is
no more visible to #12's reviewers than this transcript is.

**Ruling 4 — story-text corrections: flag, do not edit.**
- kustomize v5.7.1 claim in the story file → main.md + `docs/TODO.md`. The story file is the
  runner's manifest under ADR-0035; editing it mid-story is not task #9's call.
- AC's "30s drain per ADR-0012" (`crates/ac-service/src/main.rs:171,188`) cites an ADR containing
  nothing on drain/grace/shutdown. Cross-owner → `docs/TODO.md`, owner auth-controller. AC source
  untouched.

**Ruling 5 — @operations P8 ratified as file #10; changeset then CLOSED.**
Same shape as P1: this commit is what makes `mh-incident-response.md` scenario 5's first-line advice
wrong (`MH_MAX_CONNECTIONS` stops being a capacity figure and becomes a chosen resource guard, so
"scale horizontally" goes from imperfect to actively misleading). Owner-authored, owner-ACKed. The
three further breakages ride along under the framing-lock rule — same-owner, same-mechanism (one
single-Deployment assumption the per-instance split invalidated), inside a hunk already open. Four
runbook commands that return `NotFound` are a masked failure in the project's own sense.


**Ruling 6 — @operations' Scenario 14 escalation: IN (4 lines); the ~35 naming sites spin out.**
Raised under the invitation attached to Ruling 5, with the counter-argument against their own
position stated plainly. Strictly, Scenario 14's Option B block was wrong *before* this commit.
What is new as of this commit is that Scenario 5's Recovery block — which **we** add — says *"There is NO cert-manager in
this system; do NOT delete the Secret"* while `:911` still said to delete it. **The file now
contradicts itself on a destructive action, and that contradiction is ours.** A responder landing on
Scenario 14 gets no signal which half is stale, and the half they are reading destroys state.

The standing rule adopted for the remainder of the changeset, in @operations' words:
**a stale name fails loudly and safely; a destructive command that succeeds and then lies about
recovery does the opposite.** `NotFound` costs a minute at 3am and teaches the operator the runbook
is stale. `kubectl delete secret mh-service-tls` succeeds, destroys the only copy, and Scenario 14's
recovery-time estimate then
sends the operator to wait for a re-issuance that will never happen. The ~35 stale
`deployment/mh-service` sites, the second `kubectl scale --replicas=5` (`:537`), `kubectl edit
configmap mh-service` (`:922`, it is `mh-service-config`), `:884`'s distroless-broken `kubectl exec
… openssl`, and the dependency-line / Common-Root-Cause-1 descriptive mislabels are all in the loud class and all spin out —
they need a decision about what each command should *say*, which is a project, not a hunk.

Bounded deliberately: a **fourth hunk in a file already in the diff**, pointing at an F7
cross-reference the same commit adds ~500 lines earlier. **Changeset stayed closed at 10 files.**

**Changeset closed at 10 files.** Each addition was individually correct, but the pattern — every
reviewer turning up an adjacent broken thing while reading around their finding — does not terminate
on its own. From here, further findings go to `docs/TODO.md` with a named owner unless they meet the
P1/P8 bar (*this commit is what makes it wrong*), which is a Lead ruling, not a reviewer's call.

### Notable Gate-1 catches (recorded because each would have shipped)

- **@operations R3** — the plan's verification step (`kubectl apply -k`) would have reverted the live
  advertise-address ConfigMaps (live `10.255.255.254:24422/24425` vs checked-in
  `127.0.0.1:4434/4436`, patched *post-apply* by `setup.sh:1211-1217`). `apply` exits 0,
  `rollout status` greens, and the new env-test passes because it reads grace, not advertise
  addresses — every signal would have reported success while GC handed loopback to every client.
  Now `dev-cluster deploy mh` + two post-roll checks.
- **@security S5** — `00_cluster_health.rs:66,112` query `-n default`, which is empty, so the suite's
  only two runtime credential-leak assertions passed unconditionally. The plan spotted it and routed
  around it — a suspicious deferral by the protocol's own definition.
- **@security S1 / @operations F2** — `MH_MAX_CONNECTIONS` was sized against expected legitimate load,
  the wrong denominator for a DoS bound. Sharpest consequence, recorded at @operations' request:
  **OOMKill is SIGKILL and bypasses `terminationGracePeriodSeconds`** — the drain window this entire
  task exists to wire up is void under exactly the adversarial condition where it matters most.
- **@media-handler-consult** — the plan justified an *ingress* bound
  (`MH_MAX_CONCURRENT_UNI_STREAMS`, peer-advertised) with an *egress* failure mode (§1's slow
  subscriber). Value stands at 64; the rationale was re-pointed. @observability retracted a
  dependent answer of their own rather than let the tidier version stand.
- **@dry-reviewer D1** — the plan's claim that the `replacements:` idiom would fix the port-literal
  class was **false**: `setup.sh:1211-1217` rewrites `MH_WEBTRANSPORT_ADVERTISE_ADDRESS` via
  `kubectl patch` *after* apply, outside the kustomize build entirely. Disproving the plan's own
  justification is what moved the deferral from framing-lock to genuinely task-sized.

Reviewers self-corrected on the record in four places (@security withdrew S3; @observability
retracted the uni-streams answer; @dry-reviewer corrected the ADR-0034 tripwire claim;
@code-reviewer corrected the dt-guard subcommand count 36 → 34; @media-handler-consult withdrew a
wrong `orphan_configmap_key` argument). Recorded because the corrections, not the confirmations,
are the evidence the gate did work.

**Value changes from the preserved plan:** `MH_KEEPALIVE_INTERVAL_MS` **15000 → 10000** (1:3 rather
than 1:2 loss margin against a 30 s idle timeout; three reviewers converged independently). All
other values unchanged.

---

## Pre-Work

None. Task #1 (`2026-08-28-env-config-guard-per-instance-workloads`) landed the `dt-guard env-config`
discovery fix this task depends on.

---

## Implementation Summary

Landed the deployment-side half of ADR-0036 §1's explicit QUIC transport parameters for mh-service,
so that task #12 — which makes them required via `ConfigError::MissingEnvVar` — lands into a manifest
tree that already carries them.

**1. Four new keys in the shared `mh-service-config` ConfigMap**, referenced by `configMapKeyRef` from
**both** `mh-0-deployment.yaml` and `mh-1-deployment.yaml` (per-workload, not union — `dt-guard
env-config` check 1 is per-workload by design, and a var in mh-0 but not mh-1 CrashLoops that pod alone
once #12 lands):

| Key | Value | Note |
|---|---|---|
| `MH_MAX_CONCURRENT_UNI_STREAMS` | `64` | **Ingress** bound (peer-advertised; QUIC enforces by withholding stream credit). Below quinn's unchosen default of 100. |
| `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` | `32` | Datagram **send** buffer, per-connection, in frames. Strictly above task #12's `EGRESS_QUEUE_FRAMES = 8`. |
| `MH_KEEPALIVE_INTERVAL_MS` | `10000` | 1:3 against #12's 30 s `max_idle_timeout`; ≥3 refreshes inside a ~30 s NAT reap window. |
| `MH_MAX_CONNECTIONS` | `500` | Accept-time exhaustion guard, never capacity. 20× reduction from the unchosen 10,000 default. |

**2. `MH_TERMINATION_GRACE_SECONDS` as a per-deployment literal, derived at build time.** Deliberately
**not** a ConfigMap key: it is a per-instance quantity read off that instance's own pod spec, and a
shared key would be exactly the second, drift-prone literal R-21 forbids. Ships as the sentinel `"0"`,
overwritten by a kustomize `replacements:` block in the **base** `kustomization.yaml` (two entries, one
per instance — deliberately not merged, since `source.name` must be a literal and one entry would
silently make both instances read mh-0's grace). `options: create:` deliberately unset, so deleting
*either* side is a hard build failure and the env name stays visible to `dt-guard env-config`.

**3. Comments** on all three MAX-named keys naming what each controls, **which service enforces it**,
and **where an operator sees it trip**; `ANCHOR (DRY):` comments naming `crates/mh-service/src/config.rs`
as the sibling encoding that #12 collapses; and an explicit note that `MH_MAX_STREAMS: "100"` is
known-wrong (wrong *unit* — the real quantity is an egress bandwidth budget), is live on the wire to GC
(`grpc/gc_client.rs:140,301`), and must not be repaired here.

**4. New env-test** `crates/env-tests/tests/01_mh_deployment_config.rs` (story Test-Plan item F,
validates R-21), `smoke`-gated per @test's ruling, asserting for **both** instances that the deployed
pod's env value equals **that same pod's** `.spec.terminationGracePeriodSeconds`.

### Findings that changed the design during review

- **The bound that fails open.** `MH_MAX_CONNECTIONS=500` is **not** memory-derived today. quinn-proto
  0.11.17 leaves `receive_window = VarInt::MAX` (unbounded), `stream_receive_window = 1.25 MB`,
  `datagram_receive_buffer_size = 1.25 MB` — ~206 MB per-connection adversarial ceiling against a 1 Gi
  limit, i.e. ~5 hostile connections, not 500. Two amplifiers: MH has **no `accept_uni` loop** today, so
  unread stream data holds its windows and the worst case is the *ordinary* case; and **OOMKill is
  SIGKILL**, which bypasses `terminationGracePeriodSeconds` entirely — **the drain window this whole
  task exists to wire up is void under exactly the adversarial condition where it would matter most.**
  Lowering the number does not fix it (100 is still ~20 GB); only declaring receive-side flow control
  does, which task #12 now commits to. 500 lands as a 20× improvement with the gap named in-comment and
  tracked in `docs/TODO.md`.
- **MH has no horizontal-scaling lever at incident time.** Instances are per-instance Deployments with
  their own NodePort Services and Kind port mappings, so adding one is a manifest change, not a
  `kubectl scale`. Combined with a tripped accept guard meaning flood-or-leak rather than demand, the
  only responses are find-the-source or restart.

---

## Files Modified

| File | Change |
|---|---|
| `infra/services/mh-service/configmap.yaml` | 4 new keys + rationale/ANCHOR comments; `MH_MAX_STREAMS` known-wrong note |
| `infra/services/mh-service/mh-0-deployment.yaml` | 4 `configMapKeyRef` entries + `MH_TERMINATION_GRACE_SECONDS` sentinel |
| `infra/services/mh-service/mh-1-deployment.yaml` | same |
| `infra/services/mh-service/kustomization.yaml` | `replacements:` block (2 entries) + rationale |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | **new** — Test-Plan item F |
| `crates/env-tests/tests/00_cluster_health.rs` | `-n default` → `dark-tower` (2 sites) + `assert_pods_exist()` non-empty guard |
| `docs/observability/dashboard-conventions.md` | `:201` lead-in only — datagram bullet now live, egress still forward |
| `docs/runbooks/mh-incident-response.md` | Scenario 5: Impact/Immediate/root-causes/Recovery/cert-inspection. **Scenario 14** (4th hunk, @main ruling): Option B + the recovery-time estimate. Option B ends up carrying **no command at all** — it points at F7 and stops (see Issues §6) |
| `docs/TODO.md` | Port-Scattering in place; + kustomize-version flag, AC citation, grace-constant class |

---

## Devloop Verification Steps

1. **`validate-env-config.sh`** → `STATUS=OK REASON=env-config-clean-4-services-6-workloads`
2. **`validate-kustomize.sh`** → `STATUS=OK REASON=kustomize-clean-kubeconform-skipped`
3. **Three-case kustomize matrix, re-run on the LANDED files** (kustomize v5.5.0 embedded in kubectl
   v1.32.3 — *not* the v5.7.1 the story text claims):

   | Case | exit | output |
   |---|---|---|
   | Happy path | `0` | both deployments render `MH_TERMINATION_GRACE_SECONDS: "35"` from a `"0"` sentinel; Kind overlay identical |
   | Source field deleted (mh-0) | **`1`** | ``error: fieldPath `spec.template.spec.terminationGracePeriodSeconds` is missing for replacement source Deployment.[noVer].[noGrp]/mh-0.[noNs]`` |
   | Target env entry deleted (mh-1) | **`1`** | `error: unable to find field "spec.template.spec.containers.[name=mh-service].env.[name=MH_TERMINATION_GRACE_SECONDS].value" in replacement target` |

   The single-source-of-truth guarantee therefore holds in **both** directions under the tool that
   actually runs.

4. **Deploy + live assertion.** Used **`dev-cluster deploy mh`**, *not* a bare `kubectl apply -k`
   (see Issues §1). Post-roll checks:
   - advertise addresses still gateway form: `mh-0-config=https://10.255.255.254:24422`,
     `mh-1-config=https://10.255.255.254:24425` — **not** the checked-in `127.0.0.1` loopback values
   - both deployments rolled out successfully; no `MHDown`
   - `01_mh_deployment_config` — **2 passed**, both instances, `env=35 spec=35`
   - `00_cluster_health` credential-leak pair — **2 passed**, now against **real** pod data

   **The 4 failures in `00_cluster_health.rs` are PRE-EXISTING and NOT caused by this change.**
   `test_ac_health_endpoint`, `test_ac_ready_endpoint`, `test_grafana_reachable` and
   `test_prometheus_reachable` all reach their services over **host port-forwards** (via
   `ClusterConnection` / `ClusterPorts::from_env()`) rather than over `kubectl`. Those port-forwards
   are established by the Layer-7 harness and were not running in the shell used here, so the four
   fail with connection errors. They touch none of this diff: my edits to that file are confined to
   the two credential-leak tests and the new `assert_pods_exist()` helper, neither of which any of the
   four calls. **Verified rather than asserted** — stashed the diff, re-ran at HEAD, got the identical
   4-failed / 3-passed split with the same four test names. A Gate-3 reader should not need to
   re-derive this, and should not read them as regressions.

---

## Gate 2 — Validation (Lead record)

Run as a **headless/unattended caller**, so `DEVLOOP_FAIL_FAST=0` — **all seven layers evaluated,
nothing `NOT-RUN`**. The authority verdict is the `TOTAL_RESULT=` line, not any single layer.

### Attempt 1 — FAIL (2 of 3 attempts remain)

```
LAYER=1 RESULT=OK    LAYER=2 RESULT=FAIL   LAYER=3 RESULT=FAIL   LAYER=4 RESULT=N/A
LAYER=5 RESULT=OK    LAYER=6 RESULT=N/A    LAYER=7 RESULT=OK
TOTAL_RESULT=FAIL  TOTAL_DURATION=838
```

- **Layer 2** `STATUS=FAIL REASON=cargo-fmt-failed` — a doubled blank line before the `NAMESPACE`
  doc comment in `00_cluster_health.rs`.
- **Layer 3** `STATUS=FAIL REASON=guard-violations` — `validate-todo-tracking`:
  `main.md:521 [inline_debt_body] tech-debt body inlined; use pointer bullets instead`. 39/40 guards
  passed. Real duplication, not a formatting nit: each deferral's justification existed both in
  main.md's §Accepted Deferrals table and in `docs/TODO.md`, with nothing keeping them in step —
  **the exact mechanism this task exists to remove, committed inside the document whose §Planning
  restates the rule forbidding it.** Fixed by converting to one-line pointer bullets.
- Both are `FAIL` (exit 1) → **implementer lane, one attempt consumed**. No `PRECONDITION_FAILURE`
  at any layer, so nothing routed to the operator lane.

**Lead error, recorded because it is the same defect class the loop was studying.** On reading the
first red I told the implementer *"Layer 3 is RED on one guard. Everything else so far is green"* —
Layer 2 had already failed and I had not read far enough. A partial read reported as a complete one.
Corrected in the next message with the full layer table. The implementer's framing is the right
generalisation: **a signal that is silent about its own coverage reads as clean** — the same shape as
the credential-leak tests passing on an empty result set, and as the line-number list that looked
healthy because its count happened to be right. Running `DEVLOOP_FAIL_FAST=0` is what made the
coverage explicit and is why the second red was visible at all.

### Attempt 2 — PASS (authoritative)

```
LAYER=1 RESULT=OK   DURATION=2     LAYER=2 RESULT=OK   DURATION=1
LAYER=3 RESULT=OK   DURATION=48    LAYER=4 RESULT=N/A  DURATION=181
LAYER=5 RESULT=OK   DURATION=1     LAYER=6 RESULT=N/A  DURATION=2
LAYER=7 RESULT=OK   DURATION=477
TOTAL_RESULT=N/A  TOTAL_DURATION=712   (wrapper exit 0; zero FAIL at any layer)
```

Grep for `STATUS=FAIL|PRECONDITION|UNKNOWN` across the whole run: **none**.

**Layers 4 and 6 are the documented self-justifying gaps**, not unmeasured work: `test-aggregate-na`
and `audit-aggregate-na` from proto's registered intentional-gap placeholders, plus Layer 6's
dep-manifest gate reporting `SKIPPED-NO-DIFF REASON=no-dep-changes` (no dependency manifest touched).
`cargo test` and `nx test` both passed inside Layer 4; `cargo audit` and `buf breaking` both passed
inside Layer 6. Per ADR-0033 §6 the wrapper's own `REASON=` is the justification.

**Layer 7 `RESULT=OK` on the first attempt, against the landed manifests** — both Phase-2 suites
green (`env-tests-passed`, then `browser-e2e-passed`, 8 Playwright specs). This is the expensive
signal: the ConfigMap keys, the kustomize replacement and the rolled pods survive the full
integration suite, not only the implementer's targeted run. It also independently exercises what
@operations' R3 correction protected — under a bare `apply -k` the advertise addresses would have
been loopback and the browser E2E would have failed at the media step.

**Lead-side verification, independent of the implementer's own artifacts:** Layer-B classification
guard re-run against the final 10-row table (`STATUS=OK
REASON=cross-boundary-classification-clean-1-files`), `cargo fmt --all -- --check` clean, and
`git status` matching the ratified changeset exactly — 8 modified, plus the new env-test and this
output directory.


---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | **RESOLVED-DEFERRED** | 8 | 6 | 1 | +1 withdrawn by the reviewer (S3) as net-negative |
| Test | **RESOLVED-DEFERRED** | 1 | 1 | 0 | +1 accepted spin-out (named probe class) |
| Observability | **RESOLVED-FIXED** | 3 | 3 | 0 | all three traced to their own Gate-1 wording |
| Code Quality | **RESOLVED-FIXED** | 1 | 1 | 0 | ADR Compliance + Ownership Lens sections present |
| DRY | **RESOLVED-DEFERRED** | 4 | 3 | 1 | kubectl-helper extraction deferred, tracked |
| Operations | **RESOLVED-DEFERRED** | 7 | 6 | 1 | 3 tracked `docs/TODO.md` entries |
| Semantic Guard | **RESOLVED-FIXED** | 2 | 2 | 0 | both `[meaning-drift-in-comments]` |
| *Media-Handler (pairing)* | *pairing confirmed* | — | — | — | *not a Gate-3 reviewer* |

**No ESCALATED verdicts. Four reviewers landed RESOLVED-DEFERRED** — surfaced here rather than
averaged away: accepted deferrals remain in the diff and are a cost shift to future work. Each is a
pointer bullet under §Accepted Deferrals with a named owner and a `docs/TODO.md` home.

### Final pipeline (post-review, authoritative)

```
LAYER=1 OK(1)  LAYER=2 OK(2)  LAYER=3 OK(47)  LAYER=4 N/A(168)
LAYER=5 OK(2)  LAYER=6 N/A(1) LAYER=7 OK(471)
TOTAL_RESULT=N/A  TOTAL_DURATION=692  wrapper exit 0
```

Grep for `STATUS=FAIL|PRECONDITION|UNKNOWN` across the run: **none**. Layer 7 green on both Phase-2
suites (`env-tests-passed`, `browser-e2e-passed`).

### Ruling 7 — the 11th file, `crates/env-tests/src/lib.rs`: RATIFIED

The changeset was closed at 10 (Ruling 5). This clears it because **it is not accretion — it is the
only possible home for a fix to duplication this commit created.** @dry-reviewer found `NAMESPACE`
defined twice across two files both new-or-newly-edited here; Rust integration-test files are
separate crates and cannot import one another, so the crate root is the sole shared home. A closed
changeset that forced a valid reviewer finding into deferral on a technicality would be the rule
defeating its own purpose. The distinction that generalises: **closure blocks adjacent findings, not
findings that cannot be satisfied without the file.**

Two things make it trustworthy rather than merely arguable: the **scope-drift guard flagged it before
the implementer signalled** (`scope_drift_inbound crates/env-tests/src/lib.rs`), and the implementer
did **not** get to self-classify in their own favour — they proposed `Mine`; @security *upgraded* it
to Minor-judgment / owner security on their own by-test reasoning, with a §6.4 analysis confirming it
is not a Guarded Shared Area. Classification table: **11 rows**.

### Gate-3 findings worth the record

- **@security S7** — the fix for S5 *re-armed the defect it fixed*. `assert_pods_exist` hoisted the
  namespace to a const but left the selector as four literals, taking that value from two copies to
  four **in the commit that added the guard**. Proved by reproduction, not argument: two
  credential-leak controls green against zero pods **with the guard in place**.
- **@security S8** — the three properties making the `"0"` sentinel fail-*closed* (strict parse,
  validate-before-compute, `saturating_sub`) existed **only in this session's transcript**. Found by
  the reviewer auditing whether their own S3 *withdrawal* had left a hole. This is Ruling 3's own
  principle applied unevenly inside one loop: S1 got tracked work, S3's dependency got nothing,
  because a withdrawn finding *feels* closed. **Resolution feels like closure and isn't.**
- **@operations Gate-3** — half of R2 never landed. The sentinel comment documented the *derived* end
  of the derivation; the *source* end (`terminationGracePeriodSeconds` itself) was bare, leaving the
  operator about to change it with no signal. That inverted the argument the R2 split rested on.
- **@test** — `fetch_pod`'s `live[0]` survived the `deletionTimestamp` filter but not the
  RollingUpdate **surge** window (maxSurge=1/maxUnavailable=0 at 1 replica → two pods match and
  neither is marked for deletion). Fixed by newest-by-`creationTimestamp`.
- **The scrape-interval finding, and the pushback on it.** @code-reviewer filed the comment's "15 s"
  as a factual error citing `prometheus.yml:44` (5 s). The implementer sent evidence rather than
  complying: that file is compose-only and never deployed; the deployed config inherits `global: 15s`.
  @code-reviewer verified and agreed. **The misfiling is the finding's strongest evidence** — it
  converts "a reader might be misled" into "a careful reviewer was, during review" — which is why the
  resolution was *neither number*: the argument holds at any plausible cadence, so the figure was
  decorative. Recorded as evidence, not as anyone's error.

---


---

## Accepted Deferrals

Pointer bullets only — each body lives in `docs/TODO.md`, which is the durable, decay-tracked home. Nothing here restates a justification that lives there.

- Port SSoT for per-instance services (~28 sites; step 1 is a compare-only drift guard) — **infrastructure** — `docs/TODO.md` § Port Constant Scattering (Kind / K8s / Env-Tests)
- `MH_MAX_CONNECTIONS` chosen but not memory-derived; receive-side flow control undeclared — **media-handler (task #12)** — `docs/TODO.md` § Port Constant Scattering (Kind / K8s / Env-Tests)
- GC / MC / AC drain windows unrelated to their grace periods; `terminationGracePeriodSeconds: 35` is one constant with six encodings — **global-controller / meeting-controller / auth-controller** — `docs/TODO.md` § Cross-Service Duplication (DRY)
- AC's shutdown comments cite ADR-0012 for a drain period that ADR does not contain — **auth-controller** — `docs/TODO.md` § Documentation Hygiene
- Story text asserts a kustomize verification against v5.7.1, a version this tree does not run — **story close** — `docs/TODO.md` § Documentation Hygiene
- `mh-incident-response.md` systemic staleness (~35 unrunnable `deployment/mh-service` commands) — **operations** — `docs/TODO.md` § Documentation Hygiene

### Devloop-local scope notes (not deferrals — pointers *away* from work)

These stay here rather than moving to `docs/TODO.md`: they exist to stop someone redoing work this commit already did.

- `mh-incident-response.md` **Scenario 14's Option B block and its recovery-time estimate are FIXED HERE** — the spun-out entry excludes them; do not go looking. (Named, not numbered: @operations' correction to that entry showed a line list going stale *inside the commit that created it*.)
- Scenario 5's Impact / Immediate Response / Common Root Causes / Recovery / cert-inspection blocks are likewise fixed here.
- `cert-manager` hit accounting for that file: of the original 7, **3 are now ours-and-correct** (Scenario 5's Common Root Cause 2 and Recovery block, Scenario 14's Option B), **2 are gone**, **2 spin out** (the *Kubernetes / cloud* dependency line and Scenario 14's Common Root Cause 1 — descriptive mislabels, not destructive instructions). Re-derive with `grep -n cert-manager docs/runbooks/mh-incident-response.md`.
- Option B ends up carrying **no command at all**: it points at F7 and stops, because F7 already owns the rolling restart (see Issues §6). Options A/C/D in the same block still say `deployment/mh-service` — that is the loud-vs-silent boundary holding, not an oversight.

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `d756ed6`
2. Review all changes: `git diff d756ed6..HEAD`
3. Soft reset (preserves changes): `git reset --soft d756ed6`
4. Hard reset (clean revert): `git reset --hard d756ed6`
5. Cluster side — **`infra/devloop/dev-cluster deploy mh`**, never a bare `kubectl apply -k` (it would
   revert the advertise-address ConfigMaps). Or, when the tree is fine and only the cluster needs
   backing out: `kubectl rollout undo deployment/mh-0 -n dark-tower` and likewise `mh-1`.
6. No schema change, no migration, no data change — MTTR is one rollout in either direction.

---

## Issues Encountered & Resolutions

**1. My verification step would have silently broken the cluster** (@operations R3). The plan's
`kubectl apply -k infra/kubernetes/overlays/kind/services/mh-service/` reverts the live
advertise-address ConfigMaps: checked-in values are `127.0.0.1:4434/4436`, but the live cluster carries
`10.255.255.254:24422/24425`, patched **after** `apply -k` by `setup.sh:1211-1217`. The failure is
invisible — `apply -k` exits 0, `rollout status` greens, and my own env-test passes because it reads
grace, not advertise addresses — so every signal I planned to trust would have reported success while
GC handed loopback to every client, surfacing much later in another task's Layer-7 run. Resolution:
`dev-cluster deploy mh` + two post-roll checks, both in Verification step 4.

**2. Two dead security controls, found while routing around them** (@security S5). `00_cluster_health.rs`
queried `-n default`; nothing runs there, so `test_secrets_not_in_env_vars` and `test_secrets_not_in_logs`
— the suite's only two runtime credential-leak controls — asserted `!"".contains("password")` and passed
unconditionally. My plan spotted the wrong namespace and walked past it, which is the suspicious-deferral
profile. Resolution (@main Ruling 2): namespace fix **plus** an `assert_pods_exist()` guard that panics on
an empty pod set, so the next namespace or label drift fails loudly instead of re-disarming the controls.
**They were green before and are green now — but before they were green on nothing.**

**3. An ACKed verbatim block that would have made an incident worse.** @operations' ACKed Scenario 5
Recovery block carried `# cert-manager recreates` forward from the original. There is no cert-manager in
this system (`grep -rln cert-manager infra/ scripts/` → zero hits; no namespace, no CRDs;
`setup.sh:1185` creates the Secret imperatively). `kubectl delete secret mh-service-tls` deletes the
**only** copy; both pods mount it at `/etc/mh-tls` and would fail to mount, and the runbook then
instructs a `rollout restart` that guarantees the restart lands on pods that cannot start — converting a
*partial* rejection incident into a *total* MH outage. I stopped and asked rather than applying a block I
had disproved. @operations amended, and correctly rejected my first replacement too: it rotated MH's leaf
alone, but `generate-dev-certs.sh` regenerates **both** MC and MH leaves, so it would have left MC
serving a leaf whose fingerprint no longer matched `fingerprints.json` — the same defect shape I had just
caught. Final text points at `client-dev-local.md` F7 (the single documented sequence) rather than
restating it. **Anchor verified by slug, not by guess**: `#f7--browser-refuses-the-mcmh-webtransport-handshake`
— the em dash produces a double hyphen, exactly as the tree's existing
`#scenario-13-registermeeting-timeout--clients-kicked` precedent shows; the `#f7` short form would have
dead-ended at the moment of use.

**4. Runbook staleness is systemic, not confined to Scenario 5.** While applying P8 I found ~35 further
`deployment/mh-service` references file-wide in `mh-incident-response.md` (including a second
a second `kubectl scale --replicas=5`, and further cert-manager claims in Scenario 14). There is no
`mh-service` Deployment — only `mh-0` and `mh-1` — so those commands `NotFound`. This is **pre-existing
and not caused by this commit**, so it fails the "this commit makes it wrong" bar that justified fixing
Scenario 5, and it is ~35 sites in another specialist's file. Surfaced to @operations rather than
silently fixed or silently ignored.

**6. A duplicated step that was also an incomplete one.** @operations' first ACKed Scenario 14 text —
and the version I had already landed — ended Option B with
`kubectl rollout restart deployment/mh-0 deployment/mh-1`. They then revised it to carry **no command
at all**, on the grounds that F7 already owns that step and restating it violates the same
single-source-of-truth rule they had invoked against my first Scenario 5 attempt. Verifying the claim
rather than just accepting the improvement turned up something neither of us had said: F7's actual
sequence (`client-dev-local.md:812`) is
`kubectl rollout restart deployment/mc-0 deployment/mc-1 deployment/mh-0 deployment/mh-1`. So the
duplicate was not merely redundant — it was **incomplete**, silently dropping MC from a rotation that
regenerates *both* leaves. A responder who ran the Option B line instead of following F7 would have
restarted MH and left MC serving a leaf whose fingerprint no longer matched `fingerprints.json`:
exactly the failure mode the surrounding comment warns about. Pointing rather than restating removed
the whole class. It also dissolved a local inconsistency (Option B naming `mh-0`/`mh-1` while A/C/D
still say `mh-service`) as a side effect rather than by argument.

**5. A latent flake in my own test, caught by running it during a rollout.** `items[0]` on a
`-l instance=mh-N` query can select a **Terminating** old pod, which predates this manifest change and
carries no `MH_TERMINATION_GRACE_SECONDS` at all — a false negative on the Layer-7 retry path, the same
stale-pod class already documented in `docs/TODO.md` for the counter-delta helpers. Resolution: filter on
`metadata.deletionTimestamp` (the precise test — "the API server has accepted this pod's deletion" —
rather than on phase), and assert loudly if *every* matching pod is terminating rather than passing
vacuously.

---

## Lessons Learned

1. **An owner's ACK is not evidence — and the direction of the catches is the finding, not the
   count.** Four owner-ACKed blocks were landed-ready and wrong. **Three entered through @operations'
   own approved text and were caught downstream of the ACK** — the `cert-manager recreates` line, the
   bare `#f7` anchor, and Option B's rollout restart (which proved *incomplete*, not merely redundant).
   **One ran the other way**: my first replacement for the cert-manager block rotated MH's leaf alone
   and would have broken MC, and @operations caught it. The rollout-restart defect is the sharpest of
   the four precisely because it is *the same defect a second time*: @operations' `mh-0 mh-1` line
   would have left MC on a stale leaf — the identical MC-breaking shape they had just caught in my
   draft — **reintroduced while fixing mine** (their own observation, on reading my verification).
   Catching a defect does not inoculate you against committing it three messages later. So this is not "reviewer catches implementer's
   errors", which is ordinary review working. It is **review working against the owner's own approval,
   in both directions** — the reviewer who ACKed the first block also caught my bad replacement for it.
   Verifying a claim I was explicitly invited to trust is what stopped a runbook from turning a partial
   outage into a total one. "The owner approved it" is a routing fact, not a correctness one.
   (Attribution corrected at @operations' insistence after I had credited all four to them; the
   inverted tally would have made this read as ordinary review. §8 states the general form of which
   this is a special case.)
2. **Spotting a defect and routing around it is a deferral, and deferrals need justification.** My plan
   noticed `00_cluster_health.rs`'s `-n default` and called it "a legacy detail I am not copying." That
   sentence was the whole defect: I had found two dead security controls and written a reason to leave
   them dead. The protocol's suspicious-deferral check exists for precisely that shape.
3. **Restating an instance as a mechanism surfaces siblings, but the honest follow-through is often
   "different mechanism, not more of the same."** My plan claimed the `replacements:` idiom was "the tool
   that would fix" the port literals. It is not — a third of those encodings are written by a shell script
   *after* apply, and no build-time mechanism can reach them. The restatement was still worth doing; the
   wrong part was assuming the class shares one remedy.
4. **A bound can be chosen, defensible, and still fail open.** `500` survived three reviewers as
   "sized to the pod" before anyone did the receive-side arithmetic. What made it defensible was not
   finding a better number — no number works while `receive_window` is unbounded — but writing down the
   arithmetic, naming the missing control, and refusing to lower it to 100 as a mitigation that would have
   *looked* more careful while fixing nothing.
5. **In a stale document, adjacency is not evidence** (@operations' phrasing; it goes into their
   verdict and the operations INDEX at story close). Text sitting beside text you have just verified
   is not thereby verified. It cost two rounds and two people to hit the same trap twice — me carrying
   @operations' `cert-manager` line forward into a block I had otherwise checked, then @operations
   carrying the surrounding block forward into a fix that was otherwise correct. The corollary that
   made the second catch cheap: when you find one instance of a stale claim, **grep the whole file for
   it** rather than trusting the blast radius you assumed. The mechanism behind all four catches,
   though, was narrower than the maxim: an owner's ACK routes a change, it does not verify it — see
   §1 for the per-defect attribution, which runs in both directions and is the part worth carrying.
6. **"Fix it here" vs "spin it out" is decided by failure mode, not by size or proximity.** The
   ~35 stale `deployment/mh-service` commands and the destructive `kubectl delete secret` were the same
   age, the same file, the same root cause — and split cleanly on one question: *does it fail loudly, or
   does it succeed and lie?* A `NotFound` costs a minute. A delete that succeeds destroys the only copy
   of MH's TLS identity and then the runbook tells you to wait for a recovery that never comes. That
   test, not the line count, is what put four lines in scope and thirty-five out.
7. **I committed the exact defect this task exists to remove, in the document that restates the rule
   forbidding it.** `validate-todo-tracking` went red at Gate 2: §Accepted Deferrals carried the full
   justification body for each deferral *inline*, while the same substance also lived in the
   `docs/TODO.md` entries. Two encodings of one justification, nothing keeping them in step — in a
   main.md whose §Planning opens by restating the mechanism as *"where that same quantity is already
   encoded elsewhere, the second encoding must be derived, not retyped."* The contract (SKILL.md Step 9)
   puts the body in `docs/TODO.md` and a one-line pointer in main.md precisely so the pointer can dangle
   loudly instead of the copy going stale silently. **Knowing the rule well enough to write it down is
   not the same as applying it to your own artifact** — I audited manifests, comments, runbooks and test
   files for duplication and never turned the lens on the record I was writing while doing it. A guard
   caught what six reviewers and I did not, which is the argument for having it.
8. **The rule is not "don't duplicate justifications" — it is: if you write down something the tree
   already knows, say how to re-derive it instead** (@operations' generalisation, and the strongest
   thing this loop produced). The loop generated **nine** instances of one mechanism in a single
   commit, every one self-inflicted. At nine this is not a slip — it is **the default behaviour of
   writing prose about code**, which is why the rule has to be a habit rather than a caution. (a) My §Accepted Deferrals duplicated each justification
   across main.md and `docs/TODO.md` — caught by `validate-todo-tracking`. (b) The runbook-staleness
   entry I filed enumerated 41 line numbers; @operations re-derived them and found **16 already shifted
   and 10 sites missing** — the list went stale *inside the commit that created it*, because the
   Scenario 14 hunk was net-longer than what it replaced. Worst of all it cited `:914`, which by then
   was a line **inside our own fix**, so a reader working the list top-down would have been pointed at
   re-breaking it. (c) Auditing my own citations after that correction, `mh-0-deployment.yaml:163` (the
   `/etc/mh-tls` mount) had moved to `:217` — **my own edit shifted it 54 lines** in the same commit.
   (d) Four `:925` references in main.md pointed at the recovery-time estimate, which the Option B
   revision had pushed to `:933`. (e) My commit also invalidated a **pre-existing** citation in
   @observability's D8 entry (`dashboard-conventions.md:418` → `:420`), which nothing would have caught.
   @operations then applied the rule to the entries themselves and cleared three survivors:
   `setup.sh:1185` → `setup.sh:create_mh_tls_secret()`, `service.yaml:28-29` → the "Per-instance
   NodePort Services" comment block, and `config.rs:46` → the `DEFAULT_MAX_CONNECTIONS` const. That
   last one is the sharpest of all nine: entry 1's owner **is task #12**, and task #12's job is
   editing `config.rs` — including deleting the `unwrap_or(DEFAULT_MAX_CONNECTIONS)` that makes `:46`
   meaningful. The citation was guaranteed to be stale **at the exact moment of use, for the exact
   person it was written for**. And the ninth was mine: the `MH_MAX_STREAMS` ConfigMap comment cited
   `gc_client.rs:140 and :301`; those are inside `register()` and `attempt_reregistration()`, so
   naming them tells a reader *when* the value goes on the wire — first registration and again after
   any re-registration — which two line numbers never did.

   The ninth also exposes a **second-order** version of the D8 case, and it is the one most likely to
   recur (@operations' observation): those `gc_client.rs` line numbers had been **reviewed and
   approved** — @security asked for the citations specifically. But the question a reviewer asks of a
   citation is *"does this reference exist and is it right?"*, not *"will it survive an edit to a file
   nobody on this panel is watching?"* **Correctness review does not confer durability**; they are
   different properties and nobody was assigned the second one. That is also why the sweep had to go
   past prose into YAML comments: the rule as stated was about documents, and a ConfigMap comment
   citing Rust line numbers is the same defect somewhere neither of us had been looking.

   All nine are now named rather than numbered. The recurring bonus is that **the named form is
   better on its own merits and drift-resistance is incidental**: `setup.sh:1185` actually pointed at
   a `log_step` line rather than the `kubectl create secret tls` two lines below it, so the symbol
   name was also the more *accurate* reference. A name survives an edit; a line number does not; and
   the name usually says something the number could not. **The tell was available
   in advance and I walked past it**: I had already told @dry-reviewer the class was "greppable as
   `deployment/mh-service`", and then enumerated anyway. Knowing the re-derivation exists is the moment
   to write it down *instead of* the snapshot, not alongside it.
9. **The defect recurred inside my own fix for it, and the fix made it worse before it made it
   better** (@security S7, with a live reproduction). `assert_pods_exist()` was added to stop the
   credential-leak probes passing on an empty result set. I hoisted the *namespace* to a `const` so
   guard and probe could not disagree — and left the *selector* as four separate literals, so the
   hunk **increased** the duplication of the exact value the guard exists to police, from two copies
   to four. The drift is asymmetric and only one direction is safe: a stale guard queries nothing and
   panics, but a stale **probe** leaves the guard passing on the old selector while the probe samples
   an empty set and asserts `!"".contains("password")` — the original vacuous green, restored, with
   the guard sitting right there. @security reproduced it against the live cluster: both controls
   green against zero pods. Fixed by making guard and probe **the same expression**, not the same
   spelling. I verified the closure the same way — the identical drift now fails both tests loudly.
   The lesson is not "I missed one." It is that I applied the derive-don't-restate rule to one value
   in a hunk and not to the value beside it, which is how a fix ends up certifying the thing it was
   written to prevent.
10. **The re-derive rule applies to values, not just to locations** (@observability and @operations,
   converging). Nine instances this loop were pointers to *places* — line numbers, symbol positions.
   The tenth was a *value*: my datagram comment asserted "MH is scraped at 15 s". That number is a
   config literal, and the tree encodes **two contradicting answers** — the deployed k8s config has no
   per-job override so MH inherits the 15 s global, while `infra/docker/prometheus/prometheus.yml`
   (local compose only, never deployed) sets 5 s specifically *because* MH is the media service. So
   the claim was unverifiable to a reader today, not merely stale on a schedule. **@code-reviewer
   filed it as a factual error and cited the 5 s file**, which is the local-only one — an
   understandable read, and itself evidence for the finding: if a careful reviewer opens the nearest
   Prometheus config and reaches the wrong conclusion, a comment restating either number is a trap.
   The resolution was neither number: the argument (scrape interval >> millisecond fill/drain, so
   counters are authoritative and gauges cannot carry an alert alone) holds at any plausible cadence,
   so the figure was decorative. **The cheapest form of re-derivability is not restating the value at
   all.**
11. **A withdrawn objection can leave its premise unrecorded** (@security S8). They withdrew S3 on
   the merits — my argument that a non-numeric sentinel fails *worse* under `unwrap_or` was correct.
   But the reason `"0"` is the better sentinel is that it stays loud **given three properties of
   task #12's parse**: strict parsing, validation before the drain computation, and `saturating_sub`.
   Strip any one and `"0"` is the fail-*open* case — a zero drain, or an unsigned wrap to ~1.8e19
   seconds. Those three lived **only in this transcript**: `grep saturating` over `docs/TODO.md`, main.md
   and the manifests returned nothing, and main.md's only `grace > MARGIN` mention was original plan
   text predating the exchange. So a design choice in the shipped manifests rested on a verbal
   commitment. The sharp part is that this is **Ruling 3's own principle applied unevenly inside one
   loop**: the Lead had ruled that a comment records a decision while only a TODO entry is tracked
   work, and *"main.md for task #9 is no more visible to #12's reviewers than this transcript"* — then
   S1 got a thorough entry and S3's preconditions got nothing, because a withdrawn finding feels
   closed. **Resolution feels like closure and isn't**: the argument was settled, the dependency it
   created was not.
12. **Test the test against the failure it is supposed to catch.** Running the new env-test during an
   actual rollout — rather than after the cluster settled — is what exposed the Terminating-pod flake.
   A green test on a quiet cluster proves less than it appears to.
