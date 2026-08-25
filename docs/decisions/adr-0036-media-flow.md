# ADR-0036: Media Flow Between Participants

**Status**: Proposed.

**Date**: 2026-08-23 (revised 2026-08-24)

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
| Datagram send buffer size | quinn's default is 1 MiB ≈ **140 seconds of queued audio** before the oldest is silently discarded. A realtime path must prefer loss to unbounded latency. Express and document this in **frames of audio**, not bytes. |
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
correction a prerequisite of the first story rather than a later choice.

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
  flags                   independently-decodable; discardable;
                          all other bits: decode REJECTS if set
  payload length          delimits this frame within its group's stream
  stream sequence         end-to-end, per (sender, stream, epoch)
  extensions              optional, type-length-value, publisher-set (§7)

RELAY REGION — excluded from signature and associated data; MH rewrites per subscriber
  stream id               which of the subscriber's slots this frame fills
  hop sequence            per (connection, stream); counts what the transmitter sent

PAYLOAD                   encrypted frame (key id, authentication tag,
                          presentation timestamp all inside)
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
downgrade or exclusion, and that is MC's policy call. And a version change lands on an **epoch
boundary**, because versions may differ in what the signature and associated data cover — which is
convenient, since joining already triggers rotation (§4).

**Two sequence numbers, because they answer different questions.**

The **stream sequence** is end-to-end and publisher-set. It does two jobs: it **is** the AEAD nonce
input (§4 synthesizes the nonce rather than transmitting it), and it provides end-to-end ordering and
loss detection. MH cannot alter it — rewriting it breaks decryption, so the immutability is
cryptographic rather than conventional.

Being the nonce input is a stronger constraint than ordering alone would impose: **a repeat under one
key does not merely expose those two frames, it leaks the authentication subkey and permits forgery.**
The rule is therefore uniqueness per key, and because the key changes each epoch, uniqueness *within*
an epoch suffices — the epoch boundary **permits** a reset rather than requiring one.

**Do not reset it at epoch boundaries; keep counting per (sender, stream).** Epochs change on every
join and leave, so a resetting counter restarts constantly in a churning meeting and every reset is a
discontinuity the receiver must special-case rather than read as loss — which damages the field's
second job. Continuing to count also removes a class of bug, since resetting at the wrong moment
relative to the key swap is exactly how a silent nonce repeat happens. At 32 bits and 50 fps,
exhaustion is years away, and rotate-before-wrap remains a format rule.

It is **counted per (sender, stream)**, not per sender — which requires the key id to identify
**(sender, stream, epoch)** so that each stream has its own derived key and nonce uniqueness still
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

It is counted **per (connection, stream)** rather than per connection, because the useful question is
*which slot degraded* — the input to congestion-withheld slot state (§6) — not merely whether the
link lost packets.

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

Without signatures, attribution rests on symmetric authenticated encryption: every member derives
every other member's media key — that is precisely how anyone can decrypt anyone — so **possession of
the key is authorship**, and any participant can encrypt a frame under another's key that every
receiver accepts and labels with the wrong name. Guests carry a client-supplied display name, so the
insider bar is a meeting link.

Signatures also collapse two other problems:

- **Attribution is independent of key distribution.** Verification against a roster key would work
  identically under any §4 mechanism, so who a frame is attributed to does not depend on how media
  keys are established.
- **MC can no longer mis-attribute.** MC publishes the roster, so a roster-derived mapping would let
  a compromised MC rename the speaker — defeating end-to-end encryption while appearing to satisfy
  it. A forged roster entry does not help when the signature will not verify against a key the client
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

Media is encrypted per frame using the SFrame construction, with two deviations from
`draft-ietf-moq-secure-objects-01`: the relay region is **excluded** from the associated data (MH
rewrites it), and MoQ group and object identifiers are not used — the synthesized nonce is the key
id and stream sequence, and nothing else.

**Media keys are distributed with MLS.** The deciding argument is scale: **meetings of several
hundred participants are a real target**, and the alternative that avoids MLS does not survive it —
see *Alternatives evaluated* below.

