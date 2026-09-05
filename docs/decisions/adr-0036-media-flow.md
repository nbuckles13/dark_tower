# ADR-0036: Media Flow Between Participants

**Status**: Accepted.

**Date**: 2026-08-23 (revised 2026-08-27)

**Deciders**: media-handler, meeting-controller, protocol, client, security, test, observability,
operations. **auth-controller** reviewed §3 and §4 after the debate closed and corrected the
credential-binding mechanism (§4).

**Debate**: `docs/debates/2026-08-23-media-flow/debate.md`

---

## Context

The media path had never been debated. ADR-0028 decided part of it as a side effect of deciding the
client; `docs/ARCHITECTURE.md` asserts more that never went through `/debate`, some of it
contradicting the code.

**What exists**: the frame codec (`crates/media-protocol/`), MH's connection lifecycle with JWT auth
and registration handshake, and the SDK's `MediaTransport.connectAll()`.

**What does not**: MH has no `accept_uni` loop and no `receive_datagram` loop — neither media ingress
nor egress exists, and there is no routing table. MC's post-join dispatch handles one message and
ignores every media signalling message. `JoinResponse.user_id` is hardcoded `0` and
`encryption_keys` is `None`. There is no `getUserMedia` anywhere, no `crypto/` or `room/` in
`sdk-core`, no in-meeting view in `web-app`, and `proto/test-vectors/` — mandated by ADR-0028 — does
not exist.

**A note on the word "stream", which this document uses for three different things.** A **QUIC
stream** is a transport object — one per group of pictures (§1). A **media stream** is what a
publisher produces — one encoding of one source, the unit a send directive names (§5). A **slot** is
a subscriber's receive position, identified by the stream id in the relay region (§2, §6). Where the
distinction matters the text says which; where it says "per (connection, stream)" it means the
**media stream**, never the QUIC stream.

**How to read this document.** Sections 1–11 are prescriptive: they state what to build. A *Why*
subsection appears only where the reasoning constrains implementation — where someone who knew only
the rule would plausibly break it. Message and field names are specified in the **Appendix**, which
is the wire contract; prose here names concepts, not identifiers. Amendments this ADR forces on
Accepted documents are listed under Consequences.

---

## 1. Transport

Audio is carried in **QUIC datagrams**, one frame per datagram. Video is carried in **unidirectional
QUIC streams, one group of pictures per stream** — a stream opens at each independently-decodable
frame, carries that frame and the delta frames depending on it, and is finished or reset when the
next group begins.

**A stream's head-of-line blocking scope equals the decode-dependency scope.** That is the invariant,
and it is what fixes the granularity.

Transport mode is **not compiled in**. It is a per-stream behaviour MC assigns (§5, §8), so moving
video to datagrams-plus-FEC later is a configuration change.

MH declares its QUIC transport parameters explicitly, validated at startup:

| Setting | Why it cannot be left implicit |
|---|---|
| Maximum concurrent unidirectional streams | Must be **declared**, because a bound has to be declared to be assertable and because the default being adequate is not the same as the default being chosen. Under one-group-per-stream the pressure is mild — five video slots at a one-second keyframe cadence is roughly five opens per second and a similar number concurrent, against quinn's default of 100. The failure mode is a **slow subscriber**, whose unfinished groups accumulate; the application bound below must trip well before the transport ceiling. |
| Datagram send buffer size | quinn's default is 1 MiB ≈ **93 seconds of queued audio** (at the ~225-byte on-wire audio frame of §3 and §4) before the oldest is silently discarded. A realtime path must prefer loss to unbounded latency. Express and document this in **frames of audio**, not bytes. |
| Keepalive interval | QUIC's connection-level keepalive is what refreshes NAT bindings while no media flows — which happens whenever a participant is muted (§5). Without it a muted participant's path can be reaped by an intermediary and unmute is not instantaneous. |

MH currently builds its server configuration with no transport configuration at all
(`webtransport/server.rs:123-126`), so both defaults are live today. The application-level egress
queue bound must trip **before** the transport ceiling, so back-pressure is observable in our code
rather than inside quinn.

### Why

**Datagrams versus streams for video.** Datagrams cap near path MTU (~1100 B usable), so a ~40 KB
keyframe becomes ~37 fragments with no recovery — about 67% survival at 1% loss under
independent-loss arithmetic. Under **bursty** loss the mean is similar but the variance is far worse:
whole runs of fragments vanish, some keyframes are total losses, and users experience the variance as
*"sometimes video takes three seconds to appear."* Datagrams-only would have made forward error
correction a prerequisite rather than a later choice.

**Why a group and not a frame.** A stream per frame bounds blocking to one frame, which buys almost
nothing: if a delta frame is lost, the frames behind it depend on it and are undecodable regardless.
The isolation is real only at group boundaries — which is where a group already starts. Aligning the
stream with the group instead yields four things:

- **Churn drops by the keyframe interval.** One open per group rather than one per frame — roughly
  five per second across five slots instead of 150.
- **Retransmission becomes useful rather than wasted.** Within a group you *want* the lost packet
  back, because nothing behind it decodes without it. Under per-frame streams with reset-on-deadline
  a dropped frame is gone permanently and the decoder stalls to the next keyframe anyway — so
  per-frame discards data that per-group would have recovered.
- **Reset becomes a meaningful action**: *abandon this group and resume at the next keyframe*, which
  is the only recovery point a decoder has. Resetting a single frame's stream cascades into the rest
  of its group regardless.
- **The transport boundary, the dependency boundary, and the switch boundary become one boundary**
  (§7).

The cost is that stream-open rate now equals keyframe rate, so a synchronised keyframe burst is a
bandwidth spike **and** a stream-open spike at the same moment. The resume jitter §11 already
mandates for the bandwidth half covers the stream-open half too. It also couples two parameters that
were independent under per-frame streams: **a shorter keyframe cadence improves switch latency and
worsens stream churn** (§7).

**None of this isolates audio from video.** Both share one QUIC connection per client↔MH pair and
therefore one congestion window; datagrams receive packet-assembly priority in quinn but no reserved
share of it, and stream priority does not apply to datagrams at all. A video keyframe burst *can*
starve audio, and it manifests as silent sender-side datagram drop rather than as delay. The
guarantee is bounded head-of-line blocking. Nothing more.

---

## 2. Frame format

The frame header splits into a **publisher region** and a **relay region**. This split is the
document's organising idea: the publisher region is authenticated and immutable end-to-end; the relay
region is what MH rewrites per subscriber and is authenticated by nobody.

```
PUBLISHER REGION — covered by the signature (§3) and the AEAD associated data (§4)
  version                 protocol version
  flags                   independently-decodable; discardable; key-bearing;
                          all other bits: decode REJECTS if set
  payload length          delimits this frame within its group's stream
  stream sequence         end-to-end, per (sender, stream, generation)
  wrapped transmit key    present iff key-bearing is set; fixed size; the sender's
                          key for this frame's key id, wrapped under the meeting
                          KEK and bound to the key id (§4)
  extensions              optional, type-length-value, publisher-set (§7)

RELAY REGION — excluded from signature and associated data; MH rewrites per subscriber
  stream id               which of the subscriber's slots this frame fills
  hop sequence            per (connection, stream); counts what the transmitter sent

PAYLOAD                   SFrame object: key id and authentication tag in its own
                          clear header (the receiver must read the key id to
                          select a key before decrypting); presentation
                          timestamp inside the ciphertext
SIGNATURE                 Ed25519 over publisher region and payload (§3)
```

**The version is meeting-wide and MC-directed.** Because it sits in the publisher region it is
signed, so **MH cannot rewrite it** — a relay cannot translate between header versions without
breaking verification. Rather than weaken that, MC selects one version for the whole meeting from the
participants' declared capabilities and carries it in the send directive (§5). Every frame in a
meeting is then the same version and no translation ever arises. The signed field serves as a decode
guard and as proof the publisher used the version MC directed, which is what makes downgrade
detectable.

Two consequences. A mid-meeting joiner supporting only a lower version forces either a meeting-wide
downgrade or exclusion, and that is MC's policy call. And a version change lands on a
**transmit-key generation boundary**, because versions may differ in what the signature and
associated data cover — and a sender rotates generations freely (§4), so no coordination is needed.

**Two sequence numbers, because they answer different questions.**

The **stream sequence** is end-to-end and publisher-set. It does two jobs: it **is** the AEAD nonce
input (§4 synthesizes the nonce rather than transmitting it), and it provides end-to-end ordering and
loss detection. MH cannot alter it — rewriting it breaks decryption, so the immutability is
cryptographic rather than conventional.

Being the nonce input is a stronger constraint than ordering alone would impose: **a repeat under one
key does not merely expose those two frames, it leaks the authentication subkey and permits forgery.**
The rule is therefore uniqueness per key, and because the key changes each generation, uniqueness
*within* a generation suffices — a generation boundary **permits** a reset rather than requiring one.

**Do not reset it at generation boundaries; keep counting per (sender, stream).** Generations
advance on every video group (§4), so a resetting counter restarts constantly and every reset is a
discontinuity the receiver must special-case rather than read as loss — which damages the field's
second job. Continuing to count also removes a class of bug, since resetting at the wrong moment
relative to the key swap is exactly how a silent nonce repeat happens. At 32 bits and 50 fps,
exhaustion is years away, and rotate-before-wrap remains a format rule.

It is **counted per (sender, stream)**, not per sender — which requires the key id to identify
**(sender, stream, generation)** so that each stream has its own key and nonce uniqueness still
holds (§4). Counting per sender across streams would make the sequence sparse for any single
subscriber — gaps wherever another stream consumed numbers — and useless for loss detection.

Because it is a nonce input it **must be readable before decryption**, the same way an initialisation
vector must be; there is no variant of this design where it is inside the ciphertext. It is covered
by both the signature and the associated data, so tampering fails loudly rather than silently
reordering or replaying frames.

The **hop sequence** is set by whoever transmits — **it applies to the client's uplink as well as
MH's downlink**, so each side can detect loss on the hop it receives. It exists because **selection
is intentional gapping**: if a source sends frames 1-2-3-4 and MC has told MH to forward only 2 and
4, a subscriber measuring loss from the stream sequence would report two losses that never occurred.
The hop sequence counts **what the transmitter actually sent**, so a deliberately unforwarded frame
consumes no number and any gap the receiver observes is genuine transport loss. Get that backwards
and the metric measures selection policy instead of loss.

