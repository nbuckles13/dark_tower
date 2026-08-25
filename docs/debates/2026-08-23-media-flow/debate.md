# Debate: How should media flow between participants?

**Date**: 2026-08-23
**Status**: Complete — consensus reached 2026-08-23. ADR-0036 drafted (Proposed; one open user decision on key distribution, §4).
**Participants**: media-handler, meeting-controller, protocol, client, security, test, observability, operations

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

How should media flow between participants: transport selection, sender attribution,
receive-capability negotiation, and stream switching?

This is the first debate on the media path. It has never been debated. ADR-0028
(client architecture, Accepted) decided a substantial part of it as a side effect of
deciding the client, and `docs/ARCHITECTURE.md` asserts more that never went through
`/debate` — some of which contradicts the code. This debate ratifies what survives
scrutiny, overturns what does not, and settles what is genuinely open, so that the
implementing story is not also a design exercise.

## Context

The media path has never been debated. ADR-0028 (client architecture, Accepted)
decided a substantial part of it as a side effect of deciding the client, and
`docs/ARCHITECTURE.md` asserts more that never went through `/debate`. Some of
those assertions contradict the code. This debate ratifies what survives scrutiny,
overturns what doesn't, and settles the genuinely open items — so that the
implementing story is not also a design exercise.

### Already built and not in question

- **The frame-codec *mechanism*** — `crates/media-protocol/{frame,codec}.rs`:
  a hand-rolled compact binary header (not protobuf, which is variable-length and
  wasteful per-frame), encode/decode round-trip, two fuzz targets, shared by MH and
  clients. **Correction (team-lead error):** an earlier draft claimed this crate is
  "`Bytes`-based so the payload need never be copied". That is FALSE today —
  `codec.rs:118-125` does `vec![0u8; payload_len]` + `copy_to_slice`, allocating and
  copying the payload per frame on what becomes MH's ingress hot path. Zero-copy is
  aspirational, not current. Fixing it (`buf.slice(..)` / `copy_to_bytes`) belongs in
  the B7 rewrite, and B5's `Bytes`-refcount gate is what keeps it fixed.
  **The field set and total size are NOT settled — see B7.** What is not in
  question is that a codec of this shape lives here.
- **MH connection lifecycle** — accept, JWT validate, registration check,
  provisional accept with timeout, MC notify, disconnect cleanup.
  Env-tested in `crates/env-tests/tests/26_mh_quic.rs`.
- **SDK transport to MH** — `MediaTransport.connectAll()`, active/active,
  bounded deadline, per-MH status reporting.

### The gap, stated precisely

- MH has **no `accept_uni` loop and no `receive_datagram` loop** — no media ingress
  path exists, and no egress path either. The accepted bidi stream at
  `mh-service/src/webtransport/connection.rs:167` is the connect handshake plus a
  liveness probe; it is not the media path and is not the blocker.
- `session/mod.rs:49` `ConnectionEntry` holds id/participant/timestamp. **No routing
  table** — nothing in MH knows who publishes what or who subscribes to whom.
- MC's post-join dispatch handles only `MediaConnectionUpdate`; every media
  signaling message falls to `Some(_) => debug!("ignoring")`. **Zero handlers exist
  repo-wide** for `PublishStream` / `SubscribeToLayout` / `StreamAssignments`.
- `JoinResponse.user_id` is hardcoded `0`; `encryption_keys` is `None`;
  `existing_participants[].streams` is always empty
  (`mc-service/src/webtransport/connection.rs:995`).
- There is **no `MhServerMessage`** in the proto. `MhClientMessage` has one variant.
- Client has no `getUserMedia` anywhere. `sdk-core` has no `crypto/`, no `room/`,
  no encode/decode pipeline. `sdk-svelte` has stores only — no `VideoTile`,
  `AudioRenderer`, `VideoGrid`. `web-app` has **no in-meeting room view at all**.
  ADR-0028 specifies all of these.
- `proto/test-vectors/` — mandated by ADR-0028 — does not exist.
- `mc-service/Cargo.toml:63` declares `media-protocol` and never uses it (dead dep).
- `docs/PROJECT_STATUS.md` is badly stale (MH "SKELETON", GC/MC "Planned").
  **Do not treat it as ground truth**; fix it as part of whatever ships.

