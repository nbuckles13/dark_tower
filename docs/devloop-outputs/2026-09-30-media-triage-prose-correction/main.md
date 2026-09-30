# Devloop Output: Correct media triage prose falsified by story 2

**Date**: 2026-09-30
**Task**: Correct F13/F14 (client-dev-local), MH Scenario 17 loopback reading, MC Scenario 8 sender-id paragraph — story 2 R-29 (hear-each-other task 17)
**Specialist**: operations
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — tier=light (run-story manifest: docs-only runbook prose correction)
**Branch**: `feature/hear-each-other`
**Duration**: ~95m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `dfdc5c9107e3c87c8fe9829f83972b12dd103e80` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `operations` |
| Tier | `light — run-story manifest tier=light (docs-only runbook prose correction)` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — diff is runbook prose; no scripts/guards/semantic/checks.md surface` |

---

## Task Overview

### Objective
Story 2 (R-3 loopback removal, R-16 KEK rotate-and-reissue) falsified media triage prose in three runbooks. Correct it against shipped behaviour, citing sources instead of restating values, using each runbook's recorded-correction convention, keeping every `runbook_url` anchor resolving.

### Scope
- **Service(s)**: none (docs only)
- **Schema**: No
- **Cross-cutting**: Yes (client, MH, MC runbooks)

### Debate Decision
NOT NEEDED - prose correction to shipped behaviour.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/client-dev-local.md` (F12 note, F13 rewrite, F14 note, §4.5 rung-1/fork/rung-6 rows, §4.3 + §7 stale "no media plane", changelog) | Mine | — |
| `docs/runbooks/mh-incident-response.md` (Scenario 17 loopback subsection + `sent`-flat fork row + `no_subscriber` bullet + resolution) | Mine | — |
| `docs/runbooks/mc-incident-response.md` (Scenario 8 root cause 11 rewrite; Scenario 17 "task 18 expands" note untouched) | Mine | — |
| `docs/runbooks/mc-incident-response.md` (Scenario 17: two restated "65,536 admissions" → citation of `sender_id.rs`; no other Scenario 17 change — task 18 expands it) | Mine | — |
| `docs/user-stories/2026-09-21-hear-each-other.md` (dated correction appended to the "full rewrite stays task 18's" line) | Not mine, Minor-judgment | team-lead |
| `docs/runbooks/mh-deployment.md` (egress-drop PromQL comment: sustained `no_subscriber` is a fault — review DRY F2) | Mine | — |
| `docs/runbooks/mc-deployment.md` (restated "65,536 admissions" → citation of `sender_id.rs` — review DRY F3) | Mine | — |
| `docs/observability/alerts.md` (`MHMediaEgressQueueOverflowRate` step 2: cite `mh-service.md` for the `no_subscriber` reading — review DRY F1 / observability F1) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` (solo bullet + `no_subscriber` row: add "or a client ignoring its directive" — review security F-2) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/client.md` (one phrase: "the loopback pair" → the sent/received pair) | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (`MCKekEpochResetOnSenderIdExhaustion` description: drop the restated "65,536-id" namespace size — the allocator is `NonZeroU16`, 65,535 ids — and cite `sender_id.rs`; `runbook_url` unchanged) | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mh-alerts.yaml` (`MHMediaEgressQueueOverflowRate` description: "100% of egress in an unsubscribed meeting" → sustained-means-disagreement; `runbook_url` unchanged) | Not mine, Minor-judgment | observability |

---

## Planning

### Gate 1 — SKIPPED (tier=light; reason: run-story manifest tier=light — docs-only runbook prose correction)

(Implementer inline plan follows, written before implementation.)

### Verified premises (against code, not the task text)

- **Solo sends NOTHING.** `crates/mc-service/src/media_signaling/directive.rs:build_send_directive()` returns `DirectiveOutcome::EmittedEmptyTargets` (a success) for a publisher no egress plan names; `packages/sdk-core/src/media/pipeline/egress.ts:submit()` returns before building when no lane is active. So solo = `sent` flat, `received` flat, MH ingress and every drop reason flat. `no_subscriber` is healthy only as the in-flight tail after a sender's last holder leaves; sustained = MC directive vs MH policy disagreement. Matches `docs/observability/metrics/mh-service.md` (`mh_media_frames_dropped_total` solo bullet + reason table) and `infra/grafana/dashboards/mh-media.json`.
- **Receive is not a round trip any more.** In N-party `received` depends on OTHER senders, not on this client's `sent`. The client-side discriminator between "nobody is directed at me" and "a source MC says is active is not arriving" is `dt_client_media_receive_source_deficit_total` (counts ACTIVE assignments with no decoded frame; `docs/observability/metrics/client.md`) plus the slot row's `data-slot-state` wire token (`packages/web-app/src/lib/slotState.ts`).
- **Sender-id exhaustion rotates and reissues.** `crates/mc-service/src/media_admission/epoch.rs:AdmissionEpoch::admit()` + `crates/mc-service/src/actors/meeting.rs` (WARN "sender_id namespace exhausted; KEK epoch reset and reissue", `mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}`). Residual refusal arms (`AdmitFailed::Rotation`, `AdmitFailed::NoAllocatableId`) surface as `McError::Internal` → `error_type="internal"` with ERROR "sender_id namespace exhausted and …; refusing admission". `sender_id_space_exhausted` is absent from `JOIN_FAILURE_ERROR_TYPES` (`crates/mc-service/src/observability/metrics.rs`). `MCSenderIdSpaceExhausted` is retired; replacement info rule `MCKekEpochResetOnSenderIdExhaustion` in `infra/docker/prometheus/rules/mc-alerts.yaml` (runbook_url → MC Scenario 17).

### Changes

1. `client-dev-local.md`
   - **F13**: rewrite on the corrected premise. Symptom stays "`sent` rising, `received` flat" (anchor stays), but the discriminator now forks FIRST on whether anything is directed at you (`receive_source_deficit` + `data-slot-state`) — solo / everyone muted / `fewer_sources` is correct behaviour, not a fault; only "MC says active and nothing arrives" proceeds to the connection-up / generation-divergence rungs (kept). Recorded correction: a reader who trusted "you should be hearing yourself" would hunt a reaped NAT binding or stale MH policy that does not exist.
   - **F14**: add the N-sender note (one sender works, another does not — unrepresentable in loopback) and point at F19 / MC Scenario 18.
   - **F12**: note (recorded correction) that `sent` flat with unmute recorded is ALSO the correct state of a participant nobody holds (solo included) — `EmittedEmptyTargets`; the discriminator "nothing downstream can be at fault" stays true, but "stuck mute" is not the only cause. Task text said F12 unaffected — that was under the pre-task-16 premise (solo `sent` rising); under the corrected premise solo lands on F12's discriminator exactly.
   - **§4.5**: rung-1 description, fork table rows (`sent` flat → F12 OR nobody holds you; `sent` rising/`received` flat → F13 as now forked), and rung-6 comment ("`no_subscriber` … routine") → short tail routine, sustained is a fault.
   - **§4.3 / §7**: "no media plane on this branch" is stale since story 1 — correct in place (sweep hit, same falsified-prose class; one paragraph each).
   - Changelog row.
2. `mh-incident-response.md` Scenario 17: rename "The loopback reading…" subsection (no inbound anchors — grepped) to the N-party reading, keep the two-cause fork for the "active and not arriving" case, add recorded-correction blockquote; fork table `sent` flat row gains the nobody-holds-you arm; `no_subscriber` bullet rewritten to cite `mh-service.md` (tail healthy, sustained = MC/MH disagreement → MC Scenario 15). Heading `### Scenario 17: Media Datagram Drop` untouched.
3. `mc-incident-response.md` Scenario 8 root cause 11: replace the interim correction box + stale body with the shipped behaviour (label never emitted; exhaustion → reset + reissue; residual refusal under `internal`; triage via `mc_meeting_kek_generated_total{trigger=...}` + `sender_id namespace` log stem; driven-vs-accidental split kept; Fix = do not end the meeting). Keeps a dated recorded-correction note naming what the old text would have made a reader do. No restated namespace size — cite `sender_id.rs` / `mc_meeting_sender_ids_issued_max`. Heading anchor `#scenario-8-join-failures` untouched.
4. Sweep hits outside the three runbooks (complete the invariant): `docs/observability/metrics/client.md` "the loopback pair"; `mh-alerts.yaml` `MHMediaEgressQueueOverflowRate` description "100% of egress in an unsubscribed meeting" (a solo meeting now sends nothing). Checked and left: `docs/observability/alerts.md` (already corrected at task 16), `mc-alerts.yaml` retired-rule comments (accurate), `mh-service.md`/`mh-media.json`/`mc-media.json`/`label-taxonomy.md`/`mc-service.md` (already corrected), `docs/TODO.md` closed `[x]` entry (historical), `crates/mh-test-utils/src/media_policy.rs` (code comment describing a test shape, not triage prose), `docs/runbooks/mh-deployment.md` §Rollout diagnostic tell (still true: a shed shows as connection failure).
5. Edited at Lead's request (was: reported to Lead): `docs/user-stories/2026-09-21-hear-each-other.md` line "Task 18's Scenario 8 correction … The full rewrite stays task 18's" — this task performs that rewrite (story doc is Lead-owned manifest file).