### Keys, and who holds what

| Key | Scheme | Lifetime | Held by | Purpose |
|---|---|---|---|---|
| Identity **signing** key | Ed25519 | long-lived, survives reconnect | client generates; public half on the roster, thumbprint AC-attested | signs every frame (§3) — proves a frame came from that participant |
| MLS **node** keys | HPKE | per epoch | client; private halves never leave the device | build the group secret; never touch media directly |
| **Media** key | AES-GCM, symmetric | per epoch, per sender, per stream | **derived locally by every member**, never transmitted | encrypts frames |

The identity signing key and the media keys answer different questions and are deliberately not one
key: **the signing key answers who sent a frame; the media key answers who can read one.** Only the
media key carries a counter hazard, so only it rotates.

**Media keys are derived, not sent.** Each member derives every sender's key from the epoch secret
plus that sender's leaf index — so per-sender keys cost **zero bytes on the wire** and need no
distribution step of their own. The key id encodes **(leaf index, stream, epoch)**, which is what
makes the per-stream sequence in §2 dense and unique per key.

**Every member can derive every other member's media key.** That is inherent to the scheme and it is
exactly why §3's signatures are required for attribution — decryption proves nothing about origin.

### How it works

1. **Before requesting the meeting token**, the client generates its identity signing keypair, and
   AC issues two things over it: a `cnf.jkt` thumbprint claim inside the meeting token, proving the
   join-time holder controls that key; and a **separate peer-presentable identity attestation** over
   (participant, thumbprint, meeting), which is what other participants actually verify. The ordering
   is load-bearing; see the next subsection.
2. The client joins and MC hands it the group's public state.
3. The client admits **itself** with an MLS External Commit — no existing member needs to be online.
4. **MC serialises commits** — one wins per epoch, concurrent commits are rejected as stale — and
   fans the winner out unchanged. Its per-meeting actor already provides that serialisation point.
5. Every member processes the commit, updates its ratchet tree, and derives the new epoch secret.
6. Each member derives its own sender keys and encrypts frames under them.
7. A receiver derives the sender's key from the same epoch secret and that sender's leaf index.
8. Any join or leave produces a new commit, a new epoch, and re-derivation.

**MC's entire role is to serialise and relay opaque bytes.** It derives nothing.

### Why MC cannot read media

Members occupy the leaves of a ratchet tree. Each node has a keypair, and a member knows the private
keys of every node from its own leaf to the root; the root secret is what the epoch secret derives
from. A commit encrypts fresh path secrets to the **public** keys of sibling subtrees, so each member
decrypts exactly what it is entitled to and derives upward.

MC observes those ciphertexts and the public keys. **It holds no private key** — no leaf's, no
interior node's, no root's — and public keys plus ciphertext do not yield the secret.

This is worth contrasting with a scheme that looks similar and is not: distributing per-sender public
keys and having senders wrap a media key such that the **public** half unwraps it. That provides
authenticity, not confidentiality — anything a public key can undo, every holder of that public key
can undo, MC included. Distributing public keys is only safe when they confer the ability to
*encrypt to* a member and nothing else.

### The condition that makes it sufficient — and it is not cryptographic

**MLS alone does not stop MC. MLS plus credential validation does.**

MC controls membership, because it is the delivery service. The real attack is therefore not breaking
encryption but **MC forging an Add for a key it controls**, becoming a member, and legitimately
deriving the group secret.

What prevents it: every member's leaf carries a **credential**, and **clients validate that every
member's credential traces to an AC-issued identity for a real participant.** MC has no AC attestation
for a fabricated member. This is why the join-flow reorder in step 1 exists, and why AC credential
binding is described here as the price of being end-to-end encrypted at all rather than as an MLS
detail.