It is counted **per (connection, media stream)** rather than per connection, because the useful
question is *which slot degraded* — the input to congestion-withheld slot state (§6) — not merely
whether the link lost packets. It is emphatically **not** per QUIC stream: a video QUIC stream lasts
one group of pictures (§1), so a counter scoped to it would reset at every keyframe and detect
nothing.

It needs no authentication: it rides inside QUIC/TLS, and a transmitter lying about its own send
count only conceals drops it could already perform.

**Do not reimplement connection-level loss.** quinn reports lost packets, lost bytes, congestion
events and per-frame-type counts, and that is authoritative where an application counter would not
be. Note the asymmetry that decides where each signal is usable: **QUIC's loss statistics live at the
sender**, since loss is inferred from missing acknowledgements — a transmitter knows what it lost, a
receiver does not learn from QUIC what it failed to receive. That is precisely why receiver-side
detection needs application numbering at all. Browser transport statistics are thinner than quinn's
and have not been verified against this requirement.

**A wholly-lost stream is not detected by numbering, and does not need to be.** If every frame of a
stream is lost there is no sequence in which to find a gap — but the client knows what it was
promised: a slot assigned with nothing arriving is detectable by comparing assignments against
arrivals (§6). Coverage decomposes as partial loss → hop-sequence gaps; total absence →
assignment-versus-arrival; connection-level → QUIC's own statistics.

**Neither sequence can be inherited from QUIC.** Packet numbers are per-connection and
transport-internal; neither quinn nor wtransport exposes them. For streams it is moot, since QUIC
retransmits and delivers in order. For datagrams there is no application-visible numbering at all.

**The independently-decodable flag replaces a media-type field.** MH needs to know a frame can be
decoded without predecessors; it must not know whether that frame is audio or video (§7). The
subscriber learns media kind from its own slot declaration, never from the frame. This makes the
switching rule in §7 uniform and configuration-free.

**The key-bearing flag** announces a fixed-size wrapped transmit key following the stream sequence
(§4). A flag rather than a length: the field has one size, so presence is the only thing to signal,
and a variable-size header region is exactly what the "every byte decoded or rejected" rule forbids.
It sits in the publisher region because it is the publisher's statement about its own key, and
because a signed field cannot be added, removed, or replayed by the relay.

**The discardable flag** marks non-reference frames the sender believes can be dropped without
affecting others — real information for congestion response, since video encoders genuinely produce
them. It is publisher-declared and therefore untrusted; the bound in §7 that constrains declared
salience constrains this identically.

Under one-group-per-stream the drop decision happens **at write time**: MH re-frames as it forwards,
so it chooses whether to write a discardable frame into the subscriber's stream. It cannot drop one
after writing, since a stream is an ordered byte sequence — which is the reason the flag has to be
readable in the clear rather than inferred.

**The payload length field delimits frames within a group's stream.** Several frames share one video
stream (§1), so the transport boundary no longer marks a frame boundary and the length is load-bearing
rather than redundant. Audio, one frame per datagram, does not need it — the field is carried anyway
rather than made conditional, because a header whose layout varies by transport is not
transport-agnostic and the codec would have to know which it was decoding.

**It is a parsing trust boundary and must be treated as one.** A reader consuming frames from a
stream would otherwise pre-allocate from an attacker-controlled length. A wire-format **maximum
payload constant is enforced before any allocation, in both languages** — note this defect passes
fuzzing of the decode function while being wrong in the reader that calls it, so the fuzz target does
not cover it.

**Fragmentation remains out of scope**, and a length alone would not make the header ready for it:
that needs a fragment index, a count, and reassembly state. If it ever arrives it is a new header
version, and at that point the independently-decodable flag must be scoped to the *assembled* frame
rather than the fragment.

**No reserved bytes, and unknown flag bits are rejected.** The general rule: **every byte and every
bit is either decoded into a field the receiver inspects, or rejected if set — nothing is skipped.**
The version field is the extension path, and fail-closed is chosen deliberately over forward
compatibility. The current codec skips six reserved bytes without validating them, which is a
~180 B/s/stream covert channel out of a compromised MH that no test would fail.

**Decode is zero-copy** — a slice, not the allocate-and-copy the current codec performs.

**Cross-language test vectors gate both implementations** — vectors as single source of truth, an
executable drift guard, and a tamper-vector pair. Without them the TypeScript and Rust codecs can
disagree on the authenticated byte range, MH routes correctly because it never decrypts, and the
receiver's verification fails: black video, silent audio, both sides' unit tests green. The fuzz
corpus is regenerated for the new header.

Vectors are the **cheap, fast** gate, not the only one: end-to-end tests with real browser clients
validate the same agreement against a running system, and catch what vectors cannot — that both
implementations are wired into the paths they are supposed to serve. Vectors fail in seconds and
localise the defect; end-to-end tests prove the composition. Both, for different reasons.

---

## 3. Sender authentication

**Each frame carries an Ed25519 signature over the publisher region and payload.** Receivers verify
against the sender's identity public key, published on the roster and attested by AC. A frame that
fails verification is dropped.

This answers *"did this frame come from A"*. It does not by itself answer *"who is A"* — that is the
AC attestation on the identity key, and it is a separate concern.

**Two key lifetimes, deliberately not a pair.** The **signing key is long-lived** and persists across
reconnect; it answers who sent a frame. The **media key rotates** (§4); it answers who can read one.
Only the media key carries a counter hazard. These look symmetric and are not.

### Why

Without signatures, attribution rests on symmetric authenticated encryption: every member can unwrap
every other member's transmit key, and so can MC — that is precisely how anyone can decrypt anyone —
so **possession of the key is authorship**, and any participant can encrypt a frame under another's key that every
receiver accepts and labels with the wrong name. Guests carry a client-supplied display name, so the
insider bar is a meeting link.

Signatures also collapse two other problems:

- **Attribution is independent of key distribution.** Verification against a roster key would work
  identically under any §4 mechanism, so who a frame is attributed to does not depend on how media
  keys are established.
- **MC can no longer mis-attribute.** MC publishes the roster, so a roster-derived mapping would let
  a compromised MC rename the speaker. MC can already read media (§4); without signatures it could
  also author it as anyone. A forged roster entry does not help when the signature will not verify against a key the client
  validated independently.

### Cost, stated plainly

64 bytes per frame. On video at 30 fps and 5–40 KB frames this is under 1%. On audio it is not:
against an 80-byte Opus frame (32 kbps, 20 ms) the signature alone is **80%**, and with header and
authentication tag a frame goes from ~80 to ~174 bytes — a 32 kbps stream becomes roughly
**70 kbps**.

The absolute number remains small — five audio slots at 70 kbps is 350 kbps against a per-subscriber
video budget in the megabits — which is why per-frame signing on both is the recommendation. If the
ratio proves unacceptable, the mitigations in order of preference are **40 ms audio frames** (halves
the overhead, costs 20 ms latency) or **signing every fourth audio frame** (quarter cost, 80 ms
attribution granularity, and an attacker can inject up to that window under another identity).

Verification costs roughly 100 µs and is batch-verifiable at about 2× when a slot's frames are
verified together.

---

## 4. Encryption and key distribution

Media is encrypted per frame using the SFrame construction. The nonce is synthesized from the key id
and the stream sequence (§2) rather than transmitted, and the relay region is **excluded** from the
associated data because MH rewrites it.

**Each sender encrypts under its own transmit keys and carries them, wrapped, inside its own
frames. MC issues one key-encryption key (KEK) per meeting to every participant it admits.** If MC
lets a client in, the client gets the KEK; there is no other condition, no group protocol, and no
key material on the roster.

**This is a trust decision, recorded as the user's.** Under it, **MC — and therefore the operator —
can read media. MH, the network, and everything at rest cannot.** The debate's security position
was that a provider under which an operator service can reach media requires explicit risk
acceptance under ADR-0024 §5.7 rather than majority override; that acceptance is given here. The
grounds: every mechanism that excludes MC by construction makes membership a coordination problem
among clients — a global barrier under MLS, N² wrapping under pairwise keys, or a key server whose
own trust story needs sealed envelopes, org-issued certification, and issuer federation before it
excludes anyone — and each trades in-meeting quality, reliability, or simplicity for operator
exclusion. This system's goals are a joiner receiving media within 200 ms of join completion and
no join ever touching an existing flow. The precise claim is therefore: **media is encrypted between
clients; MH, transport, and storage cannot read it; MC can.** Nothing may describe this as
zero-trust or as end-to-end against the operator. Logs and metrics carry a **key-custody** label,
fixed at `operator` today, in place of any end-to-end boolean.

### Keys, and who holds what

| Key | Scheme | Lifetime | Held by | Purpose |
|---|---|---|---|---|
| Identity **signing** key | Ed25519 | one meeting; survives reconnect, not a fresh join | client; public half on the roster, thumbprint AC-attested | signs every frame (§3) — proves origin |
| Meeting **KEK** | AES-256, symmetric | one meeting; rotated by MC (below) | **generated at random by MC's per-meeting actor, held only in memory**; every current member | wraps transmit keys |
| **Transmit** key | AES-256-GCM | per (sender, stream, generation); sender-rotated | sender generates; carried wrapped in the sender's own frames | encrypts frames |

**Every member can unwrap every sender's transmit key.** That is inherent to a shared KEK, and it is
why §3's signatures are required for attribution: possession of a key is not authorship, and MC is
now also in the set that could forge a frame without them.

**The KEK is never derived and never persisted.** It is random, lives in the meeting actor, and dies
with the meeting. Compromise must be live: a database, a backup, or a log yields nothing, and no
master secret exists whose loss reaches backward across meetings. A derived-from-master design lacks
this property, which is why the KEK is generated rather than computed.

### How it works

1. **Before requesting the meeting token**, the client generates its identity signing keypair; AC
   attests the thumbprint in the token's `cnf` claim and issues the peer-presentable identity
   attestation (§3, and *Identity keys must be AC-attested* below).
2. The client's join request to MC carries the raw signing public key. MC checks it against the
   token's thumbprint and publishes it on the roster.
3. **MC returns the current KEK in the join response.** That is the entire key-distribution step for
   a joiner. It costs nothing beyond the join MC already performed, and it touches no existing
   member.
