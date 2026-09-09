# Devloop Output: Media-Path Runbook Scenarios, Alerts, and Prometheus Rule Loading

**Date**: 2026-09-09
**Task**: ADR-0036 story-1 media-path runbook scenarios + three alerts + structural fix so Prometheus actually loads alert rules (story task #21, incl. task-24 scope)
**Specialist**: operations
**Mode**: Agent Teams (v2)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `54b18a963dde59fe60af8a95df7801dc2a514118` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Headless | `DEVLOOP_HEADLESS=1` (run-story task #21) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `operations` (complete) |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | RESOLVED-FIXED |
| Test | RESOLVED-FIXED |
| Observability | RESOLVED-DEFERRED |
| Code Quality | RESOLVED-FIXED |
| DRY | RESOLVED-DEFERRED |
| Operations | (implementer) |
| Semantic Guard | CLEAR |
| Infrastructure (conditional) | RESOLVED-DEFERRED |

---

## Task Overview

### Objective
Full text of the task is `/tmp/devloop/story-runner/2026-08-27-hear-yourself-through-handler/task-21.prompt` (reproduced verbatim in §Task Prompt below).

### Scope
- **Service(s)**: MC + MH runbooks, client dev-local runbook, Prometheus rules/config, dt-guard, env-tests
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — governing design is ADR-0036 §11; this is documentation + alert-rule + infra-wiring implementation of an existing decision.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. Every file this task intends to touch has a row. Classification is
`Mine` / `Not mine — Mechanical` / `Not mine — Minor-judgment` / `Not mine — Domain-judgment`.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/mc-incident-response.md` | **Mine** — Scenario 15 (generation divergence), Scenario 16 (missing key material), the non-numbered "Heap and Core Dumps Contain Live Meeting KEKs" subsection, TOC rows, and stop-pointers inside Scenario 2 and Scenario 7 | — (ADR-0011 §Documentation Ownership: `docs/runbooks/` → operations) |
| `docs/runbooks/mh-incident-response.md` | **Mine** — Scenario **17** (see §P0) + TOC row | — |
| `docs/runbooks/mh-deployment.md` | **Mine** — `## Rollout With Media Flowing` + TOC + References | — |
| `docs/runbooks/client-dev-local.md` | **Mine** — §4.5 media triage ladder, §5 entries F12–F15, TOC | — |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | **Mine** — `MCMediaGenerationDivergence` (page), `MCMediaMissingKeyMaterial` (warning) | — (ADR-0011:41 — alert **rule files** are operations-owned; ADR-0031's cross-cutting review still applies) |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | **Mine** — `MHMediaEgressQueueOverflowRate` (warning) | — |
| `infra/docker/prometheus/rules/_template-service-alerts.yaml` | **Mine to place, @observability-drafted** — the header credited the `_` prefix; under `[a-z]*-alerts.yaml` the operative property is *does not begin with a lowercase letter*. Accidentally true, instructively wrong, and my change makes it load-bearing. | **observability** (wording) |
| `docs/observability/alerts.md` | **Not mine — Domain-judgment; paired route (ADR-0024 §6.3).** I wrote the three inventory entries with byte-identical PromQL; @observability wrote the prose amendments their falsified statements needed (`alerts.md:12`, the MC page-set enumeration, the MH "documents none of it" note) and the three GC selector corrections. | **observability** |
| `docs/observability/metrics/client.md` | **Not mine — @observability implements** — a blockquote at the head of the client-metric catalog stating that not one metric in it is queryable from Prometheus today. A sixth statement site for §P3a, in the place a reader goes to decide whether a signal is available *before* building on it. | **observability** |
| `docs/observability/alert-conventions.md` | **Not mine — @observability implements.** Their §Current coverage argument stays true after my change but for a different and now-real reason. | **observability** |
| `infra/docker/prometheus/prometheus.yml` | **Not mine — Minor-judgment** — enable `rule_files` with the correct glob (the commented stub named the wrong directory *and* the wrong extension) | **infrastructure** |
| `infra/kubernetes/observability/prometheus.yml` | **Not mine — Domain-judgment; NEW FILE** — the authoritative server config, extracted from the inline ConfigMap so it is hashed and rolls the pod, and so guards parse a real YAML file rather than YAML embedded in a `data:` block (§P3d) | **infrastructure** |
| `infra/kubernetes/observability/prometheus-config.yaml` | **Not mine — Domain-judgment** — inline config removed, rules volume + volumeMount added, and **`--web.enable-lifecycle` deleted** (dead config; NodePort 30090 → hostPort 9090 makes `POST /-/quit` host-reachable, and rule evaluation turns that into a **detection** outage) | **infrastructure** |
| `infra/kubernetes/observability/kustomization.yaml` | **Not mine — Domain-judgment** — `configMapGenerator` for the server config, with a per-entry `namespace:` that is load-bearing for the name-hash rewrite | **infrastructure** |
| `infra/docker/prometheus/kustomization.yaml` | **Not mine — Domain-judgment; NEW FILE** — generates the rules ConfigMap from the single on-disk copy. Lives here, not under `observability/`, because kustomize refuses file sources outside its own directory. | **infrastructure** |
| `docker-compose.yml` | **Not mine — Minor-judgment** — read-only bind mount of the rules directory at the same container path, so one glob string is valid in both deployments | **infrastructure** |
| `crates/dt-guard/src/alert_rules.rs` | **Not mine — Domain-judgment** — two new `rule_id`s inside the existing subcommand (no 36th subcommand, ADR-0034 sprawl threshold): `rule_file_loading` and `inventory_expr_drift`, plus the `loadable_rules_files()` predicate and its fixtures. Guard **machinery** per CLAUDE.md §Guard-crate ownership. | **infrastructure** (machinery); **test** on fixture adequacy |
| `crates/dt-guard/src/kustomize.rs` | **Not mine — Domain-judgment** — generalise `extract_declared_dashboards` into `extract_declared_generator_files(content, suffix)` so the rules check shares R-20's twice-repaired parser instead of re-earning its inline-comment and fail-open bugs | **infrastructure** |
| `crates/dt-guard/src/infrastructure_metrics.rs` | **Not mine — Domain-judgment; UNPLANNED, caught by the guard itself** — it read the server config out of `prometheus-config.yaml`'s `data["prometheus.yml"]`, which the extraction removed. It went red immediately with an empty valid-label set. Repointed at the extracted file, and the four silent `else { continue }` arms replaced by a hard bail, so the next such move cannot get as far as validating against an empty schema. | **infrastructure** |
| `crates/env-tests/src/fixtures/alert_rules_loaded.rs` | **Not mine — Domain-judgment; NEW FILE** — the two extraction kernels (YAML `alert:` names, `/api/v1/rules` JSON) with FIRE fixtures in the always-on lane | **test** |
| `crates/env-tests/tests/33_alert_rules_loaded.rs` | **Not mine — Domain-judgment; NEW FILE** — the live-cluster assertion, with the scope-boundary doc comment | **test** |
| `crates/env-tests/src/fixtures/metrics.rs` | **Not mine — Domain-judgment** — `PrometheusClient::rules()`; reuse, not a third hand-rolled `/api/v1/*` call site | **test** |
| `crates/env-tests/src/fixtures/mod.rs` | **Not mine — Mechanical** — module registration | **test** |
| `crates/env-tests/Cargo.toml` | **Not mine — Minor-judgment** — `serde_norway`, so the on-disk rule files are parsed structurally rather than line-grepped | **test** |
| `docs/TODO.md` | **Mixed.** **Mine**: narrowing the alerting-chain entry to breaks 2–4, rewriting the inventory-drift entry with the `d3c30f10` mechanism, narrowing the alert-name-resolution entry to non-heading references, and filing the two new entries. **Not mine — Minor-judgment**: correcting @security's S6 assertion that no Prometheus evaluates any rule (owner **security**), and the collector-export entry, which @observability owns and which I filed rather than leave a required statement site empty — hand it to them to amend. | — / **security** / **observability** |
| `docs/devloop-outputs/2026-09-09-media-path-runbooks-and-alerts/main.md` | **Mine** | — |

**Not touched, deliberately**: `infra/docker/prometheus/rules/{gc,otel}-alerts.yaml`; all Grafana dashboard JSON; any service `metrics.rs` (every metric this task references already exists in code); `infra/services/otel-collector/configmap.yaml` and a collector scrape job (§P3a — **deferred, not edited**; no hunk in this diff, tracked in the new `docs/TODO.md` entry); `scripts/layer3.sh` (the guard landed as `alert_rules.rs` rule_ids, which `run-guards.sh` auto-discovers, so there is no self-test script to register).

---

## Planning

### P-RULINGS. Gate-1 outcome — @team-lead approved with eight rulings, plus reviewer conditions

Recorded here because several rulings **change** what §P0–§P5 below said when they were drafted;
where they conflict, this section wins.

| # | Ruling | Effect |
|---|---|---|
| 1 | **P0 APPROVED — `### Scenario 17: Media Datagram Drop`**, anchor `#scenario-17-media-datagram-drop`. Lead verified independently. | Record the deviation + evidence in §Issues Encountered. |
| 2 | **P3 APPROVED — wire both configs; K8s is load-bearing.** Plus: the commented docker stub reads `rule_files: - "alerts/*.yml"` — **wrong directory *and* wrong extension**. Do **not** uncomment it as-is. | Had it been uncommented verbatim it would have loaded nothing and *looked fixed* — the same defect one layer down. |
| 3 | **Template: do NOT move or rename. Define the loadable-rules-file predicate ONCE, in `alert_rules.rs` beside `ALERTS_SUBDIR`; every consumer derives from it.** Where a consumer structurally cannot derive (kustomize cannot glob), the guard enforces **set equality** and **names which direction diverged**. Fix the template's header sentence (observability implements). | @security and @infrastructure both independently ruled for my option over @security's own stronger-in-isolation alternative, for the same reason: moving it strands `alert_rules.rs:413`'s skip with no scope — a control that can never fire. |
| 4 | **P3a — `MCMediaMissingKeyMaterial` LANDS NOW, marked premise-unmet.** Lead overruled @observability's and @infrastructure's recommendation to defer it. The five-site honesty treatment is **the requirement, not a preference**; neither reviewer is asked to sign a green applies row. | Lead's reasoning, recorded because both sides argued it well: §11's line is about controls that are *silently* dead; an absence written in five places — one of them the operator-facing inventory — is the "absence that gets noticed" the ADR prefers, and deferral would move the authored expression and its fire/apply analysis out of the inventory an operator reads and into a TODO file they don't. |
| 5 | **Env-test scope-boundary doc comment REQUIRED, not contingent on #4.** | Its green covers a rule that is loaded, evaluating and incapable of ever matching. |
| 6 | **`docs/TODO.md:736` narrowed to breaks 2–4, not deleted; `:972` corrected in the same commit.** | Deleting would silently discharge @security's still-unmet conditional acceptance. |
| 7 | **Q1 CONFIRMED — TODO line, no fourth alert.** | Same entry as the collector wiring (identical trigger). |
| 8 | **Q2 — DEFER the `alerts.md`↔rules byte-identity guard. Second decline, recorded.** Not because the trigger has not fired — it has, twice — but because landing it means first reconciling the 26 existing dual-encoded MC/MH entries this task is told not to retro-fill, and **an allow-list scoped to three names is itself a second membership set** — the exact hazard ruling #3 forbids. | Update `docs/TODO.md:151` to record the second decline, name this devloop, and state the **real** blocker (the backfill, not the guard) so the trigger stops firing on devloops that cannot discharge it. Told @dry-reviewer directly. |

**Reviewer conditions accepted on top of the rulings:**

- **@infrastructure — hash suffix VERIFIED, not assumed.** They built a minimal `kubectl kustomize` case and confirmed the nameReference transformer rewrites `spec.template.spec.volumes[].configMap.name`. So: hash suffix, pod rolls, **`--web.enable-lifecycle` deleted**, no `disableNameSuffixHash` fallback.
- **@infrastructure §P3d — the inline-`prometheus.yml` hash asymmetry.** A rules change rolls the pod; a `prometheus.yml`-only change would not, and `setup.sh:597`'s readiness wait would pass instantly against the *old* pod running the *old* config — works on a fresh cluster, fails only on an iterated one. Taking their **preferred** resolution: extract the inline `prometheus.yml` to a real file and generate it too. Both ConfigMaps hashed, both roll — and the guard then parses a real YAML file rather than a YAML string embedded in a `data:` block, which is the same parsing-hazard class as the escaped-JSON trap.
- **@security's condition on ruling #3 — comment BOTH sites as deliberately different predicates reconciled by the guard, not as one rule written twice.** The glob encodes two exclusions (`_` prefix **and** the `-alerts.yaml` suffix); `alert_rules.rs` encodes one (`_template-` prefix, any `.yaml`). They agree on today's four files and diverge on e.g. a future `mc-recording-rules.yaml`, which dt-guard would validate and Prometheus would not load. Without the comment the next author "aligns" them, and the alignment they reach for is widening the glob to `*-alerts.yaml` — which re-admits the template.
- **@test refinement A — positive control asserts the THREE NEW alert names specifically**, not merely a non-empty set. A non-empty set proves the extractor ran; it does **not** prove the new rule group loaded, because the pre-existing gc/mc/mh alerts would carry the pass while a new group silently failed to parse and was tolerated. Does not contradict #4: *loaded and evaluating* is what is asserted, *can fire* is separately disclaimed.
- **@test refinement B — the guard's two sides must be built from DIFFERENT sources or it agrees vacuously.** Expected set = `alert_rules.rs`'s `_template-` skip predicate; covered set = the **actual glob expansion**. If both sides were built from `[a-z]*`, a `Foo-alerts.yaml` — which `alert_rules.rs` *would* lint — is silently excluded by the glob and never flagged. Confirmed as the wiring.
- **@test — lane**: same `#![cfg(feature = "observability")]` gate as `30_observability.rs`. **No `is_available`/skip-on-absent early return** — absent-or-unloaded rules is the exact state this test exists to go red on; a self-skip is vacuity mechanism 5.
- **@observability — Alert 3's fire row splits into labelled halves** (MH = alert coverage; SDK = Scenario 17 runbook coverage), and its applies row **derives from `ConfigError::EgressQueueDoesNotBindFirst`** (`mh-service/src/config.rs:1211`, raised `:1474`) so it is fail-closed rather than confirmatory.
- **@observability — Alert 2's `for: 15m` vs `rate(...[5m])` is a documented anti-shape held deliberately; the reason goes in a comment ON THE RULE.**
- **@observability — one clause in Scenario 15 guarding `no_applied_generation` against the wrong remedy**: MH deliberately does not reject `policy_generation: 0`, because MH-first rejection would register no meeting and kick every client at the registration timeout — §8's opening paragraph.

**@security's Scenario 16 finding — accepted in full; it is the sharpest thing anyone raised.**
The two arms are **asymmetric in instrumentation**, and the plan did not reflect it:

- `no_roster_entry` **has** server-side corroboration: `mc_join_identity_key_presence_total{presence}` —
  the *absent* ratio answers "are clients publishing keys?" directly, and its catalog entry exists
  because *"every client omits the key and nobody notices"* must not be a silent steady state. Its
  neighbour, which is **not** the same condition: a malformed (non-0, non-32-byte) key is refused at
  the trust boundary onto `mc_session_join_failures_total{error_type="identity_key_invalid"}` — *no
  key* vs *bad key*, different remedies, one line so they are not conflated.
- `no_kek_for_generation` has **no MC-side counter and cannot have one as things stand**.
  `mc_meeting_kek_generated_total` increments unconditionally, is identically the meeting-creation
  count, MC has no meeting-creation counter to divide by — its catalog entry says in terms that the
  inference is *not computable even in principle* and that **no alert may be built on the absence of
  this counter moving**.

Put together: on the KEK arm the responder has a client-side counter that (per §P3a) does not reach
Prometheus, no MC-side counter, and no alert that can fire — standing in `mc-incident-response.md`
asking *"is the KEK actually in the meeting actor?"* with no instrument that answers it. **The honest
answer to that question in a running process is a heap dump.** So Scenario 16, not Scenario 2 or 7,
is the likeliest trigger of an improvised dump, because it is the only one where the dump looks like
*diagnosis* rather than *last resort*. Therefore:

1. **Scenario 16's KEK arm routes to the dump subsection's gate**, with the same **stop** framing as
   Scenarios 2 and 7 — not a "see also". A third cross-reference the task did not name, added
   because the reasoning holds.
2. **Scenario 16 states the asymmetry plainly**: the roster arm has server-side corroboration, the
   KEK arm has none, and **the absence of a signal on the KEK arm is not evidence the KEK is
   present.** Otherwise the roster arm's healthy-looking counter reads as covering both.
3. **The KEK arm's remedy names the non-dump path FIRST** — the per-join response-side condition
   (`meeting_kek` not exactly 32 bytes), checkable client-side without touching MC's memory, and
   `dt_client_media_kek_updates_total{source="join_response"}` as the client-side observation of
   delivery. Otherwise the gate is the first thing the responder meets and they walk through it.
4. **Stated at Scenario 16, not only in the dump subsection: no step may print, log, or otherwise
   materialise the KEK to confirm it is present.** ADR-0036 §11 puts KEK and transmit-key material in
   the credential-leak guard's scope for MC logs precisely here, and *"add a temporary log line to
   check the key is there"* is the shape that gets typed under incident pressure. It belongs where
   the responder is.

**@security (a)** — the S6 rewrite **names the cluster config** it is true of
(`infra/kubernetes/observability/prometheus-config.yaml`, authoritative per
`dashboard-conventions.md:249-253`), so a future reader who un-wires the cluster side while leaving
the docker side does not find a sentence that stays green. Preserved **verbatim**: reason (2) — a
forged binding inside `1..=65535` is indistinguishable from a real one at MH, so the counter is the
**ceiling** of detection, not coverage of impersonation — and the bolded **"THERE IS NO WORKING
DETECTOR FOR THIS TODAY."** Discharging (1) is exactly the edit that invites the next reader to
soften (2).

**@security (b)** — the `docs/TODO.md:736` narrowing preserves **verbatim** the closure-condition
sentence: *"if this entry is closed by wiring routing that does NOT distinguish a security receiver,
the severity is the thing to revisit, not the annotation."* Named specifically because that is the
sentence a narrowing edit tends to compress.

**`--web.enable-lifecycle` — conflict RESOLVED; @team-lead reversed their own ruling.** (Left below as the reasoning of record.) The Lead verified independently that line 219 is the only occurrence in the tree and reversed to *delete it, keep classification row 75*, on the ground that "pre-existing" described the **flag**, not the **risk** — rule evaluation is what turns `POST /-/quit` from a metrics gap into a detection outage, so its blast radius is a function of this change. @security's `docs/TODO.md` entry stays but now **records the removal** and names any residual NodePort exposure, rather than deferring the flag. The wiring still must not depend on `/-/reload`: hash-suffixed ConfigMap plus pod restart.


@team-lead ruled it *out of scope, record it in a TODO*. @infrastructure, who **owns the file**,
ruled *delete line 219* — it is dead configuration (they grepped `scripts/`, `infra/`,
`crates/env-tests/`; the only hit is its own declaration), and the exposure is worse than first
stated: `NodePort` 30090 is mapped to `hostPort: 9090` by `kind-config.yaml:45-46`, so
`curl -XPOST http://localhost:9090/-/quit` is **host-reachable**. Both agree on the load-bearing
half — the rules wiring must not depend on `/-/reload` — and it does not. **I am taking the owner's
call and deleting it**, on the review protocol's fix-don't-defer test (one line, inside my existing
changeset, no design ambiguity) and because the severity of that line is a *function of my change*:
today killing Prometheus is a metrics gap someone notices; after rule evaluation it is a **detection
outage**, and a stopped evaluator looks exactly like a quiet one. Flagged to @team-lead, who ruled with the owner.

---

### P0. One deviation from the task text, needing @team-lead's ruling before implementation

**The task says `docs/runbooks/mh-incident-response.md` "currently ends at Scenario 14" and reserves
`### Scenario 15: Media Datagram Drop`. That premise is stale.** Two earlier tasks in *this same
story* already took those numbers:

- `### Scenario 15: Media Sessions Declining — No Sender Binding` (commit `cc56d7fc`, sender_id binding contract)
- `### Scenario 16: Ingress Datagrams Received But Never Read` (commit `94e1be9a`, R-15 receive-path gap)

The reserved-numbers clause ("no other specialist adds numbered scenarios in this story") was
violated before this task started, so it is unsatisfiable as written. Renumbering the incumbents is
not available either: `#scenario-15-media-sessions-declining--no-sender-binding` is the live
`runbook_url` of **three** shipped alerts (`mh-alerts.yaml:206,239,261`), `#scenario-16-ingress-datagrams-received-but-never-read`
of a fourth (`:382`), plus three prose cross-references in `mh-deployment.md` and
`mc-incident-response.md`. Renumbering breaks four Layer-3-guarded links to fix a numbering
aesthetic.

**Proposal: `### Scenario 17: Media Datagram Drop`**, anchor
`docs/runbooks/mh-incident-response.md#scenario-17-media-datagram-drop`.

Verified safe: `grep -rn "scenario-15-media-datagram-drop"` over the whole tree returns **nothing**.
The task's claim that "the anchors are already referenced by alerts written elsewhere in this story"
is true for the two MC anchors and false for this one — the only forward reference to this scenario
is a prose mention with no anchor at `packages/sdk-core/src/config/clientConfig.ts:180`
("recorded as such for the media-datagram-drop runbook scenario"), which does not encode a number.
The two MC anchors (Scenario 15 and 16 in `mc-incident-response.md`) are **free and used as
specified** — that file genuinely ends at Scenario 14.

MH deployment heading `## Rollout With Media Flowing` is free and used verbatim.

**Recorded because it generalises** (@observability): this is a live fourth instance of
`docs/TODO.md:661` — *task manifest prompts are a second encoding of requirements with no mechanical
binding*. The prompt's "currently ends at Scenario 14" was true when the story was written and was
falsified by two tasks **inside the same story**, with nothing binding the two. The entry lists three
instances; this is the first where the stale premise would have produced a **broken artifact** (four
dead `runbook_url` anchors) rather than a wrong opinion. @observability verified the numbers
independently at `mh-incident-response.md:988` and `:1240`.

### P1. Runbook scenarios (write first — the alert guard resolves `runbook_url` to a real file)

**`docs/runbooks/mc-incident-response.md` → `### Scenario 15: Media Generation Divergence`**

Written for the **one-shot** form this story ships. Load-bearing content:

- What the signal is: MC sends a registration generation; MH's response echoes the generation it has
  **applied**, never the highest it received (ADR §8). A mismatch means MH is forwarding under stale
  or absent policy while `up`, readiness, handshake latency, the gRPC call result and the connection
  state all read green — §8's *partial blackhole reporting healthy*.
- **§8's "self-correcting — a lost response is re-asserted on the next tick" is FALSE in this
  story and will be stated as false.** Of §8's four re-fire triggers only *structural change*
  exists; there is no configured cadence, no connectivity-loss trigger and no
  newly-assigned-handler trigger in the tree yet. With one participant the only structural change
  available is a **rejoin**. So the resolution step is *force a structural change*, and the runbook
  will say in terms: **never wait for convergence — nothing will converge.**
- Triage splits on the `outcome` label first: `generation_mismatch` (MH applied an older generation
  — its apply path failed or its mailbox was full) vs `no_applied_generation` (MH echoed nothing —
  either an MH that predates the echo, i.e. a rollout skew, or the apply never ran).
  `transport_mode_mismatch` is called out as **not this scenario** (§8's two-ends-must-agree echo,
  its own failure) and `handler_id_mismatch` as a diagnostic that fires on every ordinary MH restart
  until `MH_HANDLER_ID` is stabilised.
- **One clause guarding against the wrong remedy for `no_applied_generation`** (@observability): the
  mirror-image outcome on MH's side is deliberately **not** symmetric. MH does not reject
  `policy_generation: 0`, because MC and MH roll independently and MH-first rejection would register
  no meeting and kick every client at the registration timeout — which is §8's opening paragraph
  almost verbatim. A responder who reads "rollout skew" and concludes *make MH stricter* would be
  reaching for the exact change ADR-0036 §8 exists to prevent.
- `mc_media_generation_divergence` appears as the **magnitude a responder reads next**, with its
  catalog caveat carried: last-write-wins pod-level, so a healthy push for another meeting erases a
  diverged reading. Never the primary evidence.
- Blast radius: per (meeting, handler). Not fleet-wide. Other meetings on the same MC and MH are
  unaffected, which is why this is diagnosable but not restart-worthy.
- Explicit **do not restart MH** — it clears the symptom, destroys the evidence, and (with no
  cadence) sheds every media session on the pod without recovering the ones that were already dark.

**`docs/runbooks/mc-incident-response.md` → `### Scenario 16: Missing Key Material`**

- Signal: `dt_client_media_frames_dropped_total{reason="no_kek_for_generation"}` and
  `{reason="no_roster_entry"}` — client-side, because MH never opens a frame and structurally
  cannot observe either.
- Both are **expected transients** at join and after a KEK rotation; **the sustained case is the
  signal**, and §11 says it is the *only* signal for a join or rotation path that has silently
  stopped delivering keys.
- **Triage splits on the reason label first, because the two remedies are different:**
  - `no_kek_for_generation` → the client has no meeting KEK for the generation the frame's wrap
    announces. Remedy is in MC's KEK delivery: the join response (`kek_updates_total{source="join_response"}`
    is the only source this story) and the KEK's presence in the meeting actor.
  - `no_roster_entry` → no usable identity key for the frame's `sender_id`, **including the case
    where MC published an empty key**. Remedy is in MC's roster publication path, not key
    generation.
- Neighbours that are *not* this scenario, named so a responder does not mis-route:
  `unwrap_failed` (KEK unwrap → key distribution) vs `decrypt_failed` (SFrame payload → key
  schedule or sender), and `no_transmit_key` (a protocol violation, not a third key reason).
- Lives in MC's runbook because **the alert fires on a client counter but the remedy is in MC's
  KEK-and-roster delivery path**. `client-dev-local.md` will **point at it**, following the in-tree
  precedent at `mc-incident-response.md:2131` ("Do not duplicate them here"), not restate it.

**`docs/runbooks/mh-incident-response.md` → `### Scenario 17: Media Datagram Drop`** (number per P0)

- Audio is one frame per QUIC datagram (§1); drops occur on **both** ends and the two ends have
  different owners.
- **MH side**: the application egress queue bound tripping —
  `mh_media_frames_dropped_total{direction="egress",reason="egress_queue_overflow"}`. This is the
  §1 transport-parameter bound. The text will say plainly that it is **not** a bandwidth budget:
  no egress budget, capacity gauge, stream ceiling or admission threshold exists in this story.
- **SDK sender side**: `dt_client_media_send_dropped_total` — §11's *the one that matters most,
  because MH structurally cannot observe it*. WebTransport exposes no send-side drop event; the SDK
  owns a bounded queue above the transport, makes the drop decision there and counts it.
- **The keepalive case, and its explicit non-discriminator.** In loopback,
  `dt_client_media_frames_sent_total` rising while `dt_client_media_frames_received_total` stays
  flat means audio is not completing its round trip — and that reading has **at least two causes
  with different remedies**: a NAT binding reaped during a mute longer than the keepalive interval,
  and MH holding stale or absent policy (MC Scenario 15). There is no unique client-side
  discriminator, and the runbook will say so rather than implying one.
- **The ladder forks, cheapest first:**

  | Reading | Meaning | Next |
  |---|---|---|
  | `sent` flat | capture or mute-release never resumed | client capture / mute state; not a transport problem |
  | `sent` rising, `received` flat | transmitting into something that is not returning | ↓ |
  | 1 | is the QUIC connection still up? | a reaped binding shows as connection failure or keepalive distress; **MH-not-forwarding leaves a healthy connection** — this one check separates the two causes |
  | 2 | generation-divergence signal (MC Sc 15) | stale/absent policy at MH |
  | 3 | keepalive configuration | interval vs. the mute duration |
  | 4 | packet capture | last rung, and credential-bearing (see below) |

- **Why `received` is trustworthy for exactly one claim**: it counts **at the wire**, before
  verification and decryption, so "a decrypt failure" is distinguishable from "nothing arriving".
  It proves datagrams arrived; it proves nothing about audibility. The post-verification story is
  carried by `dt_client_media_frames_dropped_total{reason}` and the
  `received = accepted + sum(drops by reason)` identity, and the identity holds at the crypto/parse
  boundary, **not** at playback.
- Per @security: the packet-capture rung will state that the artifact is **credential-bearing** (the
  join carries the JWT) and must not be attached to a ticket. No "raise the log level" rung anywhere
  — §11 bars it, because the incident motivating the level change is the incident that produces the
  voice-activity trace. No `by(meeting_id)`, `by(participant)` or `by(sender)` in any triage query;
  aggregation floor is pod or service.

**`docs/runbooks/mh-deployment.md` → `## Rollout With Media Flowing`**

- **MH sheds media sessions on restart, and ADR §11 records that as the decision, not merely the
  current behaviour.** Shutdown marks not-ready, cancels, and sleeps two seconds inside a
  thirty-five second grace period, with no drain phase. v1 keeps it that way: draining means either
  holding a pod open for the length of a meeting or migrating live sessions, and neither is in
  scope.
- **The client-visible consequence, stated so an operator does not diagnose it as a bug or add a
  drain**: every participant on the restarting pod loses audio at the moment the pod goes
  not-ready, and — in *this* story — does not get it back on its own, because the §8 recovery this
  decision leans on (the ≤10 s re-assert) does not exist yet. Recovery is a rejoin. That gap is
  named as the handler-restart story's, not as a defect to fix here.
- **Scaling MH to zero replicas is not a media kill switch.** MH assignment is part of the join
  flow, so zero replicas produces a **join outage** — a strictly wider blast radius than the media
  path it was meant to stop. There is no finer-grained lever.
- **Rollback is redeploy-only.** No schema change, no data change, no feature flag. MTTR is one
  rollout. `unknown_version` staying individually visible on the drop-by-reason counter is the only
  lever for detecting a version-skewed rollback.

**`docs/runbooks/mc-incident-response.md` → non-numbered subsection: heap and core dumps**

Adopting @security's six points wholesale. Placed as a subsection (not a numbered scenario), with
**stop-pointers at the rung where a responder reaches for a dump** inside Scenario 2 (Actor Panics)
and Scenario 7 (Resource Pressure) — not "see also" footers.

1. **The exposure is wider than the KEK** — enumerated: the live meeting KEKs of *every* meeting on
   that pod, meeting/user JWTs in flight, join-token material, and participant display names. Naming
   only the KEK gets the procedure applied to the KEK while the rest walks out in the same file.
2. **What a dump breaks, in §4's own terms**: §4's security argument rests on the KEK being *never
   derived and never persisted — compromise must be live; a database, a backup, or a log yields
   nothing*. A dump is precisely the act that converts live-only key material into a durable
   artifact, and one taken while a meeting runs decrypts any ciphertext of that meeting captured
   elsewhere, indefinitely, including after the meeting ends.
3. **Default posture: do not take one.** Cheaper signals first (metrics, `kubectl top`, the existing
   Scenario 1/7 ladders); the dump is the last rung behind an explicit "you are now handling key
   material" gate.
4. **Handling**: private-key-equivalent; never attached to a ticket, Slack or incident doc; never
   `kubectl cp`'d to a shared bastion or any backed-up/shared volume; encrypted at rest immediately;
   named-responder access only; a deletion deadline **with an owner**; a record of who held it.
5. **The remediation step people forget**: meetings live at dump time are key-compromised. Name the
   lever — MC's KEK rotation (§4) — and what to do if it cannot be driven for a live meeting (end
   them). A procedure that protects the file and not the meetings is not a procedure.
6. **Whether MC can produce a dump today without anyone asking** — I will check the kind node's
   `core_pattern` / `RLIMIT_CORE` and the MC container's limits and state the **actual** default plus
   the one command to re-check it. If it is genuinely indeterminate for this cluster I will say that
   explicitly rather than imply a posture.

Contains **no** copy-pasteable command that writes a dump to a shared path and **no** command that
prints key material to a terminal.

**`docs/runbooks/client-dev-local.md`**

- New **§4.5 — "I joined and I hear nothing"**, in §4.0's table idiom and §4.2's *what each green
  signal actually proves* framing (which is the right idiom precisely because the hazard here is
  every signal reading green while no audio arrives). Cheapest-signal-first, keyed on this story's
  counters, localising a silent call to exactly one of: **capture → encrypt → sign → uplink → MH
  forward → downlink → decrypt → verify → playback**. Localisation is the entire purpose of the
  loopback story.
  The signal-to-stage map (all `dt_client_media_*`, all already emitting in
  `packages/sdk-core/src/media/setup/mediaMetrics.ts`):

  | Stage | Signal | What a green reading actually proves |
  |---|---|---|
  | capture / mute | `mute_transitions_total{action}`, `frames_sent_total` flat | frames are being produced — **not** that they left |
  | encrypt / sign / uplink | `frames_sent_total` rising, `send_dropped_total{reason}`, `send_queue_depth` | frames left the device — **not** that MH accepted them |
  | MH forward | `mh_media_frames_forwarded_total{direction="egress"}` | MH sent something — **not** that it was yours |
  | downlink | `frames_received_total` (at the wire, pre-verify) | datagrams arrived — **not** that they were openable |
  | decrypt / verify | `frames_dropped_total{reason}`, `key_wrap_outcomes_total{outcome}` | which crypto step rejected, by name |
  | playback | `frames_accepted_total` rising **and still silent** | the fault is downstream of decoder handoff — decoder, output device, or a suspended audio context |

- New §5 entries `F12`–`F15` continuing the existing numbering (currently ends at F11), each with
  the file's established *symptom / why / fix* shape: silent call with `sent` flat; silent call with
  `sent` rising and `received` flat (forking exactly as MH Sc 17 does); frames accepted but silent;
  sustained key-material drops (→ points at MC Scenario 16, does not restate it).
- Cross-references follow `mh-deployment.md:161-163` and `mc-incident-response.md:2131` — point,
  never duplicate.

### P2. Alerts

#### Alert thresholds (ADR-0031 structured block — cross-cutting review required)

| Alert | Metric | Condition | For | Severity | Runbook |
|---|---|---|---|---|---|
| `MCMediaGenerationDivergence` | `mc_media_policy_pushes_total` | `sum(increase(mc_media_policy_pushes_total{outcome!~"match\|handler_id_mismatch"}[15m])) > 0` | `0m` | **page** | `docs/runbooks/mc-incident-response.md#scenario-15-media-generation-divergence` |
| `MCMediaMissingKeyMaterial` | `dt_client_media_frames_dropped_total` / `dt_client_media_frames_received_total` | ratio of `reason=~"no_kek_for_generation\|no_roster_entry"` over received `> 0.05`, guarded on received `> 0` | `15m` | warning | `docs/runbooks/mc-incident-response.md#scenario-16-missing-key-material` |
| `MHMediaEgressQueueOverflowRate` | `mh_media_frames_dropped_total` / (forwarded + dropped) | `egress_queue_overflow` over egress **attempts** `> 0.01`, guarded on the identical attempts sum `> 0` | `5m` | warning | `docs/runbooks/mh-incident-response.md#scenario-17-media-datagram-drop` |

Reviewers: observability, operations (operations is the implementer here; @team-lead holds the lens).

Expressions are **@observability's, taken verbatim** from their packet — I did not re-spell a label.
Deltas from my own first draft, all of which @observability corrected and I accept:

- **Alert 1 fires on the counter, not the gauge.** `mc_media_generation_divergence`'s own catalog
  entry says *never page on it*: it is last-write-wins pod-level and a healthy push erases a diverged
  reading before it is necessarily scraped. The gauge is the magnitude a responder reads **next**,
  and it appears in the `description` and the runbook, never in `expr`.
- **Window reconciled to `[15m]`** (this block first recorded `[5m]`). A one-shot counter with a
  longer `increase()` window holds the alert visible longer for oncall, and the annotation's "in
  15m" is consistent with it. Recorded rather than left as a plan/rule disagreement — a devloop
  record that disagrees with what shipped is a second encoding, which is the defect this Gate spent
  its time on, in miniature.
- **`outcome!~"match|handler_id_mismatch"` is load-bearing.** `handler_id_mismatch` fires on every
  ordinary MH restart until `MH_HANDLER_ID` is stable, so `outcome!="match"` would page on every MH
  rollout. This one expression is held at **three** sites (the rule, the catalog entry,
  `mc-deployment.md`'s post-deploy checklist) under one revert trigger; the rule will carry a comment
  naming that trigger (`2026-09-02-mh-stable-handler-id`) so whoever stabilises the handler id finds
  the third site.
- **Alert 1 takes no zero-denominator guard** — it is not a ratio, and a guard on a non-attempts
  expression is decorative and teaches the next reader the wrong rule.
- **Alert 3's numerator is restricted to `reason="egress_queue_overflow"`**. An unrestricted
  `direction="egress"` numerator reads 100% off a single frame in a meeting with nobody subscribed,
  because `no_subscriber` is counted once per **frame** rather than once per (frame × subscriber).
- **Nothing rests on `mh_media_egress_queue_depth`** (catalog: *no alert may rest on this gauge
  alone*) or on `dt_client_media_frames_accepted_total` as a denominator (that is the
  attempts-denominator inversion in its exact form — in a total key-delivery outage `accepted` goes
  to zero and the alert goes silent).
- `key_custody` is single-valued; it appears in **no** selector because it partitions nothing.
- Threshold provenance stated honestly: **5% is not SLO-derived and there is no baseline.** It is
  chosen against the frame rate — 20 ms/frame is 50 frames/s, so a 1–2 s join transient is well under
  1% of a 5-minute window while a sustained delivery failure sits near 100%. The `for: 15m` is doing
  the real work, not the threshold. 1% on Alert 3 is an audibility marker for audio, likewise
  unratified. Neither is presented as ratified.
- **Alert 2's `for: 15m` against a `rate(...[5m])` window is a documented anti-shape, deliberately.**
  `alert-conventions.md` §`for:` Conventions says to match `for:` to the `rate()` window and calls a
  5m window with a long `for:` "rarely what you want". Here it is exactly what we want — the join and
  KEK-rotation transients are **expected**, so the window has to outlast them, and sustained-versus-
  transient *is* the discriminator. **The reason goes in a comment ON THE RULE**, not only in this
  plan block, or someone "fixes" it to `5m` — the same failure class as the alert-name/selector
  mismatch that produced the `MHMediaEgressQueueOverflowRate` rename.
- @observability's note that `for: 0m` on Alert 1 would page during a rollout skew
  (`no_applied_generation` from a not-yet-upgraded MH): I keep `for: 0m` — the condition does not
  self-correct in this story, so delaying buys nothing — and the `description` will name the rollout
  case explicitly so oncall is not surprised.

#### Second table: does it fire, does it apply (ADR-0036)

| Control | **Does it fire?** (inject the adverse condition) | **Does it apply?** (confirm the premise against the real artifact) |
|---|---|---|
| `MCMediaGenerationDivergence` | Inject an **apply failure at MH** and assert the **acknowledged** generation does not advance — MH's `mh_media_policy_applies_total{outcome="apply_failed"}` path drives MC's `generation_mismatch`. | Confirm the signal is fed by the **applied** value from the registration response — **not** the value MC sent, **not** the value MH received. ADR-0036 names *a generation gauge fed by the received rather than the applied value* as a real, in-design instance of a control that applies but never fires. Verified by reading `mc-service` `record_media_policy_push`'s input and MH's response-construction site, not by reading the metric name. |
| `MCMediaMissingKeyMaterial` | Withhold the KEK, assert `reason="no_kek_for_generation"`; then withhold the roster entry, assert `reason="no_roster_entry"`. **The correct reason label each time** — a single "it dropped" assertion would pass with the labels swapped, and the two labels route to different remedies. | **PREMISE UNMET — SEE §P3a. This row does NOT read green.** The step is: confirm the counter reaches **Prometheus** from a **real browser client** through the SDK OTLP sink and the GC telemetry proxy. **It cannot be performed today.** `infra/services/otel-collector/configmap.yaml`'s metrics pipeline is `exporters: [debug]` — the collector's *log* — and no Prometheus job scrapes the collector. So `dt_client_*` is not a series and this alert cannot fire at any threshold. Wiring deferred to a TODO (owner infrastructure + observability, security on port and cardinality); the premise failure is stated at FIVE sites (§P3a) so nothing reads it as coverage. **LOAD-BEARING, AND THIS ALERT'S SOLE COVERAGE — NOT BELT-AND-BRACES**: when the wiring lands, the step must read the metric name back **out of Prometheus** (instant query, or `/api/v1/label/__name__/values`), never restate the SDK spelling, because OTLP→Prometheus normalisation is not guaranteed to preserve `_total`. Emission in a unit test is not the premise this alert rests on. **Why nothing else can cover it:** `dt-guard application-metrics` extracts alert metric references with `\b((?:ac\|gc\|mc\|mh)_[a-z][a-z0-9_]*)` (`crates/dt-guard/src/common/services.rs`, derived from `CANONICAL_SERVICES`). `dt_client_media_frames_dropped_total` contains none of those prefixes at a word boundary — and underscore **is** a word character, so even a hypothetical `dt_mc_…` would not match. **A typo in this alert's expression passes CI green while the alert never fires.** Same word-boundary matcher property ADR-0036 §11 names about the telemetry vocabulary guard. |
| `MHMediaEgressQueueOverflowRate` | **Two halves, deliberately labelled, because only one is this alert's fire-test** (@observability's catch — the task's "drive *each* bounded queue" phrasing invites the conflation). **(a) Alert coverage**: drive **MH's** application egress queue past its bound via a slow subscriber and assert `mh_media_frames_dropped_total{reason="egress_queue_overflow",direction="egress"}` increments — this is the only metric the expr reads. **(b) Runbook coverage, NOT alert coverage**: drive the **SDK's** queue past its bound and assert `dt_client_media_send_dropped_total{reason="egress_queue_overflow"}`. Worth keeping — it is Scenario 17's ladder — but if (b) passes and (a) is subtly wrong, an unlabelled row reads green. | **Structural, not confirmatory.** The premise is enforced at startup as `ConfigError::EgressQueueDoesNotBindFirst` (`crates/mh-service/src/config.rs:1211`, raised at `:1474`, rationale at `:367-371` and `media/queue.rs:5`). A config that violates the ordering **fails startup**, so the alert cannot silently become dead-by-construction: if quinn absorbed the drop, the counter would never increment and the alert would be dead, and startup validation is what makes that state unreachable rather than merely unobserved. This is the one applies row in the table that can be made fail-closed rather than a confirmation, which is why it is worth deriving from those lines instead of restating the relationship. |
| **Rule-file load coverage** (the structural fix, §P3) | Land a fixture rules file outside the glob / outside the ConfigMap enumeration and assert the guard **fails**; land one inside and assert it passes. Proof-of-trap per branch. | Assert the on-disk rules set is **non-empty** before comparing, with a **distinct reason token** from a content mismatch. "empty ⊆ loaded" passes vacuously; that is the same assertion-vacuity the TODO entry says the current green guard already exhibits. |

**Residual gap, recorded rather than silently accepted**: the catalog marks
`dt_client_media_send_dropped_total{reason="transport_send_refused"}` and `{reason="not_connected"}`
as *"reads zero forever; alertable at `> 0`"* — a fleet contract with no rule behind it. The task
scopes this to three alerts, so I am **not** adding a fourth. I will file it as a `docs/TODO.md`
line with the reasoning and the same `does it apply` caveat (it is another `dt_client_*` name, so it
is invisible to `application-metrics` too). Asking @team-lead + @observability to confirm that call.

#### Inventory entries in `docs/observability/alerts.md` (observability-owned)

Byte-identical PromQL, the GC discipline. Per @dry-reviewer, appending is not enough — two existing
**enumerations** go stale on contact and must be edited, not appended past:

- **`alerts.md:620`** — *"Existing MC `severity: page` alerts: `MCDown`, `MCActorPanic`,
  `MCHighMailboxDepthCritical`, `MCMediaConnectionAllFailed`. That is the complete set…"* becomes
  false the moment `MCMediaGenerationDivergence` lands. I add it to the list and **keep** the
  existing "convenience copy; verify against it" hedge.
- **`alerts.md:737-744`** — the MH banner asserts the section *"documents **none** of it"*, directly
  above where an entry would go. Reframed as an explicit **partial** inventory: which alerts are
  inventoried (**named**, never counted), that the rest are not, and that the rules file remains the
  source of truth. **No count** — the parenthetical at `:743` forbids exactly that, correctly, and I
  am not resolving one stale-enumeration bug by writing another.
- I do **not** retro-fill MH's or MC's existing alerts, per the task.

**Division of labour agreed with @observability** (ADR-0024 §6.3 paired route, which the task's
explicit pairing puts in force): **I write the three inventory entries** with byte-identical PromQL —
that half is mechanical against rule files I own — and **@observability writes the two prose
amendments** (the MC "that is the complete set" sentence and the MH "documents none of it" note), in
the same commit. That is what makes the Domain-judgment classification mean something rather than
being a label on a hunk they merely ACK. Entry shape to copy is the GC one: fenced ```promql block,
then a bare `` `for: <dur>` `` line, then a numbered **Response** list.

**Alert-3 rename accepted.** `MHMediaEgressDatagramDropRate` → **`MHMediaEgressQueueOverflowRate`**.
@observability is right that the old name overclaims against a selector restricted to
`reason="egress_queue_overflow"`: a responder seeing `connection_closed` and `no_subscriber` move
without the alert firing would reasonably "fix" it by widening the selector, which reintroduces the
`no_subscriber` false-fire (counted once per **frame**, so an unsubscribed meeting reads 100% off a
single frame). The runbook scenario heading stays "Media Datagram Drop" regardless.

### P3. Prometheus actually loads the rules (task-24 scope)

**Both reviewers who checked are right that the task text names the wrong file.**
`docs/observability/dashboard-conventions.md:249-253` declares
`infra/kubernetes/observability/prometheus-config.yaml` **AUTHORITATIVE — this is what runs**, and
`infra/docker/prometheus/prometheus.yml` **local-only, not deployed**. The K8s ConfigMap has **no**
`rule_files` at all and mounts **no** rules. Fixing only the docker file leaves the deployed cluster
evaluating nothing and leaves the live-cluster env-test with nothing to assert against. **Both get
wired; the K8s side is the load-bearing one.**

**The glob: `rules/[a-z]*-alerts.yaml`, one identical string in both configs.**
A naive `rules/*-alerts.yaml` matches `_template-service-alerts.yaml`, whose exprs are placeholders
(`sum(rate(<svc>_<operation>_errors_total[5m]))`). Prometheus validates rule files at config load and
exits non-zero — in-cluster that is CrashLoopBackOff, which fails `setup.sh:597`'s readiness wait and
**breaks whole-cluster bring-up**. Not imprecise: cluster-breaking. There is a second reason
@security raises: the template is guard-**exempt** by design, so loading it makes unscanned
annotation text into a live evaluated rule.

Relative `rule_files` resolve against the config file's directory, so the same string resolves to
`/etc/prometheus/rules` in both deployments if the container path is identical — which is the point
of keeping it one string. Docker gets `./infra/docker/prometheus/rules:/etc/prometheus/rules:ro`
(no conflict; the existing mount is a file). K8s gets a second ConfigMap volume at the same path.

Side effect I consider a feature: the template's own header already asserts *"NOT LOADED BY
PROMETHEUS — the `_` filename prefix excludes this file from Prometheus's alert-rule load glob."*
That claim is **vacuously true today** (no glob exists). This change makes it load-bearing and
**true**, so the file needs no edit. I considered @security's structurally-stronger alternative —
move the template to `infra/docker/prometheus/rules-templates/` so there is no carve-out to encode —
and am **not** proposing it, for one reason: it would leave `alert_rules.rs`'s `_template-` skip with
no scope, i.e. a control that can never fire, which is the pattern we are here to remove. Under the
glob-with-carve-out, that skip stays live and the set-equality assertion below makes the two
carve-outs un-driftable. **@security / @infrastructure: this is your call; say the word and I will
move it instead.**

**The guard.** Per @dry-reviewer and @infrastructure: **no new subcommand** (35 exist; ADR-0034's
sprawl threshold is 10). New `rule_id`s inside `crates/dt-guard/src/alert_rules.rs`, which already
owns `ALERTS_SUBDIR` and the `_template-` skip, modelled on `kustomize.rs:215 check_dashboard_coverage`
— the existing bidirectional on-disk↔declared walk, including its two hard-won repairs (inline-comment
stripping, and the `- ` bullet anchor that closed a fail-open where a *comment* naming a path counted
as declaring it). Writing a second parallel walk would re-open both. @infrastructure supplied the detail: `kustomize.rs:176-192`
records that the line-oriented parser failed in **both** directions — *fail-loud, wrong artifact* (an
inline comment left the tail after the last `/` ending in comment text, so a listed dashboard was
reported orphaned) and, worse, **fail-open** (anchoring on nothing but "a line containing `/` whose
tail ends in `.json`" meant a **standalone comment** naming a path counted as *declaring* it). The
rules ConfigMap has the identical file shape (`- gc-alerts.yaml=<path>/gc-alerts.yaml`) and the
identical comment hazard, so I generalise `extract_declared_dashboards` over the extension rather
than writing a fresh parser that re-earns both bugs. **The fail-open direction is the one that
matters here**: it would report a rules file as mounted on the strength of a comment mentioning it.

**The failure message must name WHICH DIRECTION diverged**, because the remedies are opposite:
*linted but never loaded* is the "alive, never applied" defect this task exists to fix; *loaded but
never linted* is an unvalidated rule file evaluating in production with no `runbook_url` resolution,
no severity check and **no annotation-hygiene secret scan** — the more dangerous one. "Sets differ"
is not actionable. Rationale precedent for keeping the two sets separate and machine-compared rather
than collapsed: `crates/dt-guard/src/common/services.rs`, which documents two sets that "coincide
today, and that coincidence is now load-bearing", resolved as **"a *guarded* mirror, not a trusted
one."**

The guard **reads** each encoding rather than restating it, so there is one predicate and three
consumers:

1. Parse `rule_files:` out of `infra/docker/prometheus/prometheus.yml` **and** out of the inline
   `prometheus.yml` in `infra/kubernetes/observability/prometheus-config.yaml`; expand each glob
   against the rules directory.
2. Parse the `configMapGenerator` `files:` enumeration out of
   `infra/kubernetes/observability/kustomization.yaml` — kustomize **cannot glob** (@infrastructure
   verified this empirically), so the in-cluster membership set is an enumeration, and a guard that
   only checked the glob would be green while a new rules file was never mounted.
3. Enumerate the on-disk set from `ALERTS_SUBDIR` minus the `_template-` skip.
4. **Assert set equality across all three, failing on a difference in EITHER direction.** One-way
   coverage is how this class of guard goes quietly blind. The security-relevant direction is the
   inverse of the one the task names: a file Prometheus *loads* that dt-guard does **not** validate
   ships unscanned annotations into a live rule.
5. **Anti-vacuity**: empty on-disk set is a **failure** with its own reason token, and the OK reason
   names the covered count (`alert-rule-loading-4-files-covered`), never a bare "clean" — the shape
   `grafana_datasources.rs` already uses, and the exact remedy the TODO entry asks for.

**`--web.enable-lifecycle`** (@security's #4): my wiring does **not** need `/-/reload`. I propose the
kustomize `configMapGenerator` keep its **default name-hash suffix**, so a rules change produces a new
ConfigMap name, kustomize rewrites the Deployment's volume reference, and the pod **rolls** — the
rules on disk are the rules loaded, structurally, with no reload call to forget. That makes
`--web.enable-lifecycle` unnecessary and I propose dropping it, closing the unauthenticated
`POST /-/quit` on the NodePort that becomes a **detection** outage once rules are live.
**@infrastructure has since confirmed the flag is dead configuration and asked for it deleted**, with
a sharper exposure than @security's: the Service is `type: NodePort` on 30090 (`:248-249`) and
`infra/kind/kind-config.yaml:45-46` maps `containerPort: 30090 → hostPort: 9090`, so it is
**host-reachable**, not merely node-reachable — one unauthenticated
`curl -XPOST http://localhost:9090/-/quit` stops the cluster Prometheus. They grepped `scripts/`,
`infra/` and `crates/env-tests/` for `/-/reload`, `/-/quit` and `enable-lifecycle`: the only hit is
the flag's own declaration (`setup.sh:498` is `preload_third_party_images`, a false positive). No
caller regresses. **Deleting `prometheus-config.yaml:219` in this changeset.**

**Why it is this task's business and not drive-by hardening**: this change converts the consequence.
Today killing Prometheus is a metrics gap — a dashboard goes blank and someone notices. After rule
evaluation lands, the same request is a **detection outage**, and a stopped evaluator looks exactly
like a quiet one. That is `docs/TODO.md:736`'s own "a correctly-configured quiet system and a
completely-unwired one look identical" property, arrived at from the other direction. The severity of
that line is a function of my change, which is what puts it in scope rather than in a TODO.

I still verify with `kubectl kustomize` that the volume `configMap.name` reference is rewritten for
this Deployment before relying on the hash suffix. If it is not, the fallback is
`disableNameSuffixHash: true` plus `kubectl rollout restart` (which `setup.sh` already does
structurally: apply + `rollout status`/`wait`) — **not** re-adding the lifecycle flag.

**The env-test** (test-owned; @test, this is the hunk in your domain).
`crates/env-tests/tests/33_alert_rules_loaded.rs`, using a new
`PrometheusClient::rules()` on `crates/env-tests/src/fixtures/metrics.rs` — **reuse, not a third
hand-rolled `/api/v1/*` call site** (`cluster.rs:353` and `32_media_metric_hygiene.rs:248` are already
two).

- Enumerate every `alert:` name from the on-disk rules files under the **same** exclusion predicate
  as the guard.
- **Positive control first**: assert that set is non-empty, with a reason token distinct from a
  content mismatch, so an empty-input vacuity is never triaged as content.
- Assert **every on-disk alert name ∈ the loaded groups** — that direction, never the reverse, which
  passes when nothing loaded. This also catches Prometheus's *tolerated* parse failures and mount
  misconfiguration, which are the two failures a config-file inspection cannot see.
- Parse `/api/v1/rules` as **JSON**. @infrastructure's warning is a defect this repo already shipped:
  `32_media_metric_hygiene.rs:246-290` records that an earlier version line-parsed
  `/api/v1/status/config`, which returns the config as an **escaped JSON string with zero real
  newlines**, so every extraction matched nothing and it passed on every possible input including a
  real violation. Same split as that file: parsing kernel unit-testable with FIRE fixtures, the
  cluster-gated test supplies only real input.

**`docs/TODO.md`: narrow, do not delete** (@infrastructure's F, and I agree). The entry at
`:736-753` names **four** breaks. This task closes **break 1** only. Break 1 gets closed with date
and commit; breaks 2–4 (no Alertmanager, the cited `alertmanager.yml` that does not exist, and the
documented routing matching `severity: critical` — a value **no rule uses**, so every `page` rule
would fall through to no receiver) survive as the residual entry, **together with @security's
paragraph** conditionally accepting `warning` over `page` for `MHMediaSenderBindingOutOfRange` *"on
the explicit assumption that routing reaches a security owner."* Deleting the entry wholesale would
mask three live breaks and silently discharge a conditional acceptance whose condition is still
unmet.

**One statement my change falsifies, fixed in the same commit** (@infrastructure's G):
`docs/TODO.md:972` (@security's S6) asserts *"no Prometheus in this tree evaluates any alert rule at
all … so `MHMediaSenderBindingOutOfRange` is **inert rather than quiet**."* After this change it is
**quiet, not inert**. Precedent is on point —
`docs/devloop-outputs/2026-09-02-media-path-slos-and-observability-policy/main.md:78`: *"Shipping a
change that knowingly leaves a false statement in the tree is what 'fail loudly; never mask'
forbids."* I will also re-check `docs/observability/alerts.md:12` ("loaded by Prometheus server"),
which this change makes **true** rather than false.

### P3a. PREMISE FAILURE: `dt_client_*` metrics reach no Prometheus today

Raised by @observability, **verified independently before accepting it**:

- `infra/services/otel-collector/configmap.yaml` — the **metrics** pipeline is
  `receivers: [otlp]` / `exporters: [debug]`. No `prometheus` exporter, no
  `prometheusremotewrite`.
- `infra/kubernetes/observability/prometheus-config.yaml` — scrape jobs are `prometheus`,
  `ac-service`, `gc-service`, `mc-service`, `mh-service`, `kube-state-metrics`, `node-exporter`,
  `kubelet`. **No otel-collector job.**

So browser → SDK OTLP sink → GC telemetry proxy → collector **terminates in a pod log**.
`dt_client_media_frames_dropped_total` is not a series in Prometheus, and `MCMediaMissingKeyMaterial`
**cannot fire for any reason at any threshold**.

This is the *"alive, never applied"* row of ADR-0036 §11's own table, arrived at from a second
direction — the same shape as the `rule_files` finding already in scope, one hop further down the
same pipe. It is also the case where nothing else can catch it: `dt-guard application-metrics`
cannot see a `dt_client_*` name (§P2's fire/apply table), so the applies step **is** this alert's
coverage, and an applies row reading green here would be the exact failure the ADR names rather than
a demonstration of its absence.

**Resolution: route 2 — defer the wiring, state the premise failure loudly.** My first instinct was
route 1 (make the premise true; ~15 lines of YAML). **@infrastructure, who owns that surface, costed
it and it is not 15 lines**: an exporter choice (`prometheus` pull vs `prometheusremotewrite` push),
a container port, a Service port, a scrape job, and a NetworkPolicy amendment for
observability-ns → dark-tower-ns — ~5 manifests plus two decisions needing @security on the newly
exposed port and on `dt_client_*` **cardinality reaching central storage**. That is a task, not a
fold-in, and it is the owner's call. I accept it.

**The deferral's condition is hard and I am adopting it in full: nothing in this diff may state or
imply that the counter reaches Prometheus.** Concretely:

- The fire/apply table's applies row for `MCMediaMissingKeyMaterial` is marked **PREMISE UNMET**,
  with the concrete chain (`exporters: [debug]`, no collector scrape job), not a vague hedge.
- The `alerts.md` inventory entry carries the same statement in plain form: *this alert cannot fire
  until the client metric export path exists*.
- MC Scenario 16 says the same where a responder would otherwise conclude "no alert fired, therefore
  no key-material drops".
- A new `docs/TODO.md` entry names the wiring above — owner **infrastructure** + **observability**,
  with **security** on the receiver/port and the cardinality question. It is **not** tracked today;
  @infrastructure and I both checked.

Landing that row as "confirmed" would reproduce the exact *"alive, never applied"* pattern this task
exists to close, one layer further out, **in the artifact that closes it**. That is the one outcome
worth failing the task over.

**A fifth site, from @security, that I had missed.** The env-test asserts every `alert:` name in the
rules files appears in Prometheus `/api/v1/rules` — and `MCMediaMissingKeyMaterial` **will** appear
there, because rule *loading* is independent of whether the metric exists. That green is real for
what it measures and says **nothing** about this alert being able to fire. So neither the guard's OK
reason token nor this devloop record may let *"rules load"* be read as *"alerts fire"*. That
conflation is precisely what `docs/TODO.md:736` was filed to prevent (*"'the alerts landed' must not
be read as 'the alerts fire'"*), and reintroducing it inside the task that closes it would be the
worst available outcome. The guard's reason token names **loading coverage** explicitly, and this
record states the distinction where the verification steps are listed.

**@security's `no_roster_entry` framing, adopted into MC Scenario 16.** That arm is the sharper half:
no roster entry means the receiver cannot resolve the sender's AC-attested identity public key, so
§3 signature verification — the whole sender-attribution story — is failing, and this counter is what
would tell anyone. Scenario 16 will say so, and will say that **a responder arriving there was not
paged**: they got there another way, and this alert not firing proves nothing.

**Not dropping the alert** (@security explicitly, and I agree). §11 names this signal as the only one
for a real failure mode. The rule lands so that wiring the exporter is the single remaining step.

Second-order constraint that survives either route (@observability, and I agree): OTLP→Prometheus
name normalisation is not guaranteed to preserve the `_total` suffix written in TypeScript, so the
applies step must read the name back **out of Prometheus** — an instant query returning a non-empty
result, or `/api/v1/label/__name__/values` — and must **never restate the SDK spelling**. Restating
it is expectation-written-from-memory, and with dt-guard blind here nothing else would catch it.

**Why ship the alert at all under route 2**, rather than deferring it with the wiring: the rule is
correct, reviewed, and is a precondition for ever firing — the same @operations ruling that landed
the three MH media alerts in `2026-09-05-sender-id-binding-contract` (*"a counter only a dashboard
reader ever sees does not fix that blindness — it relocates it"*). What that ruling does **not**
license is letting *"the alert landed"* read as *"the alert fires"*, which is precisely the lesson
`docs/TODO.md:745` records against that same devloop. Hence the four statement sites above.

**@team-lead: confirming route 2 rather than asking — @infrastructure owns the surface and has
ruled. Overrule me if you disagree.**

### P3b. The exclusion predicate has FOUR consumers, not three

@infrastructure's correction, accepted. The env-test is also a consumer: walking every
`*-alerts.yaml` picks up `_template-service-alerts.yaml` and asserts its placeholder alerts
(`<Svc>HighErrorRate`) appear in Prometheus's loaded groups — which they never will, by design. The
test would be **red by construction**.

The tempting fix — special-casing the template *inside the env-test* — forks a second exclusion rule
away from the glob's and lets them drift silently. **Derived, not restated.** Full consumer list,
all deriving from `crates/dt-guard/src/alert_rules.rs:34` (`ALERTS_SUBDIR`) + `:413` (`_template-`
skip):

1. `rule_files` glob in `infra/docker/prometheus/prometheus.yml`
2. `rule_files` glob in the K8s ConfigMap
3. kustomize `configMapGenerator` enumeration (cannot glob — verified empirically by @infrastructure)
4. the env-test's on-disk `alert:` name extraction

Also noted: `infra/docker/prometheus/rules/otel-alerts.yaml` already exists and comes along with the
glob. I will confirm it **parses** before asserting every alert name appears in the loaded groups —
otherwise the env-test goes red on a file that is not mine, and someone triages content when the
cause is a pre-existing parse error.

### P3c. Env-test extraction shape (@test's addendum, accepted verbatim)

- On-disk `alert:` names: **parse the YAML with serde**, not `grep 'alert:'`. A line-grep sweeps
  `mh-alerts.yaml`'s header comment lines (`# - No RegisterMeeting RPC alert:`) into the expected set.
- `/api/v1/rules`: **parse as JSON.** The repo already shipped the opposite —
  `32_media_metric_hygiene.rs:264-300` records an earlier check that line-parsed
  `/api/v1/status/config`, which returns the config as an escaped JSON string with **zero real
  newlines**, so every extraction matched nothing and it passed on every possible input including a
  real violation.
- **Both extraction kernels live in a unit-testable location** (`crates/env-tests/src/fixtures/`)
  with FIRE fixtures in the always-on Rust lane, including one proving the kernel returns the *right*
  names on known input and a **non-empty** set. The cluster-gated test supplies only real input and
  renders failures. `cargo test` cannot reach a cluster-gated file, so an inline extractor there gets
  zero unit coverage — precisely how the escaped-JSON bug shipped green.
- The "could not extract / empty input" path panics with a **could-not-evaluate** reason token
  **distinct** from the content-mismatch token, with a comment saying an empty result is not a clean
  result (mirroring `32:288-296`). Otherwise a broken extractor is triaged as a content bug and
  "fixed" by relaxing the extractor.
- Proof-of-trap: rules present on disk but absent from the loaded groups must go red — that is
  exactly the state a mount misconfiguration or a *tolerated* parse failure produces.
- **A doc comment on the test stating what it proves, not what its name suggests** (@infrastructure,
  via @observability). The assertion is satisfied perfectly by a rule that is loaded, evaluating, and
  **incapable of ever matching** — which is exactly `MCMediaMissingKeyMaterial`'s state (§P3a). The
  comment says: *this proves rules load and evaluate; it proves nothing about whether any alert can
  fire.* Without it, a green CI lane over a permanently-silent rule reads as coverage, which is the
  conflation `docs/TODO.md:736` exists to prevent.

### P4. Wording rules I am treating as hard constraints

- **No end-to-end or zero-trust claim** in any runbook, alert annotation, or dashboard text added
  here. Every service reports `key_custody=operator` and the default deployment is neither (§4). The
  permitted claim is exactly: *media is encrypted between clients; MH, transport and storage cannot
  read it; MC can.* The easiest place to violate this accidentally is the decrypt/verify rungs in
  `client-dev-local.md` and the "MH never opens a frame" line in MH Sc 17 — "MH cannot read it" is
  fine, "therefore end-to-end" is not.
- **No egress bandwidth budget, capacity gauge, stream ceiling, or admission threshold** in any added
  text. None exist in story 1; the whole egress-budget chain is story 2. The **application egress
  queue bound** in the datagram-drop scenario is a different thing and does belong.
- **Not written**: egress exhaustion (scenario or alert), restart-media-dark, sustained re-assert
  failure, keyframe storm, stream-credit stall.
- No `by(meeting_id)` / `by(participant)` / `by(sender)` in any added PromQL, runbook included.
  Aggregation floor pod or service. No "raise the log level" rung. No credential-bearing commands;
  packet-capture and browser-trace rungs carry the credential warning.

### P5. Open questions for @team-lead

1. **P0's Scenario 17 renumber** — I need this ruled before writing, since the alert's `runbook_url`
   encodes it. This is the one item I cannot proceed without.
1b. **§P3a route 2** — confirming, not asking; overrule if you disagree.
2. **The fourth-alert residual gap** (`transport_send_refused` / `not_connected` fleet contract) —
   TODO line, or in scope?
3. **@dry-reviewer's fired defer trigger** (`docs/TODO.md:151`): the alerts.md↔rules PromQL drift
   guard's stated trigger is *"the next devloop touching any `*-alerts.yaml` or alerts.md should land
   the guard before adding more dual-encoded entries."* I am adding three dual-encoded entries and am
   already opening `alert_rules.rs`, so the trigger has fired on me. **My recommendation: land it,
   scoped by an explicit coverage allow-list of inventoried alert names** (the three new ones), not
   repo-wide — repo-wide fails immediately on the 26-entry MC/MH backfill this task is told not to
   do. It is a third `rule_id` in the file I am already opening. But it is real added scope on an
   already-large task, so I want your ruling rather than assuming it. If declined, I record the
   reasoning in `docs/TODO.md` rather than letting a twice-declined trigger calcify silently.

---

## Pre-Work

None.

---

## Implementation Summary

### 1. Runbook scenarios (written first — the alert guard resolves `runbook_url` to a real file)

- **`mc-incident-response.md` → `### Scenario 15: Media Generation Divergence`.** Written for the
  one-shot form. States in a blockquote that ADR-0036 §8's "self-correcting — a lost response is
  re-asserted on the next tick" **is false in this build** and why (of §8's four re-fire triggers
  only structural change is implemented), so the resolution is *force a rejoin*, never *wait*.
  Triage table splits on `outcome`; `transport_mode_mismatch` and `handler_id_mismatch` are named as
  **not** this scenario. Carries @observability's guard against the wrong remedy: MH deliberately
  does not reject `policy_generation: 0`, so "make MH stricter" is the change §8 exists to prevent.
  Explicit **do not restart MH**.
- **`mc-incident-response.md` → `### Scenario 16: Missing Key Material`.** Splits on `reason` first,
  and states the **instrumentation asymmetry** @security found: `no_roster_entry` has server-side
  corroboration (`mc_join_identity_key_presence_total{presence}`, plus the *bad key* neighbour
  `mc_session_join_failures_total{error_type="identity_key_invalid"}` named so the two are not
  conflated); `no_kek_for_generation` has **none and can have none** —
  `mc_meeting_kek_generated_total` is identically the meeting-creation count and its catalog entry
  says the inference is not computable even in principle. So the scenario says **the absence of a
  signal on that arm is not evidence the KEK is present**, names the two non-dump checks first, and
  routes to the dump gate — a third cross-reference the task did not ask for, added because that arm
  is the likeliest place a dump gets improvised, being the only one where it looks like *diagnosis*
  rather than *last resort*. Carries the prohibition on printing or logging a KEK to confirm
  presence, stated where the responder is.
- **`mh-incident-response.md` → `### Scenario 17: Media Datagram Drop`** (number per §P0). Both ends,
  the MH-side application queue bound stated as **not** a bandwidth budget, and the keepalive case
  with its explicit **no unique client-side discriminator**. The ladder forks on `sent`, then goes
  QUIC-connection-up → generation divergence → keepalive config → packet capture, cheapest first,
  because the connection check is what separates a reaped binding (connection failure / keepalive
  distress) from MH-not-forwarding (healthy connection). A "what each counter proves / does not
  prove" table, `received` counted at the wire as the arriving-vs-failing-to-open discriminator, and
  `accepted` never `played`. `no_subscriber`'s once-per-frame counting is called out as the reason
  not to widen the alert's selector. Packet capture marked credential-bearing.
- **`mh-deployment.md` → `## Rollout With Media Flowing`.** Shed-on-restart as the **decision**, the
  client-visible consequence (audio stops, signalling does not, and in this build it does not come
  back — the ≤10 s re-assert has not shipped), a diagnostic tell distinguishing a shed from a fault,
  the **scale-to-zero is not a media kill switch** warning with a blast-radius table, redeploy-only
  rollback with MTTR = one rollout, and the post-rollout media verification block including
  "expect `handler_id_mismatch` on every MH rollout".
- **`mc-incident-response.md` → `### Heap and Core Dumps Contain Live Meeting KEKs`** (unnumbered),
  with **stop**-pointers placed at the rung a responder reaches for a dump inside Scenario 2 and
  Scenario 7 — not see-also footers. All six of @security's requirements: the exposure enumerated
  wider than the KEK; what a dump breaks in §4's own terms (it converts live-only key material into
  a durable artifact that decrypts separately-captured ciphertext indefinitely); do-not-take-one as
  the default; private-key-equivalent handling with a deletion deadline **and an owner**; the
  KEK-rotation-or-end-the-meetings remediation; and an **honest, measured** answer to whether MC can
  dump unasked.
  **It can.** Verified from inside a running MC pod: `ulimit -c` is `unlimited` and
  `/proc/sys/kernel/core_pattern` is `|/wsl-capture-crash %t %E %p %s`. `core_pattern` is
  host-global and not namespaced, so nothing in our manifests constrains it — an MC abort can write
  a core containing live KEKs with nobody asking. Stated as a standing exposure the procedure does
  not cover, with the one read-only command to re-check it on any other environment.
- **`client-dev-local.md` → `### 4.5 "I joined and I hear nothing"`** plus **F12–F15**. §4.0's table
  idiom and §4.2's *what each green signal actually proves* framing, cheapest-signal-first,
  localising to capture / encrypt / sign / uplink / MH forward / downlink / decrypt / verify /
  playback. Includes the accounting identity and where it stops holding, the `reason`-group table,
  the pod-or-service aggregation floor, no log-level rung, and an honest statement that
  `dt_client_*` reaches no Prometheus so an empty query is the expected outcome. Points at MC
  Scenario 16 and MH Scenario 17 rather than restating them.

### 2. Three alerts

`MCMediaGenerationDivergence` (page), `MCMediaMissingKeyMaterial` (warning),
`MHMediaEgressQueueOverflowRate` (warning). Expressions are @observability's verbatim. Each carries
its reasoning in a header comment, including the things a future editor would otherwise "fix":
the counter-not-gauge choice, the `handler_id_mismatch` exclusion and its three-site revert trigger,
the deliberate `for: 15m` against a `[5m]` window, the numerator restriction to
`egress_queue_overflow`, and the zero-denominator guard sitting on the full attempts sum.

**`MCMediaMissingKeyMaterial` ships marked PREMISE UNMET at five sites** (§P3a) per @team-lead's
ruling. Inventory entries added to `docs/observability/alerts.md` with byte-identical PromQL.

### 3. Prometheus actually loads the rules

`rule_files: "rules/[a-z]*-alerts.yaml"` — one identical string in both configs. The authoritative
cluster config was **extracted out of the inline ConfigMap** to a real file and is now generated with
a content hash, so a config change rolls the pod; the rules ConfigMap is generated from the single
on-disk directory by a kustomization at `infra/docker/prometheus/`, which is where it has to live
because kustomize refuses file sources outside its own directory. `--web.enable-lifecycle` deleted.

Two controls, both demonstrated by proof-of-trap rather than asserted:

- **`dt-guard alert-rules-policy` → `rule_file_loading`** — set equality between
  `loadable_rules_files()` (the single predicate) and every consumer that decides loading, **naming
  which direction diverged**. OK token now reads
  `alert-rules-clean-4-files-4-loadable-covered` and names *loading*, never firing.
- **`crates/env-tests/tests/33_alert_rules_loaded.rs`** — live-cluster, on-disk ⊆ loaded, with two
  distinct reason tokens and a doc comment stating that a green proves rules load and evaluate and
  **nothing** about whether any rule can match.

### 4. Byte-identity guard (Q2, reversed into scope)

`inventory_expr_drift`, heading→rule direction only. Four extraction properties and seven fixtures.
Comparison policy — **line sequences, zero normalisation** — stated at the compare site.

### 5. Unplanned, and the guard caught it

Extracting the inline config broke `dt-guard infrastructure-metrics`, which read the server config
out of `prometheus-config.yaml`'s `data["prometheus.yml"]`. It went red immediately with an **empty**
valid-label set and nine true-shaped findings against innocent dashboards. Repointed at the extracted
file, and its four silent `else { continue }` arms replaced with a hard bail — so the next such move
cannot reach the state of validating against an empty schema.

---

## Files Modified

**Runbooks (operations)**: `docs/runbooks/mc-incident-response.md`,
`docs/runbooks/mh-incident-response.md`, `docs/runbooks/mh-deployment.md`,
`docs/runbooks/client-dev-local.md`

**Alert rules (operations)**: `infra/docker/prometheus/rules/mc-alerts.yaml`,
`infra/docker/prometheus/rules/mh-alerts.yaml`,
`infra/docker/prometheus/rules/_template-service-alerts.yaml`

**Observability-owned (paired)**: `docs/observability/alerts.md`,
`docs/observability/alert-conventions.md`

**Prometheus wiring (infrastructure)**: `infra/kubernetes/observability/prometheus.yml` *(new)*,
`infra/kubernetes/observability/prometheus-config.yaml`,
`infra/kubernetes/observability/kustomization.yaml`,
`infra/docker/prometheus/kustomization.yaml` *(new)*,
`infra/docker/prometheus/prometheus.yml`, `docker-compose.yml`

**Guards (infrastructure machinery)**: `crates/dt-guard/src/alert_rules.rs`,
`crates/dt-guard/src/kustomize.rs`, `crates/dt-guard/src/infrastructure_metrics.rs`

**Tests (test)**: `crates/env-tests/src/fixtures/alert_rules_loaded.rs` *(new)*,
`crates/env-tests/tests/33_alert_rules_loaded.rs` *(new)*,
`crates/env-tests/src/fixtures/metrics.rs`, `crates/env-tests/src/fixtures/mod.rs`,
`crates/env-tests/Cargo.toml`

**Debt**: `docs/TODO.md`

---

## Devloop Verification Steps

| # | What | Result |
|---|---|---|
| 1 | `./scripts/guards/run-guards.sh` | **41/41 pass** (3 were red mid-work: doc-citation line numbers, the `infrastructure-metrics` regression, and cross-boundary scope drift — all fixed, none deferred) |
| 2 | `cargo test -p dt-guard --lib` | 515 pass |
| 3 | `cargo test -p env-tests --lib` | 100 pass |
| 4 | `cargo clippy --workspace --all-targets -- -D warnings` — **checked by EXIT CODE after a forced recompile**, not by grepping output | `EXIT=0`. The earlier grep-based form of this row was false; see §Issues Encountered item 0. |
| 4b | `./scripts/layer5.sh` (the pipeline's own invocation) | `RESULT=OK` |
| 5 | `cargo fmt --all --check` | clean |
| 6 | `kubectl kustomize infra/kubernetes/overlays/kind/observability/` | builds; 36 resources (baseline 35 + the rules ConfigMap); **both** generated ConfigMap name references rewritten to their hashed names |
| 6b | **The hash mechanism demonstrated live, not asserted** (@infrastructure's correction to an over-broad claim of mine) | Restoring `MCMediaGenerationDivergence`'s selector at review changed `mc-alerts.yaml`'s bytes, and the rules ConfigMap went `prometheus-rules-2gd6hmf6t9` → `prometheus-rules-69946fb5k8`, with the Deployment's `volumes[].configMap.name` rewritten in lockstep and `prometheus-config`'s hash untouched. **A one-selector edit propagated to a new ConfigMap name and a rolled pod, with no reload call and nothing for anyone to remember** — which is the property the generated-and-hashed design was chosen for, now evidenced rather than claimed. I had reported "nothing changed in the kustomize output"; the *shape* was unchanged, the hash was not. |
| 7 | **Live cluster**: `kubectl apply -k …/observability/` + `rollout status` | rolled out clean |
| 8 | **Live cluster**: `GET /api/v1/rules` | **10 groups, 58 alerting rules loaded** — previously **zero**. All three new alerts present. |
| 9 | **Live cluster**: on-disk `alert:` names vs loaded | **58 = 58, zero missing** — the env-test's assertion would pass |
| 10 | **Live cluster**: `POST /-/quit` | **403** — lifecycle API disabled, and Prometheus stayed up |
| 11 | Inventory byte-identity self-check | **`pairs=28 DRIFT=0`, orphans 0** — read the pair count first: 25 → 28 confirms the three new entries were seen |

### What the green does NOT cover — stated so 41/41 is not read as more than it supports

**`kubeconform` and `kustomize` are absent from this container**, so `dt-guard kustomize`'s R-17
degrades to `WARN dt-guard auxiliary skip (kubeconform absent)` and the guard still reports
`STATUS=OK REASON=kustomize-clean-kubeconform-skipped`. **Kubernetes schema validation of the
manifests in this changeset was therefore NOT exercised.** @infrastructure raised this and refused to
let the WARN stand in for a pass; recorded here for the same reason. The reason token names the skip,
which is the mechanism working — but a reader counting guards would not see it.

What was verified in its place, and it is stronger for the specific risk here: the real
`kubectl kustomize` render (36 resources, both ConfigMaps hashed, both Deployment references
rewritten, rules mounted read-only at the directory the glob names), followed by an actual
`kubectl apply` and `rollout status` against the live cluster, and then `/api/v1/rules` returning
content. A schema check would have proven the manifests are well-formed; the cluster proved they
work. Neither substitutes for the other, and only one of them ran.

### Proof-of-trap (each branch driven, not asserted)

| Injected | Result |
|---|---|
| Widen the glob to `rules/*-alerts.yaml` | FAIL — `_template-service-alerts.yaml` **LOADED BUT NEVER LINTED**, naming the disclosure cost |
| Delete `mh-alerts.yaml` from the `configMapGenerator` list | FAIL — never mounted in-cluster, naming the kustomize-cannot-glob reason |
| Restore both | OK, reason token names the covered count |
| Drift one inventory PromQL selector | FAIL — prints both line vectors |
| Rename an inventory heading to a non-existent alert | FAIL — orphan heading detected |

Unit fixtures cover the branches the tree cannot exercise: entry with no block, second fence,
indented fence, `for:` inside the fence, empty entry set, trailing whitespace both directions,
orphan heading, plus the loading predicate and the Go-glob subset.

### One step deliberately NOT run

`cargo test -p env-tests --test 33_alert_rules_loaded` needs the **full** port-forward set —
`ClusterConnection::new()` preflights every service port, not just Prometheus — so it belongs to the
layer-7 lane, exactly like `30_observability.rs`. Step 9 above is the equivalent assertion driven by
hand against the same live data, so the logic is verified even though the harness step is not.

---


---


---

## Code Review Results

All seven reviewers confirmed the plan at Gate 1 and returned verdicts at Gate 3. **Eight findings
across five reviewers, all fixed in-loop; one accepted deferral (collector export).**

| Reviewer | Verdict | Findings (all fixed) |
|----------|---------|----------------------|
| security | **RESOLVED-FIXED** | linted-but-never-loaded hole in the guard (demonstrated live with real alert content); S6 present-tense false clause |
| test | **RESOLVED-FIXED** | env-test's hand-written 5th predicate (derive from ConfigMap list); composed-function proof-of-trap for `check_rule_file_loading` |
| infrastructure | **RESOLVED-DEFERRED** | same two as above (co-held); verdict is DEFERRED solely on the pre-existing collector-export deferral, not on this work |
| observability | **RESOLVED-*** (selector fix + Scenario 12 cross-ref confirmed) | `MCMediaGenerationDivergence` selector changed in transit (restored to negated form); missing Scenario 12 remedy pointer on the routed `transport_mode_mismatch` row |
| code-reviewer | **RESOLVED-FIXED** | finding message whitespace runs (render-tested); "end to end" loaded phrase in F14 |
| dry-reviewer | RESOLVED-DEFERRED (expected; coverage direction deferred) | OK token named two of three scopes — inventory pair count added |
| semantic-guard | confirmed at Gate 1 | — |

**The finding that mattered most was a transcription error, not a design flaw** — the selector case,
recorded as §Lessons Learned item 3. It was caught only because the catalog states the operative
expression as a literal that can be diffed.

**Three corrections to my own reporting, each recorded rather than quietly fixed**: the vacuous
"clippy clean" (§Issues Encountered item 0), the over-broad "nothing changed in kustomize output"
(verification step 6b), and "41/41 guards" not covering the unexercised R-17 schema validation
(§What the green does NOT cover). All three are instances of the pattern this task is about, which is
why they are on the page rather than silently repaired.

---

## Accepted Deferrals

Pointer-only; the reasoning lives in the tracked entries, not here.

- **Collector metrics export** — `dt_client_*` reaches no Prometheus, so `MCMediaMissingKeyMaterial` is loaded, evaluating and incapable of ever matching. Deferred on @infrastructure's costing as task-sized; @team-lead ruled the alert lands now with the premise failure stated at five sites. → `docs/TODO.md` §Observability Debt, "`dt_client_*` METRICS REACH NO PROMETHEUS", and §P3a above.
- **Fourth alert** on `dt_client_media_send_dropped_total{reason="transport_send_refused"|"not_connected"}` — out of the task's three-alert scope, and it would create a second permanently-silent alert. → same `docs/TODO.md` entry, deliberately, because it unblocks on the identical trigger.
- **Alertmanager and routing** (breaks 2–4 of the alerting-chain entry) — explicitly permitted by the task. Entry narrowed, not deleted; @security's still-unmet conditional acceptance preserved verbatim. → `docs/TODO.md`, the NARROWED alerting-chain entry.
- **Inventory COVERAGE direction** (every rule must have an entry) — blocked on a 26-entry MC/MH backfill. Trigger retargeted at that backfill task. → `docs/TODO.md`, the PARTLY DISCHARGED inventory-drift entry.

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `54b18a963dde59fe60af8a95df7801dc2a514118`
2. Review all changes: `git diff 54b18a96..HEAD`
3. Soft reset (preserves changes): `git reset --soft 54b18a96`
4. Hard reset (clean revert): `git reset --hard 54b18a96`
5. No schema/data change; Prometheus config change reverts with the redeploy.

---

## Issues Encountered & Resolutions

0. **GATE 2 ATTEMPT 1 FAILED — 22 `clippy::indexing_slicing` errors, and my own report had said
   "clippy clean". The verification method was the defect, not just the code.**

   **What I ran**: `cargo clippy --quiet -p dt-guard -p env-tests --all-targets --all-features 2>&1 | grep -cE "^(error|warning)"` → `0`.
   **Two independent reasons that `0` meant nothing**, either of which alone is sufficient:
   - **The anchored pattern could never match.** Clippy's output is ANSI-colour-escaped, so a
     diagnostic line begins with the escape bytes `\x1b[1m\x1b[91m` and only *then* `error`.
     `^error` never matches, on any input, for any number of real errors.
   - **Clippy caches, and an up-to-date invocation prints nothing at all.** So even a correct
     pattern would have counted `0` on the second run over unchanged code. Reproducing the failure
     required `touch`ing the file first.

   **This is assertion-vacuity mechanism 5 — the check ran where it could observe nothing and
   reported clean — committed by me, in my own verification step, in the devloop whose entire
   subject is that failure class.** Recorded rather than quietly fixed because it is the sharpest
   available instance: I had spent the whole Gate writing about controls that report clean on
   scopes they never examined, and then shipped one.

   **The generalisable repair, which is the same move used everywhere else in this changeset:
   assert on the tool's own verdict, not on a transformation of its output.** `cargo clippy … ; echo $?`
   is the bound and the access in one operation; `| grep -c` is two facts that have to agree. Layer 5
   was right and my check was the weaker construction — which is precisely the argument I made for
   `.get()` over `len()`-then-index while my own harness had the same shape.

   **The fix, per @team-lead's direction — `.get()`, not `#[expect]`.** Two functions rewritten with
   no indexing or range-slicing at all: `parse_alert_inventory` (cursor reads now `lines.get(j)`,
   `while let Some(..)` instead of `while j < len` followed by `lines[j]`) and `glob_matches`
   (`split_first` and `get`, with the three-cell `a-z` range lookahead asked for as one
   `(class.get(i+1), class.get(i+2))` match rather than an `i + 2 < len` test that a later
   `class[i + 2]` has to stay consistent with). Behaviour is unchanged, and that is asserted rather
   than assumed: all 516 lib tests still pass, including the seven inventory fixtures and the
   Go-glob-subset test, and **all three proof-of-trap injections were re-driven after the rewrite**
   (widened glob → `LOADED BUT NEVER LINTED`; missing ConfigMap entry → `NEVER MOUNTED in-cluster`;
   drifted selector → `not byte-identical`).


1. **The task prompt's MH scenario number was stale, and would have produced a broken artifact.**
   It reserved `### Scenario 15: Media Datagram Drop` in `mh-incident-response.md` on the premise
   that the file "currently ends at Scenario 14". Two earlier tasks in the **same story** had
   already taken 15 and 16, and four shipped alert rules cite those anchors. Renumbering would have
   broken four Layer-3-guarded links. Resolved as **Scenario 17** with @team-lead's ruling, verified
   independently by @team-lead, @observability and @security. Tree-wide grep confirmed nothing
   referenced `scenario-15-media-datagram-drop`.
   **Recorded because it generalises**: a live fourth instance of `docs/TODO.md`'s *task manifest
   prompts are a second encoding of requirements with no mechanical binding*, and the first where
   the stale premise would have produced a broken artifact rather than a wrong opinion.
2. **The task named the wrong Prometheus.** It cited `infra/docker/prometheus/prometheus.yml`, which
   `dashboard-conventions.md` records as local-only and not deployed. The authoritative cluster
   config had **no `rule_files` key at all** and no rules volume. Both wired; the cluster side is
   what the env-test queries and is the load-bearing half.
3. **A naive glob would have broken whole-cluster bring-up.** `rules/*-alerts.yaml` matches
   `_template-service-alerts.yaml`, whose `<svc>` placeholders are not parseable PromQL; Prometheus
   exits non-zero at config load, which in-cluster is CrashLoopBackOff and fails `setup.sh`'s
   readiness wait. Resolved with the character class, and the template's header — which credited the
   `_` prefix for a property the character class actually provides — corrected.
4. **Kustomize refuses file sources outside its own directory.** The rules generator could not live
   in `infra/kubernetes/observability/`. Resolved by putting it at `infra/docker/prometheus/` — a
   *parent* of the rules directory, deliberately not inside it, because a `kustomization.yaml` in
   `rules/` would be a file the linter reads and Prometheus never loads, which is exactly the
   asymmetry the set-equality check exists to make loud.
5. **A generated ConfigMap's name reference was silently not rewritten.** `prometheus-rules` was
   rewritten to its hashed name; `prometheus-config` was not. Cause: kustomize's name-reference
   transformer only rewrites when namespaces match, and this kustomization sets no top-level
   namespace, so the generated ConfigMap landed namespace-less against a namespaced Deployment. Left
   unnoticed it would have produced a pod that could not mount its config. Fixed with a per-entry
   `namespace:` and a comment saying it is load-bearing; verified by `kubectl kustomize` before and
   after, then on the live cluster.
6. **Extracting the inline config broke a guard — loudly, which is the point.**
   `dt-guard infrastructure-metrics` read the server config out of
   `prometheus-config.yaml`'s `data["prometheus.yml"]`. After the extraction the lookup found
   nothing, the valid-label set came back **empty**, and nine dashboard panels using `namespace` were
   reported invalid. Repointed at the extracted file, and its four silent `else { continue }` arms
   replaced with a hard bail so the next such move cannot reach the state of validating against an
   empty schema. Fixed in-loop, not deferred.

---

## Lessons Learned

1. **A guard's green is a claim about its scope, and the scope is rarely what its name says.**
   `alert-rules-policy` reported `alert-rules-clean-4-files` for months while nothing in the tree
   evaluated a single rule. The verdict was correct — the syntax *was* clean — and indistinguishable
   from a working pipeline. The remedy that generalises is not "add a check" but **make the reason
   token name the scope**, so the green says what it covered.
2. **A TRUE DOCUMENT CAN PRODUCE A FALSE BELIEF, AND THAT IS HARDER TO CATCH THAN AN INCORRECT ONE.**
   @security's reading of the sharpest instance in this changeset, and it was found in a document
   nobody had flagged. `docs/observability/metrics/client.md` described emission and export
   **accurately** — the SDK emits, the GC proxy forwards — and simply **stopped** at the proxy.
   Nothing on the page was wrong, so there was nothing to notice; the reader supplies the missing
   hop themselves and concludes the metric is queryable. Contrast every other instance in this task,
   each of which had a false sentence somewhere that a careful reader could have caught.

   **And it is the site that matters most for this particular gap.** The other five statement sites
   for the collector premise failure — the rule comment, the inventory entry, MC Scenario 16,
   `client-dev-local.md` §4.5, the TODO entry — are read by someone **already investigating**. The
   catalog is read by someone **deciding what to build on**, and that is the decision the gap
   actually corrupts: an author choosing a signal for a new alert or dashboard reads the catalog,
   sees a documented metric with documented labels, and has no way to learn it cannot be queried.
   The correction instructs deletion of the block *when the exporter lands, not because the catalog
   looks pessimistic* — because the pressure on a catalog is always toward reading well.

   **The generalisable rule**: when a document describes a pipeline, "where does this text stop, and
   what will a reader assume continues past that point?" is a different question from "is any
   sentence here false?", and only the first one finds this class.

3. **THE APPROVED ARTIFACT AND THE SHIPPED ARTIFACT CAN DIVERGE IN THE COPY BETWEEN THEM, AND
   EVERY CONTROL HERE ASSUMES THEY DO NOT.** `MCMediaGenerationDivergence` shipped with a *positive*
   selector, `outcome=~"generation_mismatch|no_applied_generation"`, where the approved plan block
   recorded @observability's negated `outcome!~"match|handler_id_mismatch"`. I implemented from my
   own first draft rather than from the block that had been reviewed. **The plan was correct and the
   review was correct; the defect entered in the transcription** — which makes it different in kind
   from every other instance in this Gate, all of which were flaws in an artifact somebody could
   read.

   It mattered: the negated form covers a sixth outcome automatically, while the positive form lets
   one fall silently outside a **page** alert whose subject is *a partial blackhole reporting
   healthy*. `PolicyPushOutcome::ALL` is compile-checked in Rust, but **that compile error does not
   reach PromQL**. It also silently dropped `transport_mode_mismatch`, which nothing else pages on.

   **It was caught only because the catalog states the operative expression as a literal that could
   be diffed** — not paraphrased, not described. That is a concrete argument for writing operative
   expressions down verbatim in the SSoT: a paraphrase cannot catch a transcription error, because
   the wrong expression still satisfies the description. (@observability's point, and it is the
   reusable half.)

4. **The same failure recurred one layer out, twice, inside this task.** The rule-loading fix has an
   identical sibling in the collector export, and the env-test's own green covers a rule that can
   never match. Both are now stated. The pattern is: *fixing an instance of "alive, never applied"
   is where you are most likely to create another one*, because the fix produces artifacts that
   read as coverage.
5. **A second encoding drifts because someone improves the first.** `d3c30f10` fixed alert selectors
   that matched no pod — a correct, valuable change whose title names ADR-0036's canonical instance —
   and in doing so silently invalidated three inventory entries three directories away that were not
   in its diff. No review could have caught it. That is CLAUDE.md's single-source-of-truth rule as an
   *event* rather than a risk, and it is the strongest available argument for a mechanical guard: the
   incentive to keep improving the first encoding is permanent.
6. **"Byte-identical" is underdetermined, and the underdetermined character is where the tolerance
   hides.** A YAML block scalar ends in `\n` and a fence's content does not, so *some* decision gets
   made whether or not it is written down — and the natural reach is `.trim()`, which also swallows
   indentation. Two independent `for:`-shaped normalisations in this Gate each measured correctly
   while concealing a real difference. Writing the comparison policy at the compare site is not
   documentation; it is the decision.
7. **Read the pair count before the drift number.** `DRIFT=0` over a silently shrunken set is
   indistinguishable from success. This is the same shape as every empty-set vacuity in this task,
   and it is why the positive control asserts *this task's three alerts specifically* rather than
   merely "non-empty" — the pre-existing rules would otherwise carry the pass.
8. **When two encodings of one predicate cannot be collapsed, do not restate the second — read it.**
   Prometheus reads a config file and kustomize cannot glob, so neither can call
   `loadable_rules_files()`. The guard parses each of them and asserts set equality **naming which
   direction diverged**, because *linted but never loaded* costs detection and *loaded but never
   linted* costs disclosure. "Sets differ" would have been true and useless.
9. **An honest measurement beats a plausible procedure.** The dump subsection asked whether MC can
   core-dump unasked. Reading the pod gave `ulimit -c unlimited` and a host-global `core_pattern` our
   manifests do not control — a standing exposure no procedure covers. That one measured sentence is
   worth more than the whole handling procedure written over an assumption.