**The credential is not the meeting token, and this distinction is easy to get wrong.** A meeting
token binds *A's own* thumbprint into *A's own* token — but for B to validate A's credential, B must
verify **AC's signature over A's identity-to-key binding**, and **B never holds A's token.** Using the
meeting token as the credential would mean distributing it, which fails three ways at once: it is a
short-lived **bearer** credential that MC uses to admit you, so handing it to every peer through MC is
a credential leak; its expiry is far shorter than the membership it must vouch for, so credentials
churn or go stale; and it carries display-name PII to parties that do not need it.

So AC mints **two distinct artifacts** over the same key:

| Artifact | Audience | Nature | Lifetime |
|---|---|---|---|
| `cnf.jkt` claim in the meeting token | MC and MH, at admission | part of a **bearer** credential | the token's, short |
| **Identity attestation** over (participant, thumbprint, meeting) | every peer, carried in the MLS leaf credential | **non-bearer**, safe to distribute | matched to membership, not to the join token |

Both verify against AC's existing JWKS. The thumbprint is an RFC 7638 JWK thumbprint carried as
`cnf.jkt`; the client SDK already computes exactly that canonicalisation over the same key shape AC's
JWKS emits, so the cross-language agreement exists rather than needing to be established.

Binding a thumbprint rather than a raw key is deliberate: it forces the verifier to obtain the actual
public key from the roster and check that it hashes to the attested value, which is the property
being sought.

**Identity keys are scoped to a meeting**, not to a participant across meetings. §3's "long-lived"
means *surviving reconnect within a membership*, not persisting across meetings — a key reused across
meetings would make a participant linkable by their public key regardless of display name, which
matters most for the guests who have the least identity assurance to begin with.

**AC does not track key continuity, and should not.** It is stateless here: on token refresh the
client re-presents the same thumbprint and AC re-stamps it, with no rebinding step and no AC-side
state. Nothing at AC prevents a client presenting a *different* key at the next refresh. **Continuity
of membership is MLS's property, not the token's** — this is correct, and it is stated because the
token must not be mistaken for the thing that guarantees a stable identity across a meeting.

**Guests receive an attested pseudonym, not an attested identity.** Guest tokens stamp a
client-supplied display name with no lookup, so the attestation proves *"every frame came from the
same keyholder"* and never *"who that keyholder is."* This is worth having — it stops one guest
forging another guest's frames — but it must never be described as identity assurance for guests, and
it leaves the insider bar at "a meeting link" exactly where §3 puts it.

**Where a client-verified binding and MC's roster disagree, the client-verified binding wins.**

Skip this validation and the result is worse than no encryption, because it *looks* end-to-end: MLS
ships, MC adds itself, and the property everyone believes they have is absent with nothing reporting
it.

MLS does close the adjacent doors on its own. Commits are signed by their committer, so MC cannot
forge one attributed to a member; and confirmation tags over the transcript mean MC cannot hand
different members divergent group states without detection.

### What MC can still do

**Deny service** — drop, refuse, or partition delivery. Nothing here prevents that, and nothing
should pretend to.

**Observe membership** — who is in the group and when it changes. That is metadata, not content, and
it is the same class of leak §11 already accepts and bounds on the media path.

### Rotation applies to media keys only

The signing key persists across reconnect (§3).

| Trigger | Rule |
|---|---|
| Participant leaves | **Rotate immediately, never debounced.** A departed member can otherwise still derive current keys. |
| Participant joins | Rotate, so a joiner cannot decrypt media captured before it arrived. MLS gives this free with the Add commit. |
| Counter exhaustion | Rotate before the stream sequence wraps. |
| Reconnect without provable counter continuity | Rotate rather than resume — nonce reuse is far worse than an extra epoch. |
| MH failover or reassignment | **Not a trigger.** MH never held keys; coupling the crypto epoch to transport topology buys nothing. |

Rotation depends on every client learning promptly when participants join and leave. Roster updates
already carry that, and are wanted for other reasons.

**Rate-limit epoch changes per participant and evict flappers.** A client in a reconnect loop forcing
an epoch change per cycle is a cheap denial-of-service against every other client's CPU. That, not
steady-state joins, is the real hazard.

### Nonce-reuse invariants are structural, not comments