4. The client generates a transmit key per stream, wraps it under the KEK, and **carries the wrapped
   key in the publisher region of its own frames**, announced by the key-bearing flag (§2; cadence
   below). A receiver that already holds the
   key for that key id ignores the field; one that does not unwraps it and caches it.
5. A sender rotates a transmit key by advancing the generation and carrying the new wrapped key.
   **Rotation is O(1), involves nobody else, and needs no signalling** — which is why it can be
   aggressive.
6. MC rotates the KEK (below) by generating a new one and pushing it to every member over
   signalling. Senders wrap under the new KEK from then on. Receivers retain the previous KEK for a
   **bounded** window so frames in flight, and frames from senders that have not yet re-wrapped,
   still open.

**The KEK-source seam.** The client obtains the KEK through a seam, as §10's transport and
measurement seams; today its one implementation is the join response and the KEK-push
message. Nothing in the frame format or the wrap depends on where the KEK came from, so a key
server outside MC — should operator exclusion ever be required — changes the source and nothing
else. That is what "key delivery is pluggable" means here.

### In-band key carriage

The wrapped transmit key travels in the frame, not on the roster. This is the mechanism the
previous draft deferred as sender-driven rekeying in the manner of SRTP's Encrypted Key Transport;
with MC holding the KEK it becomes the simplest correct design rather than an optimisation, and it
is what removes every remaining round trip from joins, rotations, and speaker changes.

**Cadence.** The field costs roughly 50 bytes (32-byte key, 16-byte tag, KEK generation; the wrap
nonce is derived, not carried).

| Media | Carried on | Why |
|---|---|---|
| Video | the first **N** frames of every group, N small and configured (default 3) | a group is a QUIC stream and a switch lands on a group start (§7), so a subscriber's first frame from a new source is key-bearing by construction. Streams are reliable, so N=1 would suffice; N>1 is cheap insurance against a reader that starts one frame late |
| Audio | **every frame** | audio switching is instantaneous and autonomous at MH (§7), so a receiver's first frame from a newly selected speaker must be decryptable with no wait at all. Every-other-frame would save half the bytes for a 20 ms worst-case hole on every speaker change; every frame is the deterministic rule and the difference is small |

**Cost, stated plainly.** On audio this adds ~50 bytes to the ~174 of §3, so a 20 ms Opus stream
goes from ~70 kbps to roughly **90 kbps**; five audio slots is about 450 kbps per subscriber, still
small against video. The cost is bandwidth only — a receiver holding the key does no per-frame
unwrap. §3's mitigation ladder applies unchanged: 40 ms audio frames halve the signature and the key
overhead together.

**The field is signed and KEK-bound.** The wrap is AES-256-GCM under the KEK with the nonce derived
from the key id and the key id as associated data. **The derivation, stated so it has a normative
home:** the 12-byte GCM nonce is `0x00000000 || key_id` — four zero bytes then the 8-byte
big-endian key id, right-aligned. *This clarifies rather than decides.* It records what the story
contract already pins in two places and chooses nothing new; it is written here because a normative
derivation whose only homes are a story line and a task manifest prompt is one archival away from
having none, and this repository has already had one incident where a stale manifest prompt
contradicted the shipped contract. Zero-**prefix** rather than zero-suffix so the codebase carries
**one** padding rule: `SFrame`'s own nonce is `salt XOR BE12(counter)`, which right-aligns a
big-endian value identically. Any injective derivation would satisfy the uniqueness argument above
equally well — which is exactly why leaving it to two independent implementations was the wrong
shape. Pinned per row as `wrap_nonce_hex` in `proto/test-vectors/frame-v2.vectors.json`, which is
the file that wins if this prose and the vectors ever disagree. One transmit key therefore wraps to one
ciphertext — byte-identical from frame to frame within a generation — and nonce uniqueness under the
KEK reduces to key-id uniqueness, which generation monotonicity gives (below). It sits in the
**publisher region** (§2): covered by the signature, so MH can neither attach, strip, nor replay
it, and announced by a flag bit so the header stays fixed-layout. This forecloses one optimisation —
MH caching a sender's wrapped key and attaching it for new subscribers — and that is accepted: the
audio cadence below makes it unnecessary, and a relay that edits headers is the thing §2's split
exists to prevent.

**Receivers accept a wrapped key only for the key id of the frame carrying it.** Any member holds
the KEK and could wrap arbitrary material, but it can only place it in frames it signs — its own —
and a wrap for another sender's key id in those frames is ignored. Stated so the attack has no
surface.

**Frames that cannot be opened are dropped and counted, by reason.** Two reasons exist and they
have different remedies, so they are distinct label values on one client-side counter: **no KEK for
the carried KEK generation** — at join before the KEK arrives, or after a rotation before the push
lands; and **no roster entry for the sender** — unwrappable but not verifiable, since §3 needs the
signing key. Both are expected transiently at join and at rotation and are wrong when sustained; the
sustained case is the signal. Per §11 the counter carries a reason label and no participant or
meeting dimension.

**A non-key-bearing frame for an unknown key id is a protocol violation, not a third reason.** Under
the cadence above it cannot occur: audio carries the key on every frame, and a video subscriber
enters a group at its start (§7), whose first frame is the keyframe and is never discardable. The
only paths to it are defects — a sender omitting the flag, a reader starting mid-stream — so it is
counted in the decode-reject bucket alongside unknown flag bits, where firing means an invariant
broke rather than a state the system passes through.

**A wrap from a frame that fails verification is not cached.** After a missing roster entry lands,
the receiver waits for the next key-bearing frame — immediate for audio, up to one group for video.
That is the real cost of the no-roster case, and why the roster update must travel the same
signalling path as the KEK and land first.

### Key identifiers and stop/start

The key id encodes **(sender, stream, generation)**. Generation is **monotonic per sender across its
membership and never reset**, so a stream number reused after a stop/start lands under a new key id
and a new key. Stream sequences keep counting per (sender, stream) across stop/start, as §2
requires. Nothing needs to coordinate which stream numbers are live.

### Rotation

| Trigger | Rule |
|---|---|
| Participant leaves | **MC rotates the KEK, debounced to at most once per W** measured from the oldest un-rotated leave; departures inside one window coalesce. **A leaver is bounded by W and by nothing else**: holding the KEK, it unwraps every new transmit key as it is carried, so transmit-key rotation does not shorten this window. It receives no media from MH after leaving, so W bounds damage only where ciphertext was captured in transit. **W is configuration**, defaulting to the order of a minute. |
| Participant joins | **No action on any existing member.** The joiner receives the current KEK and unwraps each sender's key from that sender's next key-bearing frame. **Backward secrecy is bounded by the last KEK rotation, not by join**: a joiner who captured ciphertext before joining holds key-bearing frames, and the KEK it is handed unwraps them. Same caveat as the leave case — it requires ciphertext obtained outside the media path — and closing it would mean rotating the KEK on every join, the O(N) coordination this design exists to avoid. Accepted and stated. |
| Transmit-key cadence | Senders rotate transmit keys **on every video group and every T for audio**, because rotation is free. What this bounds is a **leaked transmit key** — one group or one T of media — and counter hygiene. It bounds nothing for a KEK holder, joiner or leaver, since every new key is carried under the KEK. |
| Sender resumes from empty | Rotate the transmit key. For video this coincides with the resume keyframe (§5); for audio it is the same generation bump without one. |
| Counter exhaustion | Rotate the transmit key before the stream sequence wraps. |
| Reconnect (same process, transport dropped) | MC re-issues the current KEK; the sender rotates its transmit keys rather than resuming counters. |
| Fresh join after lost state | **Identity key, token, and generation counter share one lifetime.** Losing any means a fresh join with a fresh identity, which is a new key-id namespace; no continuity is attempted. |
| MC migration (ADR-0023) | The new owner generates a fresh KEK; clients receive it on re-attach. MC state is in memory and moves with ownership. |
| MH failover or reassignment | **Not a trigger.** MH never held keys. |

**Rate-limit rotation per participant and evict flappers.** A client in a reconnect loop must not be
able to force a KEK rotation per cycle; the leave debounce covers that, and the per-sender generation
bump on reconnect is the sender's own cost.

### Nonce-reuse invariants are structural, not comments

**Encrypt once and fan out the ciphertext.** A client encrypting per destination either reuses the
counter — catastrophic for AES-GCM, yielding authentication-key recovery rather than mere
confidentiality loss — or burns two counters per frame. §5's stream model makes this hold by
construction.

**Never reuse a (key, nonce) pair.** The key id distinguishes generations, so a transmit-key rotation
permits a sequence reset; §2's rule is to keep counting regardless.

**Receivers reject replays.** Within a generation a replayed frame carries a valid signature and a
valid authentication tag — it *was* legitimately produced. The stream sequence makes rejection
possible and the receiver must use it: a sliding window per (sender, stream, generation), dropping
duplicates and anything below it.

### MH stays keyless, and that is a guard, not a convention

MC now holds the KEK and is one RPC away from handing it to MH. **No key material crosses the MC→MH
contract**, enforced by the credential-leak semantic guard extended to KEK and transmit-key material
in internal messages and in logs — not by review. Every property §7 and §11 derive from "MH cannot
see the media" depends on it.

### What MC can do, stated

- **Read media**, for meetings it owns, while they are live. The accepted cost above.
- **Deny service** and **observe membership**, as before.
- A memory dump of a live MC pod yields the KEKs of that pod's live meetings — bounded to those
  meetings, nothing retroactive.

MC still cannot forge a frame that verifies: §3's signatures are against keys the client validated
through AC, and MC cannot mint an attestation.

### Alternatives evaluated

**MLS — rejected on coordination, and it was the previous decision here (ADR-0028 §5).** MLS fuses
joins and leaves into a single global epoch change. Its O(log N) rekey is genuinely efficient, but it
is bought by making one shared group state the unit of consistency, so **every join is a synchronous
barrier that one slow client can wedge**, and a member who misses a commit is stranded from the
whole group. At scale it also needs a group creator, committer nomination with timeout
re-nomination, authorised external senders with a service-level key, and published group state. At
thousands of participants the observed failure is exactly the one this system's goals forbid: media
breaks for the slow client or never starts for the joiner.

