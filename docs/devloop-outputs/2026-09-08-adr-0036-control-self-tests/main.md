# Devloop Output: ADR-0036 §11 Control Self-Tests (fire + apply)

**Date**: 2026-09-08
**Task**: Story task 23 — self-tests demonstrating two ADR-0036 §11 controls both FIRE and APPLY: (1) credential-leak semantic-guard KEK/transmit-key extension positive fixture + clean-on-real-tree confirmation; (2) mh-service dev-only per-frame tracing feature as a real release-build compile error, plus a scrape-level assertion that no media metric carries a meeting identifier and no `e2ee` / `end_to_end` / `zero_trust` label or boolean appears in MH, MC or client media metrics.
**Specialist**: test
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~2h20m (setup 08:00Z to commit ~10:20Z), 1 implementation iteration, 3 full validation passes

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7d2661c54b5f88ad0607099043665080b520245a` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Headless | `DEVLOOP_HEADLESS=1` (run-story task #23) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (subagent_type `test`) |
| Implementing Specialist | `test` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Infrastructure (conditional) | `infrastructure` |

---

## Task Overview

### Objective

ADR-0036's "A control's coverage must be demonstrated, not asserted" section requires every gate the design adds to answer both halves — *does it fire* (inject the adverse condition) and *does it apply* (confirm the premise against the real artifact). Two §11 controls currently assert coverage without demonstrating it. This devloop supplies the demonstrations.

### Scope

- **Service(s)**: mh-service (release-build gate), mc-service (credential-leak fixture subject matter), guard/test infrastructure
- **Schema**: No
- **Cross-cutting**: Yes — security + semantic-guard own the credential-leak check definition; observability owns the metric hygiene policy

### Debate Decision

NOT NEEDED — implements an existing Accepted ADR (ADR-0036 §11 + §"A control's coverage must be demonstrated, not asserted") and a planned story task.

---

## Cross-Boundary Classification

ADR-0024 §6.4 checked first: no path below matches any key in
`scripts/guards/simple/cross-boundary-ownership.yaml`, and no path meets a §6.4
criterion, so no row is inside a Guarded Shared Area and the Mechanical
classification is available in principle. It is nevertheless used for zero rows.
`crates/dt-guard/**` is not a GSA (CLAUDE.md §Guard-crate ownership); the rows
below add test fixtures and a test harness that consume existing kernels, which
is test-domain, not guard machinery.

**Not touched, deliberately** — named because a reviewer will look for them:
`crates/mh-service/**` (no source edit; the release-build self-test invokes cargo
against the crate as it stands), `crates/mc-service/**` (the fixtures are
standalone constructions, never edits to real MC code — @semantic-guard's
requirement), `docs/observability/label-taxonomy.md` (policy derived from and
cited, never restated), `crates/dt-guard/src/**` (no matcher, rule, threshold
or scan-scope change — the two exceptions are a doc-comment bullet correction in
`release_build_profile.rs` and a KNOWN-LIMIT comment widening in `pii_vocabulary.rs`
that adds, removes and rewords no token; both carry their own table rows), `crates/dt-guard/tests/fixtures/media_telemetry_deny/**` and
`scripts/guards/simple/media-telemetry-deny.*` (observability's, landed in
c0538b56 / a99e1078).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/devloop-outputs/2026-09-08-adr-0036-control-self-tests/main.md` | Mine | — |
| `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/README.md` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_contract_crossing_meeting_kek.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_contract_crossing_transmit_key_bytes.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_contract_crossing_renamed_material.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_mc_log_meeting_kek.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_mc_log_transmit_key_bytes.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_derive_debug_raw_kek_bytes.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/neg_kek_generation_metadata.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/neg_derive_debug_secretbox_wrapper.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/neg_join_response_kek_delivery.rs` (new) | Mine | — |
| `scripts/release-feature-gate.test.sh` (new) | Mine | — |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` (new) | Mine | — |
| `crates/env-tests/src/fixtures/mod.rs` | Mine | — |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` (new) | Mine | — |
| `crates/dt-guard/src/release_build_profile.rs` | Minor-judgment / Not mine | infrastructure + security |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/pos_skip_debug_entry_removed.rs` (new) | Mine | — |
| `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/neg_redacted_len_hand_rolled_debug.rs` (new) | Mine | — |
| `scripts/guards/semantic/checks.md` | Domain-judgment / Not mine | security + semantic-guard |
| `crates/dt-guard/src/common/pii_vocabulary.rs` | Domain-judgment / Not mine | security + observability |
| `.claude/skills/devloop/review-protocol.md` | Minor-judgment / Not mine | test + code-reviewer |
| `scripts/lang/rust/compile.sh` (Layer 1 wiring) | Domain-judgment / Not mine | infrastructure (they author) |
| `scripts/layer3.sh` (pointer comment only) | Domain-judgment / Not mine | infrastructure (they author) |
| `docs/runbooks/devloop-validation.md` | Minor-judgment / Not mine | operations |
| `docs/TODO.md` | Domain-judgment / Not mine | semantic-guard + observability (overclaim entry); operations (budget entry) |

Row notes:

- **`scripts/guards/semantic/checks.md` — Domain-judgment, and I do not edit it.**
  @semantic-guard has stated the poles amendment is theirs to author and sign, and
  @security has stated the same. I request two changes and they write both: the
  §Fixture poles must-fire spellings move from bare `kek` to realistic prefixed
  compounds, with the reason stated; and the fixture directory path is named so the
  file's own §Fixture-verification runs clause has something to point at. The row
  stays in this table because the deliverable is incomplete without it, and because
  a plan that quietly assumed someone else's edit would land is the coordination
  failure this table exists to prevent.
- **`scripts/lang/rust/compile.sh` — Domain-judgment, and @infrastructure authors it,
  not me.** They upgraded my `scripts/layer3.sh` row Minor-judgment → Domain-judgment
  (upgrade-only, auto-routes to ESCALATE) and the Lead ruled for them. **My placement was
  wrong and the reason is worth recording rather than just complying with**: I asked
  whether ~2s warm fits under the 90s budget, but ADR-0033 §3's amendment clause asks a
  different question — fast tier or outside it — and §4's "inherently large/variable cost"
  principle puts compiler cost outside. `layer3.sh` invokes cargo zero times today;
  `compile.sh` is pure cargo and already runs `--release` twice. On a cold CI cache the
  Layer-3 placement would fire `BUDGET_TOTAL_BREACH` for "Cargo.lock changed" rather than
  "the fast floor regressed", training operators to ignore the token — which is §4's own
  stated reason for excluding Layer 7. I do not touch `scripts/layer3.sh` at all;
  @infrastructure adds a one-line pointer comment there for discoverability. The
  Row-1 trailer problem disappears with the move, since this is owner-implements inside
  the devloop rather than a spin-out.
- **(superseded) `scripts/layer3.sh` — Minor-judgment.** One `run_and_emit` line plus its
  explanatory comment, matching the eleven self-test wirings already there. Not
  Mechanical: layer placement is a judgment about lane budget. **Infrastructure is not
  on this team**, and a Minor-judgment edit needs the owner's `Approved-Cross-Boundary:`
  hunk-ACK trailer (@code-reviewer). @main must either pull infrastructure in for the
  two infrastructure-owned rows or spin the wiring out, or the commit lands without a
  required trailer. Raised to @main; not resolvable from inside this lane.
- **`docs/runbooks/devloop-validation.md` — Minor-judgment, unconditional, and the
  hunk moved from §6.3 to §6.1, and after a correction from @operations I am the SINGLE
  author there.** @infrastructure is not editing the runbook at all (their hunks are
  `scripts/lang/rust/compile.sh` and the `layer3.sh` pointer comment), so the
  release-feature-gate triage row plus five §6.1 corrections are mine under the existing
  operations-owned row — no new cross-boundary row, no second trailer. @operations gave
  the §6.1 re-ACK explicitly; I held out for it rather than writing on the strength of a
  §6.3 ACK. The five §6.1 corrections, all verified against the scripts: stage-2 prose
  says `cargo check --workspace` where it runs `cargo build --workspace --quiet`
  deliberately; the REASON-token table says `cargo-check-failed` where the emitted token
  is `cargo-build-failed`; two rows are missing (`cargo-build-dt-guard-failed`,
  `cargo-build-dt-story-failed`, whose outputs Layer 3's wrappers consume); the worked
  example prints `cargo-check-passed` / `tsc-passed` where the real tokens are
  `cargo-build-passed` / `nx-typecheck-passed` (worst of the five — the example block is
  what a triager pattern-matches fastest); and `nx affected -t typecheck` is stale where
  the script runs `nx run-many -t typecheck --all`, including the row's repro command.
  **The budget line uses @operations' corrected wording** — Layer 1 exceeds the 20s
  per-layer warn on any cold cache, expected and dominated by the workspace build, *not*
  by the release-feature gate. Misattributing a budget warn to the most recently added
  thing is how a true signal gets blamed on the wrong cause and then muted, which is the
  same disease as the `docs/TODO.md` entry I carry for them; the two must not contradict
  each other in the same file. Also mine: the **§6.3 stale-enumeration fix** (`dev-web.test.sh`,
  `run-guards.test.sh`, `validate-frame-vectors.test.sh`, `media-telemetry-deny.test.sh`
  are all wired in `layer3.sh` today and all missing from the list — real drift, and I am
  the one in the section even though my gate no longer joins it), and the **§8 catalogue
  row** for the media-metric hygiene assertion keyed on its distinctive triage string,
  with @observability's drafted content.
- **(superseded, §6.3 placement) `docs/runbooks/devloop-validation.md` — Minor-judgment, and unconditional.**
  @operations checked: §6.3 does enumerate wired self-tests, and **that enumeration is
  already stale** — it omits `dev-web.test.sh`, `run-guards.test.sh`,
  `validate-frame-vectors.test.sh` and `media-telemetry-deny.test.sh`, all wired in
  `layer3.sh` today. Adding mine while leaving a list I have just proved stale would be
  worse than not touching it, so the drift is closed in the same hunk. The self-test's
  REASON tokens go in §6.3's token table, not only in §8. One shape detail the triage
  entry must carry or it misleads: when the script emits `PRECONDITION_FAILURE` and
  exits non-zero, `run_and_emit` **also** emits `STATUS=FAIL REASON=<prefix>-failed` on
  the same layer — worst-status aggregation still lands the layer on
  `PRECONDITION_FAILURE` and exit 2, so the lane is right, but the log carries both
  lines and an operator who greps `STATUS=FAIL` first has been sent to the implementer
  lane by an artifact of the wrapper rather than by a verdict. The runbook describes
  what the log actually looks like.
- **`crates/dt-guard/src/release_build_profile.rs` — Minor-judgment, doc comment only**
  (@dry-reviewer D5). Its "The control being protected now exists — **one instance**,
  since 2026-09-02" bullet was falsified by c0228455: MH's `per-frame-trace`
  `compile_error!` is a second instance. The staleness is pre-existing and not caused by
  this diff, but **this task is named by that bullet** ("story task 23 lands the
  assertion that it fires"), and leaving a sentence that says "one instance" while my
  whole subject is the second instance is precisely the failure class this devloop
  exists to correct. The file was corrected once before for the same reason at
  @security's ask. Two words plus the second citation; no rule, threshold or scan scope
  is touched, and the ANCHOR: do-not-delete banner stays.

  **One added condition, accepted: the same doc comment states the guard's premise
  BACKWARDS, and that is fixed in the same hunk.** Lines 11-13 render the control as
  `#[cfg(debug_assertions)]` and call it "inert the moment `debug_assertions` is off".
  Both real controls are `not(debug_assertions)` — live in release, inert in dev — and
  the guard's own finding table at ~105-110 detects debug-assertions being *enabled*,
  consistent with reality and contradicting the module's opening paragraph. Line ~118
  repeats the inversion by labelling the throwaway-crate experiment "§11's exact
  control"; the experiment's result stands, only its description is wrong. **Verified
  against the tree.** This matters more than a stale count: a reader who takes the
  opening paragraph at face value concludes the control is inert in exactly the builds
  where it is live. @security co-owns the wording since it is their premise, and I take
  the phrasing from them rather than inventing it. Needs infrastructure's and
  @security's trailers — see the note on `scripts/layer3.sh` for how infrastructure's is
  secured.
- **`crates/env-tests/src/fixtures/mod.rs` — Mine.** One `pub mod metric_hygiene;`
  line. Named because the table must be complete (@code-reviewer).
- **`crates/dt-guard/src/common/pii_vocabulary.rs` — Domain-judgment.** One-line
  widening of the existing KNOWN LIMIT comment at lines 156-160, so it records that
  the limit applies to a *trailing* compound (`transmit_key_bytes`) and not only to
  the leading `wrapped_transmit_key` @security named it in scope; it is vocabulary
  policy content, co-owned @security + @observability. **No token is added, removed
  or reworded** — the demonstration this task lands is precisely what makes that
  limit legible, and patching the vocabulary to green a fixture is the one move
  @security and @observability have pre-agreed to raise jointly at Gate 2.
- **`docs/TODO.md` — Domain-judgment (highest tier of the edits it carries).** One row, because the scope-drift guard matches real paths; the file carries edits at two ownership tiers and takes the stricter. **§Media Path Obligations overclaim entry (semantic-guard + observability).**
  @observability supplied verbatim wording, co-signed by @semantic-guard, annotating
  *"The end-to-end / zero-trust boolean overclaim has no named guard or check"* as
  PARTIALLY MITIGATED and STILL OPEN. It lands in **my** diff as a residual rather than
  as a reviewer hand-edit, which would break Gate 2's frozen tree; both owners hunk-ACK
  the landed wording and @security reads it at Gate 2 for four specifics. Three things
  in it are load-bearing and are not trimmed: the unenforced surfaces (log, document,
  dashboard title — including the realistic `E2E: true` panel form) lead rather than
  trail; the control's limits are stated before its existence is credited; and the
  `checks.md` §Credential Leak "Not this check." forward-reference is flagged so nobody
  prunes the entry and breaks that citation. **Checkbox stays UNTICKED.** Timing is
  coupled and cuts one way only: the annotation lands with or after the checker, never
  before — citing a checker that is not in the tree asserts coverage that does not
  exist, which is the mirror failure and the worse direction.
  **Other residuals in the same file (Minor-judgment):** Per @security's point 7, no
  existing entry is ticked: the overclaim-boolean entry, the MH-frame-forwarding
  scope entry and the `wrapped_key`-CATEGORY_A entry all survive this task
  untouched.

---

## Gate 1 — Plan Approval

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (conditional — poles-parse A/A2/B, confined `skip_debug` plant, recorded cluster run) |
| Test | confirmed (after the release-arm empty-extraction hole was closed) |
| Observability | confirmed (after all three findings were fixed) |
| Code Quality | confirmed |
| DRY | confirmed (after D1-D5 were resolved) |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Infrastructure (conditional domain reviewer) | confirmed (after the Lead ruled on their ESCALATE) |

Classification-sanity guard on this file: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Fenced-code delta on both docs touched during planning is zero, verified against `HEAD` (`scripts/guards/semantic/checks.md` 18 → 18, `docs/TODO.md` 2 → 2), which is the task's no-fenced-code constraint holding rather than being asserted.

**Infrastructure was added as a conditional domain reviewer, and the reason is worth recording.** The plan carries two Minor-judgment rows into infrastructure's domain — `scripts/layer3.sh` (wiring the new shell self-test) and `crates/dt-guard/src/release_build_profile.rs` (a doc-comment correction of a bullet that commit `c0228455` had already falsified). ADR-0024 §6.3 requires the owner-specialist to *be a reviewer on the devloop* and confirm the hunk at Gate 1 and again at Gate 3; infrastructure was not on the roster, and three teammates independently routed that to the Lead as a blocker rather than working around it. The resolution is to put the owner on the team, not to route past the requirement. The alternative — spinning the wiring out — was rejected because there is no `*.test.sh` auto-runner in this tree, so an unwired self-test never runs, and the release-gate deliverable would ship as a control that exists and never fires. That is the exact failure this task exists to demonstrate against, and accepting it here would have been the task failing at its own subject.

**Gate 1 ESCALATE, ruled by the Lead: `scripts/layer3.sh` upgraded to Domain-judgment, and the wiring moved to `scripts/lang/rust/compile.sh` (Layer 1), authored by infrastructure.** Infrastructure declined to hunk-ACK the planned Layer-3 wiring and upgraded the classification instead, which under ADR-0024 §6.2 is upgrade-only and auto-routes to ESCALATE. The Lead ruled for them, after independently re-deriving their four load-bearing facts against the tree rather than accepting the summary. The decisive point was about which question the plan asked, not about arithmetic: the plan asked whether roughly two warm seconds fit under the 90s budget, while ADR-0033 §3's amendment clause specifies a different question for a new validation tool — fast tier or outside it — and §4's principle, "inherently large/variable cost", answers it, because a cargo invocation is variable compiler cost by construction. The supporting facts, verified: `scripts/layer3.sh` invokes cargo zero times today and the only cargo references anywhere under `scripts/guards/` are precondition messages naming Layer 1 as the producer, so the producer-consumer split is a real invariant this plan would have been the first thing to break; `scripts/lang/rust/compile.sh` is pure cargo, always-run, and already runs `--release` twice; and `scripts/layer-all.sh:107` sets a 20s per-layer warn against a 90s total. The failure the placement would have caused is the one §4 legislates against in terms — on a cold CI cache, `WARN BUDGET_TOTAL_BREACH` would fire for "Cargo.lock changed" rather than "the fast floor regressed", training operators to ignore the token, which is verbatim the reasoning §4 uses to exclude Layer 7 from the per-layer warn. Adopting that structure with the opposite conclusion would have been incoherent. Discoverability was the real cost on the other side — all eleven existing self-test wirings live in `layer3.sh` — and infrastructure argued it fairly rather than dismissing it; it is bought back with a one-line pointer comment there. Because infrastructure is on this team, this is owner-implements inside this devloop rather than a spin-out, and the Row-1 trailer requirement disappears with the row.

A second condition was accepted on `crates/dt-guard/src/release_build_profile.rs`. Beyond the stale "one instance" bullet that commit `c0228455` had already falsified, the module's opening paragraph states the guard's own premise backwards: lines 11-13 render the protected control as `#[cfg(debug_assertions)]` and call it inert the moment `debug_assertions` is off, while both real controls are `not(debug_assertions)` and are therefore live in release and inert in dev — and the guard's own finding table detects debug-assertions being *enabled*, consistent with reality and contradicting the paragraph that introduces it. Line 118 repeats the inversion in its label on the throwaway-crate experiment, whose result stands while its description does not. Verified against the tree by the Lead. Fixed in the same hunk, because it meets the suspicious-deferral test squarely and because the substance is this task's own subject: a guard whose module doc states its protected premise backwards is a control that reads as covering something it does not.

**Cluster availability, checked rather than assumed.** The implementer flagged before starting that item 3's APPLY half needs a real cluster and that "written but never executed" was a possible outcome. It is not: `dev-cluster status` reports the `devloop-hear-yourself-through-handler` cluster exists with pods healthy, control-plane Ready, and Prometheus reachable. The recorded live run is therefore a hard requirement of this devloop, not a best-effort. Landing a control into a lane that green-skips in CI at exit 0 and never executing it would be ADR-0036 §11's first failure row — alive, never applied — chosen rather than merely suffered.

Classification-sanity guard on this file: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

### Lead Rulings (Gate 1)

Two points deviated from task text the Lead owns, so the implementer routed both up rather than absorbing them. Both are recorded here because a requirement that disappears without a trace is indistinguishable from one that was overlooked.

**Ruling 1 — `meetingKek` is dropped from the planted must-fire set. Accepted.** The task's spelling list is prefixed `e.g.`, so it is illustrative rather than an exhaustive must-plant set, and the substantive argument is stronger than the wording. Credential-leak items 11-13 are Rust and MC-scoped; `meeting_kek` is a prost snake_case field; MC has no camelCase serde surface. The one place `meetingKek` would read naturally is MC-to-client KEK delivery, which the SAFE list in `scripts/guards/semantic/checks.md` — two paragraphs above the poles — records as ADR-0036 §4 delivery to an entitled holder, that is, explicitly not leakage. Planting it as a must-fire case would contradict the SAFE list beside it and would be the "alive but out of scope" failure ADR-0036 names as worse than an absent control. Three specialists converged independently and security verified by grep rather than asserting. Two conditions attached: the deviation is stated in this document, and the *property* the task was reaching for — camelCase matcher-blindness — is not lost with the plant. Where that property is covered instead is named in §Implementation Summary; where it is covered nowhere, that is said plainly rather than implied away.

**Ruling 2 — `cargo check --release` is accepted in place of `cargo build --release`.** The contrast the task draws is "a real build, not a CI string check", and the thing being ruled out is a static grep over CI config or a Dockerfile — a control that inspects text instead of compiling. `check --release` is not that: it runs the real compiler under the real profile with the real `cfg` set. For the arm that matters the two verbs are indistinguishable, because `compile_error!` fires during macro expansion and never reaches codegen under either. Against that, operations measured `build --release` at 50-57s on any mh-service source change, against a Layer 3 already at 48-50s under ADR-0033 §4's 90s fast-tier budget; paying 50s to reach a code path the control never uses would breach the budget on exactly the devloops that touch mh-service, and a gate that makes the fast tier painful is a gate someone eventually moves or mutes. Four conditions attached: the release profile is invoked by name and never reproduced via a `RUSTFLAGS` debug-assertions override, so the premise couples to the profile that governs the deployed image; the assertion matches the `compile_error!` message text rather than a bare non-zero exit, since a mistyped feature name, an unrelated compile error and a dep-fetch failure all exit non-zero and all read as the control firing; the deviation and its reasoning are recorded in this document; and the residual is stated honestly — the pass arm proves the crate type-checks under the release profile without the feature, not that it codegens and links.

**Standing Lead conditions carried into Gate 3.** A zero or short parse of the poles paragraph must FAIL with a reason token distinct from an ordinary drift failure, and must also fail when the `## Check: Credential Leak` heading itself is missing; if the two failures are indistinguishable the vacuous-parse case gets triaged as a drift bug and then relaxed, restoring a vacuous green. `pos_skip_debug_entry_removed.rs` must never mutate `crates/proto-gen/build.rs` in place — that file is an enumerated ADR-0024 §6.4 Guarded Shared Area. And resolving a non-firing spelling by adding vocabulary entries is foreclosed: ADR-0036 §11 rejects it in terms, and it would green the fixture while every unenumerated spelling stayed uncovered.

### Owner-authored edits during planning

Two reviewers authored edits outside the implementer's changeset during the planning phase, both accepted as owner-implements under ADR-0024 §6.3.

- `scripts/guards/semantic/checks.md` — amended by semantic-guard, with four further edits by security. Domain-judgment, Owner: security + semantic-guard. The implementer is barred from this file, and the existing poles paragraph was written entirely in bare `kek` spellings, contradicting the task's requirement outright. Security's edits fixed a pole-pair confound the amendment had introduced (must-fire moved to `meeting_kek` while must-not-fire stayed `kek: MeetingKek`, so the pair differed in both name and type and a name-matching check would pass both poles without ever exercising the redacting-wrapper logic) and confined the `skip_debug` pole to a scratch copy or provably-reverted mutation, where as written it would have deleted a live redaction control from a real file on the real build path. Semantic-guard has co-signed with no reversions.
- `docs/TODO.md` — two entries added by observability as reviewer spin-out tracking, per the ADR-0019 DRY-reviewer-style exception for findings whose fix is owned elsewhere.

---

## Planning

### The mechanism, restated (and the wider class it names)

Instance language: *add self-tests for two named ADR-0036 §11 controls.*

Mechanism language: **a control whose executor is an agent's judgment or the
compiler has no harness, so its coverage is recorded in prose. Give each such
control a demonstration with two halves that a machine can re-run — the adverse
condition injected against the real executor (FIRE), and the premise bound to the
real artifact so the demonstration reds when the premise moves (APPLY).**

The wider class has same-owner siblings the task does not name. Surfacing them
rather than silently covering or silently omitting them:

- **MC's `test-seams` `compile_error!`** (`crates/mc-service/src/lib.rs:128`) is the
  *same mechanism* as MH's `per-frame-trace` gate — it is in fact the canonical
  statement of it, which MH's comment defers to — and it has no automated
  real-build self-test either. @dry-reviewer raised this independently. **Decision:
  the release-gate self-test is table-driven over `(crate, feature)` rows and ships
  with the MH row live.** Adding the MC row is one table line, but MC is
  meeting-controller's crate and no meeting-controller reviewer is on this team, so
  I do not land it unasked. If @team-lead wants it in scope, say so and it is a
  one-line change; otherwise it becomes a `docs/TODO.md` residual with an owner.
- **Every other check in `scripts/guards/semantic/checks.md`** has the same
  no-harness property. The fixture-verification protocol this task produces is
  written to be instantiable per check, not credential-leak-specific — but only
  credential-leak is instantiated here.

### Item 1 — credential-leak key-custody fixtures (items 11-13)

**Placement.** `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/`,
following the `ts_retained_credentials/lens/` precedent @dry-reviewer named. The
`lens/` path segment is load-bearing exactly as it is there: it encodes *"the
executor is the semantic-guard agent, and the mechanical guards are expected to be
partly silent here"* in the path rather than in an exemption list. Harness at
`crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs`, mirroring
`ts_retained_credentials_fixtures.rs`.

Placement safety (@operations point 10, and the risk is real —`meeting_kek` and
`transmit_key` are live CATEGORY_A entries): `common::test_code_filter::is_scan_exempt`
excludes both the `/fixtures/` path segment and `crates/dt-guard/**`, so these files
are exempt twice over, and unreferenced `.rs` under `tests/` is in no cargo target so
fmt and clippy never see them. Both facts are already load-bearing for the
`media_telemetry_deny` fixtures. **I will prove it rather than assert it**: a full
`scripts/guards/run-guards.sh` run plus `scripts/layer2.sh`, `layer5.sh` and
`layer6.sh` after the fixtures land, with the result recorded in §Devloop
Verification Steps. If any guard reds, the fixtures move rather than acquiring a
suppression.

**What is planted** (per @semantic-guard, who executes the directed run):

| Fixture | Item | What it plants |
|---|---|---|
| `pos_contract_crossing_meeting_kek.rs` | 11 | `meeting_kek` bytes assigned into a `dark_tower.internal.v1` message construction |
| `pos_contract_crossing_transmit_key_bytes.rs` | 11 | `transmit_key_bytes` passed to a builder for an MH-client call |
| `pos_contract_crossing_renamed_material.rs` | 11 | the strongest plant — a `Vec<u8>` read from the meeting actor's KEK field, moved through a helper, renamed `material`, then crossing the contract. No vocabulary contains the name at the crossing site |
| `pos_mc_log_meeting_kek.rs` | 12 | `info!("meeting_kek={:x?}", meeting_kek)` at an MC-shaped log site |
| `pos_mc_log_transmit_key_bytes.rs` | 12 | `debug!(?transmit_key_bytes)` at an MC-shaped log site |
| `pos_derive_debug_raw_kek_bytes.rs` | 13 | `#[derive(Debug)]` over a struct whose path to key bytes is a raw `[u8; 32]` |
| `neg_kek_generation_metadata.rs` | — | `kek_generation`, `sender_id`, `key_custody` — metadata, must stay clear |
| `neg_derive_debug_secretbox_wrapper.rs` | — | `#[derive(Debug)]` where the path runs through a `SecretBox`-backed wrapper |
| `pos_skip_debug_entry_removed.rs` | 13 | removal of a `skip_debug` entry for a KEK-bearing generated message — the shape item 13 names verbatim. **Never an in-place edit of `crates/proto-gen/build.rs`**: the fixture is a standalone construction, and if the directed run needs the real file mutated it is a scratch copy or a provably-reverted ephemeral mutation with the reverted state asserted afterwards (@security, poles item 2). Deleting a live redaction control from a real file on the real build path is not an acceptable way to demonstrate a check |
| `neg_join_response_kek_delivery.rs` | — | KEK delivery to an entitled holder via the client-facing join response |
| `neg_redacted_len_hand_rolled_debug.rs` | — | the `RedactedLen` shape at `crates/proto-gen/src/lib.rs:138`-`148` — a hand-rolled `Debug` that prints a byte count and never content, which checks.md's SAFE list names as the in-tree safe form |

Every fixture opens with a banner (deliberate-leak fixture; values are synthetic;
copying any line into production is the defect this file exists to catch) and closes
with an `// Invariant:` paragraph naming BOTH the expected verdict (FIRE / CLEAR) AND
the check item (11 / 12 / 13 / value-not-name), matching the `media_telemetry_deny`
convention — so a future semantic-guard agent reaching the block can reproduce the Gate
2/3 verdicts without this conversation (@semantic-guard). Planted bytes reuse the in-tree synthetic-value
convention (`vec![0xC3u8; 32]`, `crates/proto-gen/tests/signaling_roundtrip.rs:652`)
rather than inventing a second one — no hex or base64 with the texture of captured
material, no KDF call (@security). No fixture, banner or README restates `pii_vocabulary.rs` tokens
or the poles as a parallel list (@dry-reviewer point 2, @semantic-guard).

**`meetingKek` is dropped from the planted set — a deviation from the task text, ruled
on and accepted by the Lead, recorded here rather than left to be inferred.** @security and @semantic-guard both concluded
that camelCase has no in-scope Rust/MC surface for items 11-13, so a `meetingKek`
plant would be alive-but-out-of-scope — the ADR's "worse than an absent control"
failure. It survives in the demonstration as a *row in the mechanical-floor table
below*, which is where its real property (nothing catches it) is provable without an
out-of-scope plant.

**Where the property the task was reaching for is covered instead** (Lead condition 2 —
dropping the plant is fine, dropping the property is not). The reason `meetingKek` was
named at all is camelCase matcher-blindness: a word-boundary matcher cannot see inside
a compound, and camelCase is the spelling class most likely to slip through. Its
coverage, stated plainly rather than implied:

- **Rust / MC, items 11-13** — the surface does not exist. MC has no camelCase serde
  boundary and `meeting_kek` is a prost snake_case field, so there is nothing here to
  cover and nothing uncovered.
- **Client, items 5-10** — covered by *judgment*, not by spelling. Those items are
  agent-judged and a credential reaching a client sink is a finding whatever it is
  called, so camelCase is not a gap at that layer.
- **Client, mechanically** — **not covered, and this is a real gap, not a technicality.**
  `ts_pii` reads CATEGORY_B only (user PII), so no secret identifier is detectable at a
  TS log site in any spelling. Tracked at `docs/TODO.md` §"`ts_pii` has no
  secret-identifier limb", where `meetingKek` was appended as a named spelling on
  2026-09-08 by @observability + @security rather than opening a parallel entry. Owner
  security + infrastructure; task-sized; **not closed or narrowed by this devloop.**
- **The Rust log-site half of the same matcher problem** — `transmit_key_bytes` is
  invisible to the word-boundary consumers, tracked separately and spun out to
  infrastructure (promoting `segments()` to `crate::common::` and repointing
  `rust_log_secrets` changes the matcher for every CATEGORY_A term across three
  consumers). Explicitly not attempted here, and explicitly not a blocker on the
  fixture: the fixture *documents* the floor's extent and does not repair it.

**What the harness asserts mechanically.** Three tests, each with a proof-of-trap:

1. **The mechanical floor's extent, pinned per spelling and per matcher — and
   empirically confirmed before a line of fixture was written.** Ran the shipped
   `dt-guard rust-no-secrets-in-logs` against a throwaway root carrying all three
   spellings at MC-shaped log sites: `meeting_kek` produced two VIOLATIONs
   (`secret_in_log_macro`, `secret_in_tracing_field`); `transmit_key_bytes` and
   `meetingKek` produced none. The reviewers' table is correct as stated. **The harness
   invokes the shipped binary against a throwaway root** rather than an internal
   function, because `rust_log_secrets` exposes only `run()` (repo-scoped and
   diff-based) — which is strictly stronger evidence anyway, since it exercises the real
   guard end-to-end. Scaffolding needed, discovered by running it: `scripts/lang/_get_base_ref.sh`
   and `_common.sh` copied in, a git repo with one commit, and the probe file left
   UNTRACKED so the changed-file union sees it. `assert_cmd` and `tempfile` are already
   dt-guard dev-dependencies, so no manifest change. A missing binary is a loud named
   failure, never a skip. The harness invokes the *real* matchers (`rust_log_secrets`'s CATEGORY_A word-boundary
   regex; `metric_labels::token_hit_in_set`) and asserts @observability's verified
   truth table cell by cell: `meeting_kek` fires on both; `transmit_key_bytes` does
   not fire on the word-boundary matcher (trailing `_bytes` kills the boundary) but
   does on the metric-label substring pass; `meetingKek` fires on neither. **This is
   the demonstration, not an aside**: it is what makes "the vocabulary is a floor,
   the semantic check is the control" a machine-checked fact instead of a claim, and
   it is what reds if someone takes the tempting shortcut of patching the vocabulary
   to make a fixture green. No vocabulary or matcher is modified in this devloop.
   Each cell carries a citation to the `pii_vocabulary.rs` line that explains it.
   **The truth table's spelling set is deliberately disjoint from the parsed poles set,
   and the harness says so at the table** (@dry-reviewer D4): `meetingKek` is in the
   truth table *precisely because* it is neither a pole nor planted — that is the whole
   content of its cell. Two spelling lists in one file is what a future reader unifies,
   and unifying them would delete the only row recording an uncovered spelling. These
   are N sites each making their own decision under one rule, so an assertion is the
   right instrument and a shared list would be a false SSoT hiding the fork.
2. **APPLY — the check's scope is non-empty against the real tree.** Asserts
   `crates/mc-service/src/**` holds Rust production code, that at least one in-tree
   site constructs a `dark_tower.internal.v1` message, and that MC has real tracing
   sites. If any goes to zero the check has become alive-but-never-applied and the
   test reds. This is the machine-checked half of "confirm the premise against the
   real artifact".
3. **Catalog integrity.** Directory contents and catalog agree exactly in both
   directions, so a fixture cannot be added or dropped silently, and every fixture
   is non-empty and carries its banner and `// Invariant:` block. A `pos_` fixture
   quietly emptied or neutered is the failure mode @test named, and it reds here.

**FIRE for the semantic check itself has no mechanical form, and pretending otherwise
would be the exact failure this task exists to prevent.** Its demonstration is a
**Lead-directed fixture-verification run** by @semantic-guard under
`checks.md` §Fixture-verification runs, executed *in this devloop* and recorded
verdict-by-verdict in §Devloop Verification Steps against the table above. The same
run covers the clean half on the **real** artifacts, which I enumerate here so the
run is reproducible rather than impressionistic:

- Real MC→MH contract construction: `crates/mc-service/src/grpc/mh_client.rs:247`
  (`RegisterMeetingRequest` — carries no key material by design), plus the
  `internal::v1` construction sites in `crates/mc-service/src/grpc/gc_client.rs`,
  `mc_service.rs`, `media_coordination.rs` and `main.rs`.
- Real MC key-custody code: `crates/mc-service/src/media_admission/kek.rs`,
  `identity_key.rs`, `mod.rs`; `crates/mc-service/src/actors/meeting.rs`,
  `actors/messages.rs`; `crates/mc-service/src/media_routing/generation.rs`;
  `crates/mc-service/src/webtransport/connection.rs`.
- Real MC key-adjacent logging sites: a full sweep of MC tracing macros for
  key-adjacent values found exactly one, `crates/mc-service/src/main.rs:277`
  (`error!(error = %e, "MC_BINDING_TOKEN_SECRET is not valid base64")` — decode-error
  metadata, not the secret). The only `{:?}` over key material in MC is inside
  `kek.rs`'s own `debug_redacts_key_material` test, which asserts redaction.
  `MeetingKek` is a newtype over `SecretBox<[u8; 32]>`, so its `#[derive(Debug)]` is
  the SAFE case checks.md describes.

The protocol for repeating that run lives in the fixtures `README.md`, and its content
is bounded by @dry-reviewer's D1 and D2, which conflict on their face with
@semantic-guard's reproducibility ask. Resolved rather than split: **the README carries
no per-fixture verdict table.** It names the fixture directory, names the real
artifacts for the clean half, and states that each fixture's own `// Invariant:` block
IS its expected verdict. A future agent with no access to this conversation is
therefore directed to the one home rather than to a third copy that nothing reads and
that drifts silently — which satisfies reproducibility and the single-home rule at
once, and matches the `media_telemetry_deny` README's own ruling that the Invariant
paragraph is the specification and the harness rows are derived from it.

For the same reason the README **cross-references** the `media_telemetry_deny` README
for the shared `.rs`-under-`fixtures/` safety argument and the owner-disagreement
convention rather than restating either (D2 — and the evidence that copying rots is
that that README is already stale, naming `is_test_path` where the guards actually call
`is_scan_exempt`). What is stated locally is only the genuine delta, which is real: these
fixtures plant **live CATEGORY_A tokens** (`meeting_kek`, `transmit_key`) where the media
fixtures planted none, so the double exemption is load-bearing here in a way it was not
there. Cross-referencing is read-only and needs no table row.

**Poles parsing — consent given, with a binding fail-loud condition.** @security has
consented to the harness parsing §Fixture poles as the machine-read SSoT for the
must-fire spellings, on one condition I am adopting in full: **a zero or short parse
must be a hard FAIL with a distinct reason token, never a pass.** If the `## Check:`
heading is renamed, the paragraph retitled, or the sentence reflowed so the extractor
matches nothing, an "every parsed spelling appears in some `pos_` fixture" assertion
over an empty set passes vacuously — green pipeline, zero coverage, and the whole
demonstration becomes theater. That is a live defect class already tracked in
`docs/TODO.md` §Polyglot Pipeline Follow-ups and this task is the wrong place to add a
member to it. So the harness asserts a minimum expected spelling count and fails with
a named reason on a short parse or a missing heading. The parse target is a **narrow explicit
anchor enumerating only the must-fire spellings**, never free-form identifier
extraction across the section (@semantic-guard's binding condition b, and it is
correctness rather than preference): the poles paragraph deliberately contains
spellings that must NEVER enter a must-fire set — `kek_generation` as must-not-fire
metadata, `MeetingKek` / `SecretBox` / `RedactedLen` as wrapper examples, and
`bytes` / `payload` / `material` / `blob` as the value-not-name rename examples. A
grep-all-backticks parser would harvest those as must-fire, which is actively wrong.
The anchor is a fixture-spec marker, not a second detection vocabulary, so it does not
conflict with the do-not-restate-the-vocabulary rule, and it must say so at the anchor
site so nobody later "simplifies" it away as a redundant list. Ordinary prose edits
outside the anchor do not red the build, which is the point: the prose author stays
free to reword, where free-form extraction would couple every reword to the parser.

**Landed markup and its binding parser caveat** (@semantic-guard applied the anchor to
their file with one structural refinement). The fence is
`<!-- fixture-poles:must-fire:begin -->` … `<!-- fixture-poles:must-fire:end -->`, and
the content strictly between the markers is ONLY the "Must fire:" sentence, verbatim —
the explanatory marker-note was moved to *before* `:begin` and de-backticked, because in
my proposed order its own backtick spans (the `fixtures.rs` path, `pos_`) sat inside the
fence where a span-extractor would harvest them as phantom must-fire spellings. **The
parser must use EXACT-TOKEN (full-identifier) equality, never substring `in`** — this is
correctness, not preference: under substring, the poison token `bytes` false-matches
inside the legitimate must-fire spelling `transmit_key_bytes`, and `MeetingKek`
false-matches inside `MeetingKekUpdate` (the legitimate skip_debug pole target), so a
substring poison guard would reject a correct anchor and a substring extractor would
mint phantom must-fire tokens. The eight-token poison test passes under exact-token
semantics and fails under substring; I run it under exact-token before wiring, using the
`crates/dt-guard/src/ts_retained_credentials.rs::segments()` idiom @security and
@semantic-guard both pointed at. The heading-first-then-fence locate for condition A2 is
confirmed to work — the fence sits inside `## Check: Credential Leak`.

**The contract is NOT "every fenced span appears in some `pos_` fixture" — that predicts
spurious reds, and both poles owners converged on a structural classifier instead.** The
fenced sentence legitimately contains four non-spelling spans: `dark_tower.internal.v1` (a
proto package, and in the *dotted* form — the Rust fixtures reference the `::` colon form,
so the literal never appears), `#[derive(Debug)]` (an attribute), `crates/proto-gen/build.rs`
(a file path that `pos_skip_debug_entry_removed.rs` operates on only as a scratch copy, so
the literal may not appear), and the `skip_debug(...)` call site. A literal-span contract
reds on at least two of these for reasons unrelated to drift, and the two natural responses
— relax the parse, or edit the poles prose — are exactly what this condition set exists to
prevent. **Resolution (@semantic-guard, concurred by @security): extract SPELLING tokens by
a structural classifier — snake_case identifier tokens containing none of `.` `/` `#` `(` —
over the fenced sentence.** That yields exactly `meeting_kek` and `transmit_key_bytes`, the
must-fire spellings the task names, excludes all four problem spans by construction, needs
no hand-maintained exclusion list and no per-span normalization, and matches the task's
literal wording ("every must-fire spelling"). Each extracted spelling must appear in some
`pos_` fixture; exact-token (`segments()`) semantics; the zero/short/heading-missing
HARD-FAIL with its distinct reason token stands. **The classifier has its own vacuity
mode and it gets the same treatment** (@security): a structural filter can silently yield
fewer tokens than intended — a future must-fire spelling that is not snake_case would
vanish from the drift set with no signal — so the harness asserts a minimum
classified-token count (2 today) with the "could not evaluate" reason token, and the
assertion site states that the classifier selects snake_case spellings only and that
adding a non-snake_case must-fire spelling requires revisiting it. That is the
extraction-vacuity rule applied one level down, to the extractor this task added. The shape-poles (derive-`Debug`,
`skip_debug`) stay covered by @semantic-guard's directed run plus their own fixtures — the
drift check is a spelling tripwire, not the demonstration.
`pos_skip_debug_entry_removed.rs` still cites `crates/proto-gen/build.rs` in its
deliberate-leak banner, because it owes that anyway to name the real entry it stands in
for; it is simply not what the drift check keys on.

**The contract is written down at the assertion site, in the weaker-true-claim form**
(@security). A future reader meeting a green drift assertion will otherwise over-read it as
"every fenced token is a credential spelling and every one is covered" — the stronger claim
this deliberately does not make. The comment states the true weaker claim (a must-fire
*spelling* added to the poles must gain a fixture) and names the non-spelling spans as why
it is weaker, so nobody later "strengthens" it into the exclusion list just declined. Same
discipline as stating the scrape assertion's limits at the scrape assertion: an unqualified
green is read as the strongest claim it could support, not the one it actually supports.

**The anchor delimits the EXISTING "Must fire:" sentence — it does not add a parallel
list** (@security's refinement, and it is the one that keeps this from recreating the
bug it prevents). The obvious way to satisfy the narrow-anchor condition is a tidy
machine-readable list beside the prose; that is two enumerations, one read by the
agent and one by the parser, drifting silently. So the markup fences the existing
sentence and the parsed set and the prose the agent reads are the **same characters**.
If I find myself typing a spelling twice, that is the signal to stop and go back to
the owners.

Consolidated consent terms, all three adopted: **A** — a zero or short parse hard-FAILS
with a reason token *distinct from* an ordinary poles-versus-fixtures drift failure, so
the vacuous-parse case cannot be triaged as a drift bug and "fixed" by relaxing the
parse; **A2** — the assertion also fails when the `## Check: Credential Leak` heading
itself is missing, not only when the paragraph is; **B** — the narrow explicit anchor
above. If I cannot meet A and B cleanly, both owners prefer no parse at all and I drop
the assertion, because a wrong parse would ship as a green control. **The anchor markup
is an edit to their file**: I propose the exact markup, @security or @semantic-guard
apply it. The reverse direction (a `pos_` fixture spelling absent from the poles)
is reported as a finding routed to @security and @semantic-guard, not as a build break,
because that direction is a policy question about their file. And if the parser turns
out awkward, the fix is a better parser: the prose IS the control, since the semantic
agent reads it, and degrading it into a parser-friendly word list would trade the
control for a green drift check.

**`scripts/guards/semantic/checks.md` is amended by its owners, not by me.**
@semantic-guard and @security have both claimed the §Fixture poles amendment
(bare `kek` to realistic compounds, with the word-boundary reason stated) and will
add the fixture-directory pointer that makes §Fixture-verification runs actionable.
One request to them: if they are willing for the poles paragraph to be the
machine-read SSoT, I will add a drift assertion to the harness requiring every
must-fire spelling in the poles to appear in some `pos_` fixture. If they would
rather their prose not be parsed, I drop that assertion and the fixtures cite the
paragraph instead. Their call; I will not parse their file without consent.

### Item 2 — the release-build gate, as a real build

`scripts/release-feature-gate.test.sh`, table-driven over `(crate, feature)`, wired
into **Layer 1** via `scripts/lang/rust/compile.sh`, which @infrastructure authors (see
the classification note — my `layer3.sh` placement was overruled on ADR-0033 §3/§4
grounds and the reasoning is recorded there). Compiler cost belongs outside the fast
tier, and `compile.sh` is where cargo already lives. Sources `scripts/lang/_test_helpers.sh`
for `assert_marker` / `report_results` — no helper is redefined (@dry-reviewer 5).
Header states what the *test* does and cross-references `crates/mc-service/src/lib.rs`
for the *why*; it does not restate the predicate reasoning, which MH's own comment
forbids and which would be a third copy (@dry-reviewer 4, @code-reviewer 5).

**Arm order and assertions**, resolving the one live disagreement between reviewers:

1. **Clean arm first**: `cargo check --release -p mh-service`, expect rc 0. Run first
   so a broken tree reds here rather than silently satisfying the negative arm, and
   because it warms the dependency graph the second arm reuses. It also pins the
   predicate itself — without it, an arm asserting only non-zero exit passes when the
   crate stops compiling for any unrelated reason.
2. **Fire arm**: `cargo check --release -p mh-service --features per-frame-trace`,
   expect **non-zero rc AND the `compile_error!` diagnostic text AND the
   `crates/mh-service/src/lib.rs` origin**.

   @observability asked for exit code and not stderr text; @security, @test and
   @operations asked for the text. **Both, conjoined** — the conjunction is strictly
   stronger than either and neither reviewer's failure mode survives it. Non-zero
   alone false-passes on a typo'd feature name, an unrelated compile break, or an
   offline registry (@operations 3). Text alone would miss the scenario
   @observability named — `debug-assertions = true` appearing under
   `[profile.release]`, which makes the build *succeed* — but requiring non-zero
   catches exactly that. And this is not the "CI string check" shape §11 rules out:
   the string is matched against the output of a real compiler run, not against
   source text. The expected substring is **derived from the crate's own `lib.rs`**
   rather than retyped, so it cannot drift — **and the extraction itself hard-FAILS with
   its own distinct reason token on a zero or short parse** (@test finding A). Drift was
   handled; the empty extraction was not. If a future `lib.rs` refactor moves or
   reshapes the `compile_error!` so the extractor matches nothing, the needle is the
   empty string, `output contains ""` is vacuously true, and the text assertion silently
   degrades to rc-only — which false-passes on exactly the unrelated-compile-break path
   this conjunction exists to close. That is the poles short-parse defect wearing a
   different hat, and it gets the identical treatment: marker-not-found or a needle below
   a minimum length is a loud named failure, never an empty needle.

   **The needle crosses a `\` line continuation, and this is the extraction-vacuity rule
   biting in a shape my own guard misses** (@infrastructure, verified in the tree).
   `crates/mh-service/src/lib.rs:106-108` holds the `compile_error!` string as two source
   lines joined by a backslash continuation with leading indentation; rustc renders it as
   one line. Naive concatenation yields a needle carrying a stray backslash and
   run-together indentation, which can never match the rendered diagnostic — **and the
   minimum-length guard does not catch it, because the bad needle is long.** So the
   length guard is necessary and not sufficient. **Fix, from @security: derive the needle
   from a SINGLE source line, never the concatenation.** `must never be enabled in a
   release build` sits wholly within line 107, is specific to this control, and no
   continuation can break it. Plus two cheap well-formedness assertions — the derived
   needle contains **no backslash and no run of two or more spaces**, both being
   fingerprints of having accidentally spanned a continuation — and the sufficient form:
   **assert the extracted needle actually matched on the fire arm.** Recorded as the sixth instance of
   the rule and the first where an earlier instance's own mitigation proved inadequate —
   a length check answers "did we extract something", not "did we extract the right
   thing", and only the second question closes this. @security named the variant
   precisely and it belongs in the rule: **the input must be WELL-FORMED, not merely
   present.** A needle that is non-empty, long, and simply cannot match is the same
   category of lie as an empty parse — and note where the vacuity hid both times, in the
   *reading* step rather than in the assertion.

   **Both arms pin `--color=never`** (@infrastructure): rustc colorizes on TTY detection,
   and a substring match against ANSI-wrapped output is a flake class with a free fix.

**Profile, not RUSTFLAGS** (@operations 2, @observability): the release profile is
invoked *by name*, which is the profile `infra/docker/mh-service/Dockerfile:59` uses,
so the premise is bound to the shipped artifact. No `RUSTFLAGS` override — that would
decouple the assertion from the artifact. The free forcing function is stated in the
header: if `[profile.release]` ever gains `debug-assertions = true`, the control dies
silently and this test goes red.

**`cargo check`, not `cargo build`** — @operations measured it: `cargo check --release`
is 0-2s warm and 25s cold on 32 cores, against roughly 38s of headroom under the
ADR-0033 §4 90s p95 fast-tier budget for layers 3+6; `cargo build --release -p mh-service` is a measured 50-57s whenever
mh-service source changed — i.e. on exactly the devloops where this control matters —
which would put Layer 3 alone at roughly 100s and breach the total budget. Both
`check` arms together, alternating feature sets, are a measured ~2s steady state,
because the feature toggle invalidates only mh-service's own unit. `build` also buys
nothing for correctness: `compile_error!` fires during expansion, so the fire arm
never reaches codegen under either verb, and the only thing `build` adds is linking —
which is not what this control is about. `cargo check` runs the real compiler
under the real profile and expands the real `cfg` — @operations confirmed it emits
the `compile_error!` text with rc 101 — so it is a real build in the sense the task
means (a compiler invocation, not a string scan). **Ruled on and accepted by the Lead**, whose reasoning is
recorded here because a future reader must see this was decided and not elided: the
task's contrast is with a static grep over CI config or a Dockerfile — a control that
inspects text instead of compiling — and `cargo check --release` is not that. For the
arm that matters the two verbs are *identical*, because `compile_error!` fires during
macro expansion long before codegen, so the fire arm can never distinguish them.

**The residual, stated so `check` does not imply more than it demonstrates.** The pass
arm proves mh-service type-checks under the release profile without the feature; it
does **not** prove it codegens and links. What closes that: Layer 7's
`dev-cluster rebuild-all` rebuilds service images through
`infra/docker/mh-service/Dockerfile:59`, which is `cargo build --release --package
mh-service` — the deployed artifact itself, codegen and link included. **But that lane
is cluster-gated**: on a run where Layer 7 does not execute (no devloop cluster, CI
without provisioning), nothing in the pipeline codegens mh-service under the release
profile, and this self-test does not change that. Understating the reach is the correct
failure direction here — this entire task exists because the opposite happened.

**No nested cargo** (@operations 5): a shell self-test, never a `#[test]` shelling out
to cargo under Layer 4's outer `cargo test`. **No skip path**: the arms always run.
Environment failures — cargo absent, registry unreachable, offline, disk — emit
`PRECONDITION_FAILURE` / exit 2 (operator lane) rather than collapsing into FAIL and
burning a devloop attempt (@operations 4), and are distinguished by inspecting the
clean arm's failure text. A precondition exit is loud, never a quiet pass
(@security 5c).

**Runbook** (@operations 6): §6.3 subsection plus a §8 symptom row, with three
distinguishable symptoms — the expected compile error did not fire (the control is
broken: check `[profile.release]` and the `cfg` predicate); the clean arm failed for
an unrelated reason (the tree is broken, the fire arm tells you nothing); and the
precondition/operator lane. Runbooks are @operations' per ADR-0011; I draft, they
review the hunk.

### Item 3 — media metric hygiene at scrape level

**The task's premise for "client media metrics" is false against the real artifact,
and I am not writing an assertion over a surface that is not scraped.** Verified
independently and confirmed by @observability and @operations:
`infra/services/otel-collector/configmap.yaml` declares exactly one exporter,
`debug`, at `verbosity: normal`; `infra/kubernetes/observability/prometheus-config.yaml`
has no otel-collector job. No `dt_client_*` series exists in Prometheus at all, and
the collector log carries names and counts but never attribute keys or values. A
PromQL arm over client media metrics would return empty and pass green forever —
ADR-0036's own "alert selectors matching no real pod" failure, reproduced inside the
test written to prevent it.

The client half is landed where it is observable and is **already covered**: task 19's
`packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` asserts the exact
label set, the absence of any `end_to_end`/`e2e`/`zero_trust` key, and the absence of
a meeting dimension across all media emissions; `ingress.attribution.test.ts` asserts
the same on the pipeline. R3's real enforcement is structural — `mediaMetricLabels`
takes two named strings, so a meeting label is *unrepresentable* at that boundary. I
add no second SDK test (@observability 2, @dry-reviewer).

What I add instead is a **premise pin**, which is the honest client-surface claim: an
assertion that no scraped metric NAME starts with `dt_client_`. Today that is trivially
true and it *documents* the non-reachability; the day someone adds a Prometheus
exporter to the collector it reds and forces this suite's job list to be extended. Its
failure message says exactly that, so a red is never misread as a leak.

The predicate is the metric-name prefix and **not** a `client_version` label
(@observability finding 2): `client_version` is not intrinsically client-only — a
server-side `gc_join_attempts_total{client_version}` is a reasonable bounded-cardinality
metric someone could add — so that predicate has a false-positive mode whose failure
message would *misdiagnose* it and send the reader to the wrong file. `dt_client_` is
the TS SDK's reserved namespace, machine-enforced by `R26_NAME_RE`
(`crates/dt-guard/src/ts_metric_naming.rs:79`), and no Rust crate emits it. A Rust
service cannot legitimately produce that prefix, so a red means exactly one thing and
the message can say exactly one thing.

**Shape** (@operations 7). The checker is a pure function so its own FIRE half is
demonstrable, since a bad label cannot be planted on a live cluster:

- `crates/env-tests/src/fixtures/metric_hygiene.rs` — a pure function over
  `&[QueryResult]` returning violations. **No new normalized-series type and no
  adapter** (@dry-reviewer D3): `fixtures/metrics.rs:38`'s `QueryResult` already *is*
  metric-name-plus-label-map, since `__name__` sits in its `metric` map on a
  non-aggregated query, and `results_to_instance_map` at :219 is already the
  pure-function-over-an-`Ok`-`QueryResponse`-with-synthetic-`#[cfg(test)]`-fixtures
  shape I was proposing to invent. Landing beside its collaborators in `src/fixtures/`
  rather than a second metric-helper directory at `src/`. Its `#[cfg(test)]` module
  carries the FIRE fixtures: a synthetic `mh_media_frames_forwarded_total` carrying
  `meeting_id_hash`, one carrying a trailing compound `x_meeting_id`, one carrying an
  `e2ee` label key, one carrying `zero_trust` as a label *value*, and a clean control
  that must produce no violations. Without these the arm is an asserted control.
- One deviation from @operations' wording, flagged for their ruling: the function
  takes normalized series rather than raw exposition text, with a PromQL adapter.
  `query_promql` is the proven in-tree path (`30_observability.rs`) and the PromQL
  response already carries the exact stored label set the rule is about; an
  exposition-format parser would be new untested surface between the cluster and the
  assertion. @operations ruled on this with a stronger argument than
  mine: `ClusterPorts` exposes no MH or MC metrics URL at all, so an exposition-text
  kernel would need a new port in the ADR-0030 port map, new port-forwards and a new
  Layer-7 precondition — new flake surface against a two-attempt budget where a
  precondition failure halts the whole story — in exchange for a difference that is
  currently zero. **No second adapter is added**: one correct surface, one input path,
  and a second input path with no consumer would itself be a finding.
- `crates/env-tests/tests/32_media_metric_hygiene.rs` feeds live Prometheus into that
  same kernel, over jobs `ac|gc|mc|mh-service`. It carries
  `#![cfg(feature = "observability")]`, mirroring `30_observability.rs` — that feature is
  inside `--features all`, which `layer7.sh:825` actually enables, so the file runs
  rather than compiling into silence (@test). A wrong gate here would be a dead test,
  which is this task's own subject matter.

**One fetch, both assertions — @observability finding 1, and it is structural rather
than a check that has to notice.** The anchor and the absence assertion must run
against the *same* in-memory series set, fetched once. Two separate queries leave
exactly the hole the anchor exists to close: a typo, a wrong job name or a
non-matching regex in the absence selector leaves the anchor passing (it queries the
MH family by name) while the absence assertion inspects zero series and reports clean
— a green suite with a satisfied precondition and no input, which is the "alive, never
applied" failure reproduced one level in from where I refused to put it. So: fetch the
series set once, assert it is non-empty and contains the MH media families, then run
the pure predicate over that same `Vec`. The anchor then *guarantees* the predicate
had input, because there is only one input.

**The PromQL view's own premise is checked, not commented** (@operations, and it is the
sibling of the `dt_client_` pin on the other side of the scrape). The stored label set
equals what the pod emits only because nothing strips labels between scrape and
storage. That holds today — zero `metric_relabel_configs` in `prometheus-config.yaml`,
and the four service jobs carry only target-selection `relabel_configs` with
`action: keep` — but it is a configuration fact, not a property. If a
`metric_relabel_configs` is ever added, PromQL goes blind to a label the pod still
emits and this control silently narrows while staying green, which is the exact failure
class the task is about. So the suite asserts it: `ClusterConnection::check_prometheus`
already hits `/api/v1/status/config`, so the config text is one proven call away, and
the assertion fails with a message saying the control has narrowed and the fix is to
read labels at the pod — never to widen the selector. No new plumbing, no new
precondition.

**The assertion is on `metric_relabel_configs` only, deliberately, and the comment says
why so nobody widens it** (@observability). The four service jobs do carry
`relabel_configs`, but those are `keep`-only on `__meta_kubernetes_*` — target
*discovery*, not sample rewriting. `metric_relabel_configs` is the mechanism that could
silently **hide** a violation, by dropping or replacing a label before storage; a
`relabel_configs` addition would surface as a new label and the predicate would **catch**
it as a violation. Extending the assertion to `relabel_configs` would add noise without
closing anything.

**The kernel evaluates STORED series, not exposition text, and that is the surface the
rule is about** (@observability's ruling, which is a better reason than mine): R1 bars
a meeting identifier because of cardinality and per-meeting aggregation *in the metrics
backend*, so the harm is realized at the stored series. The two surfaces can disagree
in both directions — exposition text would miss a label added by
`metric_relabel_configs` and would falsely flag one that relabeling drops before
storage. They agree today only as a configuration fact, not as a property. **No second
exposition-text adapter**: one correct surface, one input path.

**Non-vacuity is asserted first, and its absence is a precondition failure, not a
pass.** `mh_media_frames_forwarded_total` is resolved eagerly at MH startup by
`resolve_media_handles()`, so every media series exists at zero on a running MH pod
with no media flowing — that is the anchor. MC's media metrics are created lazily on
first emission and are absent on an idle cluster, so MC is **not** in the presence
anchor; it stays in the absence selector, and the module doc says why rather than
letting a regex that matches nothing read as coverage (@observability 3). Scrape lag
is handled by the existing `assert_eventually(ConsistencyCategory::MetricsScrape, …)`
category, not by a hand-rolled sleep — no wall-clock shape, ADR-0028 zero-retry
respected.

**Lane, corrected by @operations: an env-test structurally cannot emit
`PRECONDITION_FAILURE`.** `layer7.sh`'s classifier makes Phase 1 the only infra lane
and maps any non-zero suite exit to `FAIL / env-tests-failed` without grepping suite
output. So the MH-presence failure follows the in-tree precedent instead: it panics
with its own distinctive greppable triage string, the lane stays `FAIL`, and the §8
runbook row is keyed on that string — the same shape `mcMetrics.ts` uses with
`Triage Prometheus/port-forward`, which §6.7 already documents as the one exception to
fix-the-failing-test. `PRECONDITION_FAILURE` / exit 0 is used only in the layer-3
shell self-test, which can express it and where `tee_collect_statuses`' worst-status
aggregation carries it to the layer's exit 2.

**The MH/MC presence asymmetry is deliberate and is written down twice** — in the
test's doc comment and in the runbook — because it reads as an oversight and the
obvious "fix" is fatal: an MC-shaped presence check would red on every idle run and be
muted within weeks, which is how a control dies.

**The empty allowlist carries its reasoning at the assertion site** (@security): an
empty allowlist with no explanation is the first thing a future author fills in. The
comment cites the two *structural* facts — no Prometheus exporter on the collector, no
otel-collector scrape job — and deliberately not the collector's `verbosity: normal`,
which is a knob someone can turn (@operations).

**No carve-out, and therefore no second list to drift** (@observability 4,
@operations 9). The grandfathered set is the ADR-0028 *client SDK* join-flow metrics,
which are TypeScript and never scraped; @observability verified no Rust service metric
carries `meeting_id` in any form. So over these jobs the rule is absolute with an
empty allowlist. An exclusion here would be dead code that also widens the hole if a
Rust metric ever grows the label.

**Matching is by segment or case-insensitive substring, never word-boundary**
(@security 6) — that is the same lesson item 1 is about. The meeting-identifier
predicate uses substring containment, which closes a **documented** hole in
`PII_PREFIX_DENYLIST`: it matches with `starts_with`, so a trailing compound like
`x_meeting_id` is not a prefix match, and the taxonomy marks that case `[reviewer-only]`
for exactly that reason. So this assertion is **strictly stronger than the mechanical
guard on trailing compounds**, not redundant with it — the module doc says so, because
the next reader will otherwise assume the scrape check duplicates the guard
(@observability). The overclaim
predicate covers metric names, label keys **and** label values.

**Item 3's own "does it apply" half is a live hazard, and @security found it.** The
kernel's FIRE half runs in the always-run Rust test lane, so that half is safe. The
APPLY half is a Layer-7 env-test, and Layer 7 green-skips in CI:
`scripts/layer7.sh:441-442` emits `SKIPPED-NO-CLUSTER` at exit 0 and
`scripts/lang/_common.test.sh:60` asserts `OK` dominates it, so CI TOTAL stays OK. That
skip is deliberate, arbitrated and narrow — socket absent AND `GITHUB_ACTIONS` set,
with socket-absent *locally* being a loud `PRECONDITION_FAILURE` at exit 2 — and
nothing about it is being changed. But if this assertion is written here and never runs
against a real cluster, it ships as a control that has never fired in its life, which is
**verbatim the first row of §11's own failure table**. Landing that inside the task
whose subject is "a control's coverage must be demonstrated, not asserted" would be the
sharpest available own-goal.

**Commitment: the suite is run against a real cluster at least once in this devloop and
the result is recorded in §Devloop Verification Steps** — the scrape it inspected, how
many series it saw, which media families were present, and the verdict. Two things that
run must establish, because each is the difference between alive and applied: that it
saw a **non-empty** scrape (an assertion over zero series passes vacuously — the same
shape as the zero-parse condition, with a reason token distinguishing "scrape empty or
target unreachable" from "found a forbidden label"), and — with @observability's
correction to @security's wording, because adopting it as written would have undone an
earlier finding — that it inspected the **MH** media families specifically. **MH is
asserted; MC is recorded, not gated.** `resolve_media_handles()` runs unconditionally at
`crates/mh-service/src/main.rs:367`, so absence of `mh_media_*` is a genuine fault worth
a hard gate. MC's media metrics are lazily created, so their absence on an idle cluster
is the *expected* state; a hard "confirm MC media metrics were inspected" gate would red
on every idle run and be muted within weeks, which is the failure mode this task exists
to prevent. So the run record states whether MC media series were present and, if
absent, names that as an observed limitation of that run. Condition 2 must not quietly
re-symmetrise the MH/MC asymmetry already in the module doc.

**The recorded live run is a hard requirement, not best-effort — the Lead checked and a
cluster is up** (`devloop-hear-yourself-through-handler`, pods healthy, Prometheus
reachable). "Written but never executed" is off the table: with a live cluster present,
shipping this control unexecuted would be §11's first failure row by *choice* rather than
by circumstance. If the suite reds against the real cluster, that is a finding to fix,
not a reason to fall back to recording it as unrun.

**This assertion's own APPLY limit is stated in the assertion.** A scrape sees only
series that were emitted; a meeting-labelled metric on a rare path is invisible to it.
Written into the module doc so a green is never read as more than it is — that is the
"does it apply" question turned on this control itself, which §"demonstrated, not
asserted" requires.

**Framing** (@semantic-guard): the overclaim predicate is deliberately *not* part of
the credential-leak check — checks.md §"Not this check" keeps it out — and nothing
here is framed as satisfying it.

### The rule this plan produced, stated once rather than three times

Four instances, found independently by three reviewers across three surfaces — @security on the poles parser, @test on the `compile_error!` needle extraction,
@security again on the scrape series count. @security asked for it to be recorded as a
rule rather than as three fixes, and they are right that the rule is stronger than any
of the findings that produced it:

**Any assertion whose subject is EXTRACTED from something else has two failure modes,
and only one of them is visible. "The extracted value is wrong" reds loudly. "The
extraction found nothing" degrades to a tautology and reds never.** A needle of `""`
makes `contains(needle)` vacuously true; an empty spelling set makes "every spelling has
a fixture" vacuously true; an empty series set makes "no series carries a forbidden
label" vacuously true. So: **wherever this plan reads something, "did the extraction
have input" is a separate mandatory assertion carrying its own distinct reason token** —
distinct on purpose, because if the vacuity failure and the content failure look alike,
the vacuity case gets triaged as a content bug and then "fixed" by relaxing the
extractor, which restores the vacuous green with a passing test on top of it.

**A fourth instance, from a different surface, which is what shows the rule
generalises** (@observability). The runbook §8 row keyed on the literal
`Triage MH scrape/metric-registration` has the same shape with a human as the extractor:
its usefulness depends entirely on a literal matching a string the test emits, and if
either side is reworded the row silently stops matching — the runbook still reads as
coverage, the test still panics informatively, and the link is gone with nothing red. A
responder greps the string from a failing run, finds no row, and concludes the failure
is undocumented. The mitigation @observability and I had reached for — they verify the
test side, @operations verifies the runbook side — is precisely the
control-that-depends-on-two-people-remembering this rule replaces. So each side points
at the other: the literal is a named constant in the env-test whose doc comment names
the runbook section, and the §8 row names the constant. That is not mechanical, but a
reworder meets the obligation from whichever side they arrive at. No new machinery is
invented for it in this devloop.

A second, related property worth a runbook sentence rather than only a code comment: two
controls of comparable quality get different liveness answers **decided by which lane
they sit in**, not by anything about the assertions. The release-gate self-test always
runs in layer 3 and its operator-lane cases exit 2; item 3 sits in Layer 7, which
green-skips in CI at exit 0. The §8 entries name each control's lane and whether that
lane has a structurally green skip, so a reader does not assume they behave alike.

### R. Residuals recorded, nothing ticked

Per @security 7, no existing `docs/TODO.md` entry is closed: the overclaim-boolean
entry survives (§11 bars the boolean in logs, dashboards and documents too, and it
needs its own named check), as do the MH-frame-forwarding scope entry and the
`wrapped_key`-CATEGORY_A entry. New residuals to record: the MC `test-seams` sibling
row; the client media-metric scrape non-reachability; **@operations' per-layer budget
warn entry, which I carry in my diff at their request** (their finding, operations-owned
content — routed through me because `docs/TODO.md` is already in my file list and a
competing edit from them would break Gate 2's frozen tree; they accept it as a deferral
on design-decision grounds and it makes their verdict RESOLVED-DEFERRED rather than
RESOLVED-FIXED, which they want visible rather than averaged away); and @observability's proposed
widening of the `pii_vocabulary.rs:156-160` KNOWN LIMIT comment to cover trailing
compounds, which is theirs and @security's policy content and which I will author only
if they ask.

### What this deliverable is, stated plainly

For item 1 the honest description is **"the floor's extent is now provable"**, not "the
gap is closed". The word-boundary matcher cannot see past the trailing compound in
`transmit_key_bytes`, and closing that means promoting `segments()` to `crate::common::`
and repointing `rust_log_secrets` across every CATEGORY_A term in three consumers —
guard machinery, infrastructure-owned, with its own test surface, out of scope here. This
devloop makes that gap *demonstrable* rather than repairing it. A task whose whole
subject is that coverage must be demonstrated rather than asserted would be poorly served
by a record that overstates its own reach, so it is not dressed up: two reviewers forecast
a RESOLVED-DEFERRED verdict on that spin-out and that is the correct outcome for a
demonstration task.

### Resolved questions (were open at first draft)

1. `meetingKek` dropped and `cargo check --release` read as "a real build" — **both ruled
   accepted by the Lead**, with conditions recorded above (deviation logged; residual
   stated; text-plus-rc conjunction; profile by name).
2. MC's `test-seams` row — **closed as a residual.** Harness stays `(crate, feature)`-driven
   with only the MH row live, so adding MC later is one line. @dry-reviewer files the
   sibling at verdict time under `docs/TODO.md` §Cross-Service Duplication (DRY). Not my
   carry.
3. Parsing §Fixture poles — **both owners consented**, terms A/A2/B plus exact-token
   semantics and the resolution-2 path-span fix, all recorded above.
4. Kernel input type — **@operations ruled the PromQL/`&[QueryResult]` path**, no second
   adapter; @dry-reviewer showed the type already exists at `fixtures/metrics.rs:38`.

### Trailer availability (was a blocker — now dissolved)

**RESOLVED by @infrastructure's ESCALATE and the Lead's ruling, not by a trailer.** The
`scripts/layer3.sh` row ceased to exist: the self-test wiring moved to Layer 1
(`scripts/lang/rust/compile.sh`) and @infrastructure authors it, plus a one-line pointer
comment in `layer3.sh` so "where do self-tests get wired" still resolves. Both drop from
my changeset and no Row-1 trailer is needed. `release_build_profile.rs` stays mine to
author with @infrastructure's trailer. **The script's design survives the move
unchanged** — arm order, the conjoined rc + text + origin assertion, profile-by-name over
RUSTFLAGS, `cargo check` over `cargo build`, no skip path, `PRECONDITION_FAILURE`/exit 2
for environment failures, no nested cargo — and @infrastructure has no findings against
any of it. Path stays `scripts/release-feature-gate.test.sh`; they wire against exactly
that.

**Ordering, agreed with @infrastructure**: I create the script and confirm it exists and
is executable; they land the wiring immediately after. Both hunks land in the same commit,
so neither the wired-but-missing-file state nor the present-but-unwired state persists in
history.

Superseded text follows. Two infrastructure-owned Minor-judgment rows (`scripts/layer3.sh`,
`crates/dt-guard/src/release_build_profile.rs`) need `Approved-Cross-Boundary:` trailers.
The Lead is resolving this by **adding @infrastructure to the team as the conditional
domain reviewer** (ADR-0024 §6.3 — the fix for "owner not on the team" is to put them on
it, not route around the requirement), not by a spin-out. @infrastructure will either
confirm both rows with trailers or upgrade a classification to Domain-judgment, in which
case they author that hunk. **I do not start either row until the Lead relays their
answer**; everything else is unblocked on "Plan approved".

---

## Pre-Work

None.

---

## Implementation Summary

Three controls, each with a machine-runnable FIRE half and an APPLY half bound to
a real artifact.

**1. Credential-leak key-custody fixtures (items 11-13).** Eleven fixtures under
`crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/` (7 `pos_`,
4 `neg_`), each carrying a deliberate-leak banner and an `// Invariant:` block
stating verdict and check item. Harness at
`crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` asserts the four
things a machine can: bidirectional catalog integrity, poles-to-fixture drift,
the mechanical floor's per-spelling extent, and the check's scope being non-empty
against the real tree. The semantic FIRE half is @semantic-guard's directed run.

**2. Release-build gate.** `scripts/release-feature-gate.test.sh`, table-driven
over `(crate, feature, lib.rs)`, wired into Layer 1 by @infrastructure. Clean arm
first, then the fire arm asserting non-zero rc AND the `compile_error!` text AND
the gate-source origin.

**3. Metric hygiene.** Pure kernel at
`crates/env-tests/src/fixtures/metric_hygiene.rs` with nine `#[cfg(test)]` FIRE
fixtures; `crates/env-tests/tests/32_media_metric_hygiene.rs` feeds live
Prometheus into that same kernel, plus a client-namespace premise pin and a
`metric_relabel_configs` premise check.

---

## Devloop Verification Steps

Everything below was **run**, not reasoned. Where a trap was claimed, it was
injected and watched to fail.

### Release gate — the vacuous path, demonstrated

Real fire arm: `cargo check --release -p mh-service --features per-frame-trace`
returns **rc=101**, renders the message on one line, and cites
`crates/mh-service/src/lib.rs:105`.

**The trap that matters**, recorded verbatim at @security's request. With a
typo'd feature name (`per-frame-trce`), cargo emits
`error: the package 'mh-service' does not contain this feature: per-frame-trce`
and exits non-zero — so `fire-arm-exits-non-zero` **PASSES**. An rc-only
assertion would have reported the control firing while it was never reached.
The text and origin assertions red. This is the empirical proof of the vacuous
path that could only be argued for at Gate 1, and it is why the conjunction is
load-bearing rather than belt-and-braces.

**A dead assertion, found by running the trap rather than reading the code.** The
`assert_absent` guarding that case was written from memory as
`none of the selected packages contains these features`; cargo actually says
`does not contain this feature`. It passed on the exact input it existed to
catch, and had been green since the moment it was written. Fixed against real
cargo output and re-verified. This is the live instance behind mechanism 4 of the
vacuity taxonomy.

**Needle-not-found trap**: pointing the table at a `lib.rs` without the marker
yields `STATUS=PRECONDITION_FAILURE REASON=release-feature-gate-needle-not-found`,
exit 2.

**Layer 1 green** after wiring: `cargo-build-passed`,
`cargo-build-dt-guard-passed`, `cargo-build-dt-story-passed`,
`release-feature-gate-passed`, EXIT=0, with `5 passed, 0 failed`.

### Credential-leak harness — four traps injected

- **Poison-token test** (@semantic-guard's, all eight tokens): the classifier
  yields exactly `["meeting_kek", "transmit_key_bytes"]`. Zero poison tokens
  leak. The four non-spelling spans in the fenced sentence — the dotted proto
  package, `#[derive(Debug)]`, the `skip_debug(...)` call site and the
  `crates/proto-gen/build.rs` path — are excluded by construction.
- **Drift trap**: removing the two `transmit_key_bytes` fixtures reds both
  `catalog_and_directory_agree` and
  `poles_must_fire_spellings_each_have_a_fixture` with `POLES-FIXTURE-DRIFT`.
- **Floor-extent trap**: flipping the `meeting_kek` expectation to `false` reds
  with `mechanical floor extent changed for meeting_kek`.
- **Scaffolding trap fired for real during development.** The first version of
  the floor test invoked the guard from the wrong working directory; the
  diff-base resolver ran against the real repository and returned a SHA absent
  from the throwaway root. The `collecting-changed-rust-files` assertion caught
  it. Without that assertion the test would have reported "no hit" for **every**
  row and passed — including the `meeting_kek` row that must hit. An earlier
  manual verification of the same rows had "passed" only because it happened to
  be run from inside the throwaway root.

**Mechanical floor extent, measured against the shipped guard** (not inferred):
`meeting_kek` produces two VIOLATIONs at an MC log site
(`secret_in_log_macro`, `secret_in_tracing_field`); `transmit_key_bytes` and
`meetingKek` produce none. The reviewers' table is correct as stated.

### Metric hygiene — the live cluster run

Required by the Lead as a hard condition, since a control landed into a
green-skipping lane and never executed would be §11's own first failure row.

Run against `devloop-hear-yourself-through-handler` (Prometheus at
`host.containers.internal:24500`), all three tests **passed**:

- `media_metric_labels_carry_no_meeting_identifier_and_no_overclaim`
- `client_media_metrics_are_not_scrape_reachable`
- `no_metric_relabeling_narrows_this_suite_silently`

**Evidence the run was not vacuous**, which is the point of recording it:
`metric-hygiene: inspected 2585 series across jobs
[ac-service|gc-service|mc-service|mh-service]; mc_media_* present: true`.
The MH anchor family was present (the anchor gate passed), 2585 real series were
inspected by the predicate, and MC media series happened to be present on this
cluster — **recorded, not gated**, exactly as @observability required.

Nine kernel FIRE fixtures pass in the always-on lane, including the trailing
compound `x_meeting_id` that `PII_PREFIX_DENYLIST` structurally cannot catch.

### Pipeline

Layers 2, 3 and 5 green. Two real failures were found and fixed rather than
worked around: `cargo fmt` on the new Rust files, and a `collapsible_if` clippy
error in the poles parser. One classification-table defect was found by the
scope-drift guard — two `docs/TODO.md` rows carrying descriptive suffixes rather
than a real path — and corrected to a single row at the stricter tier.

---

## Files Modified

**Mine (new):**
- `scripts/release-feature-gate.test.sh` — release-build gate self-test, table-driven over `(crate, feature, lib.rs)`
- `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs` — machine-checkable half of the items 11-13 demonstration
- `crates/dt-guard/tests/fixtures/credential_leak_key_custody/README.md`
- `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/` — 11 fixtures (7 `pos_`, 4 `neg_`)
- `crates/env-tests/src/fixtures/metric_hygiene.rs` — pure predicate + 9 FIRE fixtures
- `crates/env-tests/tests/32_media_metric_hygiene.rs` — live-scrape APPLY half

**Mine (modified):**
- `crates/env-tests/src/fixtures/mod.rs` — one `pub mod` line

**Cross-boundary (modified by me, owner trailers required):**
- `crates/dt-guard/src/release_build_profile.rs` — doc comment only: polarity correction (the module stated its own protected premise backwards) + instance count. Owner: infrastructure + security.
- `crates/dt-guard/src/common/pii_vocabulary.rs` — KNOWN LIMIT comment widened to trailing compounds. No token added, removed or reworded. Owner: security + observability.
- `docs/runbooks/devloop-validation.md` — §6.1 five corrections + release-gate triage sub-table; §6.3 stale self-test enumeration; three §8 rows keyed on the env-test's triage constants. Owner: operations.
- `docs/TODO.md` — overclaim entry annotated (UNTICKED); per-layer budget entry (operations' wording); MC `test-seams` sibling residual. Owner: semantic-guard + observability; operations.
- `.claude/skills/devloop/review-protocol.md` — assertion-vacuity taxonomy. Owner: test + code-reviewer.

**Not mine (landed by their owners in this devloop):**
- `scripts/lang/rust/compile.sh`, `scripts/layer3.sh` — @infrastructure
- `scripts/guards/semantic/checks.md` — @security + @semantic-guard (poles amendment + anchor markup)

---

## Gate 2 — Validation (Lead-run)

Run by the Lead over the frozen tree, not accepted from the implementer's per-layer runs. Unattended mode (`DEVLOOP_FAIL_FAST=0`), so all seven layers were evaluated in one pass rather than stopping at the first red — which is what makes the verdict an authority rather than a partial report. Tree hash at validation: `6492f9962e9de758103e95c00b8ecf86310afd79`.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 28 |
| 2 Format | OK | 2 |
| 3 Guards | OK | 48 |
| 4 Test | N/A | 168 |
| 5 Lint | OK | 2 |
| 6 Audit | N/A | 2 |
| 7 Env-tests | OK | 675 |

`TOTAL_RESULT=N/A`, `TOTAL_DURATION=925`. **Zero `FAIL`, zero `PRECONDITION_FAILURE`, zero `NOT-RUN`.**

**Re-validated twice more, because the tree moved.** The fifteen review fixes were substantial — particularly the metric-hygiene kernel rebuild, which changed Layer 4's cargo-test surface — so the verdict bound to `6492f996` stopped applying and was not relied on. Second full pass: layers 1/2/3/5/7 OK, `TOTAL_DURATION=730`. Third pass after the final banner edit, run because this devloop's own record argues against assuming a comment-only change is inert: L1 OK 3s, L2 OK 1s, L3 OK 49s, L4 N/A 172s, L5 OK 2s, L6 N/A 1s, L7 OK 531s, `TOTAL_DURATION=759`. Across all three passes the only non-`OK` statuses were the two proto intentional-gap placeholders, the two aggregate `N/A` rollups they produce, and the Layer-6 `no-dep-changes` dep gate. Zero `FAIL`, zero `PRECONDITION_FAILURE`, zero `NOT-RUN`, every time.

The two `N/A` layers are the documented self-justifying cases and neither is an unmeasured layer. Layer 4 aggregates `cargo-test-passed` and `nx-test-passed` alongside proto's registered intentional-gap placeholder (`REASON=not-applicable-to-this-lang`); Layer 6 aggregates `cargo-audit-passed` and `buf-breaking-passed` alongside the same proto placeholder and the dep-manifest gate's `SKIPPED-NO-DIFF REASON=no-dep-changes`. Worst-child aggregation then carries `N/A` up to the total. That is the wrapper contract in ADR-0033 §6 behaving as specified, and it matches the precedent recorded in commit `a99e1078`. It is deliberately not treated the way a `NOT-RUN` would be: every layer here was evaluated.

The new gate is green in its ruled home: `STATUS=OK REASON=release-feature-gate-passed` appears in Layer 1, alongside `cargo-build-passed`, `cargo-build-dt-guard-passed` and `cargo-build-dt-story-passed`. Layer 3's twelve self-tests all pass. Layer 7 ran both Phase-2 suites against the live cluster — Rust env-tests and the eight-spec browser E2E, `browser-e2e-passed`.

### Directed credential-leak fixture-verification run (item 1's FIRE half)

The credential-leak check is agent-executed, so its fire half cannot be a `cargo test`. The Lead directed a fixture-verification run over the named files under `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/`, which is the carve-out `scripts/guards/semantic/checks.md` §Fixture-verification runs exists to authorize — without it a scope-exempt "no findings" would have been procedurally correct and completely vacuous.

**Result: 11 of 11 fixtures matched their `// Invariant:` blocks. Seven positives fired, four negatives stayed clear, zero discrepancies.** No plant needed softening and no vocabulary addition was reached for.

The load-bearing case is `pos_contract_crossing_renamed_material.rs`, the value-not-name plant, and semantic-guard's judgement on it was recorded separately rather than folded into a pass count. They traced the value rather than the name: it originates in `MeetingKeyState.kek`, is read raw by `raw()`, renamed `material` in `collect_payload()`, renamed `blob` in `register()`, and lands in `RegisterMeetingRequest.opaque_bootstrap` on the internal.v1 MC→MH contract — the meeting KEK the whole way, under three names no vocabulary contains, with the sole `kek` token a private field two hops upstream. So on this run items 11-13 demonstrably behave as the value-following check they claim to be rather than as a word list. That is the single result this half of the task existed to obtain, and it could have gone the other way.

The *applies* half — the one ADR-0036 says nobody tests unprompted — came back clean against the real tree, verified in-tree rather than asserted: the real `RegisterMeetingRequest` at `crates/mc-service/src/grpc/mh_client.rs` carries routing and identity fields only; the other internal.v1 constructions carry no key material; `MeetingKek` is a `SecretBox` wrapper whose derived `Debug` redacts, guarded by an in-tree test; and a full sweep of MC's tracing macros found exactly one key-adjacent log site, `crates/mc-service/src/main.rs`, which emits decode-error metadata and not the secret. Alive and aimed, which is both halves.

---

## Code Review Results

Fourteen findings across six reviewers at Gate 2. **All fixed, none deferred** —
every one was inside the changeset with no design ambiguity, so the
suspicious-deferral check applied to all of them.

- **@operations (2)** — `cargo-build-workspace` and `cargo-check-passed` phantom tokens.
- **@infrastructure (3)** — the same `cargo-build-workspace` token; the release-gate
  triage sub-table covering 3 of 5 assertion labels and routing two to the wrong
  fix; and the mechanism-4 misclassification (below).
- **@security (2)** — the fifth vacuity mechanism; `opaque_bootstrap` being
  fictional and why that is a security-positive fact.
- **@observability (2)** — the relabel assertion scanning the whole config body;
  the missing keys-only rationale.
- **@dry-reviewer (2 + 2 entries)** — the `Series` doc contradicting its own
  declaration; hand-maintained tallies; plus two `docs/TODO.md` entries landed on
  their behalf.
- **@test (1)** — mechanism 5, ruled in as joint owner of the review protocol.

**One finding changed a rule I had just written.** @infrastructure showed that
"never let a foreign-worded expectation carry primary coverage" misclassifies
`assert_rc 101` — rustc's compile-error code is toolchain-defined, has no in-repo
source, and *is* primary coverage, yet is entirely safe because a changed exit
code turns the assertion **red**. The real discriminator is **fail-open versus
fail-closed**, not primary versus secondary. The taxonomy now says that. Their
correction also rejected the "wrap the foreign diagnostic" remedy as relocating
the coupling rather than defending it, which was mine and was wrong.

---

---

## Gate 3 — Final Approval

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 3 | 3 | 1 spin-out (accepted at Gate 1) | Also authored 4 corrective edits to the co-owned `checks.md` |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | Ruled mechanism 5 in as owner of the review protocol |
| Observability | RESOLVED-DEFERRED | 6 | 6 | 1 spin-out | Caught the dead relabel control after it had passed all seven layers |
| Code Quality | CLEAR | 0 | — | 0 | Re-issued against the current tree after the diff moved |
| DRY | RESOLVED-DEFERRED | 9 | 9 | 0 findings; 2 extraction opportunities | Every finding fixed; classification comes from §Accepted Deferrals being non-empty |
| Operations | RESOLVED-DEFERRED | 2 | 2 | 1 (own finding) | Budget-warn entry deferred on design-decision grounds |
| Semantic Guard | CLEAR (native SAFE) | 0 | — | 0 | Executed the directed fixture-verification run |
| Infrastructure (conditional) | RESOLVED-FIXED | 3 | 3 | 0 | Owner-implemented the Layer-1 wiring after the Gate 1 ESCALATE |

Fifteen findings across six reviewers, all fixed. Three verdicts land on RESOLVED-DEFERRED for a single shared spin-out, one for the reviewer's own accepted deferral, and one because §Accepted Deferrals is non-empty with two ADR-0019 extraction opportunities that never entered the fix-or-defer flow.

### What the review actually caught, and why it is the point

Three defects surfaced that no amount of reading would have found, and each is an instance of the failure this task exists to demonstrate against.

**A dead control shipped, passed all seven layers and a live-cluster run, and was caught only because a reviewer verified a fix instead of accepting the report of it.** `no_metric_relabeling_narrows_this_suite_silently` read `/api/v1/status/config`, which returns JSON with the config embedded as an escaped string — zero literal newlines in the body. The code called `.lines()`, matched no job, and passed on every possible input including a real violation. It would have passed forever. It arrived *inside the fix to a review finding*, written roughly an hour after its own author added the assertion-vacuity taxonomy to the review protocol, and it is a fourth live instance of that taxonomy's first mechanism. The reviewer who caught it also filed it against themselves: their Finding A had asked for the scoping that introduced it, without checking the endpoint's response format. Fixed three ways, the third structural — JSON parse, a fail-closed `ServiceJobsMissing` error that can never read as clean, and the parse moved into the pure kernel where it gets FIRE fixtures, taking them from 9 to 14, including one that asserts the fixture body has zero literal newlines *before* asserting the parse, so a regression to raw scanning reds immediately.

**An assertion written from memory was green from the moment it was written.** The implementer's `assert_absent` guarding the release-gate typo case expected `none of the selected packages contains these features`; cargo says `does not contain this feature`. It passed on the exact input it existed to catch.

**A test invoking a guard from the wrong working directory would have reported "no hit" for every row, including the row that must hit.** An earlier manual verification of those rows had "passed" only because the author happened to be in the right directory at the time. This became the taxonomy's fifth mechanism — ruled in by the review protocol's owner on the grounds that the first four are all data-path failures while this one is execution context silently substituting a different subject, with the assertion well-formed and the expectation correct throughout, and a remedy different in kind: a positive control in the subject rather than a did-this-have-input token in the reader.

**A phantom REASON token was introduced into the very table whose four stale siblings the same hunk was correcting.** The implementer then swept the file rather than stopping at the two reported, and found a fourth instance in §3's worked example — by operations' own rating the worst placement, since a worked example is what a triager pattern-matches against fastest.

### Corrections that ran against their authors

Recorded because a lessons-learned section that only lists other people's errors is fiction. Security corrected a credit assigned *to* them — the fail-open/fail-closed discriminator is infrastructure's; security's own primary-versus-secondary rule misclassifies `assert_rc 101`, which is toolchain-defined, has no in-repo source, carries primary coverage, and is entirely safe because a changed exit code turns it red. Security also caught their own `sed`-range false negative on the overclaim annotation. Observability retracted an inverted acceptance criterion after verifying the correction rather than accepting it, and later filed the dead-control finding against a fix they had themselves requested. Operations conceded the layer placement on the merits and noted the pattern in their own review: reliable at naming defect *classes*, unreliable at enumerating their members — an argument for the mechanical sweep rather than for more careful reading. The implementer replaced their own banner fix with security's after seeing that theirs left a ranking in place and appended a warning against acting on it, which is the wrong shape.

### Trailers

Recorded on the commit per ADR-0024 §6.7. Infrastructure re-confirmed both cross-boundary rows at Gate 3 as §6.3 requires, not only at Gate 1.

---

## Accepted Deferrals

No findings against this diff were deferred. Pre-agreed spin-outs, owned by others:

- `docs/TODO.md` §Observability Debt — the word-boundary matcher gap on trailing compounds (owner: infrastructure).
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — per-layer budget warn dead for Layers 3 and 4 (owner: operations).
- `docs/TODO.md` §Cross-Service Duplication (DRY) — MC `test-seams` sibling gate, and the fixture-README convention hoist (filed by @dry-reviewer).

---

## Issues Encountered & Resolutions

**Three defects in my own work, all found by running rather than reading.** Each
was a green I would otherwise have believed, and each is an instance of the
taxonomy this devloop produced.

1. **A dead `assert_absent`.** Written from memory as `none of the selected
   packages contains these features`; cargo says `does not contain this feature`.
   It passed on the exact input it existed to catch and had been green since I
   wrote it. Found by injecting the typo'd-feature trap. This is taxonomy
   mechanism 4 — and I committed it in the same sitting as writing the rule
   describing it.
2. **Three of my own guards could never fire.** After the needle became a
   fixed-phrase `grep -o`, the backslash / double-space / minimum-length checks
   were structurally unreachable. @security and @infrastructure had both endorsed
   them; I declined all three and both agreed. Shipping green-forever controls
   inside this devloop would have been self-refuting.
3. **The floor test observed the wrong repository.** Invoked from the wrong
   working directory, the diff-base resolver ran against the real tree and
   returned a SHA absent from the throwaway root. My scaffolding assertion caught
   it; without it the test would have reported "no hit" for every row — including
   the row that must hit — and passed. An earlier manual verification of the same
   rows had "passed" only because I happened to be `cd`'d correctly. This became
   taxonomy mechanism 5.

**4. A DEAD CONTROL, shipped inside the fix to a review finding.** @observability
asked me to scope the `metric_relabel_configs` check to the four service jobs,
because the other four Prometheus jobs are exactly where relabeling is standard
practice and a whole-config check would fire falsely. Correct finding. My fix
split the raw HTTP body on `- job_name:` and called `.lines()` — but
`/api/v1/status/config` returns **JSON with the YAML as an escaped string**, so
the body contains **zero newline characters** (verified against the live
endpoint). Every job-name extraction returned the entire remainder of the config,
matched nothing, and the assertion passed on every possible input including a real
violation. A correct-but-noisy control became a permanently silent one.

This is the fourth live instance of taxonomy item 1 — an assertion over an empty
set — and the sharpest, because it arrived *inside the fix to a review finding*,
in the devloop about controls whose coverage is asserted rather than
demonstrated, written by the author of the rule an hour after writing it. It also
passed all seven layers and went green against the live cluster, as it always
would have; the 2585-series evidence validated the other two tests in that file
and was never evidence for this one.

Fixed three ways, the third being the structural one: parse the JSON; a distinct
`ServiceJobsMissing` error so a failed scan can never read as a clean one; and
**move the parse into the pure kernel where it gets FIRE fixtures**. The kernel
went from 9 fixtures to 14. The design was pure-kernel-plus-unit-fixtures
everywhere, and the single place it departed from that is the single place a dead
control shipped — because it sat in the cluster-gated file `cargo test` cannot
reach. Nine fixtures passing while that one had zero was the tell, unread.

**A stale token introduced into the hunk fixing stale tokens.** The §6.1
correction pass fixed four phantom-token references and introduced a fifth
(`cargo-build-workspace`) four rows below one of the corrections. Both
@operations and @infrastructure caught it independently. A third instance
(`cargo-check-passed` in §3's enum table, and a fourth in the §3 worked example)
was outside the assigned hunk and fixed anyway — same owner, same mechanism.

---

## Lessons Learned

**A correct number can be a worse failure than a wrong one.** I argued the
release-gate lane on measured wall-clock (~2s warm, fits the 90s budget) and was
overruled: ADR-0033 §4 asks *fast tier or outside it*, and compiler cost is
outside regardless of what the warm number says. The measurement was accurate and
answered the wrong question — and being accurate is exactly what stopped me
looking further. @infrastructure escalated rather than accepting it.

**Prose written in the same sitting as a correction does not inherit the
correction's scrutiny.** The `cargo-build-workspace` defect landed rows away from
the tokens it was fixing. A hand-maintained mirror of emitted tokens decays faster
than anyone expects — including while it is actively being repaired.

**"Documented dead" is still dead.** @security declined my offer to keep the
unreachable needle checks with a comment explaining they were unreachable. The
right protection against a future change is a note to the *editor* who might
reintroduce the hazard, not a runtime check that cannot fire.

**A single reviewer instruction covering two subjects can be right for one and
fatal for the other.** @security's "confirm it inspected MH and MC media metrics"
was correct for MH and would have re-armed the exact tripwire @observability had
just removed for MC. I had already written it verbatim. What caught it was that
@observability had recorded a *reason* ("MC's metrics are lazily created because
`init_metrics_recorder()` only sets buckets") rather than a preference — a
preference gives the next reader nothing to test a conflicting instruction
against.

**Every reviewer-requested fix is new code, and inherits none of the original's
scrutiny.** The dead relabel control was not in the plan, was not reviewed at
Gate 1, and went in under the momentum of clearing findings. It then passed seven
layers and a live cluster run. What caught it was @observability checking the fix
rather than accepting it — the same move that caught the four other defects in
this loop. A fix is a change, and a change is unverified until it is run against
something that would fail.

**Refusal is a load-bearing move in review.** The two best outcomes here came
from declining reviewer asks: the unreachable assertions, and the literal-span
parser contract. Both reviewers took the correction cleanly and said so. The
failure mode this avoids is a reviewer defending an ask *because it was theirs* —
and, symmetrically, an implementer accepting one because it came from a reviewer.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `7d2661c54b5f88ad0607099043665080b520245a`
2. Review all changes: `git diff 7d2661c5..HEAD`
3. Soft reset (preserves changes): `git reset --soft 7d2661c5`
4. Hard reset (clean revert): `git reset --hard 7d2661c5`