**Encrypt once and fan out the ciphertext.** A client encrypting per destination either reuses the
counter — catastrophic for AES-GCM, yielding authentication-key recovery rather than mere
confidentiality loss — or burns two counters per frame. §5's stream model is what makes this hold by
construction.

**Rotate on reconnect rather than resuming counters**, and **rotate before exhaustion.**

### Key delivery is pluggable

The delivery seam abstracts **transport as well as algorithm**, so an external key service can be
added without redesign. The goal is that our servers cannot access decrypted media **by design rather
than by implementation**, which means such a service sits in a **different trust domain** — separate
operator, separate credentials, ideally customer-operable.

Under MLS, MC is already only an ordered relay of opaque bytes, so it holds no material today. What
must not be baked in is the assumption that MC *is* the delivery path.

### Alternatives evaluated

**Pairwise sender-key sealing — ruled out on mechanism, not on a number.** Each sender seals its media
key to every peer's public key. A rotation costs each sender N−1 seals, so across N senders one
membership change is N(N−1) seal operations and as many distinct blobs to relay — roughly 250,000
operations at 500 participants, on *every* join and leave. MLS's ratchet tree makes the same change
**one commit**, broadcast unchanged, with O(log N) work per member. The gap is not a constant factor.
Pairwise remains genuinely end-to-end encrypted and is cheaper below roughly fifty participants; it
simply does not reach the target.

**Development-only provider — a test double, never shipped to a real environment.** It loses
confidentiality against us — an MC compromise, memory dump, insider, or subpoena yields plaintext —
and **MC could author media as any participant**, which is strictly larger than §3's residual gap and
sits with the operator rather than a participant. If it is ever used outside local development it
fails closed, reports `e2ee=disabled` in logs and metrics, is never described as end-to-end
encrypted, and `ARCHITECTURE.md:851-857` plus `adr-0028:134-141` are corrected in the same change.

### Costs accepted

- A **WASM crate** carrying an MLS implementation, and `wasm32-unknown-unknown` as a new target in
  the ADR-0033 validation pipeline — a new language target, not merely a new dependency. Bundle size
  lands on the join critical path and needs a gate.
- An **ADR-0027 amendment**: MLS requires HPKE, which `ring` does not implement, so this approves a
  second crypto stack for the end-to-end path. That is a deliberate decision, not paperwork.
- **Key material transits WASM linear memory** and cannot be marked non-extractable the way a
  WebCrypto handle can. This is a real weakening relative to the pairwise alternative and is accepted
  rather than solved; key material is imported inside the wasm-bindgen boundary rather than passing
  through the JS heap, which is the worse of the two — garbage-collected, never zeroed, reachable by
  any same-realm script injection.
- **Frames in flight across an epoch change.** A receiver may hold epoch N+1 while frames encrypted
  under epoch N are still arriving, so it must retain the previous epoch's keys for a **bounded**
  window. That slightly weakens the forward-secrecy claim and must be bounded and stated rather than
  discovered.

### Deferred, with a trigger

**In-band sender-driven rekeying**, in the manner of SRTP's Encrypted Key Transport: a sender chooses
its own media key and transmits it wrapped under a group key, rather than deriving it. This buys
rotation without an epoch change, at the cost of periodically carrying a wrapped key on the wire —
roughly 48–64 bytes, which is why such schemes transmit it periodically rather than per packet.

Under MLS's derivation this is unnecessary: per-sender keys cost nothing on the wire and epochs
already change on every membership event. **Trigger to revisit**: a need for sender-local rotation
between epochs — recovery from a suspected client compromise, or counter exhaustion, which at a
32-bit sequence and 50 fps is years away.

### Dependency

**auth-controller reviewed this section after the debate closed and corrected it.** The correction is
above: the meeting token cannot be the peer-verified credential. Two further findings from that
review are reflected here and in Open Items.