**Pairwise sender keys — rejected on cost under this system's send model.** Each sender wraps to
every receiver. Because every participant sends audio at all times and must be ready to send video
instantly, every participant is a sender, and a leave costs N² wraps per rotation window — around
1 MB/s of relay through MC per thousand-person meeting at a one-minute window, a double-digit share
of media traffic in the several thousands, plus an MC-side transpose with an N²-proportional working
set. Barrier-free and bounded, but it spends bandwidth and MC capacity on operator exclusion, which
this deployment does not require.

**A key server outside MC — deferred, with its design recorded.** The shape that works: the
participant holds an AC-attested X25519 encryption key; MC couriers a request to the key server,
which seals the KEK to the attested key and signs the envelope under its JWKS; the client verifies
both, which closes the two courier attacks (key substitution inbound, KEK substitution outbound).
What it does *not* close without more: the operator's AC can still attest a participant the
operator controls, so excluding an *active* operator additionally needs org-issued certification of
the participant's key, per-org trust configuration, and issuer-scoped token validation (`iss`, which
tokens do not carry today — ADR-0003 Phase 2). A KEK derived from an org master secret was found to
lack forward secrecy and would need versioned, destroyed masters bounded by backup retention. Each
piece is sound; together they are a separate design, and none is needed for the goals as stated.
**Trigger to revisit**: a customer requirement for operator exclusion. The KEK-source seam is
what keeps that design additive.

**Leave-triggered rotation without debounce** — deferred; W is the knob, and a per-org W is
configuration, not design.

### Costs accepted

- **Operator access to live media**, as the user's risk decision under ADR-0024 §5.7, stated
  wherever the property is described and labelled in telemetry.
- **~50 bytes per audio frame** for in-band key carriage, on top of §3's signature.
- **Frames in flight across a rotation**: receivers retain the previous KEK and the previous
  transmit-key generation for a bounded window, which is part of the leave-exposure figure.
- **Both membership bounds are KEK bounds.** A leaver decrypts forward until the next KEK rotation
  (≤ W); a joiner with captured ciphertext decrypts backward to the last one. Transmit-key rotation
  does not tighten either.
- **W, N, and T have no measurement behind them.** Configuration, with stated defaults.

### Identity keys must be AC-attested, and clients must check

Everything in §3 rests on a receiver knowing that the identity key it verifies against genuinely
belongs to the participant it claims. **MC publishes the roster**, so without an independent check
MC could list a fabricated participant holding a key it controls.

What that would buy is narrow but real: MC can already read media, so a fabricated participant adds
**injection** — media attributed to someone who does not exist, or, with a forged roster entry, to
someone who does. The attestation is what makes that impossible even for MC.

**The check**: every participant's identity key carries an **AC attestation**, and clients validate
it rather than trusting the roster. Where a client-verified attestation and MC's roster disagree,
**the attestation wins.**

**AC mints two distinct artifacts** over the same identity key, and they must not be conflated:

| Artifact | Audience | Nature | Lifetime |
|---|---|---|---|
| A thumbprint confirmation claim in the meeting token | MC and MH, at admission | part of a **bearer** credential | the token's, short |
| An **identity attestation** over (participant, key, meeting) | every peer | **non-bearer**, safe to distribute | matched to participation |

The meeting token cannot serve as the peer-verified credential. A meeting token binds *A's own* key
into *A's own* token, and **B never holds A's token** — so it could only work by distributing it,
which would hand every peer a short-lived bearer credential that MC uses to admit you, expire far
sooner than the participation it vouches for, and carry display-name PII to parties with no need for
it.

Binding a **thumbprint** rather than a raw key is deliberate: it forces the verifier to obtain the
actual key from the roster and check that it hashes to the attested value. The RFC 7638
canonicalisation over the OKP key shape AC's JWKS emits already exists in the tree, but in
**test-only code, not the SDK**, so a production implementation is required.

**Identity keys are scoped to a meeting**, not to a participant across meetings. A key reused across
meetings makes a participant linkable by public key regardless of display name, which matters most
for the guests who have the least identity assurance to begin with. It also bounds what a compromised
key is worth.

**AC does not track key continuity, and should not.** It is stateless here: on token refresh the
client re-presents the same thumbprint and AC re-stamps it. Nothing at AC prevents a client
presenting a different key at the next refresh — **continuity is not the token's property**, and the
token must not be mistaken for the thing that guarantees a stable identity across a meeting.

**Guests receive an attested pseudonym, not an attested identity.** Guest tokens stamp a
client-supplied display name with no lookup, so the attestation proves *"every frame came from the
same keyholder"* and never *"who that keyholder is."* Worth having — it stops one guest forging
another's frames — but never to be described as identity assurance.

### Contract changes this section forces

The attestation is a **three-edge contract change**, not one claim on an existing signing path: the
client must deliver its key thumbprint to GC, GC must pass it to AC, and the deserialise-side claims
type must expose it to MC and MH. The largest piece is not AC's — **the authenticated client→GC
join endpoint takes no request body today**, so the client's key has no channel to reach GC; the
guest join path already carries a body, so one edge needs a new shape and the other does not. MC
gains KEK generation in the meeting actor, the KEK in the join response, the KEK-push message, and
the debounced leave rotation.

## 5. What clients send

**MC directs both where a client sends and what it produces.** A send directive names the **streams
to produce**, each with its encoding parameters — codec, bitrate, resolution, frame rate — the header
version to use (§2), and its target set. A stream sent to several targets is encoded and encrypted once and transmitted N times.

Different encodings are **different streams**, not one stream with divergent targets. Opus to one
handler and G.711 to another means two encoders, two key streams, two sequences.

**A target set may be empty. That means send nothing.** Resuming from empty is a keyframe event.

### Why the stream framing matters

It is what keeps §4's encrypt-once invariant true by construction. Different encodings mean different
plaintext, so there is no single ciphertext to fan out; modelling that as one stream with per-target
encodings would silently require either per-destination encryption — nonce reuse, catastrophic — or
two counters per frame. Making it two streams puts the cost in the wire shape where it is legible: a
client's send cost is the number of streams it is told to produce.

### Client mute overrides send directives, and is enforced at capture

Two distinct things are called muting, and the terminology matters because the enforcement point
follows from **who decided**:

| | Decided by | Enforced at | Protects |
|---|---|---|---|
| **Client mute** | the participant, about themselves | the client, at capture | the user, from the server |
| **Server mute** | meeting policy, about someone else | MH, at ingress (§7) | the meeting, from a patched client |

*Host mute* is avoided as a term because it presumes a role model this system has not defined.

**Under client mute, no media leaves the device.** This is enforced client-side at capture and must
not depend on the server honouring it. The two compose — both may be active — and only server mute
requires permission to lift.

**MC keeps the send directive active while a client reports itself muted.** The client suppresses
locally; MC does not withdraw the instruction. This is what makes unmute instantaneous — resuming is
a local decision rather than a round trip — and it keeps two different states distinguishable:
*"MC has not asked you to send"* and *"you have muted yourself"* have different resumption costs and
should not collapse into one.

Muting **signals out of band** rather than transmitting substitute media. Audio simply stops; there
is no keyframe to resume from, so restart is immediate. Video stops and the receiver renders a
placeholder from the slot state (§6), not from a frame — sending encoded black or an avatar burns
uplink and encoder to transmit information the signalling layer already carries. Sending silence
would also be defensible; sending nothing is better.

*Muted*, *silent*, and *the network died* are indistinguishable to MH, which is exactly why the mute
signal must travel out of band rather than be inferred from absence. **NAT bindings are kept alive by
QUIC's connection-level keepalive** (§1), not by media flow, so a long mute does not cost the path.

---

## 6. What clients receive

**A client declares receive capability; MC composes the experience.** The client states what it can
decode and render — a list of slots, each with a media kind and optional constraints. It does not
specify who appears in them.

**Every constraint is an upper bound.** Any subset is therefore a valid fulfilment, and an
unsatisfiable request cannot be expressed. **Pins are a per-slot optional parameter**, never a
parallel list — which makes "seven pins into six slots" unrepresentable rather than merely invalid.
Slot count is capped server-side by configuration.

**Slot state is explicit on the wire.** Absence of frames is not a signal:

| State | Character |
|---|---|
| Active | steady |
| Source present, far-end muted | steady |
| Withheld by congestion | transient — the only genuinely MH-observed state |
| Fewer sources available than slots requested | steady |
| Zero requested | steady |
| Source unreachable (§9) | structurally persistent, unlike the transient states |
| Switch pending (§7) | transient; carries the requesting command identifier |

*Withheld by congestion*, *fewer sources*, and *source unreachable* are indistinguishable to a client
— all present as no media — and render completely differently. A naive client shows the same spinner
for a bandwidth problem, an under-filled grid, and a participant it can never see.

### Why capability, not layout

A per-slot constraint is a statement about what the client can **decode**, not about presentation.
Which participant lands in which slot is MC's decision, from meeting state the client does not have.
The prior contract had the client specifying grid geometry, which inverts that; geometry was never
the server's business. This is also a security property — resource-amplification-by-request becomes
structurally impossible rather than rate-limited.

---

## 7. Selection and switching

**MC decides; MH executes. MH holds no media-type semantics.**

MH is a generic prioritised forwarder. It is never told *"you are video, do video things"*; it is
told what a stream **does** — that it requires an independently-decodable frame to swap sources, or
that a superseding frame permits discarding queued ones. It derives no priority from media type;
**MC assigns each egress stream a priority group explicitly.**

Any proposal that switches on media type inside MH is a design error to be rewritten as a behaviour.

### Why MH must stay type-blind

MH cannot see the media — it is encrypted — so type-awareness would be knowledge it has no legitimate
source for. Practically, a new media kind then costs zero MH changes, which is what keeps content
share (§11) additive rather than a redesign. It is also the design's largest testability lever: MH's
forward path becomes a pure function of pushed configuration and frames, unit-testable with no QUIC,
no crypto, and no clock.

### Selection signals are publisher-supplied and untrusted

MH cannot compute audio energy, because it cannot decrypt. Every signal available to it is therefore
publisher-controlled: a declared salience value is forgeable, and activity-or-recency is gamed by
disabling discontinuous transmission and sending continuously — which is *easier*, needing no
protocol change. The two are security-equivalent, so the choice between them is a media-quality
question, not a security one.