---

## Part A — Ratify or overturn (pre-argued; expect challenge, not re-derivation)

Specialists should attack these if they are wrong, and otherwise ratify them
briefly rather than re-deriving them. Time belongs in Part B.

**A1. Hybrid transport, selected per media type.**
Audio over QUIC datagrams; video over unidirectional streams, one stream per
frame, `RESET_STREAM` on deadline. Rationale: QUIC has no *cross-stream* HOL
blocking, so one-stream-per-frame bounds blocking to a single frame, and
`RESET_STREAM` converts mandatory reliability into deadline-bounded reliability.
Datagrams cap near path MTU (~1100B usable); a ~40KB keyframe is ~37 fragments
with no recovery — ~67% survival at 1% loss, ~45% at 2%. **Datagrams-only would
force FEC into story one.** Audio at ~145B/frame fits one datagram trivially and
keeps Opus in-band FEC / RED / DRED open for the later resilience story.
`wtransport 0.7.1` supports all of it (`send_datagram`/`receive_datagram`
`connection.rs:272-297`, `max_datagram_size` `:350`, `SendStream::reset`
`stream.rs:92`, `set_priority` `:70`).
This ratifies the *mapping* for v1's media types. **How that mapping is expressed
and reaches both ends is B8, and must not be hardcoded.**
*Contradicts* `ARCHITECTURE.md:253,276,283,289,293` (datagrams throughout) and
ADR-0028's "bidirectional datagrams", which is not a coherent construct. The
header's 4-byte `payload_len` is evidence for streams — redundant in a datagram.

**A2. SFrame KID is the sender attribution; `user_id` leaves the frame header.**

> ⚠️ **AMENDED 2026-08-23 after security's initial position — the claim below that KID
> is "unforgeable" is WRONG as originally written, and the error was the team-lead's.**
> SFrame is a *symmetric* AEAD. Under SFrame-over-MLS every member derives every
> sender's key from the shared group secret — that is precisely how anyone can decrypt
> anyone — so **possession of the key is authorship**. Participant B can encrypt a frame
> under A's KID with A's key and every receiver will accept it and label it "A".
> KID attribution authenticates the sender **against MH and network attackers, and
> against cross-epoch replay. It does NOT authenticate against a malicious member of
> the same epoch.** Insider sender-authentication needs per-frame or amortised
> hash-chain signatures and is deferred, with a named trigger: **the first feature
> where attribution has consequences beyond a rendered name label** — recording,
> transcript attribution, moderation, or any audit log of "who said this".
> The rest of A2 stands: the KID *is* AEAD-covered, *is* readable pre-decryption, and
> deleting the in-clear spoofable `user_id` is strictly better. Ratify the mechanism,
> reject the word "unforgeable".
KID is in the clear (selectable pre-decryption) and covered by the AEAD tag
(unforgeable). `stream_id` handles demux to a decoder slot; KID handles identity.
`user_id` is redundant with both and, being spoofable, is a field future code
would wrongly trust. **This constrains key distribution**: KID-as-attribution
requires **per-sender keys**, not a shared group key. SFrame-over-MLS gives this
naturally (KID = MLS leaf index + epoch), and the KID→participant map falls out
of key distribution for free.
*Consequence*: removing `user_id` opens the header. **See B7** — settle its final
shape in one pass rather than field by field.

**A3. Receive capability, not layout.**
Client declares slots; MC composes the experience. Every constraint is an **upper
bound** — so any subset is a valid fulfillment and unsatisfiable requests are
unrepresentable. Pins are a **per-slot optional parameter**, not a parallel list,
so over-pinning cannot be expressed. `LayoutType`, `rows`, `columns` are deleted.
Suggested naming: `ReceiveCapabilities` / `repeated ReceiveSlot` — parallels the
existing `ParticipantCapabilities`, and "subscribe"/"layout" both wrongly imply
the client specifies an outcome. `StreamAssignment.media_handler_url` survives.

**A4. Send targeting is server-directed and symmetric with receive.**
Clients send **where they are told**, including to multiple MHs simultaneously
(shared encoder, separate transport) and including **nowhere**. Resuming from
"nowhere" is a keyframe event. This puts the N in N× uplink under MC's control.