No GSA path touched; docs + one alert annotation string.

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | owner review of mh/mc-alerts.yaml annotation edits (expr/severity/for unchanged) |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | no TODO entries |
| Operations | RESOLVED-FIXED | 3+nit | 4 | 0 | |
| Semantic Guard | — | — | — | — | not spawned (prose-only diff, no checks.md surface) |

Iteration 1 findings, all fixed (none deferred):

| Reviewer | Finding | Resolution |
|---|---|---|
| security | F-1 residual rotation-failure arm `generation_exhausted` is permanent; "never end the meeting" wrong | MC S8 bullet split by `reason` (rng: transient; generation_exhausted: end meeting, cite `kek.rs`) |
| security | F-2 sustained `no_subscriber` also = client ignoring directive | Second cause + rung-2 discriminator in MH S17 (paragraph, bullet, resolution), client rung-6 comment, `mh-service.md` (both sites), `mh-alerts.yaml` |
| operations | OPS-1 `NoAllocatableId` permanent for new joiners, not near-unreachable | MC S8 bullet split by ERROR stem; cites `epoch.rs` + TODO ledger item |
| operations / test / code-reviewer / observability | F13 row 1 wildcard stops on real faults; missing `active`+flat row; MH S17 lossy restatement of fork | F13 table: explicit healthy tokens with roster condition, `fewer_sources`+unmuted peers → MC S18, `unspecified`/non-emitted → MC defect, `active`+flat → grace interval; MH S17 defers to F13 table |
| operations | F12 test is "some peer shows you `active`", not "peers present" | F12 reworded |
| observability | F3 "is this expression" | "fires when this expression is `> 0`" |
| dry / observability | alerts.md, mh-deployment.md stale `no_subscriber` copies; remaining "65,536" restatements in mc-alerts.yaml ×2, mc-deployment.md | Fixed; alerts.md cites `mh-service.md` |
| code-reviewer | F2 story doc L669 "65,536" | Dated inline correction appended. Requirement lines (R-15/R-16 text, recorded-decisions line, task-9 prompt) left as written: Lead-owned requirement/record text, and "65,536 admissions" is the first-epoch reset point (65,535 ids, reset on the next admission) so they are not wrong as stated for that case. |

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `dfdc5c9107e3c87c8fe9829f83972b12dd103e80`
2. Safe-revert unit: the whole commit (docs only).