**The mitigation is unconditional and does not depend on which signal is used:** MC-assigned priority
groups **bound** how far a self-declared signal can promote a participant, and a per-participant
churn rate limit prevents floor-thrashing. Without them, *hold the meeting's audio floor* is a
one-line client patch. The same bound governs the discardable flag (§2).

**Signal carriage is extensible.** The publisher region carries an optional type-length-value section
rather than a fixed salience byte, because the useful signals are not one scalar and are not
audio-only: speech-versus-noise classification, onset prediction — *does it look like this user is
about to talk* — and per-media-kind salience are all plausible, and a fixed byte forces them into one
number or a wire change. A fixed field also wastes a byte on every video frame carrying nothing.

**The selection logic matters as much as the field.** What MH ranks on, how it debounces, and how it
combines multiple declared signals within a priority group are design surface, not a lookup — and
they are where quality lives.

**Authenticating a signal does not make it trustworthy.** MH holds no keys and cannot verify the
associated data at all. Authentication stops MH rewriting a value toward a receiver; it does nothing
about a lying publisher.

### Switching sources at a slot

A slot switching from source A to source B must land on an independently-decodable frame from B, or
the subscriber decodes garbage.

**MH holds the switch as pending state with a duration**, continuing to forward A meanwhile. The
switch is gated on B's next independently-decodable frame, which is why a policy change can never
break decoding — the guarantee is structural, not achieved by locking.

Under §1's one-group-per-stream shape this is cleaner than a gate: an independently-decodable frame
*is* the start of a group, so **the transport boundary, the dependency boundary and the switch
boundary are the same boundary.** Executing a switch is finishing the stream carrying A's group and
opening the next one from B's — not inspecting frames for a flag mid-stream.

**Keyframe cadence is now two parameters at once.** It sets switch latency, since a switch waits for
the next group; and it sets stream-open rate, since a group is a stream (§1). Shortening it improves
the first and worsens the second. Under per-frame streams these were independent; they no longer are,
and the value should be chosen knowing both.

Because MC turns off publishers nobody watches (§5), **B may not be sending at all**, so the switch
is two phases:

1. **Ensure B is sending.** MC issues a send directive to B. A no-op if another subscriber already
   watches B.
2. **Switch at B's next independently-decodable frame.**

Where phase 1 was required, resume-from-empty is itself a keyframe event, so **B's very first frame
is independently-decodable and the switch lands on it immediately**. The latency is then an MC→B
round trip plus encode — a different bound from the case where B was already publishing, where it is
up to one keyframe interval.

**Pending switches have a lifecycle, and it must be specified:**

| Event | Behaviour |
|---|---|
| MC reverts to A while pending | The pending switch is **abandoned**, not queued |
| B departs before producing a usable frame | Abandon and retain A; the slot must not go dark |
| B never produces one | Bounded wait, then a defined slot state — **this bound is an open item** |
| A second switch arrives for the same slot | Supersedes the pending one; the superseded command is reported abandoned |

**MH reports completion to MC by command identifier, never by state description.** If MC switches a
slot to B, reverts to A, then switches to B again, a report reading *"A→B complete"* is ambiguous
between the first and third commands and MC will act on the wrong one.

This is distinct from the policy acknowledgement in §8: **policy applied is not switch executed.**
Withholding the policy acknowledgement until a switch completes would let a slow source stall it,
leaving MC unable to distinguish a lost policy from a pending keyframe. Completion is reported
through the slot-state channel as a *switch pending* state carrying the requesting command
identifier, which also degrades safely — if the report is lost, the next re-assert restates the
desired policy and MH has either switched already, an idempotent no-op, or is still pending.

### Server mute is enforced at MH ingress

MH drops the muted source's ingress on MC's instruction. Client-side-only enforcement is not
enforcement; a patched client ignores it. This is the counterpart to client mute (§5), and the two
are enforced in different places because they protect different parties.

**Asymmetric audibility — *A can hear B but not C, while B hears both* — generalises differently for
video and audio, and the audio half is not free.**

For **video**, MC wires each (subscriber, source) edge explicitly, so a per-edge permission is the
natural shape and costs nothing beyond the edge already being assigned.

For **audio**, MH selects autonomously, so there are no explicit edges to permit. Exclusions have to
become **inputs to the selector** — a per-subscriber exclusion set evaluated during selection — which
is a real addition to the selection rule model rather than a property that falls out of it.

v1 does not ship asymmetric permissions. Nothing in the edge model precludes them, but the selector
must be designed knowing that per-subscriber exclusions are where they will land.

---

## 8. The MC→MH control plane

### What this solves

MC must tell MH how to forward media: which sources reach which subscribers, in which slots, at what
priority, over which transport. MH must apply that **without ever querying MC in the hot path** — a
round trip per forwarding decision would defeat autonomous selection.

That makes this a **push** channel, and push channels have one hard problem: **what happens when the
receiving side loses its state?** MH's registration and policy are in memory. When an MH pod restarts
— deploy, drain, crash, node loss — it comes back knowing nothing, and it **cannot ask**: MH learns
MC's address *from* the registration it just lost.

Historically the trigger was worse than absent. Registration fired once, on the first participant to
join a meeting. After an MH restart mid-meeting, no future joiner is ever "first", so registration
never re-fired and every reconnecting client was provisionally accepted and then kicked at fifteen
seconds: **a permanent media blackhole for that meeting until it emptied**, reached through an
ordinary rolling deploy. The asymmetry is that registration state lives on MH while its trigger was
keyed off MC's local view — and the half that loses the state is the half that cannot ask for it
back.

### The mechanism

**MC periodically re-asserts the full meeting registration, now carrying policy.** The registration
request gains the selector, per-egress-stream behaviours, priority groups, transport mode, and server
mute (§7) — plus a **generation** number, monotonic per (meeting, handler) and derived from the assignment
computation's *output change*, never free-running.

MC re-fires on four triggers:

| Trigger | Character |
|---|---|
| **Structural change** — a slot reassignment, a join, a leave | immediate; the normal path |
| **A handler newly assigned to a meeting** | immediate; MC programs a handler the moment it is given work |
| **Connectivity loss on the channel to a handler** | immediate; the fast path for restarts |
| **Configured cadence** | the backstop — the only *guaranteed* recovery |

**Connection loss is the fast path, and it does not require a long-lived stream.** MC holds a gRPC
channel to each handler regardless of whether calls are unary, and channel connectivity state is
observable — so a restart that severs the connection is noticed immediately rather than at the next
tick.

The cadence remains necessary because it is the only trigger that survives cases the connection event
misses: a silent path failure where the channel never reports a drop, an MC restart that loses its own
view, or a handler that restarts fast enough to re-establish before the drop is processed.

MH's handler already upserts and **drains and promotes pending connections**, so a re-assert actively
rescues clients sitting in the provisional window. MH needs no change; the whole fix is MC-side.

**The registration response carries the applied generation** — the generation MH has **applied**,
never the highest it has received.

#### Why the acknowledgement exists

A boolean "accepted" can only mean *"received and parsed"*. It cannot mean *"my live selector
reflects this"*, because MH's handler does not apply policy itself — it hands it to the session actor
over a bounded mailbox and returns. The apply is asynchronous relative to the response, so:

> MC sends generation 7 → MH enqueues, returns success → the mailbox is full or the apply errors →
> **MC believes MH runs generation 7; MH runs generation 4.** The connection is healthy, the call
> succeeded, every liveness signal is green.

That is a partial blackhole reporting healthy, which is harder to diagnose than a total one. Echoing
the *applied* generation lets MC compare what it sent against what took effect and re-snapshot on
mismatch. Echoing on **receipt** instead would reproduce exactly the bug the field exists to catch.

In steady state this is silent: MC sends generation 7, MH echoes 7, nothing happens. Because the
generation derives from output change, unchanged policy carries the same number and MH no-ops. The
signal is only interesting when the two differ, and it is self-correcting — a lost response is
re-asserted on the next tick.

The considered alternative was applying synchronously so a boolean would suffice. That works, and it
was rejected because it couples MC's control-plane latency to MH's actor queue depth — the coupling
one would least want during an incident.

#### The cadence is a recovery-time bound, not a tuning knob

**Configured cadence ≤ 10 s, and it is the maximum media-dark window after an MH restart** in the
cases the connectivity trigger does not catch.

This was misread three times during the debate, so it is stated in those words. **An MH restart is
not a structural change**: when the pod dies nothing happens in MC's meeting state — no reassignment,
no policy edit, no event — so the structural path never fires, and where the channel does not report a
drop, **the cadence tick is the only thing that recovers a restarted handler**. "A moderate cadence is fine because structural changes are
immediate" is true for policy-change latency and false for restart-recovery latency, and only the
second leaves users watching frozen video.

Two independent constraints converge on the same number, which is what makes it hard to erode later:

- The **media-dark window** above.
- **Cadence < provisional-accept timeout − margin.** A client reconnecting to a restarted handler
  waits up to the registration timeout — fifteen seconds by default — to be promoted. A cadence above
  that means clients bounce for the entire restart window instead of being rescued when the re-assert
  lands. These are two configuration values in different places encoding one relationship, so
  **startup validation on drift is required**.

The cost is small precisely because of §9's co-location: O(meetings), roughly 100 calls per second at
a thousand concurrent meetings, into a hash-map insert.

#### Supporting requirements

- **Re-assert failures are counted and paged.** A silently failing cadence is the same blackhole with
  extra steps, and nothing else detects it.
- **Dispatch is jittered.** An MH restart otherwise makes every MC re-assert every meeting on one
  tick.
- **Config-apply does not route through the connection-lifecycle mailbox.** That mailbox is bounded
  and was sized when registration was one-shot per meeting; a ten-second tick carrying full policy is
  a different workload and must not starve connection handling.
- **Any two-ends-must-agree configuration is echoed back with a loud mismatch.** Transport mode is
  declared once by MC and echoed by MH. The cautionary precedent is a capacity value advertised to GC
  and enforced nowhere — declared in one place, assumed in another, verified nowhere.

**Deferred with a trigger**: a streaming channel carrying deltas, when sub-cadence recovery becomes a
requirement or meeting size × structural churn makes full-snapshot re-assert too costly. The delta
capability is unused at v1 sizes, which is why streaming's only present gain would be sub-cadence
restart latency — bought at the cost of three verified defects that fall only on a long-lived stream:
a per-request timeout that bounds a streaming call's whole lifetime, a channel constructed per call
with no long-lived home, and an idle stream that reads healthy over a silently failed path because
keepalive defaults to off. **A stream needs keepalive to manufacture a liveness property the
periodic tick has intrinsically.**