**The work is larger than "one claim on the existing signing path."** That characterisation holds only
for the AC-internal slice, where stamping an extra field into the claims struct and signing it is
genuinely trivial. End to end it is a **three-edge contract change**, and the largest piece is not
AC's: **the client→GC join endpoint takes no request body today**, so there is no channel for the
client's thumbprint to reach GC at all. That endpoint grows a body, GC passes the field through to AC,
and the deserialise-side claims type gains it for MC and MH to read. Size the story on that, not on
the AC slice.

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
request gains the selector, per-egress-stream behaviours, priority groups, transport mode, and host
mute — plus a **generation** number, monotonic per (meeting, handler) and derived from the assignment
computation's *output change*, never free-running.

MC re-fires on three triggers:

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

**Both seams are first-story work.**

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
- **Flat prohibition: no meeting identifier on any metric anywhere in this design.** Stated as a rule
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
does — and the adopted roster field name is exactly such a spelling. A guard reporting clean while
the offending line ships reads as coverage. The directory-scoped deny catches by *shape*.

**Admission control is keyed on egress bandwidth, not connection count.** A stream-count value is
advertised to GC as capacity and enforced nowhere; the only enforced limit is a connection count two
orders of magnitude off the real constraint. The capacity figure must be **the same configuration
value that enforcement reads**, published as a gauge, or the alert threshold and the enforcement
drift silently.

**Derive rather than guard wherever two values encode one relationship.** The drain window is derived
from the termination grace period rather than validated against it; the capacity gauge publishes the
value admission control reads. A derived value cannot drift; a guard only catches drift after someone
introduces it. Startup validation is the fallback where derivation is impossible — as with the
cadence and provisional-timeout coupling in §8.

**Silent failure modes are counted on both ends** — datagram send drop, stream-credit stall, and
partial-frame discard on MH; the sender-side equivalents in the SDK. **The client-side drop matters
most: it occurs in the sender and MH structurally cannot observe it.** WebTransport exposes no
send-side drop event, so the SDK keeps the transport queue shallow, owns a bounded queue above it,
makes the drop decision there, and counts it — **making the drop observable by construction**. This
also closes the mirror of the oversized transport buffer in §1.

**Ingress denial-of-service caps**: maximum frame payload before any allocation; per-connection
concurrent-stream and creation-rate limits enforced before per-frame work; a bounded datagram queue
with drop-oldest; a server-side slot cap.

**MH sheds media sessions on restart.** Shutdown marks not-ready, cancels, and sleeps two seconds
inside a thirty-five second grace period — there is no drain phase. Recovery is §8's re-assert.
Resumes must be **jittered on both MC and client sides**, or recovery after a congestion episode, an
MH restart, or a rollout produces a synchronised keyframe storm — keyframes being an order of
magnitude larger than delta frames, from every publisher at once, into a link that just proved
constrained.

**Runbooks and the alerts citing them land in the same story**, since alert validation requires every
runbook reference to resolve to a file. Eight media scenarios are needed — restart media-dark,
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

This debate collected six instances across five specialists — five found, one pre-empted — and four
failed the *applies* half, which is the half nobody tests unprompted:

| Instance | Failure |
|---|---|
| A sensitive-token vocabulary matching no realistic spelling of its target | alive, never applies |
| Alert selectors matching no real pod — several alerts had never fired in the product's life | alive, never applied |
| A reconnect counter for a failure undetectable as configured | applies, never fires |
| A coverage test referencing a never-emitted metric, passing on absence-of-absence | applies, fires for the wrong reason |
| A generation gauge fed by the received rather than the applied value | applies, never fires when it should |
| A per-frame tracing feature guarded by CI rather than by compilation | could be out of scope entirely |

**Forward checklist — every gate this story adds answers both halves explicitly, at plan approval.**
Six historical instances explain the past and prevent nothing; the same material applied to the gates
being written now stops the seventh.

| | Does it fire? | Does it apply? |
|---|---|---|
| Ask | inject the adverse condition — an apply failure for the generation acknowledgement, a stalled subscriber for back-pressure, a release build with the tracing feature returning non-zero | confirm the premise against the real artifact — the release profile governs the deployed images, the denied directory *is* the hot path, the selector matches a real container, the metric is actually emitted |