---

## Devloop Verification Steps

Gate 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, final tree): exit 0, TOTAL_RESULT=N/A (worst child N/A). L1 OK, L2 OK, L3 OK (all guards incl. alert-rules-policy, cite-symbol-resolves, runbook anchors), L4 N/A, L5 OK, L6 N/A (no dep changes; buf breaking OK), L7 OK (env-tests + browser E2E, 869s).

Note: one earlier layer-fast run hit an ac-service sqlx `_sqlx_test_*` "database does not exist" failure caused by the Lead's layer-fast running concurrently with the implementer's against the shared test Postgres — not diff-caused; clean on isolated re-run.

## Final Summary

Corrected story-2-falsified media triage prose: client-dev-local F12/F13/F14 (+§4.3/§4.5/§7 stale media-plane text), MH Scenario 17 loopback reading (no_subscriber guidance kept, two sustained causes), MC Scenario 8 root cause 11 (R-16 rotate-and-reissue; residual refusals split rng/generation_exhausted/NoAllocatableId). Restated values (sender-id namespace size, no_subscriber reading) replaced with source citations across runbooks, alerts.md, mh-service.md, mc/mh-alerts.yaml annotations, mc/mh-deployment.md. Story doc gains two dated corrections (task 18 must not redo MC S8 rewrite; L669 namespace figure). No runbook_url anchors changed.