---

## 9. Multiple media handlers

**Media routes only within a handler.** Handler-to-handler cascade is future work and must not be
precluded.

**Visibility rule: a participant sees another if and only if they share a media handler.**

**MH is multi-handler-ready by being multi-handler-oblivious.** Its routing table is single-node
either way. MC computes the visibility graph, assigns each publisher→subscriber edge to exactly one
handler, and pushes each handler only its own edge set. MH never deduplicates and never knows peer
handlers exist.

**MC has no single-handler special case.** The assignment computation runs the general algorithm
always; one handler is simply N=1. There is no *"if one handler, take the simple path"* branch —
which also means the multi-handler path cannot ship untested behind a degenerate happy path, because
there is no separate path to go untested.

**Multi-handler send is required, not only multi-handler receive.** A client may be directed to send
the same media to several handlers, sharing encoder resources with separate transports. This follows
from the visibility rule: where A is on handler 1 and C on handler 2, B must publish to **both** for
each to receive B without cascade.

**Fanout is a policy input, not an architectural limit.** How many handler connections a client
holds, and what is sent on each, is MC's decision — computed from factors that trade against each
other:

- **Client uplink contention.** N handler connections are N independent congestion controllers
  sharing one bottleneck uplink with no knowledge of each other; the loss each induces is loss for
  the others, and aggregate goodput at two can trail one non-linearly.
- **Cost incidence.** Handler-to-handler bandwidth is on our bill; client uplink fanout is on the
  user's. These are substitutable, and which is preferable depends on the meeting and the client's
  link.
- **Handler egress budget.** MH is a passthrough forwarder, so the binding constraint is egress
  bandwidth, not CPU. Co-location thresholds derive from the per-pod egress budget as configuration,
  never asserted as a constant.

A fixed cap would decide that trade permanently and invisibly. The congestion finding is recorded as
evidence about a factor MC weighs, not as a ceiling.

**Consequence for the roster**: MC knows every participant, so the roster shows all of them while
media shows only the reachable subset. *Source unreachable* is structurally persistent, unlike the
transient slot states, and needs distinct treatment in the interface.

---

## 10. Verification

**Latency is observed, never gated.** Asserting a wall-clock end-to-end target on a local cluster
produces a permanent flake; ADR-0028 forbids quarantining quality gates, so the test would be deleted
and the headline objective would end with **zero** coverage.

**Tier 1a — pure-function gates.** Frames in equals frames out. Zero-copy fan-out asserted on buffer
refcount — one ingress buffer, N egress payloads sharing it — scoped to the fan-out, not across the
stream-read boundary where a copy is inherent. No per-frame allocation. Swap only at an
independently-decodable frame, one parameterised test covering both instant audio switching and
keyframe-gated video switching. All deadline logic takes an **injected clock**.

**Tier 1b — control-plane gates.** Cadence convergence, asserted on the **acknowledged generation in
the response** rather than on MH internals, so it survives refactoring. Its negative half is the one
gate in the set that catches a false green: **inject an apply failure and assert the acknowledgement
does not advance.** Generation monotonicity — a stale lower generation is ignored. Re-assert failure
counted. **Re-assert idempotency** — an identical re-assert is a true data-plane no-op, without which
you get connection churn at exactly the cadence interval, which is periodic media glitches wearing a
configuration disguise. Cadence-interval ceiling.

**Tier 1b — forward-path gates.** A slow subscriber does not stall a concurrent fast one. Concurrent
unfinished groups stay at or below the application cap and never reach the declared transport
ceiling.
Frames past the bound are reset or dropped per assigned policy rather than queued unboundedly.
Written against *the declared bound, whatever it is*, so it survives the outcome of the fan-out
experiment.

**Tier 1b — multi-handler.** A forced two-handler assignment gate against the clusterless assignment
function, plus an environment test at two handlers.

**Tier 2 — regression benchmark**, deltas only, coarse tripwire, driven through the transport seam
with zero syscalls. Absolute numbers are not meaningful on a shared laptop; deltas are.

**Tier 3 — objective**, non-gating, in a real environment.

**Both seams are prerequisites, not refinements** (see *Ordering constraints* under Consequences).

- **A transport trait seam in MH**, mirroring the client's transport interface. Hot-path
  per-connection I/O behind the trait; endpoint and accept loop stay concrete. It is quadruply
  justified — benchmark representativeness, deterministic reachability of drop paths for metric
  coverage, the back-pressure gate, and control-plane reconnect testing — and it is what lets a
  deterministic loss and delay shim drive the forward path at realistic fan-out **with zero
  syscalls**.
- **The measurement seam**: first-media-received on the client, forward histograms on MH, sampled
  from the start.

---

## 11. Telemetry and operational configuration

**The per-frame invariant is zero allocation and zero registry lookup** — not "histograms over
counters", which is backwards, since histograms cost more series than counters. Per-stream forwarders
own metric handles resolved once at setup, so **no metric macro is reachable from the forward
function**. Two verified violations pin this today: the payload copy in the codec's decode path, and
a label allocation in the metrics helper.

**MH self-monitors internal latency, decomposed.** Ingress-from-network to egress-to-network, broken
into time in the receive buffer, time in processing and routing, and time in the transmit buffer —
because those three have different remedies and an undifferentiated total does not tell an operator
which to pursue. This is cheaper than it appears: a monotonic clock read is a vDSO call of roughly
25 ns, and **the ingress timestamp already exists**, since deadline-based reset needs frame age
regardless. The expensive part is the histogram observation, not the clock read — so timestamp
always, observe one in N.

**Media-path telemetry must not persist the metadata leak.** MH necessarily sees, per frame, which
connection published it, its size, and its timing; with discontinuous transmission that reconstructs
who spoke, in what order, for how long, and who interrupted whom. This in-memory leak is the standard
bargain for a relay that cannot decrypt, and it is **accepted with its adversary set stated: MH,
whoever compromises MH, and a curious operator — not a network observer**, since these fields sit
inside QUIC with TLS terminated at MH. Padding is explicitly out of scope. What is not accepted is
retaining it:

- **No per-frame, per-participant, or per-stream-identity dimension** in media-path logs, metric
  labels, or span attributes. Aggregate distributions are safe; **the time-ordered sequence of sizes
  for a single stream is the voice-activity trace.**
- **Flat prohibition: no meeting identifier on any metric anywhere in this design**, with one closed, enumerated exception: the ADR-0028 join-flow metrics that already carry the client SDK's implicit join label set (`meeting_id_hash`) are grandfathered as a set and are not extended; every metric this design adds, and every future metric on any media-carrying path, is under the bar. Stated as a rule
  rather than a derivable conclusion because four specialists independently had to be corrected on it
  during the debate — a rule re-derived by every author who touches a metric will not survive. It is
  barred twice: unbounded cardinality, and per-meeting aggregation in a **two-person meeting is
  nearly per-stream**, where a two-stream series is de-anonymising by inspection. Two-person is the
  common case.
- **Aggregation floor is pod or service level.**
- **Sampling must be random, not deterministic per stream** — the natural modulo implementation meets
  the CPU budget while perfectly reconstructing the sequence.
- **A log level is not an acceptable gate.** The incident motivating a level change is the same
  incident producing the sensitive trace; enabling debug logging on a pod is one routine action away
  from a fleet-wide voice-activity trace entering the shipping pipeline.
- **Per-participant resolution lives in an armed, meeting-scoped, auto-expiring in-memory ring
  buffer** dumped on demand — not exemplars, which relocate retention into the trace backend rather
  than solving it, and not a log level. Telemetry reports *how many and how bad* via a bucketed-rank
  gauge; **MC's own assignment state answers *who***, at investigation time, in a system that
  legitimately holds that mapping.
- **Enforcement is a deny of log and metric macros scoped to a directory**, covering the whole media
  path — not a file list, which narrows silently on refactor while the guard keeps passing. This
  requires one layout constraint: lifecycle, setup, and teardown are **siblings** of the media
  directory, not children, so the directory boundary and the hot-path boundary are the same boundary.
  Deny the event macro too, since level macros expand to it, and **allow** cached-handle record and
  increment calls — the guard denies macro forms and must not touch handle methods, or it bans the
  pattern it exists to enforce.
- **A development-only per-frame tracing feature is guarded by a compile error in release builds**,
  not by a CI check. A control that has to notice fails silently for anyone building outside the
  pipeline; a compile error has nothing to notice.

**Vocabulary additions cannot be cited as the protection.** The guard's matcher is word-boundary
based and underscore is a word character, so a bare token matches but no realistic prefixed spelling
does, and a prefixed spelling of a sensitive token is the realistic form. A guard reporting clean
while the offending line ships reads as coverage. The directory-scoped deny catches by *shape*.

**Key custody is a label, not a boolean.** Every service reports `key_custody=operator` in logs and
metrics; no metric, log, or document may carry an end-to-end or zero-trust boolean, because the
default deployment is neither (§4). The credential-leak semantic guard's scope gains KEK and
transmit-key material on the MC→MH contract and in MC logs — the one place a key could leave the
component entitled to hold it.

**Admission control is keyed on egress bandwidth, not connection count.** A stream-count value is
advertised to GC as capacity and enforced only at GC placement, never at MH admission; MH's own
enforced limit is a connection count two orders of magnitude off the real constraint. The capacity figure must be **the same configuration
value that enforcement reads**, published as a gauge, or the alert threshold and the enforcement
drift silently.

**Derive rather than guard wherever two values encode one relationship.** The drain window is derived
from the termination grace period rather than validated against it; the capacity gauge publishes the
value admission control reads. A derived value cannot drift; a guard only catches drift after someone
introduces it. Startup validation is the fallback where derivation is impossible — as with the
cadence and provisional-timeout coupling in §8.

**Frames dropped for missing key material are counted on the client, by reason** — no KEK for the
carried generation, no roster entry for the sender (§4); a frame with no key and no wrap is a
protocol violation and lands in the decode-reject count instead. MH cannot
observe any of them, since it never opens a frame. Expected as a transient at join and after a KEK
rotation; a sustained rate is a join or rotation path that has silently stopped delivering keys, and
it is the only signal for it.