**This needs no new process**: ADR-0031 already requires plans touching metrics or alerts to carry a
structured block reviewed at plan approval by the cross-cutting reviewers. This is a second table in
that same block.

**A corollary, since two of this debate's three design improvements came from it: when a metric is
hard to make meaningful, that is evidence about the design, not just about the metric.** The
objection *"a reconnect counter is meaningless if its failure is undetectable"* was not answered by
adding keepalive — the control plane changed so the failure mode ceased to exist. The question *"can
we count a send-side datagram drop?"* was not answered by finding a counter — the SDK took ownership
of the queue. That is how instrumentation pays for itself rather than taxing, and it is the concrete
answer to deferring performance work.

---

## Implementation guidance

**Pre-story experiments. These gate transport choice and the wire format, and must run before the
header is frozen in code.**

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
   another's key id and asserts the receiver attributes it to the wrong participant. **Under §3's
   signatures this must now fail** — it is the test that proves the signature layer is load-bearing
   rather than decorative.

**Sequencing.** Test vectors and the header precede the forward path. The transport seam precedes the
ingress and egress loops — cheap now, expensive against concrete types. The key-provider seam
precedes any key work. **The roster field rename and the guard's consumption of the segment matcher
land in one commit**: the rename is safe only *because* the cardinality rule consumes that matcher,
and if the rename lands while the guard work slips, it routes around the guard with a name that looks
deliberate.

**Dependencies**: auth-controller sign-off (§4) before the story is written.

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
- MLS costs a WASM crate, a new compilation target in the validation pipeline, an ADR-0027 amendment
  for HPKE, and key material transiting WASM linear memory where a pure-WebCrypto alternative could
  have kept it in a non-extractable handle (§4).
- Multi-handler send costs N× client uplink and can reduce aggregate goodput on a constrained link.

### Open items — carried deliberately, not resolved
- **The egress budget has no measurement behind it.** It is better shaped than the stream-count value
  it replaces — it names the quantity that binds, admission control reads it, and the gauge publishes
  it so the alert denominator cannot drift — but there is no production deployment to measure. The
  default **fails closed, deliberately low**, chosen by the asymmetry of the error: under-estimating
  costs money linearly and visibly, over-estimating saturates the interface, degrades every stream at
  once, and fires the alert at a percentage of a fiction. MH logs the derived subscriber ceiling
  loudly at startup. Operations owns a measured figure as a dated node-pool sizing follow-up. **The
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
| `ARCHITECTURE.md:851-857` | Key distribution per the §4 branch chosen |
| ADR-0028, "bidirectional datagrams" | Not a coherent construct; superseded by §1 |
| ADR-0011, handler jitter objective | **Unmeasurable** — MH forwards and does not buffer; perceived jitter is a client-side jitter-buffer property and jitter-buffer design is out of scope. Struck, with a client-side successor deferred to the resilience story |
| ADR-0011, handler forwarding latency objective | No defined measurement point; redefined as ingress-read-complete → egress-enqueued |
| ADR-0012:388-389 | Thresholds already mandated but unimplementable; the bandwidth indicator in §11 makes them real |
| ADR-0027 | Approve a second crypto stack for the end-to-end path — MLS requires HPKE, which `ring` does not implement |
| `docs/observability/slos.md` | Referenced by ADR-0011 and does not exist; created |
| `docs/PROJECT_STATUS.md` | Badly stale — lists MH as a skeleton and GC/MC as planned; corrected |

### Explicitly out of scope
Simulcast and scalable coding; quality adaptation; forward error correction, retransmission, and
jitter-buffer design; handler-to-handler cascade; recording and transcription; screen share as a
shipped feature; codec negotiation beyond a v1 default; asymmetric per-edge permissions.

**Flagged for separate audit, not this story**: a participant-level registration RPC and its
connection-token response field may be dead in the same way a previously removed token pattern was.

---

## Participants

