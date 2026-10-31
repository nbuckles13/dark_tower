# Debate Brief: MH Replacement and Migration

**Date**: 2026-10-28
**Status**: Pre-debate brief (user-confirmed framing, not yet debated)
**Replaces**: story 4 of the ADR-0036 arc, "Survive a handler restart"

This brief records the framing the user settled before the debate. The debate should
design within it, not re-open it.

## Problem

ADR-0036 §8 assumes the handler that failed comes back: "a re-assert remains the whole
recovery story". A handler lost to hardware, power or network never returns, and
deliberate moves (maintenance, scale-down after peak) aren't covered at all. A
restarted MH also registers under a new per-incarnation `handler_id` outside the
meeting's frozen set, so even same-pod recovery doesn't work today.

## Target model

1. **Liveness.** MH sends MC a periodic notification carrying the sequence number of
   the most recent MC command it has **applied** (not received; the apply is async, see
   §8 "Why the acknowledgement exists"). MC expects one every N seconds; MH sends every
   N/2 or N/3. N is a trade-off between detection speed, detection reliability and
   cost. This covers one direction only; it must not preclude MC→MH liveness later.
2. **Replacement.** When MC detects an MH as dead or impaired (missed heartbeats, not
   keeping up), MC obtains a new MH, e.g. by asking GC (preferred in discussion: GC
   already tracks MH health and load) or from a spare allocated at meeting assignment.
3. **Migration.** MC registers the meeting on the new MH, instructs every client to
   connect to it, receives participant-connected notifications and routes media as
   normal.

## Settled points

- **The periodic §8 re-assert goes away.** A restarted MH holds no meetings, so it
  sends no heartbeats and is replaced like any other dead handler. gRPC unary
  responses are affirmative acks; the heartbeat's applied sequence covers "keeping up".
- **Recovery is the join path.** Steps in (3) mirror meeting startup when a participant
  joins. Routes that couldn't complete before must complete once clients reach the new
  MH: if A and B were only on the failed MH, they get media between them on the new one.
  This must fall out of normal MC logic (§9 visibility rule), not a special case.
- **One mechanism, three uses**: crash/loss, intentional transfer, client network
  change. Story 4 should deliver a "poor man's" intentional transfer with a small media
  blip as evidence the design is right. Long-term the intentional transfer must be
  hitless; §9's required multi-handler send (make-before-break) is the likely path.
- **Heartbeat granularity is open**: one per meeting (meetings stay independent of each
  other) versus one per (MH, MC) pair (cheaper, but MH aggregates across meetings and
  MC fans out to many meetings from one connection). Either is acceptable; weigh both.
- **Heartbeat may carry each meeting's current connected set**, making lost
  Connected/Disconnected notifications self-correct within N. Today MH gives up after
  3 attempts (`crates/mh-service/src/grpc/mc_client.rs`), and MC never learns of it.
- **Dead-MH bindings are released by MC** after the handler is removed from the meeting
  (clients told to drop it, no longer in the handler set), never on suspicion alone.
  The ledger is keyed by `(handler, connection_id)` (`crates/mc-service/src/media_admission/epoch.rs`).
- **Retire empty-`connection_id` legacy mode** (`unreleasable` set, degraded-mode
  counter, "EMPTY MEANS LEGACY" contract text); its retirement condition is already met.
  Fold in as a story task.
- **Client connectivity is a second opinion, not a trigger, in this story.** MH health
  is one decision function over several signals so client connectivity can become a
  trigger later by policy change, not restructuring. One client losing an MH usually
  means that client's network; many clients at once means the handler or datacenter.
- **Required metric**: when MC declares an MH dead, how many of the clients connected to
  it had already reported losing it — as a count and as a percentage of the clients MC
  believed connected to that handler. This is the evidence for promoting client
  connectivity to a trigger later. No meeting or handler ids as labels (§11).
- **MC detection failure is a safe outcome.** MC can't tell "MH died" from "MC's path to
  MH broke"; a false declaration costs an unnecessary migration and a blip. State this
  explicitly.

## User ruling (2026-10-28, mid-debate): MH addressing and provisioning

- **A new MH must never be alive on the same client-facing URL as an old MH.** The current Kind
  flow (per-instance Deployment + Service; a RollingUpdate surge pod sharing the old pod's URL) is
  broken, not a constraint. Every MH incarnation gets its own client-facing address.
- **MH must be spin-up-able on demand in Kind**, not a fixed set of static instances.
- Existing implementation is not a limit on the design: change what needs to change, but no
  unnecessary changes. Kind deploy shortcuts are in scope to tease apart and make robust.
- Infra topology still stays out of application code: MH reads its advertised address from config
  injected at provision time; Rust never derives it from pod names or ordinals.

## Out of scope

- **MC failure, entirely.** No stubs, no partial MC-failover hooks that could pass for a
  working control. MC failover is its own design (active-passive MC, GC monitoring MC,
  etc.); MH monitoring MC is not assumed to be the answer.

## Constraints

- **Backward compatibility is NOT a concern.** No production usage, no old code running
  anywhere. Do not design rollout affordances, version negotiation, legacy modes or
  mixed-version ordering. Change any proto or internal contract the design needs. Each
  commit must still be self-contained — that is task design and devloop scope, not
  compatibility.

## Existing code that conflicts with the target model

- **Frozen handler set**: GC selects up to 2 active/active MHs once per meeting
  incarnation (`crates/gc-service/src/services/mh_selection.rs`); MC owns it and nothing
  replaces a handler mid-meeting.
- **Clients learn handlers only at join** (`JoinResponse.media_servers`); there is no
  server→client "connect here / drop that" message, and reconnect is unwired
  (`handle_reconnect` has no caller).
- **ADR-0036 §8** (re-assert model, cadence, re-assert triggers) and **§9** (frozen set)
  need amending, plus the story 4 line in the Planned Story Arc.

## docs/TODO.md entries this design absorbs or changes

- TODO §Media Path Obligations, "Connectivity repair after an MH or MC restart" — the MH
  half is this story.
- "MC cannot see a lost `NotifyParticipantDisconnected` on a LIVE MH" — closed by
  connected-set heartbeats.
- "MC's `policy_generation` resets to 1 … before the re-assert cadence lands",
  "`EndMeeting` has no fence against a late `RegisterMeeting`" — premised on the
  periodic re-assert; re-evaluate (likely moot if the cadence is dropped).
- "A meeting whose MC never sends `EndMeeting` is never reclaimed" — MC-failure side;
  re-evaluate against the out-of-scope ruling.
- "Missing alert: MC RegisterMeeting Coordination Failures" — its plan assumed a
  re-assert cadence; re-home onto heartbeat/migration signals.
