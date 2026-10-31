# Debate: MH Replacement and Migration

**Date**: 2026-10-28
**Status**: Complete — ADR-0039 (Accepted)
**Participants**: meeting-controller, media-handler, global-controller, protocol, client, security, test, observability, operations, infrastructure (joined round 3)

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

How should Dark Tower detect a dead or impaired media handler and move a meeting's media to a replacement handler, so that one mechanism serves crash/node loss, deliberate transfer (maintenance, scale-down) and, later, client network change? This replaces ADR-0036 §8's same-handler re-assert recovery model and §9's frozen per-meeting handler set, and re-scopes story 4 of the ADR-0036 arc.

## Context

The user-confirmed framing is in `brief.md` (same directory). Design within it; do not re-open its settled points, out-of-scope ruling or constraints (in particular: backward compatibility is NOT a concern, and MC failure is entirely out of scope with no stubs).

Governing documents: `docs/decisions/adr-0036-media-flow.md` §8 (MC→MH control plane), §9 (multiple media handlers), §4 (keys; MH failover is not a KEK trigger), §6 (slot states), §11 (telemetry).

## Positions

### Initial Positions

| Specialist | Position | Satisfaction |
|------------|----------|--------------|
| meeting-controller | Per-pair heartbeat, per-meeting entries (applied policy_generation, connected set, draining); MC owns N (6 s), sends N/3; pure health fn; removal first, then GC replacement; RegisterMeeting(new) before client set update | 75 |
| media-handler | Per-pair heartbeat built in session actor; applied generation reused; response release list is the fence; MH never self-fences on MC silence; drain on SIGTERM (reverses §11 shed) | 70 |
| global-controller | MC->GC replacement RPC with whole-set exclude; GC view is a label, never a veto; no spares; no GC schema change; MH reports DRAINING to GC; unify GC staleness config | 78 |
| protocol | Unary HandlerHeartbeat on MediaCoordinationService, response = fence; remove-only connected-set reconcile; ReplaceMeetingHandler reusing MhAssignment; full-snapshot MediaHandlerSet replaces JoinResponse.media_servers (no version field, ordered stream) | TBD |
| client | Pipeline already migration-safe; full-snapshot versioned handler set with handler_ref; diff-applied MediaTransport; MH->client keepalive; post-join loss reports; client-local "reconnecting" overlay. BLOCKER: 900 s connect JWT not retained | 75 |
| security | TBD | TBD |
| test | Pure clock-injected assess(); no channel-event detection; MC owns heartbeat config; observable fence; SIGTERM vs SIGKILL drives planned vs crash; Kind zombie via node SIGSTOP/SIGCONT (feasibility open) | 80 |
| observability | Single `trigger` label; required metric = counter + ratio histogram + lead-time histogram at declaration; MH-side revoked_with_clients false-declaration count; page only on terminal replacement failure / migration timeout | 76 |
| operations | SIGTERM drain as sole planned initiator, degrades to crash path at grace - margin; MH reports Draining to GC; per-pair heartbeat on reused channel, no retries; bounded jittered migration concurrency; key handlers by handler_id (rollout URL reuse); §8 re-assert never shipped | 70 |

## Discussion

### Round 1

Initial positions converged quickly on: per-(MH, MC) heartbeat with per-meeting entries; reuse of applied `policy_generation`; MC-owned N with interval pushed to MH; heartbeat response as fence; removal independent of replacement; MC→GC replacement with whole-set exclusion and no GC veto; SIGTERM drain as the planned-transfer trigger; full-snapshot client handler-set message replacing `JoinResponse.media_servers`.

Open after round 1:
- **Mid-meeting connect credential** (client blocker): meeting JWT TTL 900 s, not retained by the SDK. Options: MC mints via AC; client re-join refresh via GC.
- **MH→client keepalive** so client loss reports arrive before QUIC idle timeout (~30 s); otherwise the required metric is near-zero by construction.
- **Handler-set version field**: client wants version + `handler_ref`; protocol argues ordered stream makes it unnecessary.
- **Handler identity during rollout**: surge pod shares the per-instance WebTransport URL; key by handler_id/handler_ref.
- **§8 per-stream transport-mode echo**: needs a home (observability).
- **Kind zombie injector feasibility** (test → operations).
- **Heartbeat message size bound** (operations).
- **`buf breaking` reserved-field requirement** vs the no-compat constraint (raised to user by Lead).

### Round 2

Converged on the `media_dial` credential (security S-1), `handler_ref` + `handler_set_version`, the
signal-named `ReleaseCause` enum, the OWNED/RELEASED/UNKNOWN fence (RELEASED only from MC's
removed/ended record), remove-only reconcile with the previous-heartbeat race rule, MH close codes,
no heartbeat chunking, and the survivor-target drain amendment. Crossed messages repeatedly reopened
settled points; the Lead froze the design (`rulings-freeze.txt`).

### Round 3 (user ruling)

The shared-URL question was escalated; the user rejected its premise: a new MH must never be alive on
the same URL as an old one, and MH must be provisionable on demand in Kind. `infrastructure` joined
and proposed hostPort slots + `mh.sh` (`infra-provisioning.txt`). Freeze item 2 collapsed to a GC
registration-time rule (`gc-register-rule.txt`); the exclude list reverted to one list. Lead ruling:
sequential address reuse after the previous holder exits is allowed; live overlap never.

### Round 4 (user review of the Proposed ADR)

The user reviewed ADR-0039 and ruled (`round4-user-review.md`, R4-1..R4-6):
- heartbeats per (meeting, handler), not per (MH, MC);
- an explicit all-or-none apply invariant;
- MC mints the dial token itself, with a TTL of several minutes;
- fencing simplified to proper cleanup on MC and client;
- the gray-failed MH as a postponed known gap, with reports counted now;
- `mh.sh` callable by automated tests.

Lead rulings L4-1..L4-10 settled the open items:
- a per-meeting Ed25519 key (security's choice over a symmetric key: MC→MH is plaintext);
- register before token;
- refresh at TTL/3, with `handler_set_version` as a membership epoch;
- the `mc_id` owner check on RELEASED;
- GC evidence as a counter plus a log event;
- the merged `mh.sh` contract (`mh-sh-contract.txt`).

One accuracy-review round followed.

## Consensus

Round 3: all ten ≥ 90. Re-reached after round 4 and its accuracy review, again all ten ≥ 90 with no
cross-cutting dissent (round-4 scores below).

| Specialist | Final |
|------------|-------|
| meeting-controller | 93 |
| media-handler | 95 |
| global-controller | 94 |
| protocol | 94 |
| client | 96 |
| infrastructure | 94 |
| security | 95 |
| test | 93 |
| observability | 96 |
| operations | 95 |

## Decision

`docs/decisions/adr-0039-mh-replacement-and-migration.md` (Accepted 2026-10-31).
Supporting files in this directory: `brief.md`, `rulings-freeze.txt`, `telemetry-contract.md`,
`test-gates.md`, `runbook-alert-changes.md`, `infra-provisioning.txt`, `gc-register-rule.txt`,
`round4-user-review.md`, `mh-sh-contract.txt`.