| Specialist | Final | Contribution |
|---|---|---|
| observability | 97 | The zero-allocation invariant, the cardinality budget, and the coverage-demonstrated principle; disproved a guard it had itself advocated |
| media-handler | 97 | Buffer-sharing fan-out; behaviours-not-types survived contact with the control plane; conceded the control-plane shape when its premises reversed |
| test | 96 | Three-tier gating, latency observed-never-gated, the control-plane gates, both seams as first-story work |
| meeting-controller | 96 | The control plane, multi-handler assignment as a clusterless pure function, distinguishable slot states |
| client | 96 | The frame format, a worker-isolated receive pipeline, three-branch sizing; browser APIs verified rather than asserted |
| protocol | 95 | The header, the identifier model, and the complete wire draft |
| operations | 94 | Six numbered requirements; found the restart blackhole, the transport defaults, and the dead alert selectors |
| security | 90 | Overturned the attribution claim; the three-way key-distribution analysis with sizing; the telemetry conditions |
| auth-controller | post-debate review | Found that the meeting token cannot be the peer-verified credential; identified the missing client→GC request body; established that no signing-key revocation path exists |

Security scored 90 conditional on the key-distribution choice landing on a genuinely end-to-end
option, and 62 had a provider that lets MC decrypt been shipped to a real environment — a decision
that under ADR-0024 §5.7 would have required explicit user risk acceptance rather than majority
override. **The user selected MLS**, so the conditional resolves at 90.

**Notable self-corrections**, recorded because they are why this converged rather than being
negotiated: security withdrew its own rotation-debounce recommendation and reversed its dismissal of
pairwise sealing, which became its second choice; media-handler retracted a proposal to defer multi-handler
support, then conceded the control-plane shape; operations withdrew a field, later reinstated it for
a case it had missed, corrected its own runbook claim, retracted most of its own requirement when the
design changed beneath it, and corrected its own escalation as a stale read; observability revised
its attribution mechanism and disproved a guard it had spent two rounds advocating; client conceded
the timestamp argument and corrected its own cost claim as an overreach; the control-plane shape
moved four times, each move adding a real constraint. The team lead was wrong three times —
asserting the attribution property was unforgeable, asserting the codec avoided copying, and
understating the audio signature overhead by half — and all three are corrected here.

---

## Appendix — wire specification

Protocol holds the full text; this is the shape.

**Frame header** — a binary codec in `media-protocol`, not protobuf. Version 2. Layout and the
publisher/relay boundary per §2. The signature (§3) trails the payload and covers the publisher
region and payload; the payload extent comes from the header's length field (§2), which is what
delimits frames sharing a group's stream.
Flags decode rejects any bit beyond those defined. A maximum-payload constant is enforced
pre-allocation in both languages. Decode slices rather than copies.

**Signalling contract** — delete the layout messages, the duplicate media-type enums, and the
single-key encryption message. Add: a receive-capability declaration with per-slot media kind and
optional pin; a slot-state enum with the seven states of §6; a send directive listing streams to
produce, each with encoding parameters and a target set carrying transport mode; a stream assignment
carrying participant identity, media kind, handler address, and slot state; and an opaque
key-distribution message pair. The roster gains, per participant, an AC-attested identity public key
— client-validated, never trusted from MC — and the media key identifiers in use, with a bounded
retention window for overlapping epochs.

**Internal contract** — delete the routing RPC and its transcode and mix options, which describe a
transcoding-mixer relay that cannot exist under end-to-end encryption and is called from nowhere.
Registration survives and *is* the control plane (§8): it gains the selector, selection rules, and
per-egress-stream behaviours carrying priority group, supersede-on-independent-frame, transport mode,
and server mute — plus a generation derived from assignment output change. Its response gains the
handler identifier, a process-start epoch, and the **applied** generation. A slot-state notification
carries §6's states plus §7's switch-completion reports keyed by command identifier, debounced by MH.

**The key carrier is opaque to MC.** The key-distribution message pair carries MLS handshake bytes
that MC relays without interpreting, which is what keeps MC an ordered relay rather than a
participant (§4).
