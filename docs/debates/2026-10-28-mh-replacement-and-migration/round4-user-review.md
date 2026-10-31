# Round 4: user review of ADR-0039 (Proposed)

The user reviewed the Proposed ADR. Items marked RULING are settled; do not reopen them. Items
marked OPEN are for this round.

## Rulings

R4-1. **Heartbeat granularity: one heartbeat per (meeting, handler).** This reverses the per-(MH, MC)
pair choice. The pair choice needed MH to aggregate across meetings, MC's gRPC handler to fan
entries out to meeting actors and fan verdicts back in, and per-pair config that had no per-pair
handshake (`RegisterMeetingRequest.heartbeat_interval_ms` was per meeting, which was inconsistent).
Per meeting, a heartbeat targets one MC meeting actor and gets one verdict from that actor's own
state, and the interval config per meeting becomes consistent. The cost (about 500 small RPC/s for
1,000 meetings on an MH at a 2 s interval, over one HTTP/2 channel) is accepted. Per-pair batching
may return later as a transport optimization only.
  - Consequence: the §1 "Size" bullet goes away (the shared decode-limit constant, the MH boot check
    and the chunking preconditions). A per-meeting message is bounded by the per-meeting caps MC
    already enforces.
  - "The actor" in §2 must name which actor: the MC meeting actor.

R4-2. **Apply refusal keeps the previous generation live, as a stated invariant.** MH already applies
all-or-none (`crates/mh-service/src/session/mod.rs`, no snapshot swap on refusal), but the ADR doesn't
say so. State it: on any refusal the previously applied generation keeps forwarding, and the reported
`applied_generation` stays at that generation.