**A5. MC decides, MH executes — for both audio selection and video switching.**
MH implements selection; MC configures it. MH must evaluate **without a round
trip**, so policy is pushed ahead of time (`RegisterMeeting` in `internal.proto`
is the natural seam) and nothing in the hot path queries MC. Shape: a default
selector (top-N by energy, excluding self) plus a priority-ordered rule list —
which generalizes to interpretation channels and must-hear prompts.

**Corollary — MH is configured with behaviours, never with media types.**
MH must hold no media-type semantics at all. It is a generic prioritised
forwarder. It is never told "you are video, do video things"; it is told what a
stream *does*: "you need an independently-decodable frame to swap sources", "you
may discard queued frames when a superseding one arrives". Likewise MH does not
derive priority from type (no built-in `audio > video`) — **MC assigns each egress
stream a priority, or a priority group, explicitly.**
Why this matters beyond tidiness: MH already cannot see the media (E2EE), so
type-awareness would be knowledge it has no legitimate source for; and a new media
kind — content share, interpretation audio, a future non-media channel — then
costs zero MH changes, which is the concrete form of B9's "do not corner content".
Any proposal that switches on media type inside MH should be treated as a design
error and rewritten as a behaviour.

**Congestion priority must encode structural facts, not dynamic ones.**
MC has meeting semantics but is off the data path and slow; MH is fast and
semantically blind. A priority derived from *who is currently speaking* goes stale
on roughly the same timescale as the control round trip that would refresh it, and
MH cannot know it is stale — it will confidently enforce a snapshot. So priority
carries only facts that do not decay (this stream's assigned rank, its priority
group, its slot's role), and MC re-pushes only when *structure* changes — a slot
reassignment — never on meeting dynamics. Anything genuinely dynamic stays in MH's
autonomous selection, evaluated locally against live signal with no staleness
window. This keeps the two mechanisms from contradicting each other rather than
merely coordinating them.

**A6. Control messages route MH→MC→client for v1.**
MC already holds a persistent WebTransport channel to every client, and in v1 the
MH-observed signal set is nearly empty. **Named trigger to revisit**: the first
transport-observed signal needing low-latency delivery to a client (bandwidth
estimate, or the keyframe request when built). Congestion-withheld slot state is
the one v1 item that is genuinely MH-observed and will exercise this path.

---

## Part B — Genuinely open

**B1. Key distribution.** `ARCHITECTURE.md:851-857` says MLS with MC as Delivery
Service. The proto says `EncryptionKeys{public_key, key_id}` — a far simpler shape
— and it is populated `None`. A2 constrains this to per-sender keys. Is MLS in
scope for a first cut, is there a decided interim that does not strand the KID
model, and what is the rotation trigger set? (ARCHITECTURE claims rotation on every
join and leave.) **Security owns the recommendation.**

**Required input: `draft-ietf-moq-secure-objects-01`** (IETF MoQ WG, July 2026).
It is the closest thing to a standardized answer for exactly the problem A2 sets
up, and we should not invent our own crypto alongside it. What it specifies:
- Explicitly **SFrame-based** — the cryptographic computations are identical.
- A **Key ID carried as an authenticated immutable property**, identifying which
  keying material decrypts the object. This is A2, standardized.
- The AEAD **nonce is synthesized from object identifiers rather than transmitted**,
  saving per-frame bytes versus stock SFrame. We can instantiate the same trick
  over our own monotonic `sequence` — their Group/Object IDs are not required for
  it (see B7).
- **Key distribution is out of scope there too**, with MLS named as the expected
  mechanism. So it constrains the construction but does not answer B1's core
  question — that remains ours.
Position required: adopt the construction, adopt it with deviations (say which and
why), or reject it (say what we do instead and why inventing is justified).

**B2. Keyframe-gated switching — does v1 take the no-request simplification?**
A slot switching source A→B must land on a B keyframe or the client decodes
garbage. MH already holds every publisher's ingress, so *correctness* needs only
"switch at the next keyframe MH observes from B" — no request path, no round trip,
worst case one keyframe interval. The request path is purely a latency
optimization. Does v1 ship the simple version with a short keyframe cadence, and
what cadence? Note audio has no keyframe — switching is instant — which is the
second structural reason audio and video selection must not share a mechanism.

**B3. Multi-MH without cascade.** Media routes only within an MH for now
(**MH↔MH cascade is explicitly future and must not be precluded**). Visibility rule:
X sees Y iff X and Y share an MH. Open: (a) the tiebreak when a source is reachable
via several shared MHs — MC must not send the same media twice; (b) how "participant
exists but is unreachable from your MHs" is represented, given it is structurally
permanent unlike the transient slot states; (c) whether MC should simply co-locate
small meetings on one MH, since partial visibility is only coherent for meetings
large enough that it is already the experience.

**B4. Slot states.** These must be distinguishable, not collapsed: active; source
present but far-end muted; withheld by congestion control; fewer sources available
than slots requested; zero requested; and (per B3) source unreachable. The first
two plus zero are steady states the UI renders differently; the middle two are
transient and are what an operator needs in aggregate.

**B5. Where the performance gates land.** Sub-250ms join-to-media
(`ARCHITECTURE.md:24`) cannot be asserted on Kind-on-WSL2 — contention makes
wall-clock timing flaky, and ADR-0028 does not permit quarantining quality gates.
Proposed three tiers, to be confirmed or replaced:
- **Unit / algorithmic (gates)** — frames in = frames out, no payload copy
  (assert on `Bytes` refcount), no per-frame allocation, bounded queue depth.
  Anything deadline-shaped takes an **injected clock**, per the existing pattern
  in `common::jwt`.
- **Regression bench (gates)** — MH ingress→egress p99 via criterion (already a
  workspace dev-dep). Deltas are meaningful on a laptop; absolutes are not.
- **SLO (does not gate)** — end-to-end in a real environment, per ADR-0031 and
  ADR-0028's journey dashboards.
Also open: media SLIs must be **histograms and sampling from day one** — per-frame
counters at 30fps × publishers × subscribers are themselves a hot-path hazard that
works fine at two participants and fails exactly when it stops being reproducible
locally.

**B6. Two seams that are cheap now and expensive later.**
- A **transport trait seam** in MH, mirroring the client's `IWebTransport`.
  `webtransport/connection.rs` calls `wtransport` types directly today. This is
  what lets a deterministic loss/delay shim drive MH's forward path at realistic
  frame rates and fan-out **with zero syscalls** — making the B5 benchmark
  representative and giving the resilience story its hook.
- The **measurement seam**: SDK `first-media-received` (already named in ADR-0028)
  and MH per-frame forward histograms. Without it, "wait and see" means finding
  out late with no data.
Confirm both belong in the first story, or say what replaces them.

**B7. Frame header shape — settle it in one pass.**
A2 removes `user_id`. Rather than shaving one field, decide the final layout now:
which identifiers and values are genuinely needed, at what widths, and whether the
header is fixed-size at all. Inputs to the decision:
- `stream_id` (demux to a decoder slot) and the SFrame KID (identity) are both
  load-bearing per A2. `user_id` is not.
- `frame_type` must stay **in the clear** — MH cannot parse an SFrame-encrypted
  bitstream, so keyframe-ness has to be visible for B2's switching to work at all.
  This is a deliberate metadata leak and the standard E2EE-SFU bargain.
- **But `FrameType{Audio, VideoKey, VideoDelta}` conflates two things** and should
  probably be split. MH needs the *decode-dependency* bit ("is this frame
  independently decodable?"); it does not need, and per A5 must not have, the
  media type. The receiving client already knows a slot's media type from its own
  `ReceiveSlot` declaration — it does not learn it from the frame.
  Collapsing to an `independently_decodable` marker makes A5's swap rule uniform
  and configuration-free: *swap source only at an independently-decodable frame*
  yields instant swapping for audio (every Opus frame qualifies) and
  keyframe-gated swapping for video, with one rule and no type knowledge.
  Note the congestion behaviour does **not** collapse the same way — a video
  keyframe supersedes what is queued behind it, whereas a newer audio frame does
  not supersede an older one (that would be an audible gap). So supersede-on-
  independent-frame stays a per-stream behaviour MC assigns, per A5.
  Confirm or refute this decomposition; it is an inference, not a ratified item.
- `payload_len` is necessary under streams and redundant under datagrams
  (self-delimiting). Does the header vary by transport, or carry the cost?
- Do `timestamp` and `sequence` both need 8 bytes, and are both needed at all
  given SFrame carries its own counter?
- `FrameFlags::discardable` currently has no consumer. Keep, or drop until SVC?
- 6 bytes are currently reserved. What is that reserve actually for?
**Constraint 1 — transport-agnostic.** The codec must not know whether it is being
framed into a datagram or a stream. That property is what keeps A1 reversible, and
it is where the real anti-cornering lives.

**Constraint 2 — split the header into publisher-immutable and relay-mutable
fields.** This is the one idea worth taking from `moq-secure-objects` (see B1),
and it is structural, not cosmetic. That draft separates **immutable properties**
(publisher-set, authenticated as AEAD AAD) from **mutable properties** (relay-
touchable, not authenticated). Our model differs from MoQ's in a way that makes
this sharper: a MoQ relay forwards object metadata intact, whereas **MH rewrites
`stream_id` per subscriber** — it is the subscriber-chosen slot label, mutable by
design.
Therefore **`stream_id` cannot be in the AAD**, while `sequence`, `timestamp`, and
`frame_type` are publisher-set and can be. Get this split wrong in either
direction and you either cannot route (authenticating a field MH must rewrite) or
cannot authenticate (leaving a publisher-set field forgeable). The header layout
should make the two groups visibly distinct so the boundary is hard to violate
later.

**Explicitly NOT adopting: MoQ Group ID / Object ID.** Evaluated and rejected as
ceremony for our use case — do not relitigate without new information.
Group-boundary switching already falls out of the in-clear decode-dependency
marker (MH detects the boundary; it does not need to name it); intra-group
ordering is already `sequence`;
and B1's nonce synthesis works over `sequence` without them. They earn their keep
for caching, seek, and fetch-past-objects — DVR and broadcast behaviours that are
explicit non-goals here. Take the construction, not the identifiers.

Note this invalidates the existing fuzz corpus and any test vectors, and
`proto/test-vectors/` (ADR-0028) still needs creating. No on-wire clients exist
outside this repo — this is the cheapest it will ever be.

**B8. How is transport selection expressed, and keyed on what?**
A1 ratifies *audio→datagram, video→stream* for v1. It does not say how that is
encoded. Hardcoding it in both the client and MH violates "config over hardcoding"
and makes any later change a coordinated deploy — which would defeat the point of
A1, whose whole justification is keeping the resilience story's options open.

**Preferred framing, per A5's behaviours-not-types corollary: transport mode is
just another per-stream behaviour MC assigns**, not a lookup keyed on media type.
That dissolves the "keyed on what?" question rather than answering it — nothing
maps type→transport anywhere, MC simply tells each stream how it is carried. It
also makes the later experiment cheap: moving video to datagrams-plus-FEC for one
meeting becomes an assignment change, not a deploy.
Remaining open:
- **Where does it live?** Proposal, symmetric with A4: MC directs not only *where*
  a client sends but *how*. The client is told its transport mode alongside its
  send targets; MH is told via `RegisterMeeting` (already the A5 seam).
- **Single source of truth.** Both ends must agree or media silently fails to
  arrive. If MC tells a client "datagrams" while MH expects streams, nothing
  works. One source must reach both — this is exactly the drift hazard the
  project's conventions call out.
- **If the behaviour framing is rejected** and a type-keyed lookup is kept anyway,
  it must not key on the header's `FrameType` — that only distinguishes
  Audio/VideoKey/VideoDelta and cannot tell camera from screen content. "Video" is
  not one thing: content behaves differently enough from camera — lower framerate,
  higher resolution, bursty keyframes on static screens, and "everyone sees it"
  rather than top-N selection — that it may well not want the same transport.
  Say why the behaviour framing was rejected.

**B9. Media session types: main only for v1, without cornering content.**
Limit v1 to main audio and main camera. `VIDEO_SCREEN` already exists in
`StreamType`, so the contract anticipates content; the requirement is that
nothing in v1 *assumes* it will never arrive. Specifically: `ReceiveSlot` (A3)
must carry a media-type dimension — a camera slot and a content slot are not
interchangeable — and the A5 selection policy must not assume every video source
competes in the same top-N pool. Confirm the slot model survives content being
added later without redesign, or say what has to change now.

---

## Required outputs per specialist

1. **Position on each of A1–A6** — ratify briefly, or attack with a concrete
   failure the proposal does not survive. Do not re-derive a ratified item.
2. **Recommendation on the B items in your domain**, with the trade-off named.
3. **The one thing most likely to be wrong** in the combined design, and the
   cheapest experiment that would find out.
4. **Failure mode at 6-month review** under the design as it stands.

## Explicit non-goals

Out of scope. Raising them is scope creep, not thoroughness:

- Simulcast, SVC, quality adaptation. (`SimulcastLayer`, `StreamQualityUpdate`
  exist in the proto; nothing implements them.)
- FEC, retransmission, RED/DRED, jitter-buffer design — the resilience story.
  A1 exists to keep these open, not to decide them.
- MH↔MH cascade — future, but must not be precluded.
- Recording, transcription, active-speaker UI.
- Screen share / content channel as a *shipped feature* — but see B9: the slot
  and selection models must not preclude it.
- Codec negotiation beyond naming a v1 default.
- Rewriting `PROJECT_STATUS.md` beyond correcting what is false.

## Definition of done

An ADR that lets the implementing story be written **without any further design
decisions** — every wire message shape named, every seam located, every deferred
item explicitly deferred with its trigger. If a specialist cannot state a
requirement without inventing a mechanism, that mechanism is missing from this
debate and should be flagged rather than invented.


---

## Positions

### Final (consensus — all participants ≥90)

| Specialist | Final | Position |
|---|---|---|
| observability | 97 | Zero-alloc-per-frame invariant, ≤500-series cardinality budget, the coverage-demonstrated line; disproved a guard it had itself advocated |
| media-handler | 97 | `Bytes` end-to-end fan-out; behaviours-not-types survived contact with the seam; conceded streaming when both its premises reversed |
| test | 96 | Three-tier gates, latency observed-never-gated, five control-plane gates, both B6 seams as story-one |
| meeting-controller | 96 | Unary seam, multi-MH as a clusterless pure function, six distinguishable slot states |
| client | 96 | 11-byte header, Worker receive pipeline, three-branch sizing; browser APIs verified rather than asserted |
| protocol | 95 | Header 42→11 bytes with one AAD boundary; seam arbitrated; identifier model locked |
| operations | 94 | Six numbered requirements; found the MH-restart blackhole, the quinn defaults, and the dead alert selectors |
| security | **90 under branch A or B; 62 under C** | Overturned A2's "unforgeable"; three-branch key-distribution analysis with sizing; telemetry conditions |

### How positions moved

The debate ran four substantive rounds. Notable reversals, all evidence-driven:

- **The MC→MH seam moved four times** — bidi → unary → hardened-streaming → unary. Each move added a real constraint; the streaming excursion is *how* the machinery streaming demands got priced, which is what made unary defensible rather than merely simpler. Ruled by the team lead when protocol and meeting-controller landed incompatible shapes in the same round.
- **security** withdrew its own join-debounce recommendation and reversed its round-one dismissal of the pairwise branch, which then became its second choice.
- **media-handler** retracted "defer multi-MH", then conceded the seam.
- **operations** withdrew the response-epoch (later reinstated for a case it had missed), corrected its own runbook claim, retracted four-fifths of R-op-7, and corrected its own escalation as a stale read.
- **observability** revised exemplars → ring buffer, and disproved the guard vocabulary it had spent two rounds advocating.
- **client** conceded the timestamp argument and corrected its own cost claim as an overreach.
- **team-lead** was wrong twice: A2's "unforgeable" claim, and "`Bytes`-based, never copied" about `media-protocol`. Both corrected in the ADR.

## Consensus

Reached 2026-08-23, all participants ≥90. One decision deliberately left to the user: the
key-distribution branch (ADR-0036 §4). Security's score is split rather than averaged because
branch C is not a design defect but a decision the user must make explicitly — ADR-0024 §5.7.

## Decision

`docs/decisions/adr-0036-media-flow.md` — **Proposed**, pending the user's branch selection.