**Silent failure modes are counted on both ends** — datagram send drop, stream-credit stall, and
partial-frame discard on MH; the sender-side equivalents in the SDK. **The client-side drop matters
most: it occurs in the sender and MH structurally cannot observe it.** WebTransport exposes no
send-side drop event, so the SDK keeps the transport queue shallow, owns a bounded queue above it,
makes the drop decision there, and counts it — **making the drop observable by construction**. This
also closes the mirror of the oversized transport buffer in §1.

**Ingress denial-of-service caps**: maximum frame payload before any allocation; per-connection
concurrent-stream and creation-rate limits enforced before per-frame work; a bounded datagram queue
with drop-oldest; a server-side slot cap.

**MH sheds media sessions on restart, and that is the decision, not merely the current behaviour.**
Shutdown marks not-ready, cancels, and sleeps two seconds inside a thirty-five second grace period,
with no drain phase, and v1 keeps it that way: draining media sessions means either holding a pod
open for the length of a meeting or migrating live sessions, and neither is in scope. Recovery is
§8's re-assert, and the ≤10 s bound is what makes shedding tolerable. The Open Items entry below is
about *testing* that recovery, not about adding a drain.
Resumes must be **jittered on both MC and client sides**, or recovery after a congestion episode, an
MH restart, or a rollout produces a synchronised keyframe storm — keyframes being an order of
magnitude larger than delta frames, from every publisher at once, into a link that just proved
constrained.

**Eight media runbook scenarios are required**, and alert validation requires every runbook
reference to resolve to a file, so none of the alerts below can exist without its runbook. They are — restart media-dark,
sustained re-assert failure, generation divergence, keyframe storm, stream-credit stall, datagram
drop, egress exhaustion, and rollout-with-media. The existing fourteen cover no media-flow failure at
all, because there is no media path yet. Sustained congestion-withholding is the **leading indicator**
for egress exhaustion and belongs named inside that scenario.

**Media kinds: main audio and main camera only.** The slot model carries a media-kind dimension and
selection pools are per-kind, so content share is additive rather than a redesign. Its egress profile
differs enough — higher resolution, bursty keyframes on otherwise static screens — that the egress
budget must be expressed **per stream, not per participant**, or adding it later is a redesign.

---

## A control's coverage must be demonstrated, not asserted

**A demonstration has two halves: does the control *fire*, and does it *apply*.** A control can be
alive and out of scope, or in scope and dead. Both are silent, and both read as coverage — which is
worse than an absent control, because an absence gets noticed.

**The remedy was the same move every time: prefer structural impossibility over a control that has to
notice.** A directory scope over a maintained file list. A compile error over a CI build guard. A
path-scoped deny over a vocabulary entry. A derived value over a drift guard. A generation derived
from output change over a tested idempotency convention.

The *applies* half is the one nobody tests unprompted. Three instances from this design, each
mapping to a gate this design adds:

| Instance | Failure |
|---|---|
| Alert selectors matching no real pod — several alerts had never fired in the product's life | alive, never applied |
| A generation gauge fed by the received rather than the applied value | applies, never fires when it should |
| A per-frame tracing feature guarded by CI rather than by compilation | could be out of scope entirely |

**Forward checklist — every gate this design adds answers both halves explicitly, at plan approval.**

| | Does it fire? | Does it apply? |
|---|---|---|
| Ask | inject the adverse condition — an apply failure for the generation acknowledgement, a stalled subscriber for back-pressure, a release build with the tracing feature returning non-zero | confirm the premise against the real artifact — the release profile governs the deployed images, the denied directory *is* the hot path, the selector matches a real container, the metric is actually emitted |

**This needs no new process**: ADR-0031 already requires plans touching metrics or alerts to carry a
structured block reviewed at plan approval by the cross-cutting reviewers. This is a second table in
that same block.

**A corollary: when a metric is hard to make meaningful, that is evidence about the design, not just
about the metric.** The
objection *"a reconnect counter is meaningless if its failure is undetectable"* was not answered by
adding keepalive — the control plane changed so the failure mode ceased to exist. The question *"can
we count a send-side datagram drop?"* was not answered by finding a counter — the SDK took ownership
of the queue. That is how instrumentation pays for itself rather than taxing, and it is the concrete
answer to deferring performance work.

---

## Consequences

### Positive
- The wire is settled, with a publisher/relay split that makes the trust boundary structural rather
  than conventional.
- MH is a pure function of pushed configuration and frames — no codec, crypto, or media-type
  knowledge — which is simultaneously the largest testability lever and defence in depth.
- Conflicts are made unrepresentable rather than resolved by precedence rules (§6).
- Sender authentication is a design rather than a deferral, and is independent of §4's outcome.
- Three latent defects were found and fixed during the debate: the restart blackhole, the unvalidated
  reserved-byte covert channel, and alert selectors that had never matched a pod.

### Negative
- Per-frame signatures roughly double audio frame size (§3).
- The accepted metadata leak is real and permanent for a relay that cannot decrypt; padding is out of
  scope.
- §1 does not isolate audio from video at the congestion-control layer.
- **MC, and therefore the operator, can read live media.** Accepted as the user's risk decision under
  ADR-0024 §5.7; MH, transport, and storage cannot, and nothing may describe the result as zero-trust
  or end-to-end against the operator (§4).
- In-band key carriage adds ~50 bytes to every audio frame (§4).
- A departed participant retains working keys until the debounced KEK rotation lands. Bounded, stated,
  and inert against anything but ciphertext obtained outside the media path (§4).
- Multi-handler send costs N× client uplink and can reduce aggregate goodput on a constrained link.

### Open items — carried deliberately, not resolved
- **The egress budget has no measurement behind it.** It is better shaped than the stream-count value
  it replaces — it names the quantity that binds, admission control reads it, and the gauge publishes
  it so the alert denominator cannot drift — but there is no production deployment to measure. The
  default **fails closed, deliberately low**, chosen by the asymmetry of the error: under-estimating
  costs money linearly and visibly, over-estimating saturates the interface, degrades every stream at
  once, and fires the alert at a percentage of a fiction. MH logs the derived subscriber ceiling
  loudly at startup. A measured figure requires a production deployment and does not yet exist. **The
  tell that this is likely rather than pessimistic: it is the failure this debate found, and the fix
  reproduces its precondition.**
- **No end-to-end test exercises the recovery machinery.** Drain, re-assert, convergence from an
  empty baseline, the generation acknowledgement, and resume jitter all converge on *"the handler
  restarted, recover cleanly"*; all are unit-tested in pieces; nothing runs a real rollout with media
  flowing. That is the shape of the bug this debate started with — a mechanism correct in isolation
  inside a system that never re-invoked it. Cheapest closure is one chaos case: establish a meeting
  with media flowing, delete the handler pod mid-call, assert media resumes within a bounded window
  **with no client rejoin**. Add the MC-restart equivalent, which currently self-heals *by accident*.
- **There is no revocation path for a compromised participant signing key.** No token denylist exists
  anywhere in the tree — `jti` is generated and never checked against anything — and AC's key rotation
  rotates AC's *own* signing keys, not participant identity keys. Token exposure is bounded to the
  token TTL, but the participant signing key survives reconnect and every refresh, and there is no
  mechanism to invalidate it mid-membership. The only lever is to stop re-attesting at the next
  refresh, which bounds exposure to one refresh interval and does nothing within it. Scoping keys per
  meeting (§4) bounds the blast radius to a single meeting; it does not close the gap. Recorded as
  absent by design today rather than deferred, because nothing in the current path provides it and
  nothing should imply otherwise.
- **Historical per-meeting debugging has no retention posture.** §11 accepts the in-memory metadata leak and bounds live per-participant resolution to an auto-expiring ring buffer, which serves debugging a meeting while it runs and cannot serve debugging one after it has ended. Operations will need the latter. Retaining per-meeting, per-participant quality data is exactly what §11 forbids, because the time-ordered per-stream record is the voice-activity trace; a decision is needed on the aggregation floor, the store, the retention period, access control, the widened adversary set (storage, backups, time), and the interaction with §4's guest pseudonymity. Recorded as open; it is a §11 amendment and belongs to its own debate.
- **The switch-abandonment bound is unspecified** (§7): how long MH waits for a source that never
  produces a usable frame, and what the slot becomes when it gives up.
- **The shared-atomic contention benchmark is unresolved.** Named at the outset as the item most
  likely to be wrong in the telemetry design; a falsifiable prior exists and it runs with the fan-out
  spike. Reported open rather than absorbed, because everything around it converged and it did not.

### Amendments required to Accepted documents
| Document | Change |
|---|---|
| `ARCHITECTURE.md:253,276,283,289,293` | QUIC datagrams throughout → hybrid per §1 |
| `ARCHITECTURE.md:24` | The join-to-media target is an objective, never a gate |
| `ARCHITECTURE.md:851-857` | Key distribution per §4: MC-issued KEK, in-band transmit keys, operator custody stated |
| ADR-0028, "bidirectional datagrams" | Not a coherent construct; superseded by §1 |
| ADR-0028 §4, 42-byte header and 64-bit user id | Superseded by §2's header and §3's attribution model |
| ADR-0028 §5, replay counter "per-sender" | Now per (sender, stream) — §2's per-stream sequence and §4's per-stream key derivation |
| ADR-0028 §5, "AES-256-GCM" | Unchanged — §4 states AES-256-GCM per ADR-0027; AES-128-GCM remains permitted only for SFrame interop |
| ADR-0028 §5, rotation on manual request and hourly | Superseded by §4's rotation table: senders rotate transmit keys on every video group and every T for audio because rotation is free; MC rotates the KEK on departure, debounced to W |
| ADR-0028 §5, MLS for key management | **Superseded.** §4 uses an MC-issued meeting KEK with sender transmit keys carried in-band. MLS's efficient rekey is bought by making every join a synchronous global barrier, which is the direct negation of the join goals |
| ADR-0011, handler jitter objective | **Unmeasurable** — MH forwards and does not buffer; perceived jitter is a client-side jitter-buffer property and jitter-buffer design is out of scope. Struck, with a client-side successor deferred with jitter-buffer design |
| ADR-0011, handler forwarding latency objective | No defined measurement point; redefined as ingress-read-complete → egress-enqueued |
| ADR-0012:388-389 | Thresholds already mandated but unimplementable; the bandwidth indicator in §11 makes them real |
| ADR-0027, key-derivation row | **Amendment required and landed.** The original row read "no amendment needed — AES-256-GCM, HKDF and Ed25519 are all already approved", which was wrong in one specific way: the approved-algorithms table named **HKDF-SHA256**, not HKDF generically. §4's ciphersuite `AES_256_GCM_SHA512_128` (0x0005) fixes the hash at SHA-512, so it requires `ring::hkdf::HKDF_SHA512`, which the table did not permit. Broadened to HKDF-SHA256 **and** HKDF-SHA512. Recorded as a correction rather than silently edited, because a reader who trusted the original would have concluded the media key schedule was covered by the existing table when it was not — and ADR-0027's table is applied by hand at Gate 1/3, so the gap would have surfaced as a reviewer objection rather than a failing check |
| `docs/observability/slos.md` | Referenced by ADR-0011 and does not exist; created |
| `docs/PROJECT_STATUS.md` | Badly stale — lists MH as a skeleton and GC/MC as planned; corrected |