R4-3. **MC mints the `media_dial` token itself, with no AC call.** As written, §5 needs an MC→AC call
per participant per handler, at join, on every migration and on every redial. The direction is:
  - MC gives MH the verification material for a meeting in `RegisterMeeting` (for example a
    per-meeting key, or MC's public key). MH already takes that meeting's whole routing policy from
    MC, so this adds no new trust.
  - MC signs `{participant, meeting, handler(aud), exp}` locally and puts a token in every
    `MediaHandlerSet`. MC pushes a fresh set before the TTL runs out and on every handler change.
  - **TTL on the order of several minutes** (user-accepted): the token only admits a client to a
    handler that relays ciphertext.
  - `MediaDialTokenRequest` and the new AC endpoint/scope are expected to go away.
  - The cross-meeting KEK guard becomes true by construction: MC join and GC trust only AC-signed
    tokens.
  - A client that has left the meeting but still holds a valid token needs no new mechanism. MH holds
    the connection pending, MC answers sender_id 0, and MH closes with `DECLINED`. State that in one
    sentence.

R4-4. **Fencing becomes "proper cleanup on MC and client"; don't over-engineer it.** Open §6 with the
invariant: once MC removes a handler from a meeting, the handler stops serving that meeting and MC
forgets it, including when the handler is a zombie (alive, but declared dead because MC's path to it
broke). Then give each mechanism next to the failure it prevents:
  - MC: drop the handler from the set and record it as removed (one-way); release its bindings; apply
    the binding gate (ignore any later `Connected` from it); send `EndMeeting(old, cause)` best-effort.
  - Client: the set without that handler leads to closing the transport (a directed removal, not a
    fault) and clearing the reconnecting state (already in §4).
  - MH: the heartbeat verdict is just `OWNED | RELEASED`. RELEASED means "not in my set", and MH ends
    the meeting. That is the retry for an `EndMeeting` that was lost, not a separate fencing
    mechanism.
  - Expected cuts: the UNKNOWN verdict (it existed only for MC restart, and MC failure is out of
    scope), the ended-meeting tombstone tuning, and any machinery that per-meeting heartbeats make
    unnecessary.
  - Keep the remove-only connected-set reconcile (settled in the brief: it closes the
    lost-`Disconnected` leak). Simplify its race rule where per-meeting ordering allows.

R4-5. **GC selecting a bad MH for future meetings: postponed, but count the reports now.**
  - A dead MH is already covered, slowly: load-report staleness, plus MC adding a handler that fails
    `RegisterMeeting` to its refused/exclude list.
  - Gray failure (GC can reach the MH, but MCs or clients can't) is a recorded known gap with a TODO
    entry. The later policy would quarantine a handler on reports from several independent MCs within
    a window, so one MC's broken path cannot take a healthy handler out.
  - **In this story**: GC counts the `replaced_handler_id` and `trigger` reports carried on
    `AcquireMediaHandler` (evidence only, no selection effect). There must be no handler_id label
    (ADR-0036 §11 cardinality); observability proposes the shape.

R4-6. **`mh.sh` must be callable by automated tests**, not only by humans.
  - Host and devloop container already work (the container gets a cluster-admin kubeconfig).
  - Define the test-facing contract: stable `list --json`, exit codes and `REASON=` lines, idempotence,
    behaviour when parallel runs share a cluster, and an env-test fixture that drives spin-up and drain.
  - CI runs no Kind or Layer 7 today. Record that as a separate, existing gap; it is not in scope here.

## OPEN for this round

O-1 (security, MC, MH, protocol): the R4-3 key model.
  - Per-meeting symmetric key vs MC asymmetric key.
  - Where the key lives and how it rotates.
  - What MH verifies.
  - The exact TTL default and refresh trigger.
  - Whether security accepts MC as the media_dial issuer.

O-2 (protocol, MC, MH): the per-meeting heartbeat message shape after R4-1 and R4-4, and where the
interval config is carried.

O-3 (observability, GC): the shape of the R4-5 counter, plus any changes to the telemetry contract
caused by R4-1 through R4-4.

O-4 (infrastructure, test): the R4-6 test-facing contract for `mh.sh`.

O-5 (everyone): anything R4-1 through R4-6 breaks in your area that this list doesn't name.

## Lead rulings after round-4 proposals (2026-10-31)

L4-1. **O-1 key model: Ed25519 keypair per meeting** (security, MH and protocol agree; MC is indifferent).
  - Only the public half travels, on `RegisterMeetingRequest.media_dial_public_key` (32 bytes). It is never persisted, logged or Debug-printed, and dies with the meeting. No rotation.
  - The token is a compact JWS, alg EdDSA, with claims {token_type=media_dial, sub, meeting_id, aud=handler_id, iat, exp}.
  - MH verifies at accept, as specified by security and MH; WRONG_HANDLER is reported only after full validation.
  - The no-key-material guard gets a one-entry exact (message, field) allow-list. Security owns its membership, infrastructure owns the construct. Never string-encode the key.
  - MC's key is never in AC's JWKS.
L4-2. **Register before token, at join too.** A handler's token appears in a set only after RegisterMeeting on that handler is acked. MH's pending-before-registration path is deleted (PendingConnection, meeting_notifiers, await_meeting_registration, MH_REGISTER_MEETING_TIMEOUT_SECONDS, mh_register_meeting_timeouts_total).
L4-3. **Refresh: TTL/3**, jittered, derived from `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` (default 300, validated at startup to [60, 900]). handler_set_version advances only on membership change. Every set carries a token for every member. Client's FAILED-report fast re-push is accepted (credential re-push to the sender only, no routing effect).
L4-4. **Token reuse**: a token is valid for any number of dials within its TTL; MH keeps no jti cache. The client retains the latest token per in-set ref under security's scoped R-23 amendment.
L4-5. **MH exp skew becomes config**, small (~30 s). It is hardcoded to 300 s today (`crates/mh-service/src/main.rs`).
L4-6. **RELEASED carries a ReleaseCause.** Removed → its removal trigger; no meeting actor → MEETING_ENDED; never in the set → UNSPECIFIED (counted).
L4-7. **The heartbeat carries the registered mc_id.** An MC that is not that MC answers an error, never RELEASED, and MH takes no action on an error. An MC restart with the same id answering RELEASED(MEETING_ENDED) is an incidental consequence of "RELEASED = not in my set", not an MC-failover mechanism (MC/ops wording).
L4-8. **MH caps concurrent pending connections per (meeting, sub)** at a small configured value.
L4-9. **O-3**: the existing gc_mh_replacement_requests_total{trigger,outcome,gc_view} plus the `gc.mh.replacement_report` INFO log event (ids and endpoint). No in-memory window. Caveat: controller_id is self-asserted, so a quorum policy needs authenticated MC identity first.
L4-10. **O-4**: infrastructure and test each drafted an mh.sh contract. They differ on exit codes, the result line and the cluster Lease. They must merge into one contract (infrastructure owns the script, test owns the fixture) and report one text.
