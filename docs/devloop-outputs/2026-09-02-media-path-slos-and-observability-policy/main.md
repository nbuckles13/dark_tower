# Devloop Output: Media-path SLOs, ADR-0011 amendments, and label/units policy

**Date**: 2026-09-02
**Task**: Create `docs/observability/slos.md`; amend ADR-0011 (strike MH audio-jitter objective, redefine MH forwarding latency measurement point, fix `MHHighJitter` alert-naming example); extend `label-taxonomy.md` (`key_custody`, identity/meeting-id bans), `dashboard-conventions.md` (units chain + Periodicity), and `alert-conventions.md` (non-zero-denominator guard).
**Specialist**: observability
**Mode**: Agent Teams (v2) — full, HEADLESS RUN (run-story task #7)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~4h (setup 03:12 → commit; Gate 1 four reconciliation rounds, Gate 3 three fix rounds)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `020e8a022d530bd48b32f846b4785d844b7cd7db` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Story | `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` task 7 |
| Headless | yes (`DEVLOOP_HEADLESS=1`) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (Gates 1-3 passed 2026-09-02) |
| Implementer | `implementer` (observability) |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `security` — Plan confirmed |
| Test | `test` — Plan confirmed |
| Observability | n/a — implementer slot |
| Code Quality | `code-reviewer` — Plan confirmed |
| DRY | `dry-reviewer` — Plan confirmed |
| Operations | `operations` — Plan confirmed (conditional; conditions accepted) |
| Semantic Guard | `semantic-guard` — Plan confirmed |
| Protocol (conditional, added at Gate 1) | `protocol` — Plan confirmed |

---

## Task Overview

### Objective

Land the observability documentation contract ADR-0036 requires of this story: the missing `docs/observability/slos.md`, the ADR-0011 amendments it depends on, and the label / units / periodicity / ratio-guard policy that the rest of the story's metrics work is written against.

### Scope
- **Service(s)**: none directly — documentation + ADR amendments. Cites MH, MC, GC, client SDK config keys and constants.
- **Schema**: No
- **Cross-cutting**: Yes — ADR-0011 is jointly owned (observability + operations per ADR-0011:40 for `slos.md`).

### Debate Decision

NOT NEEDED — ADR-0036 (§4, §10, §11) and its amendments table (line 1272) already ratify every decision this task records. The task transcribes ratified decisions into their canonical homes; it does not make new ones.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. "Mine" = observability (I am the implementing specialist).

| File | Classification | Owner | Why |
|---|---|---|---|
| `docs/observability/slos.md` (NEW) | **Joint — Domain-judgment** | observability **+ operations** | ADR-0011:40 names this jointly owned. Error budgets, burn-rate posture and paging consequence are operations' half; SLI definition and measurement point are mine. @operations must co-sign before it lands. |
| `docs/decisions/adr-0011-observability-framework.md` | **Joint — Domain-judgment** | observability **+ operations** | Striking an objective (L124) and redefining another's measurement point (L123) changes what the fleet promises. Ratified by ADR-0036's amendments table (L1266-1267), so this is transcription, not a new decision — but the SLO section is jointly owned. L367 alert-naming example is Mechanical. |
| `docs/observability/label-taxonomy.md` | **Mine** | observability | Label taxonomy is observability's sole artifact. Adding `key_custody` + the identity/meeting-id prohibition. |
| `docs/observability/dashboard-conventions.md` | **Mine** | observability | §Units rescope + throughput row + new §Periodicity. |
| `docs/observability/alert-conventions.md` | **Mine** (operations reviewer) | observability | Conventions doc is mine (ADR-0031); the new subsection is PromQL correctness, not severity routing. @operations reviews because it constrains every future ratio alert they own. |
| `docs/observability/alerts.md` | **Not mine — Mechanical** | operations | Two changes. (a) `:929` — retire the `(to be created)` hedge now the file exists. (b) **`:738-750` (added post-approval, @team-lead ruled fix-now)** — the MH block listed **`MHHighJitter`** under *Planned Warning Alerts* for the objective **this commit strikes**, so the dangling reference is one this changeset *creates*, not pre-existing debt. Entry **deleted, not repointed** (a planned-alert list is a work queue). Also: `mh-alerts.yaml` "🚧 To be created" corrected — it exists and ships 15 alerts, none inventoried; and `MHHighAudioLatency`'s pre-amendment `>30ms` removed from the candidate list, since a threshold is not ratified apart from its measurement point. |
| `docs/API_CONTRACTS.md` (`:244` only) | **Not mine — Minor-judgment** | **protocol** — *now ON the panel* | **G5, option (a), classification restored by @team-lead.** @protocol was added as a conditional domain reviewer per the devloop skill §Team Composition rule (Protocol joins when API contracts are affected by a non-Protocol implementer — not optional, and this is squarely it). My Mechanical downgrade is withdrawn: the *edit* changed from trim to append, which is a different edit rather than the same edit argued down, and ADR-0024 §6.2's monotonicity rule exists so "it got smaller" does not become a habit. Append-only pointer; **delete nothing**; `:240-245` contract sentences stay verbatim; pointer attaches to the `key_custody` mechanics clause specifically, never at section level. |
| `docs/observability/dashboards.md` (`:220`, `:222`, and the stale MH status block) | **Mine — Minor-judgment** | observability | **G1, @dry-reviewer.** A *third* encoding of the ADR-0011 MH SLO rows: `:220` restates the forwarding target, `:222` restates the **jitter objective this task strikes**. Striking ADR-0011:124 while leaving this reproduces the exact dangling-example defect one file over, in the doc a dashboard author reads first. Verified nothing implements it (zero `jitter` hits in `infra/grafana/dashboards/` and `crates/mh-service/src/`). Also demotes both to pointers at `slos.md` per C3. |
| `docs/runbooks/gc-incident-response.md` | **Not mine — Mechanical** | operations | Two adjacent one-line hedges, both `(to be created)` for files that exist. `:2035` (`slos.md`) as approved, and **`:2034` (`docs/observability/metrics/gc-service.md`) added post-approval on @team-lead's fix-now ruling** — same defect class, immediately adjacent line, same file. Pointers, not content. |
| `docs/TODO.md` | **Mixed — Minor-judgment** | observability (new entries D1-D9); the existing `MCRegisterMeetingFailureRate` entry names observability + operations | Two changes in one file. (a) **Retarget in place** the existing MC entry's false "`slos.md` **does not exist**" assertion, which this commit falsifies — appending would leave the false claim standing and grep-findable. (b) **Nine new debt entries** (D1-D9) from verified premise failures and stale claims found while implementing and at Gate 3. |
| `docs/devloop-outputs/2026-09-02-media-path-slos-and-observability-policy/main.md` | **Mine** | observability | My devloop record. |
| `infra/docker/prometheus/rules/_template-service-alerts.yaml` | **Mine** | **observability** | **Ownership corrected at Gate 1 by @operations, verified.** I had this as service-specialist-owned; ADR-0031 scopes that to `<svc>-alerts.yaml`, and ADR-0031:109 lists this template in the **Conventions doc set — owner: observability**, bundled with the two convention docs this task already edits. Guard the `<Svc>HighErrorRate` ratio example at `:26-31`. Guard-exempt (`_template-*.yaml` glob) and NOT LOADED BY PROMETHEUS, so no guard will ever catch the omission and the edit has zero runtime exposure. |
| `infra/docker/prometheus/rules/mh-alerts.yaml` (comments `:11-15`, `:27` only) | **Not mine — Minor-judgment**, route (A) **approved by @team-lead** | media-handler (authoring owner, ADR-0031) — **ABSENT from this panel**; operations co-signs as rules-directory owner (ADR-0011:41) | **C1.** Comment wording only; no rule added or changed. The current text names the missing *file* as the blocker — **this commit is what falsifies that**; the real gate is the ratified *target* (story 8). Shipping a change that knowingly leaves a false statement in the tree is what "fail loudly; never mask" forbids. Trailer: `Approved-Cross-Boundary: operations`, naming the basis (rules-dir owner per ADR-0011:41). **NO `Approved-Cross-Boundary: media-handler` trailer is written — media-handler did not co-sign and the record must not imply they did.** Their absence is surfaced as a real item flagged for story close. |

**Explicitly NOT touching** (recorded as debt with owners instead):
`packages/sdk-core/src/telemetry/telemetryConfig.ts` and `packages/sdk-core/src/media/events.ts` (client — D2 and the label divergence; task 19),
`crates/mh-service/src/observability/metrics.rs` (media-handler — the named objective constant and sample-ratio key land in task 16),
`crates/dt-guard/**` (infrastructure — the unenforced-prohibition gap security raised),
per-service `{gc,mc,mh}-alerts.yaml` **rules** (service specialists per ADR-0031 — the seven unguarded ratio alerts are named in the doc, not edited).

---

## Planning

### Mechanism restatement (wider class than the task names)

Instance-language: *"document the ratio guard, the units chain, and the scrape cadences."*

Mechanism-language: **these three are the same defect — a convention that exists only as copies at
its use sites and as prose nowhere, so the artifact a new author copies from does not carry it.**
The ratio guard is the clearest case: it exists in 11 hand-copied instances and the one artifact
designed to propagate it, `_template-service-alerts.yaml`, does not carry it. Documenting the
convention without seeding the template leaves the propagation path still broken — the next service
copies the unguarded shape and the doc is what nobody read. Same-owner siblings the task does not
name: the alert template (propagation point), and `docs/observability/dashboards.md`/`alerts.md`
(the other two "conventions live in copies" surfaces). **Surfaced to reviewers; I am not widening
scope unilaterally** — the template is service-facing and ADR-0031-owned.

### Three premise failures (verified, not inferred)

The task instructed me to cite real config keys and to say so explicitly if one does not exist.
Three of the five do not hold as stated. I will write what is true and record each as debt.

**D1 — MH is NOT scraped at 5 s in the cluster.** `infra/docker/prometheus/prometheus.yml:44`
does set `scrape_interval: 5s` for `mh-service` (and `:30`/`:37` set 10 s for gc/mc), but that file
is **dead** — there is no compose service mounting it, and the live path is
`infra/kubernetes/observability/prometheus-config.yaml`, which is in the observability
kustomization and **declares no per-job `scrape_interval` at all**: every job inherits
`global.scrape_interval: 15s` (its only occurrence, `:27`). So the cadence the task asks me to
publish as policy is three times slower than stated wherever it actually runs. Writing "MH: 5 s,
cite `prometheus.yml`" would document a fiction and mask a live gap — CLAUDE.md forbids both, and
this is exactly the "two places encode the same value" drift the SSoT rule names. **Plan**: §Periodicity
states the policy cadence and names the live config key, and records the drift as an open item;
`docs/TODO.md` gets D1 with owner infrastructure + operations (cost: MH at 5 s is 3× the series
ingest of 15 s, which is operations' call, and infrastructure is not on this panel).

**D2 — the client SDK OTel export interval is not configured.**
`packages/sdk-core/src/telemetry/telemetryConfig.ts:111` is
`new PeriodicExportingMetricReader({ exporter })` — no `exportIntervalMillis`, and
`grep exportInterval packages/` returns nothing. The reader therefore runs at the OTel JS default
(60 s), not 10 s. **There is no config key to cite.** Per the task's own instruction I state this as
an open item pointing at the task that lands it (task 19, the SDK pipeline that first needs the
cadence) rather than inventing a key name.

**D3 — the ratio-guard count is 11, not 15, and it is not universal.** Verified by parsing every
rule block: the `and sum(rate(...)) > 0` guard appears at `gc-alerts.yaml:294,361,381`,
`mc-alerts.yaml:202,222,242`, `mh-alerts.yaml:78,98,134,216,236` — **11**. `_template-service-alerts.yaml`
carries **zero** (its `<Svc>HighErrorRate` is an unguarded ratio). And "every ratio alert already
carries it" is false: `GCHighErrorRate`, `GCErrorBudgetBurnRate{Critical,Warning}`,
`GCMCAssignmentFailures`, `GCTokenRefreshFailures`, `GCDatabaseDown` and `MCGCHeartbeatWarning` are
unguarded ratios. (The `*HighMemory` ratios are correctly unguarded — a container memory *limit*
denominator is never zero.) I document the convention with the verified count and name the
non-conforming set, rather than asserting a uniformity the tree does not have.

**Two premises that DO hold**: the client SDK threads `meeting_id_hash` into the media module
(`packages/sdk-core/src/media/events.ts:44` — "Join label set threaded from MeetingSession
(`client_version`/`meeting_id_hash`/`org_id`)"), exactly as the task describes; and MH has **no**
named objective constant and **no** sample-ratio key in
`crates/mh-service/src/observability/metrics.rs` today — task 16's prompt commits to landing both,
so `slos.md` forward-references task 16 rather than a nonexistent symbol.

### What lands where

1. **`docs/observability/slos.md` (new).** Decided vs open, explicitly separated. Decided: the MH
   media-path forwarding SLI and its measurement point (ingress-read-complete → egress-enqueued);
   that join-to-first-media is an **objective, never a gate** (ADR-0036 §10 — a wall-clock assertion
   on a local cluster is a permanent flake, ADR-0028 forbids quarantining quality gates, so the test
   would be deleted and the headline objective would end with zero coverage); the existing AC/GC/MC
   targets carried over from ADR-0011 as the current register. Open: the ratified MH number (story 8),
   pointing at task 16's named constant as the future single source rather than restating a figure.
   Also records the MH jitter objective as struck, so a reader of the old ADR row lands somewhere.
2. **ADR-0011.** L124 jitter row struck with reason; L123 forwarding row gains its measurement point;
   L367 `MHHighJitter` example replaced with a real MH alert name.
3. **`label-taxonomy.md`.** `key_custody` → Shared Label Names table, bounded to the single value
   `operator`; the end-to-end/zero-trust-boolean prohibition; the meeting-identifier and
   media-path-identity rule **stated, not derived** (§11's own instruction, because four specialists
   had to be corrected during the debate), with the ADR-0028 grandfathered `meeting_id_hash` set
   named as closed-and-not-extended and media-path client metrics exempted to
   `client_version` + `org_id`.
4. **`dashboard-conventions.md`.** §Units intro rescoped from panel rendering to the config→metric→panel
   chain; a throughput row (config in human/ADR units, metrics bytes-based, panels `Bps`, bits
   equivalence in prose never a multiplication in a query, conversion single-point at config load
   upstream of the fork — live instance: §1's datagram send buffer, expressed in frames of audio
   while quinn takes bytes). New §Periodicity (no existing home — verified: zero hits for
   scrape/export cadence anywhere in `docs/observability/`).
5. **`alert-conventions.md`.** New non-zero-denominator subsection under §Threshold Patterns, with the
   attempts-denominator variant: when the denominator is *attempts* (forwarded + dropped), the guard
   sits on that **sum**, because guarding the forwarded total alone goes silent in the
   everything-dropped case — the exact case the alert exists to catch.
6. **Stale-pointer retirements + TODO updates** as classified above.

### Gate-1 reconciliation (after @security and @operations pre-plan input)

@security and @operations both sent pre-plan input that independently reached D1/D2/D3. All of it is
verified and accepted. Changes to the plan:

**Guard count settled at 14 conjuncts / 11 guards.** @operations counted 14 multi-line `and`
conjuncts; I counted 11 guards. Both are right about different things, and the doc will say so
precisely rather than pick a number. The 14 multi-line conjuncts break down as: **11 non-zero-denominator
guards** (`mh:78,98,134,216,236`; `mc:202,222,242`; `gc:294,361,381`), **1 rate floor**
(`mc:69`, `> 0.1` — not a zero guard), and **2 offset-baseline silence conjuncts** (`gc:123,140`,
which establish that traffic *used to* flow — a different idiom). Two inline conjuncts are neither:
`mc:136` (sanity) and `gc:146` (`and on()` vector matching). Template: **zero**.

**Scope additions accepted, all with owner sign-off:**

- **(D1) `infra/kubernetes/observability/prometheus-config.yaml` — NOT edited; spun out.**
  *(Corrected: an earlier draft of this line recorded @operations as having authorized the edit on
  cost grounds. They did not, and the misattribution is retracted. @operations' actual ruling is
  option (b): they have **no cost objection** — scrape interval changes samples-per-series, not
  cardinality — and they think (a) is the right end state, but "cost is not an objection" is not
  the owner's co-sign. The edit is Domain-judgment on a deployed config owned by **infrastructure**,
  who is absent from this panel, which ADR-0024 §6.3 routes to spin-out.)*
  **CLOSED — @team-lead affirmed (b) with two grounds beyond @operations'**, and the "can
  infrastructure be pulled in to co-sign?" question is answered **no**:
  1. **Owner-review does not clear the tier.** ADR-0024 §6.3 routes Domain-judgment to
     owner-*implements* — a separate owner-implemented devloop, or `--paired-with=infrastructure`.
     Owner-*reviews* satisfies Minor-judgment, not Domain-judgment. `--paired-with` is a spawn-time
     flag and cannot be retrofitted mid-flight, so adding infrastructure to the panel would not have
     fixed the tier.
  2. **Decisive and independent**: @operations' own (a)-path condition requires a host-side
     `kubectl apply -k infra/kubernetes/overlays/kind/observability/` plus a Prometheus reload. This
     container cannot perform host operations, and under the devloop skill's Headless Mode rule 1 a
     host-op the container cannot perform is a **terminal escalation** — `.devloop-escalation.json`
     and a stopped story. Trading a completable documentation task for a stopped story, to land
     three lines the spin-out captures anyway, is not a trade worth making.

  **What (b) is, stated so it is not misread as a dodge**: documenting the true 15 s deployed cadence
  **is** the fail-loudly outcome; publishing 5 s would be the masked one. Deployment path confirmed
  by @operations: `infra/kind/scripts/setup.sh:587` applies
  `infra/kubernetes/overlays/kind/observability/`, a labels-only passthrough over this file.
- **(D3a) Add the guard to `_template-service-alerts.yaml`'s ratio example** (~2 lines). This is the
  propagation-point fix from the mechanism restatement above, requested independently by
  @operations. Documenting the convention while the copy-paste source ships the unguarded shape
  fixes the smaller half.
- **(D4) Precision pass on four stale breadcrumbs**, per @operations: the `docs/TODO.md`
  `MCRegisterMeetingFailureRate` entry asserts "`slos.md` **does not exist**", which is false the
  moment this commits; `mh-alerts.yaml:11-15` and `:27` point at the missing *file* when the real
  blocker is the missing *target*; plus the two `(to be created)` hedges. **What lands here does not
  unblock either alert** — the blocker was never the file, it was the absence of a ratified target,
  which lands in story 8. Said out loud in all four places so the next reader does not re-derive the
  wall. `mh-alerts.yaml` is media-handler's (ADR-0031), also not on this panel.
- **(D5) State target precedence explicitly.** @operations' question: ADR-0011:40 makes `slos.md`
  canonical but ADR-0011:118-124 still holds a live target table, and nothing states which wins.
  Resolution adopted (their preference, and correct): **`slos.md` is authoritative**; ADR-0011's
  table is relabelled as historical initial values with a pointer. Two target tables and no stated
  precedence is a 3am failure.

**Wording constraints accepted from @security** (all verified against the guard sources):

- The new meeting-id / identity rule is tagged **`[reviewer-only]`**, not `[guard-enforced]`, and
  gets a row in the §Machine-Enforced vs Reviewer-Only index at `:431`. Verified: `meeting_id` is in
  **no** guard vocabulary (`pii_vocabulary.rs` has no `meeting` token — only a comment at `:41`), so
  a raw `meeting_id` metric label passes every guard in the tree today. Tagging it enforced would be
  a false coverage claim; the guard-degradation lesson at `8bba6da` is fresh.
- The rule must say in one clause that the existing `_hash`-suffix exemption **does not** cover
  `meeting_id_hash`. Verified the conflict is real and mechanical: `HASHED_SUFFIXES`
  (`pii_vocabulary.rs:286`) contains `_hash` and `_id_hash`, consumed by
  `metric_labels.rs:529 is_hashed_label()`, so `meeting_id_hash` is exempted **by construction**.
  Two doc sections that disagree is how the next author ships it.
- **The media-path client exemption is written as a required end state, not a present fact.** Verified
  the live divergence: `MeetingSession.ts:308` feeds the full join label set into
  `media/events.ts:44`, and `media/__tests__/media-transport.test.ts:270` *asserts*
  `meeting_id_hash` in the media label set. Writing "media-path client metrics carry only
  `client_version` and `org_id`" in the present indicative would legitimise a live leak. Named with
  its file; follow-up filed to `docs/TODO.md` (owner: client).
- Three named surfaces for the identity half (metric labels, log fields, span attributes); "any
  metric anywhere" for the meeting-id half; aggregation floor stated as **pod** (stricter of §11's
  "pod or service").
- The grandfather reproduces its **closedness** — "grandfathered as a set and not extended" — not
  just the exemption, so it cannot be read as an appendable carve-out list.
- `key_custody` bounded values written as a **constraint, not a snapshot**: single permitted value
  `operator`, any addition requires an ADR-0036 §4 amendment. ADR-0036 §4's "today" is not
  transcribed.
- The no-boolean statement keeps §4's claim in both directions — media is encrypted between clients;
  MH, the network and storage cannot read it; **MC, and therefore the operator, can** — covering all
  four carriers (metric, log, dashboard, document), with no wording that lets a reader infer the
  keyless relay makes the system E2E.
- New TODO entry (owner observability + security): the flat prohibition applies "anywhere in this
  design", which is wider than ADR-0036 §11's directory-scoped macro deny, and **nothing enforces
  the gap**.
- Any PromQL example I add carries no absolute `http(s)://` runbook URL
  (`crates/dt-guard/src/alert_rules.rs:70` rejects those) and no token-shaped literals — doc examples
  get copy-pasted into scanned files.

**Accepted from @code-reviewer**: both ADR-0011 edits carry the existing inline-blockquote amendment
convention (`> **Amendment YYYY-MM-DD** (ADR-0036 …): …`, the shape already at ADR-0011:69), and the
`:367` example is replaced with an alert that **actually exists** — `MHTokenRefreshFailures`
(`mh-alerts.yaml`), parallel in shape to the `MCSessionFailures` example beside it — not a second
dangling name.

**One deferral, task-authorized.** @operations would support setting `exportIntervalMillis` in the
SDK; I am not doing it here. It is client-owned, client is not on this panel, and the task prompt
explicitly directs that a missing key be recorded "as an open item pointing at the task that lands
it". Recorded against task 19 with the operational consequence @operations named: a 60 s client
export against a 5–15 s MH scrape means client-side first-media and drop counts cannot be lined up
against MH-side metrics on any incident timeline.

### Gate-1 round 2 — @operations corrections accepted (both verified, I was wrong on both)

**The docker-compose Prometheus config is NOT dead.** I claimed it was orphaned. `docker-compose.yml:98`
mounts it (`./infra/docker/prometheus/prometheus.yml:/etc/prometheus/prometheus.yml`, config-file flag
at `:91`). My grep only covered `infra/docker/`, never the repo root. It is a **live-but-not-deployed
alternate local stack**, which changes the remedy: were it dead, deleting it would be the clean SSoT
fix; since it is live, this is genuine two-config drift and the doc must state **precedence**.

**`_template-service-alerts.yaml` is mine, not a service specialist's.** Verified: ADR-0031 scopes
service-specialist ownership to `<svc>-alerts.yaml`; ADR-0031:109 lists this template in the
**Conventions doc set — owner: observability**, in the same bullet as `dashboard-conventions.md`,
`alert-conventions.md` and `label-taxonomy.md`. So D3a is a `Mine` row and the deferral was never
available. Reinforcing: its header excludes it from `validate-alert-rules.sh` via the `_template-*.yaml`
glob, so **no guard will ever catch this omission**, and it is NOT LOADED BY PROMETHEUS, so the edit
carries zero runtime risk.

**The guard's rationale, rewritten — the obvious justification is wrong.** @operations checked the
PromQL semantics and is right, and this matches what I found independently: with no series
`sum(rate(...))` returns an empty vector and the division yields empty; with flat series it is
`0/0` = NaN, and `NaN > threshold` is false. **Neither case fires.** The guard is therefore *not*
divide-by-zero hygiene, and writing it that way invites the next author to conclude it is decorative
and drop it. The load-bearing reason is the **attempts-denominator inversion**: when the denominator
is `forwarded + dropped` rather than a pre-existing total, guarding on the forwarded total alone
means that in the everything-dropped case the forwarded rate is 0, the guard evaluates false, and the
alert goes **silent precisely when the failure is total**. That is what the doc will say.

**§Periodicity operative numbers are the deployed ones.** @operations' hard condition, accepted: **MH
15 s**, MC/GC 15 s — what actually runs, since the Kind path
(`setup.sh:587` → `overlays/kind/observability/`, a labels-only passthrough) has no per-job override.
The 5 s / 10 s appear as **intended-but-not-in-effect**, attributed to the compose stack. An oncall
must not reason about media-quality resolution from a number that is 3× off.

**D1's recorded reason is ownership, not cost.** Also accepted: scrape interval changes
samples-per-series, not cardinality — it does not multiply series count, and on two MH pods writing
to an `emptyDir` the storage delta is noise. Recording a cost objection that does not exist would
give the next person a phantom obstacle to overcome. The reason is ADR-0024 §6.3 spin-out.

**C1/C2/C3 — all three confirmed.**
- **C1**: route **(A)** — minimal factual correction to `mh-alerts.yaml:11-15` and `:27`, carrying
  `Approved-Cross-Boundary: operations`, media-handler flagged at Gate 3. (B) ships a false statement
  in the meantime.
- **C2**: **retarget, not append.** The `docs/TODO.md` MC entry's "There is a hard blocker …
  `slos.md` **does not exist**" sentence is edited in place so the entry reads true after this
  commit. An appended note leaves the false claim standing and grep-findable, and the next reader
  hits it first.
- **C3**: **`slos.md` is authoritative** for SLO targets; ADR-0011:118-124 is demoted to
  initial/historical values with a pointer. Stated explicitly **in both files**.
  **@code-reviewer Point A, accepted — this is a THIRD amendment to an Accepted ADR** (on top of the
  L123 redefine and the L124 strike), and a demoted-but-still-present target table is exactly the
  surface where a silent relabel becomes a finding. It gets its **own dated inline-blockquote
  amendment trace** in the ADR-0011:69 house style, citing **both** ADR-0036's amendments table and
  **ADR-0011:40's own delegation** of "Current SLO targets" to `slos.md` — so the demotion reads as
  the ADR honouring a delegation it already made, not as an outside edit. Not folded into the
  L123/L124 blockquote: it is a different decision (precedence between two tables) and needs to be
  findable as one. So **three** traced amendments total.

**Two further accepted refinements**: the doc names the offset/floor variants (`gc:123,140`;
`mc:69`) as *deliberately different* idioms, so nobody normalizes them into conformance and breaks
the seasonal-baseline alerts; and the 7-unguarded-ratio TODO entry names each alert explicitly and
records the NaN nuance, so whoever picks it up does not "fix" them with the wrong mental model.
@operations spot-checked my list (`GCHighErrorRate:33`, `GCErrorBudgetBurnRateCritical:101`,
`GCDatabaseDown:83`, `MCGCHeartbeatWarning:148` — four at `severity: page`) and accepted the
deferral: GC/MC specialists are not on this panel and it is seven alerts' worth of judgment.

### Gate-1 round 3 — @test, @dry-reviewer, @security round 2, @operations round 2

**@security A — exemplars are a fourth surface, and the rule as scoped misses them. Accepted; this
is the most valuable catch of the gate.** A Prometheus exemplar is not a metric label, a log field
or a span attribute — it hangs off a histogram bucket with its own label set — so a per-stream
exemplar on the MH forwarding histogram would reconstruct exactly the time-ordered sequence §11
protects **while passing a rule phrased against those three surfaces**. And that histogram is
precisely where someone will reach for one, because it is where this story's SLI is measured.
ADR-0036 §11 rejects exemplars *by name* but only for the per-participant-resolution case, so it
reads as advice about one design choice rather than a prohibition on the surface. Exemplar labels
are named as the **fourth surface**, citing §11 so it reads as ratified rather than as my addition.

**@security B — the template is guard-exempt; say so where I edit it.** Verified:
`alert_rules.rs:413` skips `_template-` files, stated at the module doc `:20`. So the template gets
no `runbook_url` check, no severity check and **no annotation-hygiene secret scan**, while being the
copy-paste source for every service. Keeping the "no annotations block at all" choice for the ratio
example, and adding one line to the template header + the new `alert-conventions.md` subsection
recording that the template is guard-exempt, so a placeholder committed there ships unvalidated into
every service that copies it. Currently that property lives only in a Rust doc comment.

**@security C — separate end-to-end *encryption* from end-to-end *latency*.** Accepted. `slos.md`
discusses join-to-first-media, an end-to-end latency measure, in the same commit that bans
end-to-end booleans. One clause scopes the prohibition to claims of end-to-end **encryption** or
zero-trust as deployment properties; latency measured end-to-end across the media path is a
measurement term and unaffected. Without it, either a reader concludes the docs contradict
themselves, or a future keyword check misfires on the legitimate usage and gets weakened — which is
how a real control dies.

**@security D — one sentence of rationale.** Accepted, held to one sentence: per-frame size and
timing per stream reconstruct who spoke, in what order and for how long, against §11's stated
adversary set (MH, whoever compromises MH, a curious operator — **not** a network observer). §11's
"state the rule, do not derive it" is about not making the author re-derive the *conclusion*; one
sentence of why is what lets them judge the case the rule did not anticipate.

**@dry-reviewer G1 — `docs/observability/dashboards.md:220,222` is a third encoding. Accepted, row
added.** `:222` restates the very jitter objective this task strikes; `:220` restates the forwarding
target. Striking ADR-0011:124 while leaving this reproduces the dangling-example defect one file
over, in the doc a dashboard author reads *first*. Verified: zero `jitter` hits across
`infra/grafana/dashboards/` and `crates/mh-service/src/` — nothing implements it. Also demoting both
rows to pointers at `slos.md` per C3, and correcting the stale "**Status**: 🚧 To be created" block
(`mh-overview.json` exists).

**@dry-reviewer G2 — the mechanism, not just today's drift.** Accepted and it sharpens §Periodicity.
Both Prometheus configs are live (`docker-compose.yml:87-98` mounts the compose one), so this is two
live configs encoding three cadences and already disagreeing — not one dead and one live. Since D1
resolves as (b), the reciprocal `ANCHOR (DRY):` comments and the sync guard become part of the
spun-out D1 spec rather than this diff; the repo precedent they cite
(`packages/sdk-core/src/validation/limits.ts:94`, plus `validate-subdomain-regex-sync.sh` /
`validate-slug-class-sync.sh`) goes into the TODO entry so the spin-out has a shape. **The
doc-side half is mine and lands here**: §Periodicity names *which file is authoritative*, because
"cite the config key" is ambiguous when there are two keys per cadence — the subsection would
otherwise inherit the ambiguity it exists to remove.

**@dry-reviewer G3 — the clause goes at `:200` too.** Accepted. One canonical statement in the new
rule plus a pointer from §Hashed / opaque suffixes at `:200`, which is `[guard-enforced]` and
mechanically wins. A reader who lands on §Hashed-suffixes must not be able to leave it believing
`meeting_id_hash` is fine.

**@test — ADR-0028 citation nuance. Accepted; I would have overstated it.** Verified ADR-0028:223-226:
the literal policy is quarantine within 24 h, then fix-or-delete within a sprint; the only
never-quarantine class is crypto tests. So "ADR-0028 forbids quarantining quality gates" is not a
blanket rule. I keep ADR-0036 §10's outcome logic, which survives intact — a permanent wall-clock
flake is unfixable, so it exits quarantine by *deletion*, and the headline objective ends with zero
coverage — but I state it as that chain rather than as a blanket no-quarantine prohibition.

**@test — name the coverage that DOES exist.** Accepted, and it fixes a real weakness: "objective,
never a gate" left alone reads as "unverified". `slos.md` will point at the structural coverage,
story R-2 (line 21): client mute stops all media leaving the device within one frame, **asserted at
the client transport seam on egress count staying flat — never by absence of audible output**. That
is deterministic and gate-able, and it is the answer to "so what does verify the media path".

**@test — no bare integer for the guard count.** Partially accepted, reconciled with @operations'
request for a stated count. The doc gives the count **with the date and the exact reproducing
command**, plus the file:line list, so it is re-derivable rather than asserted and rots visibly
rather than silently. @test is right that the task's own suggested grep (`grep -rn 'and sum(rate'`)
returns **zero** — the guards are multi-line YAML — which is itself worth recording, since that is
the grep a future author will try first.

**@operations round 2 — burn-rate rows: present, and mostly not empty. Accepted, better than my
proposal.** Only **two** cells are genuinely unknown (the target, and the error budget derived from
it). Everything else is already decided by ADR-0011:127-129 and gets written down now: Critical
>10× burn for 1 h, Warning >5× for 6 h, 30-day measurement window. Story 8 then ratifies **one
number** instead of re-litigating the whole alerting posture because the table looked blank. The two
unknown cells state the blocking condition and landing task in-cell. Rationale accepted verbatim: a
table with the burn-rate rows *absent* is indistinguishable from an SLO never meant to have
burn-rate alerting, and that indistinguishability is how `mh-alerts.yaml`'s omission survived four
months behind a green dashboard panel.

**@operations round 2 — one rule, one source.** The "no MH burn-rate alert may be authored until the
target lands" statement must read identically in `slos.md` and in the `mh-alerts.yaml:11-15` comment
corrected under C1. Two sources for one rule is how this started.

**@dry-reviewer G5 — the key-custody rule would be its third copy. Accepted.** Verified both existing
statements: ADR-0036:1097 (the decision record — keeps its statement, that is its job) and
`docs/API_CONTRACTS.md:244`, which states the *telemetry* rule and would become an operational copy
that can disagree with mine. Resolution: **`label-taxonomy.md` becomes the canonical operational
home** (label definition, bounded values, prohibition); `API_CONTRACTS.md:244` is trimmed to its
contract-level accuracy claim — "media is encrypted between clients; MH cannot read it; MC can",
which *is* API_CONTRACTS' job — with a pointer to the taxonomy for the telemetry spelling. Their
catch on scope is sharp: my task text widens the prohibition to "metric, log, **dashboard** or
document" where ADR-0036:1097 says "metric, log, or document". **Only the canonical copy carries the
widened wording**, or the two read as different rules. Adds an `docs/API_CONTRACTS.md` row.

**@dry-reviewer G6 — the units row's cited instance is not in the tree, and there is ratified prior
art. Accepted, with one correction to their evidence.** Their grep reported zero hits for the
datagram send buffer; I get **two**, but both are unrelated —
`crates/mh-service/tests/transport_seam_reachability.rs:65` (a test fn name) and
`crates/mh-service/src/transport/mod.rs:444` (an error-display test). So their conclusion stands:
**there is no datagram-send-buffer config key**, and writing it as a present-tense example would be
a fiction of the same shape as D2. The row therefore cites **two** instances, both forward-referenced
by landing task: the datagram send buffer in frames of audio (story 1) and `MH_EGRESS_BUDGET_BPS`
(story 2, already specified at story line 83 — "bits/s, converted once at load upstream of the
enforcement/gauge fork; nothing downstream sees bits", which is verbatim my throughput row's
conversion clause, already ratified). Two instances is also what makes it a rule rather than a
one-off, which is the case for it being in a conventions doc at all. And for §Periodicity: the
sample-ratio gauge cites `mh_media_egress_budget_bytes_per_second{basis="unmeasured"}` (same story
line) as the existing "config value published as a gauge" precedent, rather than inventing the
pattern.

**@security ruling on the G5 widening and the API_CONTRACTS trim — accepted, and it narrows my cut.**
The "dashboard" widening is **absorbed into the taxonomy, no ADR-0036 amendment**: it is a ratchet
(strictly stricter, relaxes nothing) within §11's evident intent, and ceremony for a prohibition that
only broadens buys nothing. But it must not land as a silent textual difference from ADR-0036:1097 —
the default reconciliation goes the wrong way, with a future author reading the extra carrier as a
transcription error and "correcting" it down to §11's three, silently deleting dashboards from the
prohibition as a non-security-reviewed edit. So the taxonomy states the delta and its reason in one
clause: dashboards are named explicitly because a Grafana panel is the most likely place someone
renders a custody boolean (a stat panel reading "E2E: true" is a **product claim rendered to an
operator**), and because a dashboard JSON is not obviously a "document" to the person adding a panel.

On `API_CONTRACTS.md:244`, @security's cut line supersedes my looser "trim to the accuracy claim":
**two sentences stay in place, not behind a pointer** — the both-directions accuracy claim (media
encrypted between clients; MH, transport and storage cannot read it; **MC can**, with the
operator-custody risk acceptance) and **the prohibition itself** (never described as end-to-end
against the operator or as zero-trust). Reason accepted: `API_CONTRACTS.md` is what an integrator or
anyone drafting external-facing copy reads, and it is the file an overclaim propagates outward from;
a pointer is no substitute when the failure mode is someone writing an SDK README off this file and
not following the link. That is the same claim serving two readers, not harmful duplication. Only the
**operational mechanics** become a pointer — that telemetry carries `key_custody=operator`, its
boundedness, which surfaces carry it, the four carriers. `:245` ("No key material crosses the MC→MH
contract, and none rides on the roster") is untouched: a wire-contract statement with no home in the
taxonomy. If @dry-reviewer objects that this leaves two copies of the accuracy claim, it routes to
@security rather than being resolved in-thread.

**@operations round 3 — the stale dashboard index is a class of two, both mine. Accepted; all
verified on disk.** Two refinements to G1 and one addition:

- **The jitter line at `:222` is deleted, not pointed at `slos.md`.** @operations is right and I had
  this wrong: it sits in a **planned-panels** list, so demoting it to a pointer would instruct whoever
  builds the MH SLO panels to build a jitter panel for a struck objective against a metric that will
  never be emitted. Strike the objective, strike the planned panel.
- **`dashboards.md:215-216` and `:134` are the same one-line defect twice.** Both say
  `**Status**: 🚧 To be created` / `(planned)` for dashboards that exist: `mh-overview.json`
  (53,638 B) and `ac-overview.json` (75,849 B). This is the index an oncall consults to learn what
  exists, and for the service this whole story is about it says there is nothing to open — at 3am
  that means hand-rolling PromQL against a deployed 53 KB dashboard nobody knew was there. I am
  already editing `:220`/`:222` inside the MH block; per the protocol's anti-pattern check this is a
  **class of two, not a scope question**, so both are fixed (four lines). Verified the other two
  blocks are *accurate*: `:232` (`platform-overview.json`) and `:248` (`database-performance.json`)
  genuinely do not exist.
- **The broader index gap is a TODO, not this task.** 14 dashboard JSONs exist
  (`ac`/`gc`/`mc`/`mh` × overview/slos/logs, plus `errors-overview.json` and the template); the index
  lists almost none. Filed with the count so it is not rediscovered from scratch. Owner:
  observability.

@operations also flagged `docs/API_CONTRACTS.md` needs its row classified before it lands — it
already carries one (Not mine, **Minor-judgment**, owner protocol, protocol absent and flagged to
@team-lead), and their narrow condition matches @security's independently: the operator-custody
accuracy claim stays legible on the contract surface, and only the prohibition's *operational
spelling* moves to the single source.

**@test — the changeset mutates the tree state the doc describes. Accepted; I had not spotted this
and it would have shipped self-contradictory.** Seeding the guard into
`_template-service-alerts.yaml` in *this* commit means the doc must describe the **post-change**
state. Three consequences:

- **The template is not framed as a counter-example.** After the edit it carries the guard. The
  framing becomes: this changeset **seeds** the previously-missing guard into the template so the
  convention propagates to future services — and the reason the template was the gap is recorded
  (`_`-prefixed, not loaded by Prometheus, skipped entirely by `dt-guard` per
  `alert_rules.rs:413`), because that is why nothing ever caught it.
- **The count stays 11, and the doc says why.** The template's guard is an **example in a
  non-loaded file**, not a live rule instance, so it is not a 12th. The doc states the live count
  (11 across `{gc,mc,mh}-alerts.yaml`) and separately records that the template now carries the
  seeded example — rather than a single blended number that means neither thing.
- **The reproducing command is re-run against the working tree at the end**, with its actual output
  pasted, so a reader who runs it gets a match. A count captured pre-edit mismatches on day one.

**@dry-reviewer G5 ownership wrinkle — taking option (a), pointer-only.** They are right that
trimming normative text out of `docs/API_CONTRACTS.md` is a cross-boundary edit into an absent owner
(protocol), same shape as D1 at smaller stakes; and that the file is **not** a GSA (not in
`cross-boundary-ownership.yaml`, which enumerates only `proto/**`, `crates/**`,
`db/migrations/**`), so no trailer is guard-required. Option (a) — leave `:244` intact and **append**
a pointer naming `label-taxonomy.md` as the canonical telemetry home — is value-neutral,
structure-preserving, deletes no normative text, needs no absent owner, and still removes the
*authority* ambiguity that G5 was actually about. The row downgrades from Minor-judgment to
**Mechanical**. It also satisfies @security's constraint trivially, since nothing is removed: both
the both-directions accuracy claim and the prohibition stay exactly where an integrator reads them.
Flagged to @security for confirmation, since their ruling asked for the mechanics to *move* and (a)
leaves them in place with subordinated authority; (b) remains available as a trivial follow-up.

**G6 grep reconciled**, no action: the delta was case-sensitivity only. @dry-reviewer also ran
`grep -rniE "send[_ ]?buffer" crates/ --include=*.rs` → **zero hits**. Conclusion double-confirmed
from both directions; neither of us re-runs it.

**Spin-out tracking**: per review-protocol §Spin-out tracking the `docs/TODO.md` entry for D1 is
**@dry-reviewer's** at verdict time, not mine. I supply the shape (`ANCHOR (DRY)` + sync guard) and
the target slug if one is assigned.

**@security — pointer scoping on `API_CONTRACTS.md:244`. Accepted; it closes the mirror-image
failure of option (a).** Pointer-only has a risk the full trim does not: a pointer placed at section
level, or worded broadly ("for key-custody telemetry policy see `label-taxonomy.md`"), *becomes the
warrant for the deletion it was meant to prevent* — a future reader arrives to dedup, reads it as
"the taxonomy owns this material", and removes the paragraph including the two contract-level
sentences. So the pointer is scoped, not general:

- attached **adjacent to the mechanics clause** (the `key_custody=operator` sentence), never at the
  end of the paragraph or under the heading;
- worded to name what is canonical **elsewhere** and by implication what is not — the label's
  definition, bounded values and carrier surfaces live in `label-taxonomy.md`, and on divergence the
  taxonomy wins **for the label mechanics only**;
- explicitly not making the taxonomy canonical for the accuracy claim or the prohibition, which stay
  authoritative in the contract doc. One clause carries both halves: the claim and the prohibition
  are stated here as contract; the telemetry label that carries them is specified in the taxonomy.

**@test — the "seven unguarded ratio alerts" figure gets the same re-derivable treatment as the 11.**
Accepted: it is another asserted count and would rot the same way. It ships with its reproducing
command and the explicit list (`GCHighErrorRate`, `GCErrorBudgetBurnRateCritical`,
`GCErrorBudgetBurnRateWarning`, `GCMCAssignmentFailures`, `GCTokenRefreshFailures`, `GCDatabaseDown`,
`MCGCHeartbeatWarning`), re-run against the working tree at the end alongside the 11. @test
independently reproduced the 11 and agreed `mc:69` and `gc:123,140` are correctly excluded as
distinct idioms.

### §Periodicity — writing constraints from @team-lead (binding)

D1 resolving as (b) makes §Periodicity the only place the truth gets recorded, so its wording is
specified rather than left to me:

- The **deployed cadence is 15 s** for MH, MC and GC, and that is what triage actually gets. Stated
  plainly, up front.
- 5 s / 10 s exist in `infra/docker/prometheus/prometheus.yml` and are **intended-but-not-in-effect
  on the deployed path**. Named as such.
- **Which config is authoritative is named explicitly** — @dry-reviewer's G2 ambiguity must not
  survive this task, since "cite the config key" is meaningless while two keys per cadence disagree.
- **D1 is named as the item that closes the gap.**
- **Do NOT soften into "cadences vary by environment."** That phrasing converts a defect into a
  design property and is precisely the masking CLAUDE.md forbids.
- @operations' judgment that **a forwarding-latency histogram is near-useless for triage at 15 s**
  is carried into the entry — it is the point of the entry, not colour.

### Scope — CLOSED at twelve files

@team-lead approved the six Gate-1 additions: every one is a same-family stale-claim defect,
independently reviewer-verified, and cutting them would leave the tree asserting things this commit
makes false. **Scope is now closed** — anything further goes back to @team-lead before I write it.

### Non-goals

No alert rule is added (that is task 21 / the MC entry's second stage). No metric is defined. No
number is invented — every figure is either cited to its config key or declared open with the task
that ratifies it.


---

## Pre-Work

None — tree clean at `020e8a0`.

---

## Implementation Summary

Docs + one guard-exempt YAML template. **No code, no metric, no alert rule, and no number invented** —
every figure is either cited to its config key, carried from an existing ratified source, or declared
OPEN with the task that ratifies it.

### 1. `docs/observability/slos.md` (new, 220 lines)

Authority stated first: this file is authoritative for SLO targets, ADR-0011's table is
initial/historical, and **on divergence this file wins**. Joint ownership is stated as a *failure
mode* — the halves are split explicitly (SLI + measurement point = observability; error budget,
burn-rate posture, paging consequence = operations) because "nobody's sole property" is why the file
went unwritten for months.

Live register carries the existing AC/GC/MC targets. The **MH media-path forwarding SLO** separates
decided from open: the measurement point (**ingress-read-complete → egress-enqueued**) is decided,
with its exclusions stated and the reason the predecessor was unfalsifiable (no measurement point, so
any measurement could be argued into compliance). Target, error budget: **OPEN, story 8**. Per
@operations, burn-rate rows are **present and mostly filled** — only two cells are unknown; shapes,
window and policy are already decided by ADR-0011 and written down, so story 8 ratifies one number
rather than re-litigating the posture from a blank table.

**Join-to-first-media** recorded as objective-never-a-gate with the three-step chain (@test's
correction: not a blanket ADR-0028 no-quarantine claim — flake → quarantine → *fix-or-delete* →
deletion → zero coverage). Paired with **what IS gated**, so it does not read as unverified: story
R-2's structural assertion at the client transport seam on egress count staying flat.

**MH audio jitter struck** with reasoning. Forward references (task-16 objective constant,
sample-ratio gauge) are written in the forward-reference voice and say plainly they will not grep.

### 2. `docs/decisions/adr-0011-observability-framework.md` — three traced amendments

All in the existing `> **Amendment YYYY-MM-DD**` house style at `:69`:
(a) **table demotion** — precedence to `slos.md`, citing ADR-0011:40's own delegation so it reads as
the ADR honouring a commitment it already made, kept as its own blockquote per @code-reviewer;
(b) **forwarding-latency measurement point** redefined, noting the `< 30ms` figure predates it and is
not ratified against it; (c) **jitter struck**. Table rows marked inline (`~~struck~~`,
"measurement point redefined"). `MHHighJitter` → **`MHTokenRefreshFailures`**, an alert that exists.

### 3. `docs/observability/label-taxonomy.md` — +150 lines

`key_custody` row added, bounded as a **constraint not a snapshot** (single value `operator`;
extension requires an ADR-0036 §4 amendment). New **§Key custody** with the no-boolean prohibition
carrying §4's claim in both directions, @security's **encryption-vs-latency scope clause**, and the
**dashboards-widening delta stated with its reason** so it is not "corrected" back down. New
**§Media-path identity** with R1 (no meeting id, raw or hashed — both grounds, closed grandfather),
R2 (no participant/stream identity; **exemplars named as the fourth surface** per @security), R3
(client exemption **as a required end state**, with the live divergence and its asserting test named).
Closes with an **enforcement-reality** block: `[reviewer-only]`, no guard covers this, and why a
vocabulary entry would be inert. Cross-pointer added at §Hashed suffixes; four index rows added.

### 4. `docs/observability/dashboard-conventions.md` — +112 lines

**§Units rescoped** to the config → metric → panel chain. **Throughput row** + subsection: config in
human/ADR units, single-point conversion at load upstream of the fork, bytes-based metrics, `Bps`
panels, bits equivalence in prose never a multiplication in a query — with both live instances as
forward references (datagram send buffer in frames; `MH_EGRESS_BUDGET_BPS`).

**New §Periodicity.** Names **which config is authoritative** (the k8s one) before citing any key —
@dry-reviewer's G2 ambiguity. States the **deployed cadence is 15 s**, with 5 s/10 s as
intended-but-not-in-effect, carries @operations' judgment that a forwarding-latency histogram is
near-useless for triage at 15 s, and explicitly refuses the "cadences vary by environment" framing.
Client export interval and MH sample ratio recorded as **no key exists** with landing tasks.

### 5. `docs/observability/alert-conventions.md` — +117 lines

**§Non-Zero-Denominator Guard.** Leads with **the reason is NOT divide-by-zero** (NaN semantics
already suppress it), then the load-bearing case: the **attempts-denominator inversion**, with a
wrong/right pair showing that guarding one addend goes silent in the everything-dropped case. Names
the three deliberate look-alike idioms so nobody normalises them. Coverage stated with its
**reproducing command**, re-run post-edit.

### 6. `infra/docker/prometheus/rules/_template-service-alerts.yaml`

Guard seeded into the ratio example with the "not divide-by-zero" reason inline, plus a header note
that the file is **guard-exempt and that cuts both ways** (@security) — no runbook_url check, no
severity check, no annotation-hygiene secret scan.

### 7-12. Stale-claim corrections

`alerts.md:929` + `gc-incident-response.md:2035` hedges retired. `dashboards.md`: AC/MH status blocks
corrected (both dashboards exist); MH SLO rows demoted to pointers; **jitter panel deleted, not
demoted** (@operations — it sat in a *planned-panels* list, so a pointer would instruct someone to
build a panel for a struck objective). `API_CONTRACTS.md:244`: pointer **appended adjacent to the
mechanics clause**, nothing deleted. `mh-alerts.yaml`: both omission comments retargeted from the
missing *file* to the unratified *target*. `docs/TODO.md`: MC entry's false assertion **retargeted in
place**; nine new entries, D1-D9.

### Verification

All guards green: `alert-rules`, `metric-labels`, `application-metrics`, `dashboard-panels`,
`metric-coverage`, `kustomize`, `no-pii-in-logs`, `cite-no-line-numbers`, `cite-symbol-resolves`,
`cross-boundary-classification`. (`no-pii-in-logs` emits one pre-existing WARNING in
`ac-service/src/services/token_service.rs:326`, untouched by this change.)

**Reproducing command re-run against the post-change tree**, per @test — line numbers in
`mh-alerts.yaml` **shifted** when its comments grew (`78,98,134,216,236` → `87,107,143,225,245`), and
the doc was corrected to the post-edit values and each verified individually. This is exactly the
staleness @test predicted.

---

## Files Modified

| # | File | Change |
|---|---|---|
| 1 | `docs/observability/slos.md` | **NEW**, 220 lines |
| 2 | `docs/decisions/adr-0011-observability-framework.md` | 3 traced amendments + alert-name fix |
| 3 | `docs/observability/label-taxonomy.md` | +150: `key_custody`, §Key custody, §Media-path identity, cross-pointer, 4 index rows |
| 4 | `docs/observability/dashboard-conventions.md` | +112: §Units rescope + throughput, new §Periodicity |
| 5 | `docs/observability/alert-conventions.md` | +117: §Non-Zero-Denominator Guard |
| 6 | `infra/docker/prometheus/rules/_template-service-alerts.yaml` | Guard seeded; guard-exempt warning |
| 7 | `docs/observability/dashboards.md` | AC/MH status; SLO rows → pointers; jitter panel deleted |
| 8 | `docs/observability/alerts.md` | `:929` hedge retired |
| 9 | `docs/runbooks/gc-incident-response.md` | `:2035` hedge retired |
| 10 | `docs/API_CONTRACTS.md` | `:244` pointer appended, nothing deleted |
| 11 | `infra/docker/prometheus/rules/mh-alerts.yaml` | Omission comments retargeted to the *target* |
| 12 | `docs/TODO.md` | MC entry retargeted in place **and its tail reconciled**; **+9 entries (D1-D9)** |
| 13 | `docs/devloop-outputs/2026-09-02-media-path-slos-and-observability-policy/main.md` | This record |

**Not touched, deliberately**: `infra/kubernetes/observability/prometheus-config.yaml` (D1, spun out
to infrastructure) and `docs/runbooks/ac-service-incident-response.md:1074` (verified — already
points cleanly at the path with no hedge; no edit needed).

**Two further stale claims surfaced during implementation. I stopped and reported rather than
absorbing them; @team-lead ruled FIX BOTH NOW, and both are fixed in this changeset.** Neither is a
thirteenth file — both are inside files already in the diff and both are consequences of edits
already approved.

- **`docs/observability/alerts.md:738-750`** listed **`MHHighJitter` — "Jitter p99 >20ms"** under
  *Planned Warning Alerts*. The ruling's reasoning is the decisive point and worth recording: this is
  **not pre-existing debt**. Before this diff the entry was consistent with ADR-0011:124; **this
  commit is what makes it dangle.** Shipping a change that strikes an objective while leaving a
  planned-alert entry pointing at it is masking a failure the change introduced. It is also the exact
  defect I was scoped to fix at ADR-0011:367, in the worse location — a *Planned Warning Alerts* list
  reads as a work queue, so someone picks it up and builds an alert against a metric that will never
  be emitted. **Deleted, not repointed.** The review protocol's own suspicious-deferral check
  confirms it: under ~5 LoC, inside the existing changeset, zero design ambiguity — all three hit, so
  it is a fix, not a deferral. Same block: the `mh-alerts.yaml` "🚧 To be created" status corrected
  (it exists, ships 15 alerts, none inventoried), and `MHHighAudioLatency`'s pre-amendment `>30ms`
  dropped from the candidate list — a stale number sitting beside a struck one is how the next reader
  concludes the amendment was only partly applied.
- **`docs/runbooks/gc-incident-response.md:2034`** — GC metrics catalog hedged as "(to be created)"
  when `docs/observability/metrics/gc-service.md` exists. One line, adjacent to `:2035` which I was
  already editing, same defect class. Retired.

**The two interim entries were removed from `docs/TODO.md`** — the one for the
`gc-incident-response.md` metrics-catalog hedge, and the one for the `alerts.md`
`MHHighJitter`/`MHHighAudioLatency` inventory. Work done in the same commit is not debt, and a TODO
entry for it decays into a false signal. *(Named by subject rather than by number: both labels were
later reused by newly-filed entries — see F9 in Code Review Results.)*


---

## Devloop Verification Steps

### Gate 2 — `./scripts/layer-all.sh` (unattended, `DEVLOOP_FAIL_FAST=0`, all seven layers)

**Verdict: PASS.** Wrapper exit 0. No `FAIL`, no `PRECONDITION_FAILURE`, no `FAIL-MISSING-VERB`, no `NOT-RUN`.

```
LAYER=1 RESULT=OK   DURATION=1      LAYER=5 RESULT=OK   DURATION=1
LAYER=2 RESULT=OK   DURATION=2      LAYER=6 RESULT=N/A  DURATION=2
LAYER=3 RESULT=OK   DURATION=23     LAYER=7 RESULT=OK   DURATION=457
LAYER=4 RESULT=N/A  DURATION=169
TOTAL_DURATION=655 TOTAL_RESULT=N/A
```

**On `TOTAL_RESULT=N/A`** — this is an aggregation artifact, not a failure. `N/A` ranks 4 in
`scripts/lang/_common.sh:__status_rank`, above `OK` (3) and below `FAIL` (5), so a single N/A child
dominates a layer and the worst layer dominates the total. Both N/A layers were re-run individually
to capture their REASON tokens (the first pass was piped through `tail`, which discarded the
per-language lines — an N/A must never be accepted on a truncated log):

| Layer | Per-language STATUS | Aggregate |
|-------|---------------------|-----------|
| 4 (Test) | proto `N/A not-applicable-to-this-lang`; rust `OK cargo-test-passed`; ts `OK nx-test-passed` | `N/A test-aggregate-na` |
| 6 (Audit) | proto `N/A not-applicable-to-this-lang`; rust `OK cargo-audit-passed`; ts `SKIPPED-NO-DIFF no-dep-changes`; `OK buf-breaking-passed` | `N/A audit-aggregate-na` |

Both trace to proto's deliberately-absent test/audit phases, registered as intentional-gap
placeholder wrappers emitting `REASON=not-applicable-to-this-lang`. ADR-0033 §6 and the devloop
skill name this exact case as self-justifying — the wrapper's own REASON is the justification, and
no implementer action is owed. The Layer-6 `SKIPPED-NO-DIFF no-dep-changes` is the documented
within-wrapper dep-manifest gate (this diff changes no dependency manifest).

Layer 7 ran both Phase-2 suites against the live cluster: Rust env-tests `OK env-tests-passed`,
browser E2E `OK browser-e2e-passed` (8/8 Playwright specs, 1.3m).


### Gate 2 — final re-run on the post-review tree

The diff grew during Gate 3 (483 → 1737 staged insertions across nineteen fixes), so the pipeline
was re-run on the final tree. **Verdict: PASS.** Wrapper exit 0.

```
LAYER=1 RESULT=OK   DURATION=1      LAYER=5 RESULT=OK   DURATION=1
LAYER=2 RESULT=OK   DURATION=1      LAYER=6 RESULT=N/A  DURATION=2
LAYER=3 RESULT=OK   DURATION=24     LAYER=7 RESULT=OK   DURATION=232
LAYER=4 RESULT=N/A  DURATION=155
TOTAL_DURATION=416 TOTAL_RESULT=N/A
```

Layers 4 and 6 are the same self-justifying proto intentional-gap placeholders as the first run
(`not-applicable-to-this-lang`), with rust `cargo-test-passed` / ts `nx-test-passed`, rust
`cargo-audit-passed` / `buf-breaking-passed`, and Layer 7 `env-tests-passed` + `browser-e2e-passed`.

**One transient Layer-3 failure, diagnosed and not masked.** An intermediate run reported
`STATUS=FAIL REASON=run-story-selftest-failed` while `scripts/workflow/run-story.test.sh` itself
printed `290 passed, 0 failed`. The failure was its containment check:

```
CONTAINMENT-FAILURE — the REAL repository at /work changed during this run.
The post-run state is STABLE across two samples, so no concurrent writer explains this:
the suite is the actor. Treat as a real containment failure. Do NOT weaken this check.
```

The check's conclusion was wrong here, and the reason is worth recording because the heuristic will
mislead the same way again. There *was* a concurrent writer — the Lead started the pipeline while
@implementer was still filing D9. `stat` confirmed `docs/TODO.md` written at 03:31:51 and this
`main.md` at 03:32:01, inside the run's window. The suite samples the post-run state twice to rule
out a concurrent writer; both samples were taken after the writes had stopped, so the state looked
stable and the suite attributed the change to itself.

Operator-caused, not diff-caused: the changeset touches nothing under `scripts/`. Per the
`docs/runbooks/devloop-validation.md` §6.3 reproduce-on-retry discriminator, it was re-run on a
quiescent tree and Layer 3 returned `OK` — it did not reproduce. **The check was not weakened,
skipped, or annotated**; the fix was to stop running the pipeline against a tree a teammate was
still writing to. Lead process note: do not start `layer-all.sh` until every teammate has confirmed
it has finished writing.

---

## Code Review Results

**Gate 3 — five reviewers filed; @team-lead adjudicated all findings as fix-now (each under ~5 LoC,
inside a file already in the diff, no design ambiguity — the review protocol's suspicious-deferral
triad, all three hit). Every finding fixed. No thirteenth file; the twelve-file closure holds.**

| # | Finding | Reviewers | Resolution |
|---|---|---|---|
| 1 | `dashboard-conventions.md` called `mh_media_egress_budget_bytes_per_second` "the existing precedent" — it does not exist | @test, @operations, @dry-reviewer | Reframed as a story-2 **forward reference**. It contradicted my own §Units labelling 40 lines above, so the file disagreed with itself about one symbol. |
| 2 | `alerts.md` "ships **15 alerts**" — it ships **13** | @operations, @code-reviewer, @dry-reviewer | **Literal removed entirely.** A count here is a second encoding of `mh-alerts.yaml` with no guard, stale on the next rule. |
| 3 | "Remaining unbuilt candidates" listed `MHDown` and `MHHighCPU`, both shipping | @operations, @dry-reviewer | Removed. **Self-inflicted**: the exact defect I deleted `MHHighJitter` for, reintroduced two bullets below the removal note, contradicting the sentence above it. List **re-derived programmatically** against the rules file, not edited by hand. |
| 4 | `dashboards.md:140-141` restated AC targets as **p95**; `slos.md` says **p99** | @operations, @dry-reviewer | Both rows demoted to the same pointer form as the MH block. A live disagreement **this commit created** — I applied C3 to the MH block and not the AC block in the same file. |
| 5 | `docs/TODO.md` MC entry: correction prepended, contradicting tail left standing | @operations | Tail reconciled. Highest-consequence finding: "task 7 defines the re-assert-failure objective" was unfulfilled, and the handler-restart story is told to land its rule "against that threshold". Recorded that **both stages moved**, with the structural reason (no cadence or applied-generation emission exists yet, so an objective now would have no metric to attach to). |
| 6 | `key_custody` scope **narrowed** to "media-path" vs §11's unqualified "logs and metrics" | @security | **Reverted to §11's wording.** A widening is a ratchet needing no amendment; a narrowing is a relaxation and would need one. Mine was unflagged in the very section that flags the widening as deliberate. |
| 7 | ADR-0011's canonical example taught `severity: critical` and an absolute `runbook_url`, both guard-rejected | @security | Fixed to `page` and the repo-relative form; prose taxonomy at `:326` corrected; dated amendment added. Per @team-lead's correction, the amendment states that **the guard does not scan `docs/decisions/`** (`ALERTS_SUBDIR` is the rules dir only) — so nothing flagged it, and an author copying it gets a red gate with no hint why. |
| 8 | §Periodicity forbids restating literals, then tabulates them | @dry-reviewer | Exemption stated explicitly: the figures document a drift (which cannot be described without both sides), are point-in-time, and vanish when D1 closes. |
| 9 | §Periodicity silently becomes wrong when D1 lands | @dry-reviewer | Doc-update trigger added to D1's **Fix** clause, plus an in-section note that every statement inverts on apply. |
| 10 | Coverage table cited drifting line numbers for the conforming set, stable names for the non-conforming set | @dry-reviewer | All 11 now cited **by alert name**. Names are stable and greppable; the line numbers had already shifted once inside this changeset. |
| 11 | Burn-rate pair duplicated with precedence scoped to miss it | @dry-reviewer | Precedence extended explicitly: ADR-0011 owns the pair as fleet-wide *policy*; `slos.md`'s authority covers *targets*; ADR-0011 wins on divergence. |
| 12 | `### Service-local labels` orphaned under §Media-path identity | @dry-reviewer | Re-parented under §Shared Label Names. Pure move, no wording change. |

| 13 | `slos.md` declares precedence over an **incomplete** register — ~11 targets the tree asserts are absent, one of which **pages** | @operations (F6) + @dry-reviewer (F8), independently | **ONE** caveat in §Authority, merging both evidence sets per @team-lead. The commit itself created the load-bearing half: before it, the scattered targets were peers with no stated winner; declaring a winner converted every omitted target from untidy to **contradicted**. Load-bearing sentence made unambiguous — *an absent row means not-yet-ratified into this file, not that there is no objective; a live alert whose target is absent here is still a live alert*. Table of the four known live omissions with their shipped artifacts. The file had indicted itself: it names "an absent row is indistinguishable from a decision not to have an SLO" and then omitted rows. |

**Two-entries-for-one-gap, caught by @team-lead before re-review.** @dry-reviewer filed an entry for it while I
filed a second one for the same defect. Duplicating a debt entry as the fix for a duplication finding, in
a task whose subject is single-source-of-truth, would have been the diff contradicting itself. **Mine
deleted; theirs widened** (and later renumbered to **D8** to close the hole — F9) with @operations' live-artifact evidence (`GCMCAssignmentSlow`, `ac-slos.json`,
`mc-slos.json`, the triage-inversion consequence, and the ratifier list). One entry, one caveat, one
pointer. @dry-reviewer's **wrong-fix trap survives into both** — do not bulk-copy the literals into
the register, because that promotes unratified dashboard thresholds to fleet SLOs by clerical action,
which is how the pre-amendment `>30ms` became load-bearing. No backfill attempted; @team-lead did not
authorise one and it is per-target joint ratification.

| 14 | **F7** — §Authority's fourth row wrote `MH forwarding p95 < 100 ms` into the authoritative register; the number does not exist | @operations | Row relabelled to **MH GC-heartbeat RPC latency** (`mh_gc_heartbeat_latency_seconds`, `metrics/mh-service.md:51`) and the conflict clause dropped — heartbeat RPC latency and ingress-read-complete→egress-enqueued are different operations, so there was never a conflict to reconcile. D8's "concretely" sentence re-exampled to `dashboards.md:32`'s MC-assignment 20 ms; D8's bare citation left intact (accurate as a list entry). **Most consequential finding of the review**: a phantom number in the file this commit made authoritative, one screen above the row stating the forwarding target is OPEN — written four paragraphs above the warning against exactly this. |
| 15 | **F9** — deleting the duplicate D8 left `D1…D7, D9` with a hole, and `main.md` asserted both readings of `D7`; `D8` meant three things across the run | @dry-reviewer | The **SLO-register-backfill** entry renumbered to close the hole (TODO, `slos.md` ×2, `main.md` ×3), list reordered contiguous, and `main.md`'s historical note rewritten to name removed items **by subject rather than number**. Full numeric sweep run afterwards, catching one further stale range. *(Deliberately written without the old/new numbers: the label freed by that renumber was subsequently reused by the commit-boundary entry filed at Gate 3, so a note phrased in numbers here would now describe the wrong entry — this row applying its own finding.)* |

**Filed, not fixed (cross-owner, @team-lead concurred)**: `ac-slos.json` plots token issuance at
**p95** while the AC SLO is **p99** (@operations). A compliance dashboard tracking a quantile its SLO
is not written against, under-reporting the tail the objective bounds. AC-owned per ADR-0031, AC not
on this panel, and changing a shipped compliance signal makes its history non-comparable. **D7**,
owner auth-controller with observability + operations reviewing. It became visible only because a
register now exists — the register working as intended on its first day.

**Verdicts**: @protocol CLEAR. @semantic-guard CLEAR. @security RESOLVED-FIXED (both). @code-reviewer RESOLVED-FIXED (re-verified the guard-accepted forms against `alert_rules.rs`). @test RESOLVED-FIXED
(re-verified the F5 gauge fix and independently re-checked all 11 name-based coverage citations plus
the four look-alike/inline mappings against the tree). @operations, @dry-reviewer, @code-reviewer:
fixes returned for re-verification.

**Open, not blocking me**: @code-reviewer raised an Ownership Lens upgrade on the `alerts.md` hunk
(Mechanical → Minor-judgment, since it deletes entries and adds normative guidance to an
operations-owned doc). @team-lead is handling it; resolution is an
`Approved-Cross-Boundary: operations` trailer, which @operations authored the rulings for.

---

## Accepted Deferrals

- `docs/TODO.md` §Observability Debt D2 — client SDK OTel export interval unset, runs at 60s default
- `docs/TODO.md` §Observability Debt D1 — scrape-cadence drift, spun out to infrastructure

---

## Debt Filed That Is Not A Deferral

Scope context, deliberately kept out of §Accepted Deferrals, which is reserved for findings that
remain in the diff. D3-D9 are gaps this change *exposed* or task outputs the plan called for, not
findings left unfixed: D3 seven unguarded ratio alerts, D4 the unenforced identity prohibition,
D5 client `meeting_id_hash` inertia, D6 dashboard catalog gaps, D7 `ac-slos.json` p95-vs-p99,
D8 the SLO-register backfill, D9 the commit-boundary tracked-or-staged assertion. Listing them as
deferrals would misrepresent the cost shift that section exists to surface.

---

## Rollback Procedure

1. Start commit: `020e8a022d530bd48b32f846b4785d844b7cd7db`
2. `git diff 020e8a0..HEAD`
3. `git reset --hard 020e8a0` (docs-only; no migrations or manifests applied)

---

## Issues Encountered & Resolutions

### Issue 1: Three of the task prompt's five cited config keys did not exist as described
**Problem**: The prompt instructed citing MH's 5s scrape interval, the client SDK's 10s OTel export
interval, and fifteen ratio-guard instances "across mh, gc, mc and the service template." Verified
against the tree: MH is scraped at 15s on the deployed Kind path (`prometheus-config.yaml` declares
no per-job override; the 5s value lives only in the non-deployed compose stack), the SDK sets no
`exportIntervalMillis` at all (60s OTel default), and there are 11 live guards with the template
carrying zero — inverting the prompt's claim about it. A fourth, the datagram-send-buffer config
key, does not exist either.
**Resolution**: Wrote what is true, filed each gap as debt, and used forward-reference voice for
symbols landing in later tasks. Transcribing the prompt would have shipped a media-path triage doc
wrong on every cadence it stated.

### Issue 2: A phantom SLO target introduced by the fix for an SLO-completeness finding
**Problem**: The fix for @operations' F6 added an omissions table that attributed
`metrics/mh-service.md:51` to MH *forwarding* latency. That line belongs to
`mh_gc_heartbeat_latency_seconds`, a control-plane RPC objective, and the MH catalog documents no
forwarding metric at all. It wrote `MH forwarding p95 < 100 ms` into the register this commit makes
authoritative, one screen above the row stating that target is OPEN — four paragraphs above the
file's own warning against exactly that, which names the pre-amendment `>30ms` as the precedent.
**Resolution**: Caught by @operations as F7. Row relabelled to GC-heartbeat RPC latency, conflict
clause dropped. Recorded as more consequential than a sibling factual slip: authority is what turns
a wrong number into a load-bearing one.

### Issue 3: The central deliverable was untracked and would have been dropped at commit
**Problem**: `docs/observability/slos.md` was untracked with nothing staged. A `git commit -am`
would have committed the eleven modified tracked files and silently dropped it, shipping ~15
confident pointers asserting authority over a nonexistent path — strictly worse than the
`(to be created)` hedges this commit retires, since a hedge announces the absence and a confident
pointer does not.
**Resolution**: Caught by @operations at Gate 3. `git add -A`; 13 files staged, 0 untracked,
verified with content in the index. Not a guard blind spot — `crates/dt-guard/src/common/git_changes.rs:132`
documents the changed set as a deliberate union with untracked files, so all eleven guards read the
file and went green about an artifact that would have vanished. The green was the source of the
false assurance. Filed as D9 with the wrong-fix trap and routed to infrastructure/test.

### Issue 4: A Layer-3 containment failure caused by the Lead, not the diff
**Problem**: An intermediate `layer-all.sh` run reported `STATUS=FAIL REASON=run-story-selftest-failed`
while the suite printed `290 passed, 0 failed`.
**Resolution**: Diagnosed as the suite's containment check firing because the Lead started the
pipeline while @implementer was still writing D9. Re-run on a quiescent tree returned Layer 3 `OK`.
The check was not weakened or annotated. Detail in §Devloop Verification Steps.

---

## Lessons Learned

*(Seeded at planning; completed at Gate 3.)*

- **Three of the task prompt's five config-key premises were false about the tree** (MH scrape
  cadence, client OTel export interval, ratio-guard count), plus a fourth instance (the
  datagram-send-buffer config key) that does not exist. A task prompt is a specification of intent,
  not a description of the tree, and every literal it asserts needs verifying before transcription.
  The prompt's own suggested grep for the ratio guard returns **zero** — the guards are multi-line
  YAML — which is plausibly how a "fifteen instance" figure was formed without being checkable.
- **@dry-reviewer's warning, recorded here because story close reads this file**: tasks **8** and
  **16** rest on the same assumptions this task found broken. Task 16 in particular is told to define
  the ADR-0011 forwarding objective as a named constant and publish a sample-ratio gauge — neither
  symbol exists today, which is fine and expected, but its prompt also inherits the same
  config-cadence and guard-count premises. Whoever picks up 8 or 16 should verify before citing.
- **Three wrong counts in this task all came from one mechanism: a grep whose match set was wider
  than the thing being counted.** The task prompt's "fifteen instances" of the ratio guard (actually
  11 — the loose pattern caught offset-baselines and a rate floor); my own "ships 15 alerts"
  (actually 13 — `grep -c "alert:"` matched two header comment lines containing "No … alert:"); and
  the shifted `mh-alerts.yaml` line citations (my own edit moved the lines my own doc cited). The
  re-derivable-command discipline is right and caught all three, but **the command has to be narrow
  enough that its output *is* the claim** — `grep -c "alert:"` counts lines containing a string,
  which is not the same fact as "how many alerts ship". Where the claim is a count, anchor the
  pattern (`^\s*- alert:`); where it is an identity, cite the **name**, not the line number. Both
  fixes landed in this changeset.
- **Separately, and not an instance of the pattern below**: the task's central artifact sat untracked until Gate 3, and eleven green guards had validated it anyway. That is a pipeline gap, not a content error — see `docs/TODO.md` §Observability Debt **D9**.
- **When you rewrite a block, the surviving rows become new claims and need the same verification as
  the new ones.** This was my dominant failure mode across Gate 3 — four instances, and the generic
  "check your work" does not capture it. In every case the *framing* I wrote was correct, sometimes
  better than what was asked; the defect was **a detail I introduced or retained while my attention
  was on the frame**, unverified because it felt like part of the thing already being fixed.
  (i) Rewrote the `alerts.md` MH block and left `MHDown`/`MHHighCPU` in a candidate list while the
  same edit declared the file exists. (ii) Applied the pointer-demotion rule to `dashboards.md`'s MH
  block and not its AC block, same file, same commit. (iii) Filed a duplicate debt entry as the fix
  for a duplication finding. (iv) F7 below.
- **F7 was the same mechanism with a materially larger blast radius, and the distinction is the
  useful half.** The other three put a wrong claim in a document. F7 put a **phantom number into the
  file this same commit made authoritative** — `MH forwarding p95 < 100 ms`, misattributed from
  `metrics/mh-service.md:51`, which is `mh_gc_heartbeat_latency_seconds` (GC heartbeat RPC latency, a
  different operation; the MH catalog contains no forwarding metric at all). It landed one screen
  above the row whose whole purpose is to say the forwarding target is OPEN, so the register offered
  two candidate forwarding numbers — a struck 30 ms and a phantom 100 ms — for the one objective
  meant to have none, handing story 8 a stray anchor. And it was written **four paragraphs above the
  warning against exactly this**, which names the `>30ms` precedent as the mechanism. A wrong number
  in an authoritative register does not sit inert the way a wrong number in a catalog does; authority
  is what converts a slip into a defect.
- **A deletion from a numbered list is a rename of everything after it.** Deleting the duplicate D8
  left `docs/TODO.md` reading `D1…D7, D9` with a hole, and `main.md` asserting both readings of `D7`
  in one document — `D8` meant three different things across this run. That is the
  identifier-namespace instance of the very defect this task exists to fix: one label, two encodings,
  no stated winner, defeating precisely the reconciliation the next person attempts. Two rules:
  renumber to close the hole, and **write historical notes by subject, never by number** — numbers get
  reused, subjects do not, so a note written in reusable identifiers is guaranteed to rot.
- **A convention that exists only as copies at its use sites propagates its own violation.** The
  ratio guard had 11 hand-copied instances and zero presence in the one artifact designed to spread
  it (`_template-service-alerts.yaml`), which is also guard-exempt (`alert_rules.rs:413`), so nothing
  would ever have caught the omission. Documenting a convention without seeding the propagation point
  fixes the half nobody copies from.