### Assumptions that must be validated before the wire format is frozen

The design rests on four claims that are falsifiable and have not been measured. Each has a named
experiment; the header is not frozen in code until they have run.

1. **Fan-out spike**, handler server half and browser client half in one harness through the
   transport seam, swept at fan-out **with a slow-subscriber arm**. Its job is to **validate
   one-group-per-stream and establish the application bound**, not to choose the shape — the
   granularity is decided in §1 by dependency alignment, which is a design argument rather than an
   empirical one. Running at full speed with fast readers will not find the ceiling; the binding
   quantity is **concurrent unfinished groups under receiver backpressure**, not raw open rate. Keep
   a per-frame arm only as a control, to confirm the churn argument holds in practice.
2. **Capture-resistance selector test**, pure logic, no media: scripted speaker turns with crosstalk
   and near-threshold flapping; assert correct selection with zero control round trips **and** that a
   participant declaring maximum salience cannot outrank a higher priority group.
3. **Contended-counter benchmark**: a shared cached counter versus per-stream sharding at increasing
   thread counts. Low cardinality means every stream hits the same atomic — a tension inside the
   telemetry design, named rather than assumed away.
4. **Insider-forgery regression test**, in the test-vector harness: one participant encrypts under
   another's key id, and the test asserts the receiver **rejects the frame** rather than attributing
   it. It is the test that proves the signature layer is load-bearing rather than decorative.

   > **DISCHARGED at story task 15 (2026-09-05, `docs/devloop-outputs/2026-09-05-sdk-frame-v2-sframe-stack/`).**
   > The regression now runs in a **second, independently-authored implementation** — the TypeScript
   > `@darktower/sdk-core` frame codec — against the same vectors the non-production Rust generator
   > produces. Vector row `insider_forgery_other_sender_key_id`: a frame signed by Bob but sealed
   > under Alice's transmit key and carrying Alice's key id is verified against the identity key
   > resolved from `key_id.sender_id` (Alice's), and verification **fails before any decryption**; the
   > row additionally asserts `would_decrypt_if_verification_skipped`, so the rejection *proves* the
   > signature layer load-bearing rather than merely asserting it. **Authoring the row at task 8 did
   > not discharge this — running it in a second implementation does**, which is exactly this
   > section's "not frozen in code until they have run." The cross-language gate is now closed:
   > `proto/test-vectors/frame-v2.vectors.json` carries `gated_by.typescript: true` and
   > `cross_language_property_established: true`, enforced by
   > `scripts/guards/simple/validate-frame-vectors.sh` check g14, which hard-fails on any drift
   > between the flag and reality in either direction. The header is frozen in code for the
   > signature/attribution property. (Assumptions 1–3 remain scaling/telemetry experiments, unaffected
   > by this discharge.)
   >
   > **What the discharge rests on, and its limit — recorded so the limit travels with the claim.**
   > Three of the computations these vectors gate have **no oracle outside this repository**: the AEAD
   > associated-data span (publisher region only), the Ed25519 signed range (publisher ‖ payload, relay
   > region excluded), and the detached-tag split. No RFC and no external vector checks them — the
   > vendored sframe-wg anchor (`proto/test-vectors/external/sframe-wg/PROVENANCE.md`) gates only the
   > key schedule, the nonce derivation, AES-256-GCM and the tag length, and the vectors file's own
   > `_non_production` header says the same. So this discharge is **exactly as strong as the genuine
   > independence of the two implementations** — the non-production Rust generator and the TypeScript
   > codec, each written from this ADR and RFC 9605 rather than from the other. A green gate here is a
   > cross-check between two independent derivations, **not** a proof against an external standard: were
   > the two ever to converge by one mirroring the other, the vectors would validate a shared error
   > instead of catching it. That independence is a property to preserve, not a fact to assume — it is
   > why the codec was authored from the spec, not from the generator, and why replacing either side
   > with a port of the other would silently hollow out this discharge while leaving every gate green.

### Ordering constraints that follow from the design

- **Test vectors and the header precede the forward path.** Vectors are the drift guard between the
  two codecs (§2); a forward path written against an unfrozen header is written against nothing.
- **The transport seam precedes the ingress and egress loops.** Cheap against nothing, expensive
  against concrete types (§10).
- **The KEK-source seam precedes any key work.** It is what keeps a future key server additive (§4).
- **Every runbook an alert cites exists when the alert lands**, because alert validation requires
  every runbook reference to resolve to a file (§11).

### Explicitly out of scope
Simulcast and scalable coding; quality adaptation; forward error correction, retransmission, and
jitter-buffer design; handler-to-handler cascade; recording and transcription; screen share as a
shipped feature; codec negotiation beyond a v1 default; asymmetric per-edge permissions.

**Flagged for separate audit**: a participant-level registration RPC and its
connection-token response field may be dead in the same way a previously removed token pattern was.

---

## Participants

Decided by media-handler, meeting-controller, protocol, client, security, test, observability, and
operations, with auth-controller reviewing §4 after the debate closed. Satisfaction scores,
per-specialist contributions, and the record of positions changed during the debate are in
`docs/debates/2026-08-23-media-flow/debate.md`, not here.

**Operator access to live media is accepted risk, recorded as the user's under ADR-0024 §5.7 rather
than a majority override.** Security's position was that any design under which an operator service
can read media requires that acceptance explicitly; it was given, and §4 records the grounds.

---

## Appendix — wire specification

Protocol holds the full text; this is the shape.

**Frame header** — a binary codec in `media-protocol`, not protobuf. Version 2. Layout and the
publisher/relay boundary per §2, including the key-bearing flag and the wrapped-transmit-key field it
announces in the publisher region. The signature (§3) trails the payload and covers the publisher
region and payload; the payload extent comes from the header's length field (§2), which is what
delimits frames sharing a group's stream.
Flags decode rejects any bit beyond those defined. A maximum-payload constant is enforced
pre-allocation in both languages. Decode slices rather than copies.

**Signalling contract** — delete the layout messages, the duplicate media-type enums, and the
single-key encryption message. Add: a receive-capability declaration with per-slot media kind and
optional pin; a slot-state enum with the seven states of §6; a send directive listing streams to
produce, each with encoding parameters and a target set carrying transport mode; a stream assignment
carrying participant identity, media kind, handler address, and slot state; the meeting KEK
and its generation in the join response; and a KEK-push server message carrying a new KEK and
generation on rotation. The roster gains, per participant, an AC-attested identity public key —
client-validated, never trusted from MC. **No key material rides on the roster.** The existing host-mute request message is **renamed to
server mute** per §5's terminology; it is the same message, and the rename should land with the other
signalling changes rather than separately.

**Internal contract** — delete the routing RPC and its transcode and mix options, which describe a
transcoding-mixer relay that cannot exist under end-to-end encryption and is called from nowhere.
Registration survives and *is* the control plane (§8): it gains the selector, selection rules, and
per-egress-stream behaviours carrying priority group, supersede-on-independent-frame, transport mode,
and server mute — plus a generation derived from assignment output change. Its response gains the
handler identifier, a process-start epoch, and the **applied** generation. A slot-state notification
carries §6's states plus §7's switch-completion reports keyed by command identifier, debounced by MH.

**Transmit keys ride in frames, not in signalling.** The publisher region gains a **key-bearing**
flag and, when set, a fixed-size wrapped transmit key — AES-256-GCM under the meeting KEK, nonce
derived from the key id, key id as associated data, carrying the KEK generation — on the first N
frames of every video group and on every audio frame (§4). It is signed; MH forwards it unchanged.
**No key material crosses the MC→MH contract**, and the credential-leak guard covers KEK and
transmit-key material **in logs**.

> **Correction (2026-09-01, ADR-0036 internal-contract reshape).** This sentence
> previously read "...in internal messages and logs." **The "internal messages" half was
> false and is corrected here rather than quietly deleted, because a reader who trusted it
> would have believed a `.proto` file was mechanically scanned for key material when it is
> not — and, worse, would have spent their review attention elsewhere.** Every
> credential- and PII-detecting module in `dt-guard` is extension-scoped to `.rs` or
> `.ts`/`.tsx`/`.svelte` (`rust_secrets.rs`, `rust_log_secrets.rs`, `rust_pii.rs`,
> `instrument_skip_all.rs`, `ts_secrets.rs`, `ts_pii.rs`, `ts_retained_credentials.rs`,
> `ts_metric_naming.rs`); `.proto` appears in that crate only in citation-resolution,
> index-scope and GSA-path code, none of which are scanners. The semantic layer is not a
> fallback either: `scripts/guards/semantic/checks.md` scopes its Credential Leak check to
> Rust and TypeScript in its own preamble.
>
> This is row one of this ADR's own taxonomy below — *a control can be alive and out of
> scope, or in scope and dead; both read as coverage, which is worse than an absent
> control, because an absence gets noticed*. Unlike the `buf breaking` carve-out, there is
> **no loudness leg available** here: there is no scanner to print a warning from, so this
> corrected text is itself the control. A `.proto` scanner is tracked in `docs/TODO.md`
> (owner: security for the check definition, `dt-guard`/infrastructure for
> implementation). Until it exists, the enforcement is that no key-shaped field exists on
> the MC→MH contract and that reintroducing one is a review failure — reviewer-enforced,
> not structural, and named as the weaker form it is.
