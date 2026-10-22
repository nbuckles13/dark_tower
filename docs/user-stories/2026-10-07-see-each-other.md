# User Story: See Each Other

**Date**: 2026-10-07
**Status**: Ready
**Participants**: auth-controller, global-controller, meeting-controller, media-handler, client, database, protocol, infrastructure, security, test, observability, operations

## Story

As a **participant**, I want **to see up to M other participants' video alongside hearing up to N of them — where M video and N audio receive slots are independently configured, any participant may publish audio, video, both or neither, and each participant's audio and video can be muted independently by themselves or by the host** so that **the ADR-0036 media path carries video — on-demand keyframes, per-slot decode pipelines, and explicit avatar-versus-video slot state — with the same enforcement and key hygiene story 2 proved for audio**.

**Governing design**: `docs/decisions/adr-0036-media-flow.md` (Accepted). This is story 3 of the ADR-0036 arc (*Addendum — Planned Story Arc*). Builds on story 2 (`docs/user-stories/2026-09-21-hear-each-other.md`): join-order slot fill, slot stability (R-4), server mute at MH ingress (R-9), KEK rotation on leave, multi-handler visibility (R-33). Dynamic (active-speaker) selection stays in story 5; handler-restart re-assert stays in story 4.

### Scope decisions (user, 2026-10-07, before planning)

These were settled in discussion with the user and are carried into planning, not re-litigated.

**Slots**
- N audio and M video receive slots are independent; either may be 0. A client may publish audio, video, both or neither, and receive either or both. A client that neither publishes nor receives is refused at join. What a client publishes is fixed at join (camera off is mute, not a capability change).
- Fill rule per slot of a kind: (1) the earliest joiner who already holds a slot of the other kind but not this one (completes a pair); else (2) the earliest eligible joiner holding no slot of this kind. **Stability beats pairing**: a join or leave never moves anyone else (story 2 R-4 extended). Pairing is by publisher — the client associates a participant's audio and video by sender id — not by slot index.
- Audio slots take audio publishers only; a participant who does not publish audio has no audio representation. Video slots take any participant; one who does not publish video occupies the slot with a new **"no video"** slot state, rendered as a placeholder/avatar.
- Accepted consequence: under join-order fill a no-camera participant can hold a video slot while a later participant's camera goes unseen. Story 5 may rank real video above placeholders.

**Send directives and keyframes**
- MC requests video only from watched publishers, and withdraws the directive (empty target set) when the last watcher leaves. A start always begins with a keyframe.
- **No keyframe interval.** Keyframes are produced on demand, requested by MH directly on the client's existing authenticated WebTransport connection, repeated until one arrives.
- **GoP counter**: sender-domain, in the clear publisher region, 32-bit, increments on every keyframe including the one that starts a resume, never resets while the sender id lives (may restart on story 2 R-16 sender-id reissue). A request carries the newest counter the requester has seen; the sender produces a keyframe only if the request's counter is at least its current counter, so stale or back-to-back requests yield one keyframe. MH can request only after it has received at least one frame from that source; until a group start arrives MH holds the subscriber and never forwards mid-group. A requested-but-never-arriving source needs an explicit pending slot state.
- Client-side bounds: a configurable minimum interval between request-driven keyframes; requests accepted only for streams the client is currently sending to that handler.

**Keys**
- Video transmit keys rotate every T like audio (the analogue of `audioRotationPeriodMs`) and at group start, the new key carried in-band. ADR §4's key-bearing rule becomes "the first N frames after any key change", so neither T nor a leave-triggered KEK rotation forces a keyframe.

**Mute**
- Four independent controls: client audio, client video, server audio, server video. Server mute is host-only (story 2 R-8).
- Server mute: MC withdraws that stream's send directive (the client stops capture, so the camera light goes off) **and** MH keeps dropping at ingress as the backstop against a patched client.
- Client mute: MC may keep the directive active if that is simpler. Nothing is ever sent while muted — no silence frames, no DTX, no video.
- Server unmute restores capture unless the participant is client-muted (the two compose independently). The participant's UI notifies them when a server unmute resumes capture.
- Devices: any video mute (client or server) stops the camera track. Audio client mute keeps the mic track open and only stops sending.
- MH learns client mute only through MC policy (single source of truth), alongside server mute in the registration snapshot. MH treats a muted source as **paused** (a behaviour; MH stays type-blind): a switch to a paused source completes immediately as "forward nothing", cancelling any pending keyframe wait and stopping forwarding of the previous source.

**Rendering and client pipeline**
- MC slot state is authoritative for avatar versus video. Clients decode all video they receive but render from MC state: each slot's latest decoded frame is tagged with its source (key-id sender), so video appears the moment MC's update names that source. This closes the race where unmuted video arrives before MC's update.
- Clients have M decode pipelines (jitter buffer + decoder), each tied to a video receive slot. A new source on a slot does not create a new decoder; its frames flow through the existing jitter buffer into the existing decoder. Whether pipelines are pre-built or spun up on demand is open for design, but must be robust.
- On a source change the jitter buffer is **not** flushed: the old source's remaining frames play out, then the new source's from its keyframe, with the new source's timestamps re-based onto the current playout timeline so relative delay is unchanged. Catching up a grown buffer is future work.

**Amended 2026-10-21 (user, Design §Wire freeze amendment 4):** "repeated until one arrives" and "MH can request only after it has received at least one frame" above are superseded. The second was a limitation (MH needs the counter), not a rule. Requests are now event-driven and never repeated, and a publisher keys whenever a target has no group in progress.

**Codec**
- VP8, 640×360, 30 fps, ~800 kbps — all config-driven with these defaults.

**ADR-0036 amendments expected**: §1/§7 (keyframe cadence → on-demand requests with GoP counter), §4 (key-bearing rule; video rotation every T), §5 (withdraw directive on server mute; four mute controls), §6 ("no video" slot state).

## Requirements

**Confirmed by the user 2026-10-08** after two review rounds. Synthesized from all 12 specialists' proposals (2026-10-07) and revised with the user's review: over-cap declarations truncate rather than fail; MC slot state is live/avatar and never names the live source; pending is MH-internal; receiver-driven recovery requests; keyframe requests are per stream; loopback self-check and a debug stats overlay as debug options; v3 is the only header version; MH forwards video cut-through; every hop abandons per destination and recovers via the next hop's request; resume jitter moved to story 4.

**Headline: N+1 see and hear each other**
- [ ] R-1: N+1 distinct accounts join one meeting from N+1 browsing contexts; each sees up to M others' video and hears up to N others. N (audio) and M (video) receive slots are configured independently and either may be 0. Each kind has its own server-side cap; a client declaring more slots of a kind than the cap allows is **truncated** to the cap, and each excess slot reports a permanent **unavailable** state on the wire — visible, never a silent participant, and never a join failure. This lets server-side policy limit slots without the client knowing the cap before join (supersedes story 2 R-1's fail-loudly rule). *(client, meeting-controller, infrastructure, operations)*
- [ ] R-2: A participant declares at join what it publishes — audio, video, both or neither — and that set is fixed for the membership, reconnects included. A participant that would neither publish nor receive is refused at join with a generic, counted reason, never admitted to a silent empty meeting. The join carries both the publish set and the initial receive declaration, so the check runs at join; a later redeclaration that would leave the participant neither publishing nor receiving is also refused. A declaration naming a kind this story does not serve (screen share) or an unknown value is rejected, never silently ignored. *(meeting-controller, protocol, client, security)*
- [ ] R-3: Slots of each kind fill per subscriber by: (1) the earliest joiner already holding a slot of the other kind but not this one; else (2) the earliest eligible joiner holding no slot of this kind. Audio slots take audio publishers only; video slots take any participant sharing a connected handler with the subscriber, never the subscriber itself — receive-only participants included, shown with no video (a future receive-only use such as recording that must not occupy slots is handled when it arrives). Story 2's stability rule extends to both kinds: no join, leave, mute or redeclaration moves an occupied slot. Each assignment names the publisher (live video slots excepted — R-5; there the verified key-id sender names it), and the client pairs a participant's audio and video by publisher, not slot index. *(meeting-controller, client)*
- [ ] R-4: Every video slot's state is explicit on the wire and rendered distinctly, never inferred from absent frames. A slot is either **live** (real media should be arriving; MC does not name the source) or shows an **avatar for a named participant** with a reason — no video (publishes none), client-muted, server-muted — or is fewer-sources, unreachable, or unavailable (R-1). A live slot with no decoded frame yet keeps showing what it showed before; no spinner for a steady state. "Pending" (source assigned, no group start yet) is MH-internal, persists with no give-up timeout while the source stays assigned, and is visible as an MH metric only; an MH→MC switch-completion report is deferred to story 5, where dynamic selection needs it. *(meeting-controller, protocol, media-handler, client)*
- [ ] R-5: What a slot shows follows MC's slot state. In a live slot the client renders the decoded video and attributes it to the sender named by the frame's signature-verified key id — MC never names the live source, which avoids a race with the new source's keyframe (and with active-speaker switching later). A video frame whose signature verifies against a rostered sender may be decrypted and decoded whatever the slot's state, so it is ready when the slot turns live; it is never shown in an avatar slot. On live→avatar the slot switches at once and trailing frames are ignored; on avatar→live it keeps the avatar until the first frame decodes. Audio keeps drop-before-open for unassigned senders. Media kind is taken from the slot, never the frame. *(client, security)*
- [ ] R-6: The client runs M video decode pipelines, one per video slot. A new source on a slot reuses that slot's pipeline: no new decoder, no jitter-buffer flush — the old source plays out, then the new source from its keyframe, re-based onto the current playout timeline so relative delay is unchanged. A fault on one slot never stalls or corrupts another slot or audio. Decode resources are bounded by M, never by sender count; decoded video above the configured receive bounds is dropped and counted. *(client, security)*
- [ ] R-7: A debug option, off by default, lets a participant check their own camera and microphone by local loopback — rendered or played straight from the local capture, never through the encoder, packetiser or a handler. Working before join is desirable but not required in this story. *(client)*

**Sending video**
- [ ] R-8: MC directs a publisher's video only while at least one subscriber holds it in a video slot and it is not video-server-muted, and withdraws the directive when the last watcher leaves; audio follows the same watched / not-server-muted rule. Video is VP8 at MC-configured defaults (640×360, 30 fps, ~800 kbps). Every start or resume begins with a keyframe. A client whose browser cannot encode or decode VP8 fails loudly at media start. *(meeting-controller, client, protocol)*

**Keyframes on demand**
- [ ] R-9: There is no keyframe interval. Every video frame carries a 32-bit GoP counter, per (sender, stream), in the signed cleartext publisher region; it increments on every keyframe including a resume and never resets while the sender id lives. *(protocol, client)*
- [ ] R-10: A subscriber's first forwarded frame from any source is the start of a group; a switch from A to B keeps forwarding A until B's group start, and is abandoned (A kept) if B leaves first. MH requests a keyframe over the publisher's existing connection, carrying the newest counter it has seen from that source, at most once per (source stream, counter) and never repeated: requests travel on a reliable stream, and every loss that needs a new keyframe is itself a new event. A source MH has not yet seen needs no request, because the publisher keys for any target with no group in progress (R-12). The publisher produces a keyframe only for a request whose counter is at least its current one, so stale, repeated or multi-handler requests coalesce into one keyframe that reaches every target. *(media-handler, protocol, client; amended 2026-10-21, user)*
- [ ] R-11: A keyframe request names one stream, and only that stream produces a keyframe — leaving room for simulcast, where a request for one encoding must not key every encoding. A publisher honours a keyframe request only for a stream it is currently sending to that handler and never more often than a configured minimum interval; a request can never start capture or cause a muted or undirected stream to send. Refused and malformed requests are counted by reason. No subscriber churn, mute toggling, reconnect loop or compromised handler can push a publisher's keyframe rate above that bound. *(client, security)*
- [ ] R-12: Every hop that cannot keep up with one destination abandons the current group for that destination only — the publisher toward one of its handlers, and MH toward one subscriber — and never stalls the encoder, the source's other destinations, or anyone's audio. Backlog per destination is capped; past the cap the hop resets that destination's group stream (a half-written frame is never left looking complete). A publisher starts a new group with a keyframe whenever a target it is directed to send to has no group in progress — at start, when the target is added, and after abandoning that target — bounded by the minimum keyframe interval (held as the single deferral, never dropped), because the handler may never see a group the browser discarded. MH recovers a subscriber it abandoned by its ordinary keyframe request. One keyframe reaches every target. Repeated recovery on a path whose capacity is below the fixed bitrate is accepted in this story: it is counted and bounded by the keyframe interval. An ended or reset source ends every downstream stream. *(media-handler, client, operations; amended 2026-10-21, user)*
- [ ] R-13: A receiver whose slot loses decodability (failed verify/decrypt, decoder error, local drop, abandoned group) sends its handler one recovery request for that slot per loss event over its existing connection, carrying the sender and GoP counter from the failing frame's own header — the same counter-based request, client→MH instead of MH→client — with no pacing timer or retry; a newer request replaces an older one still queued. MH maps the receiver's slot to the source stream (fields are remapped, not forwarded verbatim) and forwards at most one request per (source stream, counter); duplicates from any receiver, stale, ahead-of-source and wrong-source requests are counted by outcome and never forwarded. A slot that receives nothing from its new source is MH's to recover (R-10/R-12). *(client, media-handler, protocol; amended 2026-10-21, user)*

**Keys**
- [ ] R-14: Video transmit keys rotate every T (configured separately from audio's period), at every group start and on every KEK receipt; the new key rides the first N frames after any key change, and a key-bearing frame is never discardable. Neither T nor a leave-triggered KEK rotation forces a keyframe. A leaver cannot open post-rotation video (client integration test); a replayed earlier video group is rejected, never re-rendered. All story-2 nonce and replay invariants hold per (KEK generation, sender, stream). *(client, security, protocol)*

**Mute — four independent controls**
- [ ] R-15: Client audio, client video, server audio and server video mute are independent and compose. A request about one kind never changes the other kind's state. Server mute and unmute of either kind are host-only, requester taken from the connection, refused generically and counted, rate-bounded (story 2 R-8). Every client renders all four states distinctly, and the host has a mute and unmute affordance per kind per participant. *(meeting-controller, protocol, security, client)*
- [ ] R-16: While a participant's kind is server-muted, its send directive for that kind is withdrawn (capture stops; for video the camera light goes off), and nothing from that participant reaches any subscriber's slot of that kind however the client labels or transports it — the other kind keeps flowing. The mute survives a re-assert, an MH restart and a reconnect within grace. *(meeting-controller, media-handler, security)*
- [ ] R-17: Under client mute nothing of that kind leaves the device — no silence, no DTX, no video. Client video mute stops the camera track; client audio mute keeps the mic open. MH learns client mute only through MC's policy; a switch to a client-muted source completes at once as "forward nothing", cancelling any pending keyframe wait and stopping the previous source. Client mute alone never makes MH drop ingress, so a resume keyframe that beats MC's update still reaches subscribers. Client-mute churn cannot drive unbounded policy re-pushes. *(client, meeting-controller, media-handler, operations)*
- [ ] R-18: A server unmute restores capture unless the participant is client-muted, and the participant is visibly notified at that moment, non-suppressibly, able to client-mute at once. The ADR states plainly that a host action can resume a participant's camera or mic. *(client, security, meeting-controller)*
- [ ] R-19: A declared video publisher whose camera fails or is revoked mid-call presents to others as video-muted, counted at the client — never as a perpetually pending slot. *(client, operations)*

**Handler transport**
- [ ] R-20: One meeting carries datagram audio and stream-per-group video together. MH attributes every ingress frame to exactly one of its sender's MC-declared streams without parsing the payload and forwards it only along that stream's edges; an unmatched frame is dropped and counted; audio never reaches a video slot and video never reaches an audio slot. Video is prioritised below audio by MC's assignment, keeping MH type-blind. The two-ends-agree transport check fails loudly on mismatch per stream. *(media-handler, protocol, meeting-controller)*
- [ ] R-21: MH forwards video cut-through, never waiting for a whole frame: it reads the frame header (maximum payload enforced against the length field before anything is accepted), writes a per-subscriber rewritten header, then passes payload bytes to each subscriber as they arrive. MH holds per incoming stream only a header and the chunk in flight, and per subscriber stream a capped backlog (R-12). A publisher that stalls mid-frame is reset downstream after a configured bound, counted. Per-connection stream creation rate and concurrent streams are limited and counted; the client's stream reader enforces the same maximum payload before allocation; the readers are fuzzed. The backlog cap × streams is validated at startup against the pod memory limit, so exhaustion is a counted reset, never an out-of-memory kill. *(media-handler, client, security, operations)*

**Wire contract**
- [ ] R-22: The frame header moves to v3 (carrying the GoP counter), replacing v2 as a declared intentional wire break; v3 is the only version, and a peer on any other version is refused loudly — no allowlist or floor machinery (no production clients exist). Cross-language vectors are regenerated with tamper rows, the two implementations written from the spec, not each other. VP8 is expressible; every new message and value is covered by the contract gates. *(protocol, test, security)*

**Configuration, capacity, deployment, rollback**
- [ ] R-23: Every new knob — per-kind slot caps, video codec parameters, video rotation period, client minimum keyframe interval, stream and buffer bounds (MH keyframe-request retry, aggregation and recovery-rate keys removed 2026-10-21) — is environment-configured, startup-validated, logged absent-vs-defaulted, in the ConfigMap of every workload that reads it before the image that requires it, and covered by the env-config drift guard. Effective N, M and the caps are readable at runtime. *(operations, infrastructure, meeting-controller, media-handler, client)*
- [ ] R-24: MH's per-stream video cost cannot drift below MC's directed video bitrate plus framing — derived from it or failing validation on drift. The Kind egress budget is re-sized for N audio + M video edges per subscriber, and its live env-test still enforces both sides of the window. *(infrastructure, media-handler, meeting-controller, operations)*
- [ ] R-25: Rolling back any one of MH, MC or the SDK by one version, between builds that both include this story, with the other components and ConfigMaps in place, starts and serves and never degrades audio; video may be absent. This story's own deploy is one coordinated cutover (config → MH → MC → client): a single-component rollback across it cannot hold (v3-only header, R-22; a pre-story MH rejects mixed transports), so rolling back across the cutover is all components together. Each version-skew path is refused and counted where an operator can query it. The deploy order is written in the MH and MC deployment runbooks before the code lands. *(operations, infrastructure)* *(amended in design, confirmed by the user 2026-10-08)*
- [ ] R-26: Setting MC's video slot cap to 0 disables video: every video slot is truncated to unavailable (R-1), no publisher is watched, so no camera is directed to send; audio is unaffected. It applies to declarations after MC picks up the config. The runbook states this lever and its blast radius. *(operations, meeting-controller)*
- [ ] R-27: `dev-web.sh` documents a multi-participant video demo. A build-time-gated synthetic test-pattern camera — derived per participant and changing every frame — serves the demo and the env-tests, is absent from the production bundle (proven by the bundle test), and uses no durable identity. No insecure browser flag appears anywhere. *(infrastructure, client, test, security)*
- [ ] R-28: The browser test substrate (devloop and CI Chromium) is shown to encode and decode VP8; if it cannot, the browser layer fails loudly — it never skips or degrades to audio-only. Env-tests run a config-driven test codec profile (e.g. lower resolution and frame rate than production); cohort size, CPU load and Kind capacity for the video cohort are verified as evidence, not assumed. *(infrastructure, test)*

**Telemetry**
- [ ] R-29: Client and MC media series are split by media kind end to end, proven by reading a video client series back out of Prometheus; MH series stay type-blind (labelled by behaviour, never kind), and the label taxonomy doc is corrected to match. *(observability, client, infrastructure, global-controller)*
- [ ] R-30: Video is observable without identity: frame accounting per kind at the parse boundary; MH stream-path drops by reason including frames from a paused source; keyframe requests sent, honoured and refused by reason; keyframes produced by cause; pending slots as a count and oldest age; client video slots that MC reports live but that show neither decodable video nor an avatar, as time-in-state and a current count (added 2026-10-21); join-to-first-video and assignment-to-first-render latency (open objectives, not gates); encoder, camera and decoder health; directive outcomes including withdrawals. No frame sizes, keyframe timings, GoP values or sender/stream identity appear in logs, labels, spans or the test bus. Every series lands with catalog, dashboard and alert-inventory entries. *(observability, client, media-handler, meeting-controller, security)*

**Operations**
- [ ] R-31: Runbook scenarios exist before their alerts: keyframe storm, stream-credit stall / slow video subscriber, video slot stuck pending; server mute, egress exhaustion and rollout scenarios extended to video (including "camera light still on after server mute" and the mixed-transport rollback hazard); a `client-dev-local` ladder for camera permission and no-video; the send-directive counter's wiring revised so a withdrawal is not read as total media failure. *(operations)*

**Test**
- [ ] R-32: Receive-side verification per video slot at three layers — key-id sender is the source the test expects in that slot from the join order and fill rule (MC does not name live sources, R-5); the frame verifies and decrypts under that sender's roster key; the decoded picture carries that sender's pattern — plus each slot's rendered state, exposed on the test bus and asserted per receiver per sender. The pattern detector shares no code with the generator. Deadline logic runs on an injected clock; no test sleeps on wall time. *(test, client, media-handler)*
- [ ] R-33: Env-test scenarios ship inside the implementing specialists' tasks: see-and-hear by content; publish-capability mixes; video over-subscription; watched-only directives; on-demand keyframe for a late joiner and a never-sending source; server video mute at every receiver incl. a patched client; client mute; rotation on leave with video flowing; two-handler visibility for video; story-2 audio specs green with M=0. None gates on a wall-clock number. *(test, all implementers)*
- [ ] R-34: A manual test plan for the see-each-other demo (real camera and test-pattern; all four mute controls; publish mixes) is written and run before close. *(test)*

**Debug**
- [ ] R-35: A debug statistics overlay, hidden by default, shows per slot and per sent stream: packet and bit rates, jitter-buffer depth, decode and drop counts, keyframes. It is local to the client and never exported to telemetry. *(client)*

**Deferred to a later adaptive-capacity story (user, 2026-10-08)**: MH monitoring its own fullness; per-stream profiles from 90p to 4K under user and org limits; GC placing on live headroom; consider replacing the bitrate budget with a simple client-count resource model. This story's profile-derived stream cost is interim. Recorded in the ADR-0036 addendum by task 1.

**Deferred to a later media-resilience story (user, 2026-10-08)**: a give-up threshold after repeated recovery on one path, with MC notified; a "poor connection" slot state; bitrate adaptation and simulcast as the real fix for paths below the fixed bitrate.

**Documents landing with this story**
- [ ] R-36: ADR-0036 amended: §1/§7 on-demand keyframes and the slow-subscriber/recovery story (cadence text removed); §2 v3 header; §4 key-bearing rule and video rotation; §5 server-mute withdrawal, four controls, host can resume capture; §6 live/avatar slot states (MC does not name the live source), "no video" and "unavailable", truncation of over-cap declarations, pending as MH-internal with the switch-abandonment open item closed, and the video decode-before-live exception; §7 per-kind server-mute property stated in terms of slots; §10 client-side gate for pipeline reuse, no flush and re-basing. The `internal.proto`/`forward.rs` "foreclosed" comments and the label taxonomy are corrected to match. The ADR addendum's story-4 line gains resume/keyframe jitter after a mass reconnect (moved from this story), and its story-5 line gains: a dynamic switch keeps the old source directed until MH confirms the swap landed on the new source's keyframe, which needs the MH→MC switch-completion report deferred from R-4. *(protocol, security, meeting-controller, media-handler, client, test, observability)*

---

## Architecture Validation

**Result**: PASS — all twelve specialists. test initially reported FAIL on test-infrastructure gaps (no video in the SDK, MH or fixtures; no MH→client channel); reclassified as PASS because every gap is in-story implementation on existing seams, and the one candidate architectural gap (MH→client messages) rides the send half of the existing client↔MH bidirectional stream. No new service path, channel or component: keyframe and recovery requests use the existing authenticated client↔MH connection; mute, directives and the per-stream source list ride existing MC↔client signalling and the MC→MH registration RPC.

---

## Design

**Wire freeze.** Messages crossed repeatedly during design; the lead froze the wire on 2026-10-08 (three amendments; the third, user-directed, restored hop-local stream ids). Protocol's FINAL wire spec is authoritative. Key rulings:
- **Stream ids are hop-local** (user, amendment 3; ADR-0036 §2's original relay-region design). Uplink: the publisher stamps the relay `stream_id` with MC's send id (`SendStream.stream_number`; audio 0, video 1). MH attributes each ingress frame by (connection-bound sender, uplink id) against MC's per-stream `source_streams`; no payload parse; unknown id → `unmatched_stream`, declared id on the wrong transport → `transport_mismatch`. Downlink: MH rewrites the id to the receiver's slot id. MH never distinguishes audio from video; each independent stream (simulcast layers later) gets its own id. The label is publisher-set and unauthenticated at MH; R-16 holds in slot terms (a server-muted stream is dropped at ingress, its kind has no live route, receivers decode by slot kind). A signed header stream number was considered and dropped: the publisher sets both label and key id, so signing protected nothing.
- **Slot state is a flat enum:** ACTIVE (video carries no sender; audio names it), AVATAR_NO_VIDEO=8, UNAVAILABLE=9, AVATAR_CLIENT_MUTED=10, AVATAR_SERVER_MUTED=11 (server wins), FEWER_SOURCES. SOURCE_MUTED removed in-story; ZERO_REQUESTED reserved; unreachable stays on `unreachable_sender_ids`.
- **Recovery request** = {slot_id, sender_id, gop_counter}; sender_id is a staleness guard only (`source_mismatch`); counter equal → forward, lower → stale, higher → `counter_ahead`.
- **v3 header:** `version=3 | flags | payload_length | stream_sequence | gop_counter(u32) | …`, 14-byte fixed prefix, only version; key-bearing+discardable rejected. GoP counter increments on every independently-decodable frame (every audio frame); at u32::MAX the stream stops, never wraps.
- **Internal contract:** `SourceStream {sender_id, stream_number, transport_mode, disposition FORWARD|PAUSED|DROP_AT_INGRESS}` replaces `MutedSource`; `EgressStream.transport_mode` removed (derived); per-stream echo `applied_source_streams`.
- **Config single sources:** shared `media-codec-profile` ConfigMap (8 keys since amendment 4: `MEDIA_VIDEO_{CODEC,WIDTH,HEIGHT,FRAME_RATE_HZ,MAX_BITRATE_BPS}`, `MEDIA_AUDIO_{CODEC,MAX_BITRATE_BPS,FRAME_RATE_HZ}`; the minimum keyframe interval moved to MC's ConfigMap as `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`) read by MC and MH; MH derives both stream costs from it (no headroom knob). MC caps `MC_MAX_AUDIO_RECEIVE_SLOTS`=8, `MC_MAX_VIDEO_RECEIVE_SLOTS`=4 stay in MC's own ConfigMap so the R-26 lever rolls MC only.

**Amendment 4 (user, 2026-10-21): event-driven keyframes and recovery.** Raised by the MH-B (task 10) Gate-1 escalation. When a publisher abandons a group toward a handler, a browser RESET discards unsent bytes, so MH may never see the next group. The publisher never self-keyed and nothing retried usefully, so that handler's subscribers froze. Under the frozen rules nothing could recover. This amendment supersedes conflicting text anywhere in this file and in every task prompt.

- **The publisher keys whenever a target has no group in progress.** This covers a directed start, a target added to an active stream, and abandonment of that target. That keyframe goes to every target of the stream, since there is one encoder. It is bounded by max(directed minimum interval, compiled 100 ms floor). If it is too soon, it is held as the single deferral and never dropped. This replaces "the publisher never keys itself on abandonment" (R-12) for the publisher→MH hop. MH→subscriber recovery stays request-driven, because MH always knows its own abandonments. **Whoever decides an abandonment repairs it.** The publisher re-keys only after abandonments it decided itself (its per-target backlog cap). A stream MH refuses or stops (STOP_SENDING, e.g. ingress refusal or the stream-open rate limit) is MH's decision. The publisher does not re-key for it; MH requests when it wants the stream again. Otherwise a refused stream would loop at the minimum interval.
- **MH requests at most once per (source stream, counter), and never retries.**
  - Requests ride a reliable stream, so repeating the same counter achieves nothing.
  - A request that produces no keyframe was refused or held:
    - "not sending": the start will begin with a keyframe;
    - stale: a newer group is in flight;
    - min-interval: deferred, not dropped.
  - A later loss is a new event with a newer counter.
  - MH requests only for a source it has seen, because it needs the counter. An unseen source needs no request, because the publisher keys for every target with no group in progress.
  - The retry loop and `MH_KEYFRAME_REQUEST_RETRY_MS` are removed.
  - MH's pending-age gauge and the Video Slot Stuck Pending alert make a missing event loud instead of masked.
- **MH dedupes receiver recovery requests by counter, not by time.**
  - A request whose counter equals the source's current counter is forwarded once per (source stream, counter).
  - Every other request is not forwarded, and each is counted by outcome:
    - a duplicate from the same or another receiver: `duplicate`;
    - a lower counter: `stale`;
    - a higher counter: `counter_ahead`;
    - the wrong source: `source_mismatch`.
  - Forwarded requests are therefore bounded by groups produced, which the publisher's minimum interval bounds.
  - The 100 ms aggregation window, `MH_KEYFRAME_REQUEST_AGGREGATION_MS`, the per-connection recovery rate limit and `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC` are removed.
  - Junk costs MH a fixed-size parse and a lookup, bounded by QUIC flow control like any other client traffic.
- **Senders replace obsolete requests.** Each sender (MH→publisher, receiver→MH) keeps at most one queued, unsent request per stream or slot. A newer request replaces an older one, so an obsolete request never waits behind a newer one.
- **Receiver recovery is event-driven.** A receiver sends one recovery request per loss event, with no pacing timer and no retry. It takes `sender_id` and `gop_counter` from the failing frame's own header, never from the last verified frame, which after a switch names the previous source. A slot that receives nothing from its new source is MH's to recover (R-10/R-12), not the receiver's.
- **Lead rulings applying this amendment (2026-10-21):**
  - **Loss event:**
    - It is the first failure on a slot at a given (`sender_id`, `gop_counter`). Further failures at that same pair send nothing, so a decoder-error loop can't flood.
    - A stream reset before any header is parsed has no failing frame, so it sends nothing; it is MH's to recover.
    - The request's header values may be unverified, for example after a verify failure. That is bounded because MH forwards only on an exact counter match for the slot's current source, and `source_mismatch` catches the rest. Security confirms this at task 14's Gate 1. The `RecoveryRequest.sender_id` proto comment saying "signature-verified" is corrected in task 10.
  - **MH dedupe:** MH keeps one dedupe record per source stream, shared by its own requests and forwarded receiver requests. A receiver request for a counter MH has already requested counts as `duplicate`. A failed control-stream write is counted, not re-sent. A queued request is discarded on group start, edge removal, PAUSED, DROP_AT_INGRESS or unbind.
  - **Keyframes produced by cause:** `source` takes three values:
    - `request`;
    - `directive_start` (start, resume, target added);
    - `abandonment` (the publisher re-keying after its own abandonment).
    That keeps a lossy-uplink keying loop distinguishable from a request storm.
  - **Unfulfilled-slot series:** names are `dt_client_media_slot_unfulfilled_duration_ms` (histogram) and `dt_client_media_slots_unfulfilled` (gauge, panel only). Histogram buckets extend past 100 s (10× the 10000 ms maximum interval), and every exit from the unfulfilled state is observed, including teardown. The A9 alert `ClientVideoSlotUnfulfilled` anchors to a scenario OPS-B writes in `mc-incident-response.md`, which points to MH 22 for the MH half.
  - **Pre-header refusal:** a stream MH refuses before reading its header gives MH no counter to request with. Only the stream-open rate limit does that. A conforming publisher opens at most 1000 / `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` group streams per second per video stream. MH boot-validates that `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC` is at least 2× that rate, which keeps MH's read of `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (R-23 "MH reads it too" holds with this purpose). So only a non-conforming publisher can be refused before its header. It gets no repair, and the refusal is counted (`stream_rate_limited`). Refusals made after the header is read (attribution, mute) give MH the counter.
  - **MH reads every stream's header before refusing it (user, 2026-10-21).** That includes streams over the stream-open rate limit. MH reads the fixed header prefix, records the source's counter, then stops the stream, counted `stream_rate_limited`. Its ordinary once-per-counter request logic then applies. MH never stops a stream before its counter is read, except for a malformed header (non-conforming publisher, counted, no repair). No stall bound applies before the header completes, because nothing has been forwarded yet. The declared QUIC concurrent-stream limit bounds headerless streams.
  - **The minimum keyframe interval is MC-only config (user, 2026-10-21, option (a)).** `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` leaves the shared `media-codec-profile`, which now has 8 keys, and becomes `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` in MC's own ConfigMap (`infra/services/mc-service/config.env`). Range 100..=10000, default 500. It is still carried on the video directive as `min_keyframe_interval_ms`, and `mc_media_video_keyframe_min_interval_seconds` is unchanged. MH has no use for it. A change to it now rolls MC only. Ownership:
    - **Task 10 (MH-B):** removes MH's ref (mh-0/mh-1), its env-test 01 assertion, and the MH runbook row and comments.
    - **Task 12 (MC-A):** moves and renames the key, paired with infrastructure, as the task that first reads it:
      - `profile.env`, MC `config.env`, and the mc-0/mc-1 refs;
      - env-test 02, `.env.example` and `DEVELOPMENT.md`;
      - the `mc-deployment.md` row;
      - the key name in the `signaling.proto` `min_keyframe_interval_ms` comment.
  - **Accepted residual:** if a directed camera's capture never begins and that isn't reported as a mute or a capture failure (for example a permission prompt left open), the slot stays unfulfilled until A3 or A9 fires. It is loud, not masked; the fix belongs to the client capture-state reporting.
- **Unfulfilled-slot visibility.** The client measures how long a video slot that MC reports ACTIVE shows neither decodable video nor an avatar state: a time-in-state histogram plus a "currently unfulfilled" gauge. It has no identity and no slot ids. The series feeds a dashboard panel, an alert threshold and the R-35 overlay.
- **Removed config keys.** The three MH keys above are removed in one deploy, because no binary reads them (MH-B had not landed). Removal covers `mh-service/config.env`, the mh-0/mh-1 deployment refs, the `profile.env` comment, env-test `01_mh_deployment_config.rs`, `.env.example`, `docs/DEVELOPMENT.md` and the runbook Configuration Reference rows.
- **Fixture (T-A, landed) extended in MH-B (task 10), paired with test:** add a fault that resets the group and keeps running, then self-keys the next group within `min_interval` using the existing deferral, logged as a new `GroupStartCause` (e.g. `TargetAbandoned`). Also correct the stale "while MH retries" wording in the `GroupStartSource::None` doc comment. The env-test for publisher→MH abandonment recovery uses it.
- **Landed text corrected in MH-B (task 10), paired with protocol, infrastructure and operations:**
  - `signaling.proto` (the `min_keyframe_interval_ms` comment, the `KeyframeRequest`/`RecoveryRequest` block);
  - ADR-0036 (§1/§7 on-demand keyframes, every-hop abandonment, the §7 switch-table amendment), as a new dated blockquote that keeps the superseded text visible.

**Requirement amendments made during design** (traceable; text above unchanged unless noted):
- **R-23:** the minimum keyframe interval is MC config (`MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` since amendment 4, formerly the profile key `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`; 100..10000, default 500), carried on the video directive; the client enforces max(directed, compiled 100 ms floor). A client-reported value would let a patched client suppress the keyframe-storm alert. MH no longer reads it (amendment 4).
- **R-25:** *confirmed by the user 2026-10-08* — restated to hold between builds that both include this story; this story's own deploy is one coordinated cutover (config → MH → MC → client) and rolling back across it is all together, because a v3-only header (R-22) and MH's former mixed-transport reject make a single-component rollback across the cutover lose audio. Each skew path is refused and counted.
- **R-20:** attribution is by the hop-local uplink relay id plus the declared transport (amendment 3).

### meeting-controller
MC is audio-only end to end today: it accepts video receive slots and ignores them, and its slot table, render and directive know only stream 0. Story 3 makes every one of them two-kind. The join now carries the publish set (fixed for the membership; a reconnect that changes it is refused) and the initial receive declaration. A participant that would neither publish nor receive is refused, judged on the declaration as sent. Each kind is truncated to its own cap (`MC_MAX_AUDIO_RECEIVE_SLOTS`, `MC_MAX_VIDEO_RECEIVE_SLOTS`; video 0 is the off lever) with excess slots reported UNAVAILABLE. Slots fill per kind in join order, completing a publisher's audio/video pair first, and stability beats pairing: nothing occupied ever moves. Video slots take any participant, so receive-only and audio-only participants show as AVATAR_NO_VIDEO. MC never names a live video source; avatars name the participant with a reason, server-muted winning over client-muted. The MC→MH snapshot gains a per-stream source list (FORWARD / PAUSED / DROP_AT_INGRESS) covering every published stream, and video edges are stream-per-group, supersede-on-keyframe, priority 2 below audio. MC directs VP8 from the shared `media-codec-profile` (which MH also reads to derive its costs) only while a stream is watched and not server-muted. Withdrawal means empty targets, never removal. Mute becomes four independent controls. Server mute of a kind withdraws that directive and drops at MH ingress. Client mute only pauses at MH and never touches the directive, which stays blind to client mute (ADR §5). Two tasks: MC-A (slots, directives, vocabulary), then MC-B (mute controls, plus deleting the superseded proto fields with media-handler).

### media-handler
**Video forwarding.** MH forwards stream-per-group video beside datagram audio in one meeting. Forwarding is cut-through: header parsed and rewritten per subscriber, payload chunks fanned out zero-copy.

**Attribution.** Stream ids are hop-local. Each ingress frame maps to one declared stream by (bound sender, uplink relay `stream_id` = MC's send id), matched against MC's new `source_streams`. The arrival transport is checked as a consistency guard.
- The label is publisher-set and MH does not authenticate it. Per-kind server mute therefore holds in slot terms: a relabelled frame reaches only the other kind's slots.
- Unmatched and wrong-transport frames are dropped and counted. Downlink ids are rewritten to the subscriber's slot.

**Mute.** Server-muted streams are dropped at ingress. Paused (client-muted) streams are never dropped, and a switch to one completes at once.

**Edges.** A pure edge state machine makes every subscriber's first frame a group start. During a switch it keeps A until B's group start, and abandons the switch if B leaves.

**Back-pressure.** A per-destination backlog cap, a publisher stall or a publisher reset makes MH reset that stream (never finish it), and the edge waits again.

**Keyframes** *(amendment 4, 2026-10-21)*. A waiting edge whose source MH has seen requests a keyframe once per (source stream, counter), carrying the newest GoP counter, and never retries. Receiver recovery requests are remapped, guarded by `sender_id` and counter equality, and forwarded once per (source stream, counter); duplicates are counted, not forwarded. Each outbound request queue keeps only the newest request per stream. "Pending" exists only as MH metrics, which make a missing event loud.

**Costs.** Both egress costs are derived from `media-codec-profile` with no headroom knob (interim; adaptive capacity is a future story). Memory is checked at startup against the downward-API limit.

**Telemetry.** All series are type-blind and pod-level.

**Tasks.** Three: 8 (MH-A1: attribution, dispositions, mixed-transport registrations, derived costs, plus a legacy no-`source_streams` path that MC-B deletes); 9 (MH-A2: stream ingress, cut-through, edge state machine, abandonment, stream limits, memory check); 10 (MH-B: keyframe and recovery requests, pending metrics).

### client
The SDK gains video beside story 2's audio, reusing its crypto, KEK holder and slot-edge model.

Publish side:
- Camera or a build-gated test pattern feeds a VP8 encoder with no keyframe interval.
- Each frame is sealed once and fanned out on one unidirectional stream per group per handler. A slow handler's group is reset, and the publisher keys for any target with no group in progress: at start, when the target is added, and after abandonment (amendment 4).
- The GoP counter shares the non-resettable owner with the stream sequence. Video keys rotate on T, at group start and on KEK receipt, riding the first N frames after any change.
- Keyframe requests are honoured only for a directed, unmuted stream sent to that handler, at most once per max(MC interval, 100 ms floor), with one deferred request.
- Any video withdrawal or mute stops the camera; the mic stays open. Uplink frames stamp the relay stream id with MC's send id, fixing a stale slot-id stamp.
- The join carries the fixed publish set and N audio plus M video slots, where 0 is allowed. This migration (C-A1) lands before MC-A with two marked shims that C-A2 deletes.

Receive side:
- A whole-frame stream reader that enforces the payload bound before allocating.
- One receive policy: video verifies, then decodes in any state, and renders only in active slots, attributed to the verified sender.
- M reusable pipelines switch source without a flush.
- Undecodable slots send one recovery request per loss event, taking the sender and counter from the failing frame; unfulfilled live slots are measured (amendment 4).

Web app:
- A paired video grid with four mute controls, host per-kind controls and a non-suppressible server-unmute notice.
- A local-only self-check and stats overlay (C-C2, off the critical path).
- A three-layer evidence bus and the client-owned browser env-tests.

### global-controller
No GC implementation is needed: slot counts, publish sets, keyframe requests and mute all bypass GC, and MH's `max_streams` ceiling is already derived against the video cost, so placement and `assign_meeting` are unchanged (interface checks confirmed). GC's only touch point is R-29: `media_kind` joins `MEDIA_DATAPOINT_EXTRA` in `crates/gc-service/src/services/telemetry_filter.rs` inside OBS-A (task 2), with global-controller paired as reviewer on that edit.

### auth-controller
Nothing needed from auth-controller. Interface checks were confirmed against code for R-15, R-1/R-33 and R-28. R-15's host-only per-kind server mute reuses the meeting token's `MeetingRole::Host`, which MC derives once at accept; guests are pinned to "guest". R-1/R-33 use existing AC registration. R-28's cohort evidence must stay under Kind's AC limit of 100 user registrations per minute (`infra/services/ac-service/config.env`), so video cohorts reuse the shared once-per-run registration rather than registering per scenario.

### Database Changes
N/A. This story needs no schema, migration, index or query work: the per-kind slot caps are MC environment config, and publish sets, the four mute states, GoP counters and video keys live only in MC/MH memory or inside frames (§4 forbids persisting key material). The only touched column, `media_handlers.max_streams`, keeps its INTEGER type and meaning (MH binds a lower derived ceiling once video cost follows MC's directed bitrate, R-24), so rollback has no schema step.

### Protocol Changes
Story 3's wire changes land additively in one task (P1), and the frame header v3 flips in a second (P2). The client↔MC signalling gains:
- a publish set and the initial receive declaration on the join;
- per-kind caps on the join response;
- `CODEC_VP8` and a required `min_keyframe_interval_ms` on video encoding parameters;
- per-kind mute fields that become `optional bool`, so a request about one kind never touches the other;
- a flat slot-state enum: `AVATAR_NO_VIDEO`, `UNAVAILABLE`, `AVATAR_CLIENT_MUTED`, `AVATAR_SERVER_MUTED`.

A live video slot names no sender. `SOURCE_MUTED` is superseded and `ZERO_REQUESTED` reserved. The flat enum makes "live plus a reason" unrepresentable and maps one-to-one onto telemetry.

The existing MH bidi stream now carries messages in both directions: `MhServerMessage.KeyframeRequest{stream_number, gop_counter}` from MH, and `RecoveryRequest{slot_id, sender_id, gop_counter}` from the client. In the recovery request, `sender_id` is only a staleness guard.

On the internal contract, `source_streams` replaces the sender-only muted set. Each entry is (sender, stream, transport, disposition FORWARD/PAUSED/DROP_AT_INGRESS). The transport echo becomes per stream. An edge's transport derives from its candidate's declared stream. Superseded fields are marked, and MC-B deletes them.

Stream ids in the relay region are hop-local:
- On the uplink the publisher stamps MC's send id. MH looks up (bound sender, that id) in `source_streams`, never parses the payload, and never distinguishes audio from video.
- On the downlink MH writes the receiver's slot id.
- R-16 holds in slot terms: server-muted streams are dropped at ingress, and receivers decode by slot kind.

Frame header v3 adds only a 32-bit `gop_counter`, which grows the fixed prefix from 10 to 14 bytes. A frame that is both key-bearing and discardable is a decode reject.

v3 is the only version, cut over atomically: the TypeScript codec is written from the ADR §2 table first, then P2 generates the vectors, makes the Rust codec v3-only with the cut-through parser and fuzz targets, flips `PROTOCOL_VERSION`, and deletes every v2 path. P1 also carries the ADR-0036 R-36 amendment bundle and the future adaptive-capacity story line.

---

## Cross-Cutting Requirements

### Security
No security-owned task. Security reviews every task touching the media path at Gate 3 against `design/security-criteria.md`, and authors the security sections of the R-36 ADR amendment in P1.

- **Per-kind server mute is a slot property, not a frame property.** Stream ids are hop-local (ADR §2). The publisher stamps the uplink relay `stream_id` with MC's send id. MH attributes ingress by (connection-bound sender, that id) against MC's `source_streams` and checks the arrival transport for consistency. MH never parses the payload. The label is publisher-set and unauthenticated, and signing it would add nothing, because the publisher also sets the key id. The guarantee (R-16) is that nothing from a sender whose kind is server-muted reaches a slot of that kind: the muted stream is DROP_AT_INGRESS, so a relabelled frame lands only in the other kind's slots, where receivers decode by slot kind. The "FORECLOSED" comments are rewritten to this property.
- **Video decode-before-active (R-5)**: video is verified first. A frame from a rostered sender may be decoded in any slot state but is rendered only in an ACTIVE slot, attributed to the verified key-id sender. Self frames, and video frames from a sender server-muted for video, are dropped (`self_sender`, `sender_server_muted`; the latter is the receiver-side signal that MH's video ingress drop failed). Audio keeps drop-before-open.
- **Keyframe and recovery requests are untrusted.** The publisher bound is max(directed, compiled floor), and a request can never start capture or make a muted or undirected stream send. MH coalesces by counter equality (equal forwards, lower is stale, higher is `counter_ahead`); `sender_id` serves only as a staleness guard.
- **Keys (R-14)**: key-bearing on the first N frames after any key change, never discardable (decode reject). KEK receipt rotates video keys, so the leaver bound W holds for video.
- **Privacy**: the camera stops on any withdrawal or mute. A server unmute that resumes capture always shows a non-suppressible notice, whatever the message order. The test pattern is build-gated and derived from `sender_id`. No GoP value, frame size, timing or stream identity leaves the process.

### Observability
- **Metrics**: Client and MC media series gain `media_kind` {audio, video} end to end (SDK per-kind handles, GC datapoint allowlist, collector `keep_keys` with an exact value rewrite, guard checks), proven by Prometheus read-back. It is the only new pipeline key. Session-level series (KEK, roster, capture source), `mc_media_send_directives_total` (composed per participant, before any stream exists) and unreachable senders stay kind-free. MH stays type-blind and is labelled by behaviour only: group starts and resets, edge pending (count, oldest age, duration, exits), keyframe and recovery requests, paused-source arrivals counted rather than dropped (R-17), and stream-path drop reasons. The client adds keyframe, recovery, abandonment, encoder, capture-failure and render-latency series. MC adds a withdrawals counter and the directed minimum keyframe interval as a gauge.
- **Logs**: No GoP values, frame sizes, keyframe timings or sender/stream identity in any media-path log, label, span or the test bus. MC's control-plane decision logs may carry identity, as today.
- **Traces**: No new span attributes. `media_kind` is datapoint-only and stripped from spans.
- **Dashboards**: mc-media, mh-media and client-media gain video rows. Emitting tasks land the basic panels (the guard requires it); OBS-B owns layout and alert-linked panels.
- **Alerts**: A1–A8, all warnings, with thresholds derived from server-side config gauges and never from client-reported values:
  - A1 keyframe storm
  - A2 fleet backlog resets
  - A3 edge stuck pending
  - A4 client sending while muted
  - A5 server-muted video reaching clients
  - A6 video decode-error ratio
  - A7 missing-key-material re-derived per kind
  - A8 MH transport inconsistency

  MC's echo mismatch keeps paging via `MCMediaGenerationDivergence`. A new directive rule closes the story-2 TODO. Join-to-first-video and slot-active-to-first-render are open SLO objectives, never gates.

### Test
Acceptance framing, which must not be overclaimed: every participant runs the identical TypeScript SFrame stack and MH is keyless, so a self-consistent crypto error is invisible to the multi-party tests. The v3 cross-language vectors (P2) remain the only independent crypto check. No env-test gates on a wall-clock number. All deadline logic runs on an injected clock.

Test infrastructure (T-A) provides:
- Rust video publisher/subscriber fixtures, keyless, with independent label (the uplink relay stream id, which is MC's send id) and transport knobs, a keyframe-request answer, and a fixture-only group cadence that is never a production keyframe interval;
- a pattern detector that shares no code with the generator;
- an independent R-3 slot oracle, needed because MC never names the source of a live video slot;
- cohort extensions;
- a VP8 probe, independent of the SDK's, in Layer 7 (a precondition failure that is never green) and in the CI component tier.

Env-test scenarios (driver, owner, requirements):
- See and hear by content, three layers plus the rendered state: Browser, C-C1. R-1, R-5, R-6, R-32.
- Publish-capability mix: Rust MC-A; Browser C-C1. R-2, R-3, R-4.
- Over-subscription and truncation to unavailable: Rust MC-A; Browser C-C1. R-1, R-4.
- Watched-only directives, camera off when unwatched: Rust MC-A; Browser C-C1. R-8.
- Two-handler video visibility: Rust MC-A. R-3.
- Per-stream attribution, mixed meeting, relabel while muted: Rust MH-A1. R-16, R-20.
- Cut-through forwarding, late subscriber at a group start, stall and abandonment: Rust MH-A2. R-10, R-12, R-21.
- Server video mute including a patched client: Rust MH-A1 and MC-B; Browser C-C1. R-15, R-16, R-18.
- Client mute (PAUSED forwards): Rust MH-A1 and MC-B; Browser C-C1. R-17.
- On-demand keyframe, never-keying source, recovery remap, resync: Rust MH-B. R-10, R-12, R-13.
- Rotation on leave with video flowing: Rust MC-A; Browser C-C1. R-14.
- Story-2 audio specs green with M=0: Browser C-C1. R-33.

Capacity evidence and the manual plan, written and run: T-B. R-28, R-34.

### Deployment
**Infrastructure (Deployment).** Codec parameters get one home: a new generator-only base, `infra/services/media-codec-profile/`. It holds the `MEDIA_*` codec keys covering VP8 640×360 at 30 fps and 800 kbps and the Opus parameters (eight since amendment 4; the 500 ms minimum keyframe interval moved to MC's ConfigMap as `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`). MC reads the profile for send directives; MH derives both per-stream egress costs from it (R-24, ADR-0036 §11 "derive rather than guard"), retiring `MH_STREAM_COST_*`. Content addressing (ADR-0038 §2) rolls MC and MH together on any profile change. For that reason, membership is restricted to values both services must agree on, a rule stated in the profile header. The per-kind slot caps, including the R-26 video-off lever (`MC_MAX_VIDEO_RECEIVE_SLOTS=0`), stay in MC's own ConfigMap so the lever never rolls MH. New MC and MH keys land as required refs before the images that read them (INF-A). Superseded keys are retired over two deploys through `optional: true` refs, with a scheduled deletion. MH learns its memory limit through the downward API, and a new guard fails any `resourceFieldRef` to a limit the container does not declare, since Kubernetes otherwise silently substitutes the node's capacity. `dt-guard env-config` learns to resolve a ref to a generator in a sibling base. Kind runs a lower test profile (320×180, 15 fps, 250 kbps) as one patch on the profile, per R-28; production quality for a demo means removing that one patch and redeploying, with no new mechanism. An env-test fails loudly, naming the patch, if the live profile differs from it. The Kind egress budget is re-sized against MH's published derived ceiling for the largest per-file load plus the retained-edge floor, while the S9 exhaustion test can still exceed it (INF-C). No network-policy, Service or topology change is needed.

### Operations
- **Runbook**:
  - MH gains Scenarios 20–23: Video Keyframe Storm, Slow Video Subscriber (Stream-Credit Stall), Video Slot Stuck Pending, and Client Sending While Client-Muted (a client privacy defect).
  - MC gains Scenario 22, Client Video Decode Failures. Each is the anchor for an observability alert (A1–A6), so OPS-B lands before OBS-B.
  - MH 19, MH 18, MC 21, MC 18, MC 8 and MC 16 extend to video, per kind. `client-dev-local` gains F21 onward for camera and "no video".
  - The deployment runbooks (OPS-A, written before any code) carry:
    - the cutover order config → MH → MC → SDK, with MH before MC so per-stream transport acceptance is live before the first video egress;
    - the per-path skew table;
    - the Mixed-Transport Rollback Hazard subsection;
    - Configuration Reference rows for every new and retired key;
    - a premise sweep of text this story falsifies.
- **Monitoring**:
  - The 24-hour watch list, in rise order: transport mismatch at zero, admission ratio, backlog and memory, pending age, keyframe and recovery rates, mute signals, client decode drops, and a client video series present at all.
  - Ops owns no alert rules.
- **Rollback**:
  - Code and config only, with no schema change.
  - Config rollback is safe on retained hash generations.
  - Rollback across the v2→v3 cutover is all together, because v3 is the only version. Within the v3 era, one component by one version holds (R-25, confirmed 2026-10-08).
  - Retired keys survive one deploy as optional refs.
  - **Lever**: `MC_MAX_VIDEO_RECEIVE_SLOTS=0` turns video off with audio untouched. It costs an MC restart per pod, because the ConfigMap is content-addressed. It is kept out of `media-codec-profile` so it never sheds MH sessions, and it comes before any rollback.

---

## Assumptions

| # | Assumption | Made By | Reason Not Blocked |
|---|-----------|---------|-------------------|
| 1 | Kind runs a lower env-test codec profile (320×180, 15 fps, 250 kbps) as one patch; production quality by editing that patch and redeploying | infrastructure, test, lead | R-28 requires a config-driven test profile; no new deploy knob (prefer existing tooling) |
| 2 | Kind egress ceiling ≈120 edges covers the largest per-file env-test sum (72) plus story 2's retained-edge floor, below S9's 240 | infrastructure, test | Escalate if the window is empty; never shrink grace or serialise |
| 3 | Video slot cap default 4, audio 8 | meeting-controller, infrastructure | Config; demo is P=6/N=5/M=4 = 54 edges |
| 4 | Camera stops on any video withdrawal (unwatched or server-muted), re-acquired on restore without a prompt loop | client, security | Stronger privacy; cost is a getUserMedia round trip when a watcher appears |
| 5 | Server unmute restores the directive regardless of client mute; the client keeps capture off | meeting-controller, security | ADR §5 keeps the directive blind to client mute; R-18 met at the client |
| 6 | The self `ParticipantMuteUpdate` precedes the directive change; client also fails safe (notice on any resume after a recorded server mute) | security, MC, client | R-18 notice cannot be lost to ordering |
| 7 | Reconnect presenting a different publish set is refused and counted | security, MC | Prevents slipping out from under a mute |
| 8 | ~~Recovery aggregation (100 ms) is not bound to the min keyframe interval; MH retry is (default 500)~~ Superseded 2026-10-21 by amendment 4: no aggregation window and no retry; MH dedupes by counter | media-handler | Client defers one in-interval request rather than refusing it |
| 9 | Test's VP8 preflight stays independent of the SDK's probe | test, lead | Same rule as detector vs generator |
| 10 | Audio codec params move into the shared profile; MH derives audio cost too | lead | Closes TODO "MH_STREAM_COST_AUDIO_BPS under-costs audio"; one owner per rule |

## Clarification Questions

| # | Question | Asked By | Status | Answer |
|---|---------|----------|--------|--------|
| 1 | Receiver-driven recovery requests? | client | Answered | Yes; same counter mechanism client→MH, MH remaps and dedupes by counter (R-13, amendment 4) |
| 2 | Local camera/mic check? | client | Answered | Debug loopback option, off by default (R-7) |
| 3 | Operator video-off lever? | operations | Answered | Video slot cap 0 (R-26); over-cap declarations truncate (R-1) |
| 4 | Where is "refused at join" checked? | meeting-controller | Answered | Join carries publish set + receive declaration (R-2) |
| 5 | Receive-only participants in video slots? | meeting-controller | Answered | Yes, as no-video (R-3) |
| 6 | Client needs a pending state? | lead | Answered | No; MH-internal metric; MH→MC switch report deferred to story 5 (R-4) |
| 7 | MC names live video source? | lead | Answered | No; identity from verified key id (R-5) |
| 8 | Resume jitter after mass reconnect? | operations | Answered | Moved to story 4 |
| 9 | Cut-through vs reassembly at MH? | lead | Answered | Cut-through (R-21) |
| 10 | Recovery source after abandonment? | lead | Answered | Next hop's ordinary request; amended 2026-10-21: the publisher keys for any target with no group in progress, including after abandoning it (R-12, amendment 4) |
| 11 | R-25 vs R-22 (rollback across the v3 break) | operations, MH, protocol, client | Answered | Restated: holds between builds that both include this story; the cutover is coordinated (user, 2026-10-08) |
| 12 | Superseded proto field deletion owner | protocol, MC | Answered | MC-B (task 15), in-story (user, 2026-10-08) |

---

## Implementation Plan

Human-readable intent only. Status and devloop output live in the manifest below (ADR-0035 §4). Task ids are the manifest's ids.

| # | Task | Specialist | Dependencies | Covers |
|---|------|-----------|--------------|--------|
| 1 | P1: additive signaling/internal proto changes + ADR-0036 R-36 amendment bundle (normative v3 layout table) | protocol | — | proto, code, unit-tests, docs |
| 2 | OBS-A: `media_kind` split end to end (GC allowlist, collector, guard, client per-kind handles, MC labels) | observability | 1 | code, guard, docs, env-tests |
| 3 | INF-A: `media-codec-profile`, MC caps, MH keys, retirements, downward-API memory-limit guard | infrastructure | — | manifests, guard, docs, deploy |
| 4 | OPS-A: deploy order, v3 cutover and rollback, video-off lever, directive wiring, premise sweep | operations | — | runbooks |
| 5 | C-v3: TS frame codec v3 written from the ADR table, dual-encode until P2 | client | 1 | code, unit-tests |
| 6 | P2: v3 cutover — vectors, Rust codec v3-only, header-prefix parser + fuzz, audio GoP counter, atomic flip, v2 deleted | protocol | 1, 5 | code, unit-tests, fuzz, guards |
| 7 | T-A: video test infrastructure (Rust fixtures with group-start source, pattern detector, slot oracle, cohort, VP8 probe) | test | 1, 3, 6 | test-infrastructure |
| 8 | MH-A1: per-stream attribution, server mute / PAUSED, mixed-transport registrations, derived costs (transitional legacy path deleted by MC-B) | media-handler | 1, 3, 6, 7 | code, unit-tests, integration-tests, env-tests |
| 9 | MH-A2: stream ingress, cut-through forwarding, edge state machine, per-destination abandonment, stream limits, memory startup check | media-handler | 3, 6, 7, 8 | code, unit-tests, integration-tests, env-tests |
| 10 | MH-B: on-demand keyframes (once per counter, no retry), recovery remap and counter dedupe, pending visibility, amendment-4 cleanup of landed text and keys | media-handler | 1, 3, 6, 7, 9 | code, unit-tests, integration-tests, env-tests |
| 11 | C-A1: SDK contract migration — per-kind join and declaration, flat slot states, per-kind mute and directives, send-id stamping, uni-stream seam and MH control channel, slot env-var rename (two shims deleted by C-A2) | client | 2, 3, 6 | code, unit-tests, integration-tests |
| 12 | MC-A: video slots, flat slot-state migration, directives, join/caps, source_streams | meeting-controller | 1, 2, 3, 6, 7, 8, 9, 11 | code, unit-tests, integration-tests, env-tests |
| 13 | C-A2: SDK video publish path (capture and test pattern, VP8, stream-per-group egress, video keys, keyframe requests, keyframe-floor guard); deletes C-A1's shims | client | 11, 12 | code, unit-tests, integration-tests |
| 14 | C-B: SDK video receive path (uni-stream reader, M slot pipelines, live/avatar render, recovery requests) | client | 11 | code, unit-tests, integration-tests |
| 15 | MC-B: four mute controls + superseded proto field and legacy-path deletion | meeting-controller | 7, 8, 9, 10, 11, 12, 13, 14 | code, integration-tests, env-tests, docs |
| 16 | INF-C: video in Kind — test codec profile, egress budget resize, dev-web.sh demo docs | infrastructure | 3, 8, 11, 13 | manifests, script, env-tests |
| 17 | C-C1: the see-each-other grid, mute UI, R-18 notice, evidence bus, client browser env-tests, story-2 reruns | client | 7, 10, 12, 13, 14, 15, 16 | code, unit-tests, env-tests |
| 18 | C-C2: local-only debug options — R-7 loopback self-check, R-35 stats overlay | client | 13, 14 | code, unit-tests |
| 19 | OPS-B: video incident scenarios, extended scenarios, client-dev-local ladder, 24h watch list | operations | 4, 6, 8, 9, 10, 11, 12, 13, 14, 15 | runbooks |
| 20 | OBS-B: video dashboards, alerts A1–A8, catalogs, SLO open objectives | observability | 2, 4, 8, 9, 10, 11, 12, 13, 14, 15, 19 | dashboards, alerts, docs |
| 21 | T-B: capacity evidence + manual test plan, written and run | test | 16, 17, 18, 19, 20 | evidence, manual test plan |

Parallel fronts at start: 1, 3, 4. Gate-1 tier: full for all except 4, 19, 21 (docs/evidence only; reasons in the manifest). Task numbers equal manifest ids. Repo-bound text refers to tasks by code only.

Revised 2026-10-13 after a final review: MH-A, C-A and C-C were each split in two to fit one devloop session; C-A1 lands before MC-A (with two transitional SDK shims) so story-2 browser specs survive MC-A's join and slot-state change; P2 owns the audio GoP counter; uplink hop-sequence loss accounting and the profile-membership guard were dropped (no requirement traced to them).

### Requirements Coverage

| Req | Covered By Tasks |
|-----|-----------------|
| R-1 | 11, 12, 17 |
| R-2 | 1, 11, 12, 17, 19 |
| R-3 | 7, 11, 12, 17 |
| R-4 | 1, 10, 12, 14, 17, 19 |
| R-5 | 1, 14, 17, 21 |
| R-6 | 14 |
| R-7 | 18, 21 |
| R-8 | 1, 12, 13 |
| R-9 | 1, 5, 6, 10, 13 |
| R-10 | 1, 9, 10, 13 |
| R-11 | 1, 10, 13, 19 |
| R-12 | 9, 10, 13, 14, 19 |
| R-13 | 1, 10, 14, 19 |
| R-14 | 1, 5, 6, 13, 14 |
| R-15 | 1, 11, 15, 17, 21 |
| R-16 | 1, 8, 9, 11, 13, 15, 17, 19 |
| R-17 | 1, 8, 9, 11, 13, 15, 17, 19 |
| R-18 | 1, 11, 15, 17, 19 |
| R-19 | 13, 17, 19, 21 |
| R-20 | 1, 6, 8, 9, 11, 12 |
| R-21 | 6, 9, 14, 19 |
| R-22 | 1, 5, 6 |
| R-23 | 3, 4, 8, 9, 10, 11, 12, 13 |
| R-24 | 3, 8, 12, 16 |
| R-25 | 3, 4, 6, 15 |
| R-26 | 3, 4, 12, 19, 21 |
| R-27 | 13, 16, 21 |
| R-28 | 7, 16, 17, 21 |
| R-29 | 2, 11, 12, 13, 14, 17, 20 |
| R-30 | 2, 8, 9, 10, 12, 13, 14, 15, 20 |
| R-31 | 4, 19, 20 |
| R-32 | 7, 14, 17 |
| R-33 | 7, 8, 9, 10, 12, 15, 17 |
| R-34 | 21 |
| R-35 | 18, 21 |
| R-36 | 1, 4, 8, 15 |

### Aspect Coverage

| Aspect | Covered By Tasks | N/A? |
|--------|-----------------|------|
| Code | 1, 2, 5, 6, 8–15, 17, 18 | |
| Database | — | N/A: no schema, migration or query work (database) |
| Tests | 5–18, 21 (env-tests in 2, 8–10, 12, 15, 16, 17) | |
| Observability | 2, 20 (series land in-task in 8–15) | |
| Deployment | 3, 16 | |
| Operations | 4, 19 | |

---

## Task Metadata (dt-story manifest v1)

```yaml
# task-metadata (dt-story manifest v1)
story: 2026-10-07-see-each-other
tasks:
- id: 1
  status: completed
  specialist: protocol
  prompt: |-
    Story 3 protocol contract and ADR-0036 amendment bundle (docs/user-stories/2026-10-07-see-each-other.md; requirements R-2, R-4, R-5, R-8, R-9, R-10, R-11, R-13, R-14, R-15, R-16, R-17, R-18, R-20, R-22 (wire half), R-36). Task 1 (P1), protocol. No dependencies. Every `proto/**` edit is a Guarded Shared Area edit. Pair with security as reviewer of every ADR edit and of the rewritten mute reasoning; with meeting-controller, media-handler and client as consumers; with observability for the slot-state vocabulary telemetry mirrors.

    AUTHORITY. The wire is frozen. The story file's Design §Wire freeze and §Protocol Changes are authoritative; read both first. This prompt adds only what is task-specific: tags, comment content, comment rewrites and the ADR text. If this prompt and the story file disagree, the story file wins; stop and report.

    WHAT THIS TASK IS. Every story-3 wire change lands here ADDITIVELY, so the workspace builds and every existing test stays green; nothing a consumer reads today is removed.
    - Superseded fields stay, marked "SUPERSEDED -- deleted by MC-B". MC-B is task 15; it deletes them in-story from the proto and both consumers (R-25 confirmed 2026-10-08: rollback across the v3 cutover is all components together).
    - In every repo-bound text (proto comments, code comments, ADR, docs) refer to tasks by CODE only, never by number.
    - Frame header v3 is implemented by task 6 (P2), not here. Its NORMATIVE LAYOUT TABLE lands here, in ADR-0036 §2. Task 5 (C-v3, client) writes the TypeScript codec from that table before any v3 vectors exist, and P2's Rust generator is written from the same table (R-22: written from the spec, not from each other). Make §2 complete enough that an implementer needs nothing else.

    SIGNALING (`proto/dark_tower/signaling/v1/signaling.proto`):
    (1) `Codec`: `CODEC_VP8 = 5`. It is the only video codec MC directs in story 3. A client declares it only after `VideoEncoder`/`VideoDecoder.isConfigSupported` both pass.
    (2) `JoinRequest` (R-2): `repeated MediaKind publish_kinds = 8;` and `ReceiveCapability receive_capability = 9;`.
       - Empty publish set: publishes nothing.
       - Rejected and counted, never ignored: UNSPECIFIED, VIDEO_SCREEN, unknown, duplicate.
       - `receive_capability` is the initial declaration; the post-join `ClientMessage.receive_capability` stays the redeclaration path.
       - MC refuses (generic, counted) a join with no publish kinds and no slots in the declaration AS SENT (before truncation), and any redeclaration that would leave the participant neither publishing nor receiving. Truncation never fails a join (R-1).
       - A reconnect presenting a different publish set is refused and counted.
    (3) `JoinResponse`: `optional uint32 max_audio_receive_slots = 13;` and `optional uint32 max_video_receive_slots = 14;`. They exist for runtime readability (R-23) and are informational, since MC truncates. Mark `max_receive_slots = 12` "SUPERSEDED -- deleted by MC-B, then reserved by tag and name".
    (4) `SlotState`: add `SLOT_STATE_AVATAR_NO_VIDEO = 8`, `SLOT_STATE_UNAVAILABLE = 9`, `SLOT_STATE_AVATAR_CLIENT_MUTED = 10`, `SLOT_STATE_AVATAR_SERVER_MUTED = 11`, with semantics per the wire freeze.
       - UNAVAILABLE has no sender and no url, and echoes the declared `slot_id`; a video cap of 0 makes every video slot UNAVAILABLE (R-26).
       - SERVER_MUTED wins over CLIENT_MUTED.
       - The muted avatars apply to both kinds.
       - Telemetry labels follow wire names (`active`, never "live").
       - Mark `SLOT_STATE_SOURCE_MUTED` "SUPERSEDED -- deleted by MC-B, then reserved by number and name", and delete its "no enum value will be added" paragraph (R-4 overrules it).
       - Reserve `SLOT_STATE_ZERO_REQUESTED` now, by number and name: a subscriber declaring zero of a kind owns no slot id of that kind to echo, so it is unemittable. Close the docs/TODO.md entry "`SLOT_STATE_ZERO_REQUESTED` may be unemittable in principle" with a pointer here.
       - `SWITCH_PENDING`, `WITHHELD_BY_CONGESTION` and `SOURCE_UNREACHABLE` are retained but never emitted, and consumers fail closed on them. Say why at each: pending is MH-internal and the MH-to-MC switch report is deferred to story 5; congestion withholding is not built; unreachable stays on `unreachable_sender_ids` (story 2 R-33).
    (5) `StreamAssignment.sender_id` (R-5):
       - Absent on a video `ACTIVE` slot; the client attributes live video from the verified key-id sender.
       - Present on audio `ACTIVE` and on every avatar state.
       - Absent is never coerced to 0.
    (6) `MuteRequest` and `ServerMuteRequest` (R-15): `audio_muted` and `video_muted` become `optional bool` (tag- and wire-compatible).
       - Absent means that kind is unchanged; both absent is rejected and counted.
       - Name the defect being fixed: MC's `ServerMuteAction::from_request` folds both fields into one action.
       - Keep `reason`'s caveats.
    (7) `ParticipantMuteUpdate` (R-18): a participant learns its OWN server-mute state only from its own copy, so MC MUST send that self update before the directive change that restores capture. The client MUST NOT resume capture on a server-unmute path without showing the non-suppressible notice, even if the order is violated (the fail-safe).
    (8) `EncodingParameters`: `optional uint32 min_keyframe_interval_ms = 6;`.
       - Video only, and required on video; the client refuses a video stream without it and enforces max(directed, compiled 100 ms floor). Audio ignores it.
       - Its value is the shared `media-codec-profile` key `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (100..=10000, default 500). MC directs it and publishes `mc_media_video_keyframe_min_interval_seconds`; MH also reads it. It is not an MC-only key.
    (9) `SendDirective`/`SendStream` comments (R-8, R-16, R-17):
       - Direct a kind only while it is watched and not server-muted; a withdrawn stream stays listed with an EMPTY target set; a kind not published is never listed.
       - Server mute withdraws (capture stops, camera light off); client mute MAY stay directed.
       - Streams: video is `stream_number` 1, `MEDIA_KIND_VIDEO_CAMERA`, `CODEC_VP8`, `TRANSPORT_MODE_STREAM_PER_GROUP`; audio stays 0. `stream_number` is also the uplink relay id (14).
       - Fix the header sentence "MC keeps a send directive active while a client reports itself client-muted".
       - `header_version` is v3 only, derived from `PROTOCOL_VERSION`; delete the floor language.
    (10) `ParticipantCapabilities.supported_header_versions`: a join whose list lacks 3 is refused at join, generically and counted at MC. This is the queryable signal for an old SDK meeting a new MC. No allowlist, no floor. Delete the "ENFORCEMENT NOT YET ALLOCATED" paragraph and close the docs/TODO.md entry about the version allowlist and minimum floor that "neither task owns", naming MC-A as owner.
    (11) New `MhServerMessage` (MH to client): `oneof message { KeyframeRequest keyframe_request = 1; }`, plus envelope `trace_parent = 20; trace_state = 21;` (MH sets none).
       - `KeyframeRequest { uint32 stream_number = 1; uint32 gop_counter = 2; }`.
       - It rides the send half of the client-opened MH bidi stream, with the same 4-byte length prefix as `MhClientMessage`, size-bounded. No sender id, strings or URL.
       - State R-10 and R-11's publisher rules at the message:
         - keys only if the counter is at least its current counter, so requests coalesce into one keyframe reaching every target;
         - only the named stream keys;
         - only for a stream it currently sends to that handler, at most once per directed minimum interval;
         - never starts capture or makes a muted or undirected stream send;
         - refusals are counted by reason.
       - State MH's rules: request only after at least one frame from the source, retrying until a group start arrives at `MH_KEYFRAME_REQUEST_RETRY_MS` (default 500, range 100..=10000, must be at least `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`).
    (12) `MhClientMessage`: `RecoveryRequest recovery_request = 2;` with `RecoveryRequest { uint32 slot_id = 1; uint32 sender_id = 2; uint32 gop_counter = 3; }` (R-13).
       - Semantics per the wire freeze: `sender_id` is a staleness guard only (`source_mismatch`); equal counter forwards, lower is `stale`, higher is `counter_ahead`, with no clamping.
       - MH re-sends with its own fields, at most one per source stream per 100 ms by default; a counter of 0 is legal.
       - It is not a token-refresh variant, so leave the read-once pointer at `NotifyParticipantConnectedRequest.participant_id` as is.
    (13) Every new GoP counter, stream number, slot id and sender id field is stream identity: never a log field, metric label, span attribute or test-bus value (R-30). Point at `SubscriberSlot`'s wording rather than restating it.
    (14) Relay region, hop-local ids.
       - Uplink: the publisher stamps MC's send id, through one function for both kinds.
       - Downlink: MH rewrites the relay id to the subscriber's `slot_id`.
       - MH never distinguishes audio from video.
       - Fix every comment that calls the relay `stream_id` merely subscriber-scoped (comment-only, in scope because this task sets the rule): `ReceiveSlot.slot_id`; `SendStream.stream_number`; `crates/media-protocol/src/frame.rs` (module doc and `STREAM_ID_FIELD_BYTES`); the identifier table in `crates/mh-service/src/routing/mod.rs`; the TS frame codec doc if it says so.
       - The SDK stamping fix (`AudioPipeline.ts` ~861 stamps the subscriber's own first slot id) belongs to C-A1.
       - Keep the 16-bit relay id versus 8-bit key-id stream cross-references accurate.

    INTERNAL (`proto/dark_tower/internal/v1/internal.proto`):
    (15) `SourceStream { uint32 sender_id = 1; uint32 stream_number = 2; dark_tower.signaling.v1.TransportMode transport_mode = 3; SourceDisposition disposition = 4; }` on `RegisterMeetingRequest.source_streams = 8`, with `SourceDisposition` UNSPECIFIED 0 / FORWARD 1 / PAUSED 2 / DROP_AT_INGRESS 3 (STANDARD-prefixed names).
       - Scoped per handler, like the old muted set.
       - Behaviour names, so MH learns neither kind nor moderation.
       - PAUSED is client mute (R-17): a switch to a paused source completes at once as forward-nothing, cancelling any keyframe wait; there is no ingress drop.
       - DROP_AT_INGRESS is server mute (R-16) and implies paused; it drops before any forwarding decision, counted `server_muted`.
       - UNSPECIFIED rejects. A disposition change advances `policy_generation`.
       - Immediate hard rejects, before building any table: a duplicate (sender_id, stream_number); a candidate absent from the list; an egress whose candidates declare different modes (`rejected_transport_inconsistent`); a count over `MH_MAX_SOURCE_STREAMS_PER_MEETING`, which replaces `MH_MAX_MUTED_SOURCES_PER_MEETING` (key landed by task 3, INF-A).
       - Reference `SubscriberSlot` for the sender_id rules rather than restating them. `stream_number` is the uplink relay id, with SSoT `SendStream.stream_number`.
       - Plain scalars and enums only, so `validate-internal-proto-no-key-material.sh` stays green.
    (16) At `SourceStream`, state the attribution rule (R-20) per the wire freeze: (connection-bound sender, uplink relay id) is looked up in the list, with no payload parse; drops are `unmatched_stream` and `transport_mismatch`; an edge's transport derives from its candidate. Also record that a signed header copy of the label was considered and dropped, because the publisher sets both label and key id, so nobody re-adds it.
    (17) Rewrite, do not delete, the "PER-STREAM (AND PER-KIND) INGRESS MUTE IS FORECLOSED" reasoning at `MutedSource` and in `crates/mh-service/src/media/forward.rs` (comment-only).
       - New text: the muted party chooses the uplink label and MH does not authenticate it. What it cannot do is make a muted kind reach a slot of that kind. A server-muted stream is DROP_AT_INGRESS, so that kind has no live route. A relabelled frame must match a declared (stream, transport), so it reaches only the other stream's kind of slot, where receivers decode by slot kind.
       - Use the dated-correction convention: say what the old text claimed and why it is superseded.
    (18) `RegisterMeetingResponse`: `repeated SourceStreamEcho applied_source_streams = 6;` with `SourceStreamEcho { uint32 sender_id = 1; uint32 stream_number = 2; dark_tower.signaling.v1.TransportMode transport_mode = 3; }`.
       - Built from the APPLIED snapshot. MC compares per stream, unconditionally; missing or UNSPECIFIED is a mismatch. Carry the existing reasoning.
       - Per stream, not per-mode counts, which read green on a swap.
       - Mark `transport_mode = 5` "SUPERSEDED -- deleted by MC-B".
       - Close TODO §Media Path Obligations item "(e) The single `transport_mode` echo expires at story 3 (video)", noting that MH-A1 retires the story-2 homogeneous-mode reject.
    (19) Mark `EgressStream.transport_mode = 6` "SUPERSEDED -- deleted by MC-B; an edge's mode derives from its candidate's `SourceStream`". Mark `server_muted_sources = 7` and `MutedSource` "SUPERSEDED by `source_streams` -- deleted by MC-B".
    (20) `priority_group`: a lower number is a higher priority; MC assigns audio 1 and video 2 (R-20). `supersede_on_independent_frame` is true for video edges.

    ADR-0036 AMENDMENT BUNDLE (`docs/decisions/adr-0036-media-flow.md`, R-36). Date it 2026-10-13 in the amendment/correction convention, keeping reversed text visible. Refer to tasks by code only. Security reviews all of it; MC, MH, client, test and observability review their sections.
    (a) §1/§7, on-demand keyframes (R-9, R-10, R-12, R-13, R-21).
       - No keyframe interval. MH requests with its newest counter, after one frame, retrying until a group start. Receivers send recovery requests, which MH remaps and aggregates. The publisher keys only for a counter at least its current one, so requests coalesce.
       - Amend away the "one-second keyframe cadence" and "two parameters" text: switch latency is now a round trip plus encode, and stream-open rate follows keyframes actually produced.
       - Every hop abandons per destination (capped backlog, reset past it). Recovery comes only from the next hop's ordinary request.
       - MH forwards video cut-through, and a publisher stalled mid-frame is reset downstream after a bound.
    (b) §2, the normative v3 table, big-endian.
       - Fields in order: version (1, = 3); flags (1: bit0 independently-decodable, bit1 discardable, bit2 key-bearing, others reject); payload_length (4); stream_sequence (4); gop_counter (4, NEW); wrapped transmit key (50, iff key-bearing: kek_generation 2, key 32, tag 16); ext_length (2) and extensions; relay: stream_id (2), hop_sequence (4); payload; Ed25519 signature (64).
       - Fixed prefix 14 bytes (was 10); v3 adds ONLY `gop_counter`. The publisher region (signed, AEAD associated data) is everything before the relay region.
       - `gop_counter`: per (sender, stream); it increments on every independently-decodable frame, so every audio frame, with no per-kind branch. Every frame of a group carries its keyframe's value. It never resets while the sender id lives (it may restart on story 2 R-16 reissue).
       - It never wraps: at u32::MAX the sender stops that stream, counts it, and resumes only under a fresh stream number or sender id.
       - Decode rejects key-bearing plus discardable (`key_bearing_discardable`).
       - Relay ids are hop-local per (14); the uplink id is MH's attribution label (publisher-set, unauthenticated). The uplink `hop_sequence` is carried and unvalidated.
       - v3 is the only version, others are refused (`unknown_version`), and v2 is an intentional wire break with no dual decode. The vectors file wins over prose.
    (c) §4 (R-14):
       - Key-bearing on the first N frames after ANY transmit-key change (every T, group start, KEK receipt), N configured (default 3); a key-bearing frame is never discardable.
       - Video keys rotate every T (separate from audio's), at group start and on KEK receipt. Neither T nor a leave-triggered KEK rotation forces a keyframe.
       - Nonce and replay invariants hold per (KEK generation, sender, stream). Update the cadence rows.
    (d) §5 (R-15 to R-18):
       - Four independent, composing controls. Server mute withdraws the directive, with MH's ingress drop as the backstop; client mute MAY stay directed, and nothing leaves the device (no silence, DTX or video).
       - Video mute stops the camera; audio mutes keep the mic open.
       - State plainly that a host's server unmute can resume a camera or mic unless the participant is client-muted, with a non-suppressible notice.
       - Record (7)'s obligation and fail-safe.
    (e) §6 (R-1, R-4, R-5): the flat state table per the wire freeze.
       - MC never names the live video source. Over-cap declarations truncate to UNAVAILABLE, superseding the reject rule and story 2 R-1.
       - Pending is MH-internal, with no give-up timeout while assigned; this closes the Open Item "The switch-abandonment bound is unspecified".
       - The video decode-before-live exception: a verified video frame may be decoded in any slot state but is rendered only in a live slot. Audio keeps drop-before-open.
    (f) §7 (R-16, R-20): the per-kind server-mute property in slot terms, and the attribution rule of (16) with an unverified, publisher-set label. MH never distinguishes audio from video; every independent stream, simulcast layers later, gets its own hop-local id.
    (g) §10 (R-6): the client gate for per-slot pipeline reuse. No jitter-buffer flush, re-based playout, decode bounded by M.
    (h) Addendum:
       - Story 3's line points at this story file.
       - Story 4 gains resume and keyframe jitter after a mass reconnect.
       - Story 5 gains: "a dynamic switch keeps the old source directed until MH confirms the swap landed on the new source's keyframe, which needs the MH-to-MC switch-completion report deferred from story 3 R-4".
       - New future story "adaptive capacity": MH monitors its own fullness; per-stream profiles from 90p to 4K under user and org limits; GC places on live headroom; consider a client-count resource model (like `MH_MAX_CONNECTIONS`) in place of the bitrate budget, since slot caps bound per-client egress.
       - State that story 3's media-codec-profile cost derivation is interim.
    (i) Appendix: summarise the signalling contract, internal contract and v3 header changes.

    Also update `docs/API_CONTRACTS.md` and `docs/WEBTRANSPORT_FLOW.md`: after `MhConnectRequest` the MH bidi stream carries `RecoveryRequest` from client to MH and `KeyframeRequest` from MH to client, with the same framing. Update `docs/specialist-knowledge/protocol/INDEX.md` pointers. Do not edit `docs/observability/label-taxonomy.md`; task 2 (OBS-A) owns it.

    GATES AND ACCEPTANCE.
    - Regenerate Rust (`crates/proto-gen`) and TypeScript (`packages/proto-gen` and any checked-in SDK bindings).
    - Extend `crates/proto-gen/tests/signaling_roundtrip.rs` and `internal_roundtrip.rs` for every new message, field and value:
      - optional-mute presence versus false;
      - the reserved number not decoding as a named value;
      - both envelopes, `SourceStream`/`SourceDisposition`, the per-stream echo, VP8 and `min_keyframe_interval_ms` presence;
      - a test that pins unknown enum values decoding as open-enum integers, for consumers to reject.
    - `packages/proto-gen/scripts/verify-codegen.sh`: presence of every new name, absence of the reserved ones.
    - No `skip_debug` change, since nothing new is key material.
    - `buf STANDARD` passes, reserving vacated tags and names in the change that vacates them (only ZERO_REQUESTED now).
    - `buf breaking`: declare the expected Layer-6 findings up front per `docs/protocol/CONVENTIONS.md` "Intentional wire breaks". An unpredicted finding is a regression.
    - Acceptance:
      - workspace and both codegen pipelines green, with no consumer edits beyond comments;
      - the three TODO entries closed;
      - every SUPERSEDED marker names MC-B;
      - no repo text names a plan number;
      - security has reviewed the ADR text.
    - No env-tests: every behaviour is cluster-observable only in consumer tasks.

    DOWNSTREAM:
    - Task 5 (C-v3) writes the TS v3 codec from (b).
    - Task 6 (P2) implements v3, generates vectors and flips.
    - These consume the shapes: tasks 8/9 (MH-A1/MH-A2), 10 (MH-B), 11/13 (C-A1/C-A2), 12 (MC-A) and 14 (C-B).
    - Task 15 (MC-B) deletes the superseded fields.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - `SourceStream { sender_id, stream_number, transport_mode, disposition FORWARD|PAUSED|DROP_AT_INGRESS }` is plain scalars plus enums. UNSPECIFIED rejects. `validate-internal-proto-no-key-material.sh` passes unmodified (no `bytes`, no cross-package types).
    - `ServerMuteRequest` and `MuteRequest` per-kind fields are `optional bool`. A request with both absent is rejected. The existing `reason` caveats (truncate before logging, never trusted markup) are kept.
    - `KeyframeRequest { stream_number, gop_counter }` and `RecoveryRequest { slot_id, sender_id, gop_counter }` are fixed scalars only: no strings, no URL, size-bounded on the existing bidi stream with its length prefix. `RecoveryRequest.sender_id` is documented as a staleness guard only, never authority.
    - Join `publish_kinds` and `receive_capability` are closed enums; VIDEO_SCREEN and UNSPECIFIED are rejected. A join whose `supported_header_versions` lacks 3 is refused and counted.
    - The `internal.proto` and `forward.rs` "FORECLOSED" text is rewritten, not deleted, to the slot-terms property: stream ids are hop-local; the uplink relay `stream_id` (MC's send id) is publisher-set and unauthenticated at MH, and signing it would add nothing because the publisher also sets the key id; nothing from a server-muted kind reaches a slot of that kind.
    - R-36 ADR text, security-authored sections:
      - §4: key-bearing on the first N frames after ANY key change; key-bearing frames are never discardable; video rotates every T, at group start and on KEK receipt; the leaver bound W holds for video.
      - §5: a host action can resume a participant's camera or mic, with a non-suppressible notice.
      - §6: video decode-before-active, rendered only in an ACTIVE slot and attributed to the verified key-id sender.
      - §7: per-kind server mute stated in slot terms. Attribution is by (sender, hop-local uplink stream id), and each independent stream, future simulcast layers included, gets its own id. It is never stated as "MH identifies the frame's stream".
    - `key_custody=operator` wording is unchanged. Nothing describes the media path as end-to-end or zero-trust.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-20-story3-protocol-contract
  tag: story-2026-10-07-see-each-other-task-1
- id: 2
  status: completed
  specialist: observability
  deps:
  - 1
  prompt: |-
    Story 3 "see each other", task OBS-A: split the media series by `media_kind` end to end (docs/user-stories/2026-10-07-see-each-other.md, requirements R-29 and R-30 (the per-kind accounting half); governing design docs/decisions/adr-0036-media-flow.md §11). Implementing specialist: observability. Pair with global-controller as reviewer of the `crates/gc-service/src/services/telemetry_filter.rs` edit, infrastructure as reviewer of the `infra/services/otel-collector/collector.yaml` edit, client as reviewer of the `packages/sdk-core` call sites, and meeting-controller as reviewer of the `crates/mc-service` recorder changes. Pair with security as reviewer for the label-admission decision (a new key crossing the browser-to-cluster trust boundary).

    WHY THIS TASK EXISTS. Video lands in this story, and today no client media series and no MC media series distinguishes audio from video. The SDK's media label set is exactly {client_version, org_id, key_custody} (`packages/sdk-core/src/media/setup/mediaMetrics.ts:mediaMetricLabels`), so the moment video flows, video frames, drops, decoder errors and mute transitions sum silently into the audio series and every existing ratio alert (e.g. `MCMediaMissingKeyMaterial`) mixes two frame rates. This task owns the consistency rule "every client and MC media series whose subject has a media kind carries `media_kind`, and no MH series ever does", and it owns every instance of that rule that exists today. Later tasks (MC-A, MC-B, C-A1, C-A2, C-B, MH-A1, MH-A2, MH-B) only add series on the machinery this task builds; they never re-decide the rule.

    DEPENDENCIES. Task 1 (P1, protocol: additive signaling/internal proto changes). From P1 this task needs the per-kind mute fields on `MuteRequest` and `ServerMuteRequest` becoming `optional bool` (tag-preserving; both absent rejects), so MC can tell which kind a mute request names. Nothing else from P1 is consumed. Tasks 11 (C-A1), 12 (MC-A) and 13 (C-A2) depend on this task; land it early.

    THE LABEL. Key `media_kind`, closed value set {`audio`, `video`}. Its single source of truth is `docs/observability/label-taxonomy.md` §"Declared, not yet carried: `media_kind` and `content_kind`"; the SDK's TypeScript union is its code home on the client and a Rust `&'static str` vocabulary (a two-variant enum with `ALL` + `label()`, following `crates/mc-service/src/media_signaling/outcome.rs`) is its code home in MC. If a shared home in `crates/common/src/observability/labels.rs` beside `KEY_CUSTODY_LABEL` is the natural place for the Rust key constant, put the key constant there (MH must NOT import it; see the type-blind rule). `content_kind` stays declared-not-carried (content share is out of scope for this story; do not emit it).

    HOP 1 — GC telemetry proxy (`crates/gc-service/src/services/telemetry_filter.rs`). Add `"media_kind"` to `MEDIA_DATAPOINT_EXTRA` (the datapoint-only tier; it must remain stripped on spans and exemplars, because the tier is selected by call site and never by `AttrKind`). Extend the array's doc comment: name the {audio, video} vocabulary and its SSoT (the taxonomy section above), and state that GC bounds only the KEY while the collector's value rewrite is the content bound — the same structural split `key_custody` already uses. Update every pin: the `[&str; 6]` type becomes `[&str; 7]`, the length assert near line 1222, the fixture-equality test near line 1230, and every survival/drop test that iterates the array. Add one integration case in `crates/gc-service/tests/telemetry_proxy_tests.rs`: `media_kind` survives on a metric datapoint and is stripped from a span attribute. GC needs no value-content rule (global-controller's ruling). No other new key: keyframe cause reuses the existing `source` key and every other new client series uses `reason` or `outcome`, so `media_kind` is the ONLY key this story adds to any allowlist.

    HOP 2 — collector (`infra/services/otel-collector/collector.yaml`). Add `media_kind` to `keep_keys` group 2 (the per-metric discriminators, currently `reason, outcome, action, source, mode`) and update the group comment above it. Add a value rule in the same block as the existing `set(attributes[...], "invalid")` rewrites (around lines 277-283), but EXACT rather than shape-based, because the domain is closed: rewrite any non-string value, and any string not matching `^(audio|video)$`, to the reserved sentinel `invalid`. Keep both regex anchors (the existing comment explains why they are load-bearing in OTTL/RE2). Do NOT add any new name to `filter/client_metric_names` in this task: the `client-metrics-export` guard rule G1 requires the collector name list to equal `docs/observability/metrics/client.md`'s `Exported: yes` set and G3 requires every exported name to have an emitter, so each new video series name enters the collector list inside the client task that adds its emitter.

    HOP 3 — dt-guard (`crates/dt-guard/src/client_metrics_export.rs`). The existing rules must pass against the new configuration: G4 (`keep_keys` ⊆ GC `ALLOWLIST ∪ MEDIA_DATAPOINT_EXTRA`), G6 (the `invalid` sentinel collides with no TypeScript media vocabulary token — `audio`/`video` do not), Rule 1 (every resolved label key at every emission of an exported name is forwarded by GC and kept by the collector — the per-kind handle construction below must remain resolvable by the extractor; if the extractor cannot resolve the new label bag it fails closed, and the fix is to keep the bag statically resolvable, never to widen the extractor's tolerance) and Rule 2 (every label a loaded rule matches or groups on survives the pipeline). Check whether the guard's own fixture `MEDIA_DATAPOINT_EXTRA` arrays (near lines 2934 and 2957) are meant to mirror the real array: if they mirror it, give them the seventh entry; if they are synthetic inputs, leave them and say so in a comment. Machinery changes to the guard (new matcher, new rule shape) are out of scope; if one becomes necessary, stop and raise it, because guard machinery is infrastructure's (CLAUDE.md guard-crate ownership).

    CLIENT — per-kind handles (`packages/sdk-core/src/media/setup/mediaMetrics.ts`). Change `mediaMetricLabels(identity)` to `mediaMetricLabels(identity, kind)` with `kind: MediaKind` (`'audio' | 'video'`), still built BY ALLOW-LIST from named values — no ambient label bag can reach a media metric, and the complete set becomes {client_version, org_id, key_custody, media_kind}. Split `MediaMetrics` so that per-kind handles are resolved once at setup per kind (instantiate per kind, or one object exposing two resolved per-kind handle sets — your design, but the hot path still calls methods and never names a metric or builds a label object, per ADR §11's zero-allocation invariant). SESSION-LEVEL series stay KIND-FREE, because labelling them per kind would double-count one event: `dt_client_media_kek_updates_total`, `_kek_retention_violations_total`, `_kek_retention_anomalies_total`, `_kek_install_refusals_total`, `_kek_generations_retained_total`, `_roster_key_rebinds_total`, and `dt_client_media_capture_source` (a build-time property). Every other existing `dt_client_media_*` series (frames sent/received/accepted/dropped, send dropped, send queue depth, key wrap outcomes, downlink gap/reorder, undeclared stream id, decoder errors, mute transitions, decode queue dropped, receive source deficit, receive slots rejected, time-to-first-media-frame) is per kind and today always `media_kind="audio"`. Update every production construction site (`packages/sdk-core/src/session/MeetingSession.ts` near lines 384 and 563) and every test construction site (`__tests__/receiveLanes.test.ts`, `ingress.hopSender.test.ts`, `ingress.attribution.test.ts`, `measurement.test.ts`, `audioPipeline.degradation.test.ts`, `mediaMetrics.test.ts`); the `mediaMetrics.test.ts` allow-list assertion becomes the four-key set for per-kind handles and the three-key set for session-level ones. Do NOT retire `receive_slots_rejected_total` here: task 11 (C-A1), which owns the join's receive declaration and so the over-cap path, retires it with the over-cap throw and its catalog rows, because R-1 replaces rejection with truncation. Do NOT move the `frames_received` counting point here (`docs/TODO.md` entry on the parse boundary): that is task 14 (C-B)'s, with video.

    MC — kind on existing recorders (`crates/mc-service/src/observability/metrics.rs`). Change signatures so every call site must pass the kind (a missed site is then a compile error, not a silent default):
    - `record_slot_state(kind, state)` → `mc_media_slot_states_total{slot_state, media_kind, key_custody}`; the kind is the slot's kind (a declared receive slot's kind, never the frame's).
    - `record_send_targets(kind, count)` → `mc_media_send_targets_total{media_kind, ...}`, counted per stream in the directive rather than summed across streams (today `meeting_media.rs` near line 974 sums targets across `directive.streams`; split it per stream kind).
    - `set_receive_slot_cap(kind, cap)` → `mc_media_receive_slot_cap{media_kind}`. Today there is one cap; publish it for `audio` and, if MC has no video cap yet, publish video only when MC-A introduces the per-kind cap (state in the catalog row which task makes `media_kind="video"` appear). A reading of 0 for `video` is how an operator confirms the R-26 video-off lever applied — say so in the catalog row.
    - `record_mute_request(kind, outcome)` and `record_server_mute_request(kind, action, outcome)`: one increment PER KIND THE REQUEST NAMES (an optional per-kind field that is present), including request-level refusals (`rate_limited`, `actor_unavailable`, `not_permitted`, …), which count once per named kind. Restate the counting unit in each recorder's doc ("one increment per (request, named kind)", replacing "one increment per request"); the `ServerMuteAction × ServerMuteOutcome` zero-initialised cross product becomes `× MediaKind`.
    - `set_server_muted_sources(kind, total)` from a per-kind census (the census in `actors/meeting_media.rs` near line 175 keys per meeting; key it per (meeting, kind) — MC already holds `audio_server_muted` / `video_server_muted` per participant in `actors/messages.rs`).
    - `mc_media_send_directives_total` stays KIND-FREE, deliberately. A SendDirective is composed per participant across all its streams, and its failure outcomes (`assignment_failed`, `meeting_state_unavailable`, `handler_url_unresolved`, `unknown_stream_number`, `transport_mode_unspecified`) occur before any stream exists, so a kind label would have no honest value. The kind split for directive activity lives on `mc_media_send_targets_total` (here) and on the withdrawals counter MC-A adds (`mc_media_send_directive_withdrawals_total{media_kind, reason=unwatched|server_muted}`). Record this in the recorder's doc so nobody "fixes" it later.
    - `mc_media_unreachable_senders_total` stays KIND-FREE: unreachability is a property of a sender relative to a subscriber's handlers, not of a kind.
    Zero-initialise every new label combination at startup the way the existing recorders do (`zero_initialize_counters()` and `WebTransportServer::new()`), so `rate()` has a series before the first event.

    MH — TYPE-BLIND, NO `media_kind`. ADR-0036 §7 bars media-type semantics in MH ("Any proposal that switches on media type inside MH is a design error to be rewritten as a behaviour"). No `mh_*` series may carry `media_kind`; MH labels by behaviour it was told (e.g. transport `datagram`/`stream`) and never by kind. Make this structural rather than reviewed: extend `crates/env-tests/src/fixtures/metric_hygiene.rs` / `crates/env-tests/tests/32_media_metric_hygiene.rs` so the live hygiene kernel FAILS if any `mh_*` series carries a `media_kind` label, and add the matching source-level check if the existing `dt-guard metric-labels` policy (`crates/dt-guard/src/metric_labels.rs`) has a per-service deny construct that can express it as content (a list entry) rather than machinery; if it needs new machinery, do not build it — record the gap in `docs/TODO.md` §Guard Coverage Gaps with infrastructure as owner.

    DOCS. Correct the label taxonomy (`docs/observability/label-taxonomy.md`). The `slot_state` bullet: P1 already dropped the stale "eight-value" count (the domain changed); keep it count-free and pointing at `slot_state_label`. `media_kind` moves from "declared, not yet carried" to carried, by MC and the client SDK only; delete the sentence saying MH will carry it and state the type-blind rule with its ADR §7 citation; keep `content_kind` declared-not-carried. Update `docs/observability/metrics/client.md` (the label set of every per-kind series, the kind-free list with the reason) and `docs/observability/metrics/mc-service.md` (each changed recorder's labels, cardinality and counting unit; the two deliberately kind-free series and why). Update the dashboards and alerts that read these series ONLY where a selector or grouping would now break or silently change meaning (`infra/grafana/dashboards/mc-media.json`, `client-media.json`, `infra/docker/prometheus/rules/mc-alerts.yaml`, `client-alerts.yaml`): `sum(...)` and `sum by (reason|outcome)` queries keep working because they aggregate the new label away; a query that compares series by matching label sets (`on(...)`, `ignoring(...)`, vector matching between a kind-labelled and a kind-free series) must be checked and fixed. Do NOT re-derive `MCMediaMissingKeyMaterial` per kind here — that is task 20 (OBS-B), once video emits. Record each check you made in the devloop output so OBS-B inherits it.

    PROOF (R-29). Two layers. (1) Env-test: extend `crates/env-tests/tests/32_media_metric_hygiene.rs` (or a sibling file in the same numbering band) to assert that `mc_media_slot_states_total` and `mc_media_send_targets_total` series in Prometheus carry `media_kind="audio"` after a join, and that no `mh_*` series carries `media_kind`. (2) Browser layer: the story-2 browser suite already reads client series back out of Prometheus through `packages/web-app/e2e/mcMetrics.ts` (the suite's one PromQL home) using names from `packages/web-app/e2e/clientMetricNames.ts`; extend the multi-party read-back (`packages/web-app/e2e/multi-party-hear.spec.ts`) to assert `dt_client_media_frames_received_total{media_kind="audio"}` is present, which proves all three hops end to end (SDK emission, GC forward, collector keep + value rule). Pair with test for the spec edit and with client, who owns that spec's assertions. The `media_kind="video"` read-back is task 14 (C-B)'s, once video flows. Also extend `crates/env-tests/tests/31_gc_telemetry.rs` with one case: an authenticated OTLP metrics POST carrying `media_kind="video"` reaches Prometheus with the label intact, and one carrying `media_kind="bogus"` reaches it as `media_kind="invalid"` — the value rule fires AND applies.

    ADR-0031 BLOCK (ADR-0036 "A control's coverage must be demonstrated"). For the collector value rule and the new hygiene assertion, state both halves in the plan: does it fire (inject `bogus` and an `mh_*` series with the label, see each fail/rewrite) and does it apply (the rule is in the deployed collector config the Kind overlay loads; the hygiene test runs against the live Prometheus, not a fixture).

    ACCEPTANCE. All of: the three hops changed together and `dt-guard client-metrics-export` passes; GC unit and integration tests updated and green; the SDK per-kind handle split with session-level series kind-free and allow-list tests updated; every listed MC recorder takes a kind and every call site passes one; `mc_media_send_directives_total` and `mc_media_unreachable_senders_total` stay kind-free with the reason documented; taxonomy and both catalogs updated; existing alerts and dashboards checked; the env-test and browser read-back proofs green; `layer-all.sh` green.

    OUT OF SCOPE. New video series (each lands in its emitting task with its own catalog row, dashboard panel and, for client names, collector allowlist entry); video alerts and dashboard rows (task 20, OBS-B); retiring `receive_slots_rejected_total` (C-A1); moving the received-frames counting point (C-B); `content_kind`.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - `media_kind` is the only new label dimension. No sender, stream, slot, meeting or participant identity on any series. `key_custody=operator` is retained.
    - The telemetry_filter / GC proxy edit keeps the user-JWT gate, PII drop and size/rate caps intact. No new label passes the filter unbounded.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-20-media-kind-label-split
  tag: story-2026-10-07-see-each-other-task-2
- id: 3
  status: completed
  specialist: infrastructure
  prompt: |-
    Land the media configuration surface for story 3, "See Each Other" (docs/user-stories/2026-10-07-see-each-other.md; covers R-23, R-24 (the derivation half), R-25 (the manifest half), R-26 (the cap's home); ADR-0036 §11 "Derive rather than guard", ADR-0038 §2 content-addressed ConfigMaps). This is task 3 (INF-A). It has NO dependencies and MUST land before the code tasks that read these keys: task 8 (MH-A1, media-handler: per-stream attribution, derived costs, MH config reads, memory startup check) and task 12 (MC-A, meeting-controller: video slots, send directives, per-kind caps) both require every key below as a required read with no code fallback. Behaviour on the cluster must be unchanged when this task deploys alone: today's images ignore every new key, and every key they still read stays present.

    WHAT TO BUILD

    1. A new shared, generator-only Kustomize base `infra/services/media-codec-profile/`: a `kustomization.yaml` with one `configMapGenerator` named `media-codec-profile` (namespace dark-tower, matching the other bases) sourced from `profile.env`, and no other resources. Include it from the Kind environment root's service list (`infra/kubernetes/overlays/kind/services/kustomization.yaml`, alongside the per-service overlays) so it is part of the ONE render `deploy.sh:apply_env_root()` applies. No network policy, Service or workload changes. Verified in planning with `kubectl kustomize`: a `configMapKeyRef` in the mc-service or mh-service base naming `media-codec-profile` is rewritten to the generator's hash-suffixed name in the root render, so a profile change rolls MC AND MH together. Write that consequence into the profile header: changing any profile value is a coordinated MC+MH roll, and an MH roll sheds every media session on the pod (ADR-0036 §11), which is why the R-26 video-off lever is deliberately NOT in this file.

       `profile.env` holds exactly nine keys, production defaults, each with an operator-facing comment in the house style of `infra/services/mc-service/config.env` (what it is, who enforces it and where, where an operator sees it, related-but-different values that must not be unified):
       - `MEDIA_VIDEO_CODEC=vp8`
       - `MEDIA_VIDEO_WIDTH=640`
       - `MEDIA_VIDEO_HEIGHT=360`
       - `MEDIA_VIDEO_FRAME_RATE_HZ=30`
       - `MEDIA_VIDEO_MAX_BITRATE_BPS=800000`
       - `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS=500` (MC carries it on the video directive as `min_keyframe_interval_ms`; MH reads it to validate `MH_KEYFRAME_REQUEST_RETRY_MS` ≥ it at boot)
       - `MEDIA_AUDIO_CODEC=opus`
       - `MEDIA_AUDIO_MAX_BITRATE_BPS=48000`
       - `MEDIA_AUDIO_FRAME_RATE_HZ=50`
       Bounds are owned by the readers and must be cited, not restated as numbers: MC validates codec/width/height/frame-rate/bitrate/min-keyframe-interval (constants in `crates/mc-service/src/media_signaling/directive.rs`, incl. `MIN_KEYFRAME_INTERVAL_MS_FLOOR`/`_CEILING` and the existing `AUDIO_BITRATE_MIN/MAX_BPS`, `AUDIO_FRAME_RATE_MIN/MAX_HZ`); MH refuses to boot if the audio values fall outside its compiled datagram-buffer envelope. Carry over the substance of the existing comments on `MC_AUDIO_MAX_BITRATE_BPS` and `MC_AUDIO_FRAME_RATE_HZ` in `infra/services/mc-service/config.env` (the MH buffer-latency coupling, the "do not unify" constants, the 25..50 Hz reasoning), re-homed to the new keys and corrected for the fact that MH now READS these values rather than assuming them. State in the header the membership rule ruled by the lead with operations: a value belongs in this profile ONLY if both MC and MH must agree on it (and both read it); an MC-only knob goes in `mc-service/config.env`, an MH-only knob in `mh-service/config.env`. This rule is a header comment ONLY — no guard enforces it (dropped by the user in the 2026-10-13 revision: no requirement traces to it).

       The cost derivation that motivates this (R-24): MH derives each per-stream egress cost as MAX_BITRATE_BPS + (code-derived per-frame overhead bits × FRAME_RATE_HZ), for audio and video, from these keys — no headroom knob, and the overhead constant lives in MH code (MH-A1 pins it with a unit test). Expected: video ≈ 0.83 Mbps at production defaults, ≈ 0.265 Mbps at the Kind test profile; audio ≈ 90 kbps. State in the profile header and at the `MH_EGRESS_BUDGET_BPS` comment that this bitrate-derived cost model is INTERIM (user-approved capacity note, 2026-10-08): ADR-0036's addendum, amended by task 1 (P1), carries a future "adaptive capacity" story (MH self-monitored fullness, per-stream profiles under user/org limits, GC placing on live headroom, possibly a client-count resource model replacing the bitrate budget) — cite that addendum line rather than paraphrasing it, so nobody tunes this model as if it were the long-term design. Do NOT write those figures into any manifest comment: the derived cost is published by MH as the ceiling gauge `mh_media_egress_stream_ceiling`, and a restated figure goes stale the moment the overhead constant moves.

    2. Required `configMapKeyRef` entries on EVERY instance — per workload, never union (`dt-guard env-config` rule 1 checks per workload; a key on mc-0 missing from mc-1 crash-loops that pod alone):
       - mc-0 and mc-1 (`infra/services/mc-service/mc-{0,1}-deployment.yaml`): all nine profile keys.
       - mh-0 and mh-1 (`infra/services/mh-service/mh-{0,1}-deployment.yaml`): `MEDIA_VIDEO_MAX_BITRATE_BPS`, `MEDIA_VIDEO_FRAME_RATE_HZ`, `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, `MEDIA_AUDIO_MAX_BITRATE_BPS`, `MEDIA_AUDIO_FRAME_RATE_HZ`.
       Env name equals key name (the repo-wide frozen `key == env name` rule).

    3. MC keys in `infra/services/mc-service/config.env` (shared, not per-instance), both referenced from mc-0 and mc-1:
       - `MC_MAX_AUDIO_RECEIVE_SLOTS=8` (MC bounds 1..64)
       - `MC_MAX_VIDEO_RECEIVE_SLOTS=4` (MC bounds 0..64; 0 is the R-26 video-off lever)
       Per the user's ruling, a declaration over a kind's cap is TRUNCATED to the cap and each excess slot reports a permanent UNAVAILABLE state (slot state value 9); it is never a join failure (supersedes story 2 R-1's fail-loudly rule) — say this in the comment where an operator sees the cap. At `MC_MAX_VIDEO_RECEIVE_SLOTS` write the lever and its blast radius: setting 0 truncates every video slot to unavailable so no publisher is watched and no camera is directed; audio is unaffected; it applies to declarations after the new MC pods are up; because this ConfigMap is content-addressed it ROLLS both MC pods (one restart per pod's meetings) and is not a hot toggle; it is deliberately NOT in `media-codec-profile` so pulling it never rolls MH (an MH roll sheds every media session). Operations' runbook carries the same text (task 4, OPS-A); cite `docs/runbooks/mc-deployment.md` rather than duplicating paragraphs.

    4. MH keys in `infra/services/mh-service/config.env`, all required on mh-0 and mh-1 (defaults and bounds are media-handler's, given here so the comments are correct; cite `crates/mh-service/src/config.rs` as the home of the bounds):
       - `MH_KEYFRAME_REQUEST_RETRY_MS=500` (100..=10000; MH refuses to boot unless ≥ `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, because a retry faster than the publisher's minimum interval is wasted)
       - `MH_KEYFRAME_REQUEST_AGGREGATION_MS=100` (10..=1000; validated ≤ the retry interval; R-13's user-confirmed default; aggregation coalesces many receivers' recovery requests per source stream, it does not pace the publisher, so it is deliberately not checked against the minimum interval — the client defers an early request and counts it as `min_interval`, never refuses it)
       - `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC=20` (1..=200)
       - `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC=10` (1..=100)
       - `MH_EGRESS_STREAM_BACKLOG_BYTES=262144` (16384..=4194304; per-subscriber-stream backlog cap, R-12/R-21: past it MH resets that destination's group stream, counted)
       - `MH_STREAM_MID_FRAME_STALL_MS=1000` (100..=10000; a publisher stalled mid-frame is reset downstream after this, counted)
       - `MH_MAX_SOURCE_STREAMS_PER_MEETING=1024` (1..=4096; REPLACES `MH_MAX_MUTED_SOURCES_PER_MEETING`, backing protocol's `source_streams` list; validated ≥ `MH_MAX_EGRESS_STREAMS_PER_MEETING` as today — cite media-handler's exact floor rule in config.rs)
       - `MH_POD_MEMORY_LIMIT_BYTES`: NOT a ConfigMap key. Inject it on both mh-0 and mh-1 via the downward API, `valueFrom.resourceFieldRef` with `containerName` the MH container, `resource: limits.memory`, `divisor: 1`, so the 1 Gi limit has exactly one home (the Deployment's `resources.limits.memory`) and MH's R-21 startup check (backlog × egress stream ceiling plus the receive-side bound × `MH_MAX_CONNECTIONS` within a fixed fraction of it) is derived, never restated. Update the `MH_MAX_CONNECTIONS` block's "changing resources.limits.memory invalidates the arithmetic" note to say MH now checks the relationship at boot.
       `MH_MAX_CONCURRENT_UNI_STREAMS` stays 64; `MH_EGRESS_BUDGET_BPS` base value stays 10000000 (see 7).

    5. Retirements — two deploys, using the method `docs/TODO.md`'s `MH_MAX_STREAMS` entry prescribes (read that entry in full first; its rule is "never delete a ConfigMap key while a HARD ref to it exists in any Deployment revision `kubectl rollout undo` can reach"). In THIS task: flip each retiring key's refs to `optional: true` on every workload that has one, keep the key, and replace its comment with a RETIRED — PENDING REMOVAL banner in the style of the existing `MH_MAX_STREAMS` banner (what supersedes it, why it is still here, that nothing reads it once the named code task lands — name it by CODE only, e.g. "read removed by MH-A1", never by task number, in anything committed — the scheduled removal). The retiring set:
       - MC: `MC_MAX_RECEIVE_SLOTS` (a TOTAL across kinds today, `capability.rs` ~185; superseded by the per-kind caps; MC-A removes the read), `MC_AUDIO_CODEC`, `MC_AUDIO_MAX_BITRATE_BPS`, `MC_AUDIO_FRAME_RATE_HZ` (superseded by `MEDIA_AUDIO_*`; MC-A removes the reads).
       - MH: `MH_STREAM_COST_AUDIO_BPS`, `MH_STREAM_COST_VIDEO_BPS` (superseded by the derived costs; MH-A1 removes the reads), `MH_MAX_MUTED_SOURCES_PER_MEETING` (superseded by `MH_MAX_SOURCE_STREAMS_PER_MEETING`; MH-A1 removes the read), and `MH_MAX_STREAMS` step (1) — that entry's trigger ("the MH egress-budget code task's image has been deployed and proven") has fired with story 2, so do step (1) exactly as the entry specifies, including updating the `MH_MEDIA_LATENCY_SAMPLE_RATIO` "Today that is this key alone" sentence in both MH deployment files, which a second optional ref falsifies.
       Optional refs on keys the OLD images still read are the point: the current images keep reading them until MC-A/MH-A1 land, and the keys stay present, so nothing changes at deploy. Why `optional: true` does not contradict the deployment files' standing "DO NOT flip them" warning: that warning covers keys whose absence must be loud; a key nothing reads (after the code tasks) cannot be loud. Say this once where the warning lives.
       Carve the retiring MH keys out of the not-optional assertion in `crates/env-tests/tests/01_mh_deployment_config.rs` (its `STORY2_KEYS`-style list asserts every ref exists and is NOT optional) in the same change, and add the new MH keys to its required-and-not-optional set.
       Update the `MH_MAX_STREAMS` entry in `docs/TODO.md` (step 1 done, step 2 pending) and file ONE scheduled step-2 entry covering every key in the retiring set — delete key and refs in one change; trigger: the MH-A1 and MC-A images deployed and proven (rolled out, serving, no rollback taken); owner infrastructure with operations; site inventory per key (ConfigMap line, each deployment ref, env-test lists, `.env.example`, `docs/DEVELOPMENT.md`, runbook rows). R-25 is confirmed (2026-10-08): this story ships as one coordinated cutover and rollback across it is all components together; say in that entry that step 2 must not run before the rollback window for this story's cutover has closed.

    6. Guard machinery in `crates/dt-guard` (infrastructure owns machinery; policy content is noted per item). Every new rule gets positive and negative fixtures in the existing test style, and `dt-guard env-config` and `dt-guard kustomize` stay green on the tree:
       a. Cross-base generator resolution in `crates/dt-guard/src/env_config.rs`. Today a workload's `configMapKeyRef` is resolved against ConfigMaps (and generators) in its own service dir only, so a ref to `media-codec-profile` would read as unresolved. Teach the resolver that a ref may name a generator in a sibling base under `infra/services/**`, resolving through the ONE generator parser and containment-gated source resolver `crates/dt-guard/src/common/kustomize_generators.rs:generator_data` (do not add a second parser). Orphan-key detection for the shared generator must consider refs from every workload in every base (a profile key referenced by nobody is still an orphan). A ref name that matches generators in two bases is a hard, named error, never last-wins (mirror `duplicate_configmap_name`).
       b. Downward-API limit rule (operations' catch; policy content: operations, machinery: you): any container env var sourced from `valueFrom.resourceFieldRef` with `resource: limits.<r>` (or `requests.<r>`) requires that same container (or the named `containerName`) to declare `resources.limits.<r>` (respectively `requests.<r>`); otherwise FAIL naming workload, container and resource. Reason, write it in the rule's doc comment: if the limit is absent, Kubernetes silently resolves the env var to the NODE's allocatable value, so MH's R-21 memory check would pass vacuously against gigabytes — fail-open with no signal. Put it where the pod-spec walk already lives (`crates/dt-guard/src/common/pod_spec.rs` for the walk; the rule in the kustomize policy module or env_config, your call, stated in the module doc). Negative fixture: drop `limits.memory` from an MH-shaped workload and see it fail.
       c. Confirm `dt-guard kustomize` (build, orphan manifests, kubeconform, securityContext, R-21 content addressing, the annotation-size rule) covers the new base without special cases; a generator-only base must not trip the orphan-manifest or "workload missing" checks. If any rule needs a change to accept it, make that change in this task and test it.

    7. `MH_EGRESS_BUDGET_BPS` base placeholder (operations' ruling): KEEP the value 10000000. One sentence in its comment (~line 314, "admits fewer streams than even one small audio-only meeting needs") becomes false once costs are derived (the placeholder now admits about 12 streams) and is derived arithmetic the same comment forbids. Delete it. Replace it with only the checkable property: the placeholder's derived ceiling sits below MH's recommended minimum (`EGRESS_STREAM_CEILING_RECOMMENDED_MIN`, which media-handler re-derives for P×(N+M) = 54 in MH-A1), so MH's configure-me nudge fires; and it sits above the refuse-to-boot floor of 2. Cite the MH-A1 unit test that asserts it by name: `base_placeholder_budget_is_below_recommended_min`. Do NOT touch the Kind budget patch (`infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml`) in this task: the Kind re-size is task 16 (INF-C), after MH-A1's derivation is deployed. Its comment's formula (`ceiling = budget / max(cost_audio, cost_video)`) stays true in shape; add a one-line note there that the two costs are now derived by MH from `media-codec-profile` and that INF-C re-sizes the value.

    8. Docs that mirror config (the `docs/TODO.md` "local-run blocks" entry's defer trigger fires because MC-A/MH-A1 add `MissingEnvVar`s): update the MC and MH blocks in `docs/DEVELOPMENT.md` and the MC and MH sections of `.env.example` for every new key (including the nine profile keys in both blocks where each service reads them, and `MH_POD_MEMORY_LIMIT_BYTES` noted as downward-API-injected in-cluster and set by hand for a local run), remove nothing the old images still need while marking the retiring keys retired, and keep the `ANCHOR (DRY)` comments naming each block's source of truth. Note in the anchor that the profile keys' home is `infra/services/media-codec-profile/profile.env`. Operations writes the deploy order, rollback and video-off lever into `docs/runbooks/mc-deployment.md` and `docs/runbooks/mh-deployment.md` in task 4 (OPS-A); you give them the final key list (it is the list above) and review their Configuration Reference rows for exact names. Do not edit those runbooks' prose yourself beyond correcting a key name.

    ENV-TESTS (Covers: env-tests; pair with test for fixture discipline — use `crates/env-tests/src/fixtures/kube.rs:configmap_key()`/`resolve_generation()` and `eventual.rs:assert_eventually()`, no wall-clock sleeps):
    - In `01_mh_deployment_config.rs` (and the MC equivalent if one exists; otherwise add the MC assertions beside it): the live mc-0/mc-1/mh-0/mh-1 pods resolve every new key from the CURRENT ConfigMap generation; MC and MH resolve the SAME generation of `media-codec-profile` (one hashed name, proving the shared-source property on the live cluster rather than by inspection); each retiring key's ref is present and `optional: true`; `MH_POD_MEMORY_LIMIT_BYTES` in the running MH container equals the pod's `resources.limits.memory` in bytes.
    - Live-ConfigMap tests that read `MC_MAX_RECEIVE_SLOTS` today (`01_mh_deployment_config.rs` ~920, `28_mh_egress_admission.rs` ~237, `crates/env-tests/src/fixtures/egress_admission.rs`) keep working in this task because the key stays present; task 16 (INF-C) moves them to the per-kind caps and the ceiling gauge. Do not change their logic here.

    GATES AND ACCEPTANCE
    - `scripts/layer-all.sh` green; Layer 3 includes `dt-guard env-config`, `dt-guard kustomize`, and the hermetic script self-tests; Layer 7 deploys this task alone onto the current images and every existing env-test and browser spec stays green (behaviour unchanged at cutover).
    - `kubectl kustomize` of the Kind root renders one `media-codec-profile-<hash>` ConfigMap, referenced by hashed name from all four MC/MH Deployments.
    - Changing one profile value (verify in a scratch render, do not commit) changes the pod template hash of mc-0, mc-1, mh-0 and mh-1; changing `MC_MAX_VIDEO_RECEIVE_SLOTS` changes mc-0/mc-1 only.
    - No key value appears in two committed places (profile keys exist only in `profile.env`; the memory limit only in `resources.limits.memory`); the docs mirrors are anchored, not authoritative.
    - Fail loudly, never mask: no rule gains an allowlist or exemption to make this tree pass; if an existing rule rejects the new shape for a real reason, fix the shape or change the rule with a test, and say which in the devloop output.

    Pair with operations (deploy order and runbook key names; the retirement entry), test (env-test patterns), media-handler (MH key bounds and comment accuracy), and meeting-controller (MC key bounds). Dependencies: none. Provides: every config key read by task 8 (MH-A1), task 9 (MH-A2: backlog, stall, stream-open bounds), task 10 (MH-B: keyframe retry and aggregation) and task 12 (MC-A); through MC's directive, the codec parameters tasks 11/13 (C-A1/C-A2) encode at; and the `media-codec-profile` home that task 16 (INF-C) patches for the Kind test profile.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Every new key is startup-validated with bounds:
      - `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, a shared-profile key, is required, nonzero and bounded;
      - `MH_KEYFRAME_REQUEST_RETRY_MS` (default 500, range 100..=10000) is ≥ `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`.
    - The downward-API memory-limit guard stays, so MH's backlog × streams startup check reads the real pod limit.
    - No secret or credential is added to ConfigMaps. The env-config drift guard covers every new key on every workload.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-20-media-codec-profile-config
  tag: story-2026-10-07-see-each-other-task-3
- id: 4
  status: completed
  specialist: operations
  prompt: |-
    Video deploy order, v3 cutover and rollback, the video-off lever, send-directive wiring, and the premise sweep (docs/user-stories/2026-10-07-see-each-other.md, R-23 runbook rows, R-25, R-26 runbook half, R-31 directive-wiring and premise items; ADR-0036 §8 "two-ends-must-agree … loud mismatch", §11 eight required media scenarios, "A control's coverage must be demonstrated, not asserted" applied to prose). Task 4 (OPS-A), specialist operations. Dependencies: none. R-25 requires the deploy order to be written in the MH and MC deployment runbooks BEFORE the code lands, so this task runs on the first front, in parallel with task 1 (P1: protocol contract and the ADR-0036 R-36 amendment bundle) and task 3 (INF-A: the media config surface, the shared `media-codec-profile` ConfigMap, MC and MH keys, retirements, and the downward-API memory-limit guard). In anything you write into the repo, refer to other tasks by CODE only (for example "deleted by MC-B"), never by task number. Key names below are the confirmed final names from INF-A's design. If INF-A has landed when you start, cite the live keys in `infra/services/*/config.env` and the new `media-codec-profile` ConfigMap; if it has not, write the same names. Either way, at Gate 1 diff every key name you cite against INF-A's prompt or the tree, and reconcile any difference rather than writing a guess. This task is documentation only, with no code, manifest or alert-rule edits. It still carries R-25, so review it as load-bearing: pair with infrastructure (deploy tooling and key ordering), protocol (the skew paths) and observability (anchor and alert cross-references).

    R-25 IS CONFIRMED (user, 2026-10-08). R-25 holds between builds that both include this story. This story ships as ONE coordinated cutover, in the order config → MH → MC → client SDK, and rolling back ACROSS that cutover means all of them together. Why, stated from code so the runbook can say it plainly: MH ingress calls `decode_datagram` on every audio frame (`crates/mh-service/src/media/forward.rs`), and the codec rejects any version other than `PROTOCOL_VERSION` as `unknown_version`. After task 6 (P2) the version is 3 and the only one; v2 is deleted in Rust, TS and the vectors. The SDK sends whatever `header_version` MC directs, and after P2 refuses a non-3 directive. So a pre-story MH, MC or SDK rolled back alone against the post-story other two silences audio. That failure is loud, but it is total. A one-component one-version rollback is safe only within the v3 era. MC-B (task 15) deletes the superseded proto fields in-story, which is part of why the cutover is all-or-nothing.

    WHAT TO WRITE

    1. `docs/runbooks/mh-deployment.md` and `docs/runbooks/mc-deployment.md`: the story-3 cutover.
       - Order: (a) ConfigMaps first. INF-A adds the shared `media-codec-profile` ConfigMap (required on mc-0, mc-1, mh-0 and mh-1), the new required MC and MH keys, and flips the retiring keys to `optional: true` with RETIRED banners. (b) MH. (c) MC. (d) the browser SDK/web-app bundle, a separately named step, because Kind's `infra/kind/scripts/deploy.sh` does not deploy it (Vite dev server or bundle).
       - Why MH precedes MC inside the cutover: MH's apply-time per-stream transport check and its `applied_source_streams` echo must be live before any MC emits a video egress stream. A story-2 MH hard-rejects a registration whose egress transports are heterogeneous, which takes the whole meeting's AUDIO dark, not just video.
       - RECONCILE, do not stack, the existing forced-order text. Three passages must be re-derived and either scoped or superseded with a dated correction stating what a reader who trusted the old text would wrongly do: `mc-deployment.md` §"0. Roll the browser SDK first (story 2 task 20 and later)"; `mh-deployment.md`'s "STOP — DEPLOY ORDER IS FORCED. MC FIRST, THEN MH" banner (the sender_id binding contract); and the §Cross-service ordering rollback constraint ("MH first, then MC"). For each, state whether it still binds within the v3 era. The sender_id contract is satisfied by every post-story build, so it binds only against pre-story builds, and those are already covered by the all-together rule. State how it interacts with the cutover. Never leave two "forced order" banners that contradict each other.
       - Skew-detection table (wire-freeze ruling 9), one row per one-component skew across the cutover, each naming an operator-queryable counted reason, never a browser console:
         - old SDK → new MC: refused at join because `supported_header_versions` lacks 3, counted by MC (the join-refusal token MC-A, task 12, ships; cite it from the tree);
         - old MH ← v3 frames: dropped and counted as `unknown_version` on MH's codec-drop counter;
         - old MC directing v2 → v3 SDK: refused by the SDK and counted in an exported client series (the token the client publish path, tasks 11/13 C-A1/C-A2, ships).
         Keep and re-scope the existing "One version-skew detector survives a rollback" paragraph (the client `unknown_version` receive drop) so it agrees with this table.
       - Rollback, three layers:
         - CONFIG rollback is safe: ConfigMaps are content-addressed (ADR-0038 §2), so `kubectl rollout undo` lands on the previous hash generation, which holds every key it references, and the `optional: true` refs cover a partial revert.
         - WITHIN the v3 era, one component by one version holds (R-25).
         - ACROSS the cutover it is all together. On Kind: check out the previous commit and run `deploy.sh`; prior content-tagged images survive `prune_superseded_images()` and the prior ConfigMap generation is retained. Then roll back the SDK bundle. Rewrite `mh-deployment.md` §"Rollback for the media path" (today it says "Redeploy only — kubectl rollout undo" of mh-0/mh-1 alone and "Partial rollback: Not available"), which would silence audio if applied alone across this cutover.
       - New subsection under §"Rollout With Media Flowing" titled exactly "Mixed-Transport Rollback Hazard" (anchor `#mixed-transport-rollback-hazard`; observability's alert A8 links to it):
         - the hazard: an MC that emits a video egress stream against an MH that does not accept it;
         - MC side: `mc_media_policy_pushes_total{outcome="transport_mode_mismatch"}`, whose meaning widens to any per-stream echo disagreement (a declared stream absent from `applied_source_streams`, echoed UNSPECIFIED, or echoed with a different mode). It PAGES via `MCMediaGenerationDivergence` into `mc-incident-response.md` Scenario 15. Cross-link that row both ways, and fork on MC's log line: absent from the echo means an older or mis-ordered MH build (roll MH forward); a different mode means config;
         - MH side (A8, warning, `for: 15m`): `mh_media_policy_applies_total{outcome="rejected_transport_inconsistent"}` (an egress whose candidates' declared streams differ in transport; `EgressStream.transport_mode` no longer exists, per wire-freeze Amendment 1, and an edge's transport derives from its candidate's declared SourceStream) and the MH drop reasons `transport_mismatch` (declared stream, wrong arrival transport: a sender or contract fault) versus `unmatched_stream` (no declared stream matches: MC/MH declaration drift), with their different remedies;
         - keep the existing Scenario 12 trap ("roll MH FORWARD; do NOT roll MC back") consistent with this subsection.
       - Configuration Reference (MC: §Environment Variables and the startup-line checklist; MH: §Required environment keys). One row per key with default, bounds, what it does, and the failure if absent:
         - `media-codec-profile`: `MEDIA_VIDEO_CODEC`=vp8, `MEDIA_VIDEO_WIDTH`=640, `MEDIA_VIDEO_HEIGHT`=360, `MEDIA_VIDEO_FRAME_RATE_HZ`=30, `MEDIA_VIDEO_MAX_BITRATE_BPS`=800000, `MEDIA_AUDIO_CODEC`=opus, `MEDIA_AUDIO_MAX_BITRATE_BPS`=48000, `MEDIA_AUDIO_FRAME_RATE_HZ`=50, `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`=500. That is NINE keys. MC reads all of them; it carries the minimum keyframe interval on the video send directive as `min_keyframe_interval_ms` and publishes it as `mc_media_video_keyframe_min_interval_seconds`. MH reads the video and audio bitrate and frame-rate pairs plus the minimum keyframe interval, which it uses to validate its retry key at boot. The minimum keyframe interval is a PROFILE key, not an MC key. Bounds belong to the readers; cite their constants and do not restate numbers. Kind patches video to 320×180 / 15 fps / 250000 (the R-28 test profile). There is no `dev-web.sh` override for the profile: to change it, edit or remove the Kind patch and redeploy. State the membership rule (only values both services must agree on) as the rule the profile header carries. It is a header comment only; there is no membership guard;
         - MC: `MC_MAX_AUDIO_RECEIVE_SLOTS`=8 (1..64), `MC_MAX_VIDEO_RECEIVE_SLOTS`=4 (0..64; 0 = video off);
         - MH: `MH_KEYFRAME_REQUEST_RETRY_MS`=500 (100..=10000; MH refuses to boot unless it is ≥ `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, because a retry faster than the publisher's minimum interval is wasted), `MH_KEYFRAME_REQUEST_AGGREGATION_MS`=100 (10..1000), `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC`=20 (1..200), `MH_STREAM_MID_FRAME_STALL_MS`=1000 (100..10000), `MH_EGRESS_STREAM_BACKLOG_BYTES`=262144 (16384..4194304), `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`=10 (1..100), `MH_MAX_SOURCE_STREAMS_PER_MEETING`=1024, and `MH_POD_MEMORY_LIMIT_BYTES` (downward API from `limits.memory`, never restated; a kustomize-policy guard fails if the container lacks `resources.limits.memory`, because an absent limit resolves to node allocatable and would make MH's startup backlog-vs-memory validation pass vacuously);
         - RETIRED (optional ref plus banner now, deletion is a scheduled TODO, the `MH_MAX_STREAMS` two-deploy method): `MC_MAX_RECEIVE_SLOTS` (a TOTAL across kinds in story 2), `MC_AUDIO_CODEC`, `MC_AUDIO_MAX_BITRATE_BPS`, `MC_AUDIO_FRAME_RATE_HZ`, `MH_STREAM_COST_AUDIO_BPS`, `MH_STREAM_COST_VIDEO_BPS` (MH now derives both costs as bitrate plus code-derived per-frame overhead × frame rate, with no headroom knob), `MH_MAX_MUTED_SOURCES_PER_MEETING` (replaced by `MH_MAX_SOURCE_STREAMS_PER_MEETING`), and `MH_MAX_STREAMS` step 1.
         Prefer citing the source over restating a value wherever the runbook already has a home for it.

    2. The video-off lever (R-26). Rewrite `mh-deployment.md` §"Operator levers: there are none for media". Its first sentence ("This story adds zero operator-reachable media levers") becomes false; add a dated correction. Keep the two existing bullets (lowering `MH_EGRESS_BUDGET_BPS` is not a kill switch; server mute is reachable by a service-credential holder) and add:
       - `MC_MAX_VIDEO_RECEIVE_SLOTS=0` disables video. Every declared video slot truncates to UNAVAILABLE (R-1), no publisher is watched, so no camera is directed to send. Audio is unaffected.
       - Its cost, in these terms: the key lives in `infra/services/mc-service/config.env` ONLY, never in `media-codec-profile`, so it never rolls MH. The MC ConfigMap is content-addressed, so changing it rolls both MC pods. It is not a hot toggle: one MC restart for every meeting on each pod, applying to declarations after the new pods are up.
       - For a video-only incident the lever comes BEFORE any rollback, because it does not cross the v3 wire break.
       - Beside it, the cost of a `media-codec-profile` change: a coordinated MC+MH roll, so MH sheds every session on the pod.
       Add matching text to `mc-deployment.md` (Configuration Reference row plus a short "video-off lever" note). Verification of the lever itself is test's and MC's (an MC integration test with cap 0, plus one manual-test-plan step), not an env-test that rolls MC mid-suite. Cite those; do not invent one here.

    3. Send-directive wiring (R-31; `docs/TODO.md` §Media Path Obligations entry "`mc_media_send_directives_total` has no alert rule …"). In `docs/runbooks/mc-incident-response.md`, the query section around the "`mc_media_send_directives_total{outcome}` — is MC telling clients to send?" block (~L2007): rewrite so a WITHDRAWAL reads as healthy. Withdrawal is now routine: an unwatched publisher (R-8), and server mute of that kind (R-16). Observability has asked MC for a separate `mc_media_send_directive_withdrawals_total{media_kind, reason}`. MC-A (task 12) decides between that and new outcome tokens on the existing counter. Write the section so it is correct under the shape MC shipped (check the tree), and partition every token into normal and fault in prose. OBS-B (task 20) writes the alert rule and closes the TODO; do not write a rule here.

    4. Premise sweep (the story-2 ADR-0031 review obligation: when a requirement retires a behaviour, grep runbooks, dashboards AND alert rule annotations for text asserting it). Sites: `docs/runbooks/*.md`, `docs/observability/**`, `infra/grafana/dashboards/*.json` panel descriptions, and `infra/docker/prometheus/rules/*.yaml` annotations. Premises this story falsifies:
       - the media path is audio-only or datagram-only;
       - "server mute is sender-scoped, exact for audio-only" (MH 19, story-2 R-9 note). It is now per declared stream: MH attributes by (connection-bound sender, uplink relay-region `stream_id`) looked up in MC's `source_streams`, where the uplink `stream_id` is the publisher-stamped MC send id (`SendStream.stream_number`; audio 0, video 1 today) and is hop-local, unauthenticated and publisher-set. R-16 holds in slot terms because a server-muted stream is DROP_AT_INGRESS and receivers take the decoder from the slot kind (wire-freeze Amendment 3). There is no signed header stream number;
       - "an over-cap receive declaration fails loudly" (story-2 R-1, `client-dev-local` F18 "the media error names the receive-slot cap"). Now truncation to UNAVAILABLE, never a failure;
       - "there are no operator levers for media";
       - a single `transport_mode` echo or `EgressStream.transport_mode`. Removed; the echo is per stream as `applied_source_streams`;
       - any keyframe cadence or "one-second keyframe" text (there is no keyframe interval, R-9);
       - `MH_STREAM_COST_VIDEO_BPS` described as inert or dominant;
       - `SOURCE_MUTED` (removed in-story) and `ZERO_REQUESTED` (reserved) semantics. Slot states are the flat enum ACTIVE, FEWER_SOURCES, AVATAR_NO_VIDEO=8, UNAVAILABLE=9, AVATAR_CLIENT_MUTED=10, AVATAR_SERVER_MUTED=11 (server wins when both), with unreachable still carried by per-subscriber `unreachable_sender_ids`; use the wire names, "active" never "live";
       - v2 header references, and any text describing a v3 header field other than the added `gop_counter` (the v3 fixed prefix is 14 bytes).
       Correct runbook sites in place with each runbook's recorded-correction convention. For a dashboard, alert annotation or observability-doc site, do not edit it: list it, with file and line, in the devloop output under a "Premise sites handed to OBS-B" heading, and send the list to observability, because OBS-B (task 20) owns those files in this story. Wherever a corrected passage restated a value, replace the restatement with a citation of its source.

    CONSTRAINTS AND GATES
    - No runbook text may claim end-to-end, zero-trust or forward secrecy (key custody is operator, ADR-0036 §4/§11).
    - Every `runbook_url` in `infra/docker/prometheus/rules/` must still resolve under `dt-guard` after your edits. Renaming a heading that an anchor targets is a breaking change: grep `runbook_url` before renaming any heading, and keep existing anchors stable.
    - Layer validation must pass (`scripts/layer-all.sh`); this is docs-only, so failures are guard failures to fix, never to skip.
    - Acceptance:
      (1) The forward cutover order, the three-layer rollback text and the skew table exist in both deployment runbooks and do not contradict any surviving forced-order passage.
      (2) The `#mixed-transport-rollback-hazard` anchor exists and is cross-linked with MC Scenario 15's `transport_mode_mismatch` row.
      (3) The operator-lever section states the video-off lever with its MC-restart cost and its place before rollback.
      (4) Every new and retired key has a Configuration Reference row whose name matches the tree or INF-A's prompt.
      (5) The directive query reads withdrawals as healthy.
      (6) The premise sweep lists every site checked, with corrected or handed-off status, so a reviewer can see absence was checked, not assumed.
    - Gate-1 tier: light (documentation only), but review by infrastructure, protocol and observability is required because it carries R-25.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - The video-off lever (video cap 0) is documented with its blast radius. The runbook states plainly that a host unmute can resume a participant's camera or mic (R-18).
    - The runbooks state that server mute is reachable by any holder of the MH service credential via the registration contract, never "impossible" (story 2 R-29 wording, extended to per-kind).
    - Rollback across the v3 cutover is all components together (R-25). No runbook step proposes mixing v2 and v3 components.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-story3-ops-a-video-cutover-runbooks
  tag: story-2026-10-07-see-each-other-task-4
  tier: light
  tier_reason: docs-only runbook edits; no code, contract or security surface
- id: 5
  status: completed
  specialist: client
  deps:
  - 1
  prompt: |-
    Story 3 "see each other", task 5 (C-v3): the TypeScript frame codec for header v3, written from the ADR before any v3 vector exists (docs/user-stories/2026-10-07-see-each-other.md; requirements R-9, R-14 (codec half), R-22; governing design docs/decisions/adr-0036-media-flow.md §2 as amended by task 1 (P1)). Specialist: client. Pair with protocol as reviewer of layout conformance, and with security as reviewer of the authenticated ranges.

    DEPENDENCY. Task 1 (P1, protocol) lands the normative v3 layout table in ADR-0036 §2 and the additive proto changes. That table is your ONLY specification. Task 6 (P2, protocol) comes AFTER you. P2 regenerates `proto/test-vectors/` for v3 from the non-production Rust generator, flips `PROTOCOL_VERSION` to 3 across the cluster, regenerates `packages/sdk-core/src/media/frame/wireConstants.ts`, and deletes the SDK v2 path. Client is paired on P2.

    WHY THIS TASK EXISTS SEPARATELY (R-22). The cross-language vectors only catch a defect if the TypeScript and Rust codecs are independent derivations. ADR-0036's "What the discharge rests on" paragraph says the gate is exactly as strong as that independence. So you write the TS v3 codec from the ADR table BEFORE the v3 vectors and the v3 Rust codec exist.
    - Do NOT read `crates/media-protocol/src/frame.rs`'s v3 changes, the generator, or any draft vectors.
    - If the ADR table is ambiguous, ask protocol for an ADR clarification. Never resolve an ambiguity by reading the Rust.
    - Record in the module header that this codec was authored from the ADR table by C-v3, and that replacing either side with a port of the other would hollow out the R-22 gate.

    THE V3 LAYOUT (story file Design §Wire freeze; protocol's FINAL message). It is big-endian throughout. The ADR table governs if anything here disagrees, and you report the disagreement:
    - `version` (u8, = 3)
    - `flags` (u8: independently-decodable, discardable, key-bearing; every other bit is a decode reject)
    - `payload_length` (u32)
    - `stream_sequence` (u32)
    - `gop_counter` (u32, NEW)

    This makes a 14-byte fixed publisher prefix, which grows by 4 from v2's 10 (v3 adds ONLY `gop_counter`). It is followed by:
    - the wrapped transmit key (50 bytes, iff key-bearing);
    - `ext_len` (u16) and the extension TLVs (registry unchanged);
    - the relay region: `stream_id` (u16), `hop_sequence` (u32);
    - the payload (SFrame object);
    - the Ed25519 signature (64).

    The AEAD associated data stays the publisher region only. The signed range stays publisher region ‖ payload, skipping the relay region. The existing two-deviation header comment in `frameCodec.ts` applies unchanged and must be kept accurate for v3.

    New semantics the codec must enforce or expose:
    - The relay region is unchanged in layout. Its `stream_id` is HOP-LOCAL (Amendment 3, ADR §2's original design): on the uplink the publisher writes MC's send id (`SendStream.stream_number`) there, which MH uses to attribute the frame; on the downlink MH rewrites it to the receiver's slot id. It stays outside the AAD and the signed range. The encoder takes the relay `stream_id` as an input and does not interpret it. Stamping MC's send id on the uplink is the publish path's job task 11 (C-A1), not this task's.
    - `gop_counter` is exposed on decode and is an encoder input. Its semantics (per sender and stream; increments on every independently-decodable frame, so every audio frame; never wraps) belong to the sender: the owner lands with the v3 flip in task 6 (P2), and video's use of it is task 13 (C-A2). The codec only carries it.
    - A frame with BOTH key-bearing and discardable set is a decode reject in every implementation (ADR §4: a key-bearing frame is never discardable). The token is `key_bearing_discardable`, which P2 adds to the vectors; never mint a client-only token.
    - The maximum-payload check against `payload_length` happens before any slice or allocation, as today.

    TRANSITIONAL DUAL VERSION. Until P2, the cluster runs v2. The SDK must encode and decode whichever version MC directs in `SendDirective.header_version`, and decode by the received `version` byte.
    - Keep v2 working and every existing v2 vector green.
    - Select the version from the existing `HEADER_VERSION` SSoT / directive, never from a literal.
    - The dual path is explicitly temporary: P2 deletes v2 in the same task as the flip, so dual-encode cannot outlive the story. Mark every v2 branch with a comment naming P2 as its deletion owner.

    TESTS (no fenced blocks in docs or comments):
    - Hand-built v3 round-trip unit tests from the ADR table:
      - every flag combination, including the key-bearing+discardable reject;
      - relay `stream_id` at 0 and 65535, with the AAD and signature unchanged when it is rewritten;
      - `gop_counter` at 0 and u32::MAX;
      - key-bearing and non-key-bearing, with and without extensions;
      - payload_length at 0, at max, and at max+1 (rejected before allocation);
      - truncation at every offset of a v3 header (each rejects, none throws untyped).
    - An assertion that the v3 AAD slice and signed range are taken from the received buffer as subarrays and never re-serialized, mirroring the existing `naive_contiguous_signed_input_hex` near-miss check: assert the naive contiguous range differs from the real signed range.
    - A self-consistency test is NOT evidence of correctness, so do not present it as such. Say in the test file header that P2's independent vectors are the gate this task exists to make meaningful.
    - The existing hot-path layout rule (`media/__tests__/hotPathLayout.test.ts`) stays green: `frameCodec.ts` remains logging-free.

    ACCEPTANCE:
    - v3 encode and decode implemented from the ADR table, with no reference to the Rust v3 work.
    - v2 unchanged and green.
    - The version is driven by the directive or SSoT.
    - The key-bearing+discardable reject is in place.
    - `pnpm` lint, typecheck and unit tests are green for `packages/sdk-core`.
    - No env-tests: nothing is cluster-observable until P2 flips the version.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Written from the ADR layout table and the spec, NOT from the Rust generator (preserves ADR Assumption 4's independence).
    - `gop_counter` (the only v3 addition; fixed prefix 10 → 14 bytes) is inside the signed and AAD-covered publisher region. Every byte is decoded or rejected, and unknown flag bits reject.
    - Key-bearing + discardable is a decode reject.
    - Dual-encode is transitional only and is removed by task 6 (P2). There is no runtime path to choose v2 except MC's directive during the transition.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-c-v3-ts-frame-codec
  tag: story-2026-10-07-see-each-other-task-5
- id: 6
  status: completed
  specialist: protocol
  deps:
  - 1
  - 5
  prompt: |-
    Frame header v3 cutover (docs/user-stories/2026-10-07-see-each-other.md; requirements R-9, R-14 (key-bearing never discardable), R-20 (hop-local uplink relay stream id as MH's attribution label, read by the header-prefix parse), R-21 (codec half: header-prefix parse, cut-through reader, fuzzing), R-22). Task 6 (P2), protocol. Pair with client for the SDK edits and the cross-language gate (client wrote the TS v3 codec in task 5 (C-v3) and must confirm its conformance), and with security as reviewer of the vectors, the reject vocabulary and the parser's trust boundary. media-handler and test review the header-prefix API they consume.

    DEPENDENCIES. Task 1 (P1, protocol): landed the NORMATIVE v3 layout table in ADR-0036 §2 and every proto shape; read §2 and implement from it. Task 5 (C-v3, client): landed the TypeScript v3 codec in `packages/sdk-core/src/media/frame/`, written from the §2 table before any v3 vectors existed, living beside the v2 path and encoding whichever `header_version` MC directs (today 2). This task is where that codec first meets vectors produced by an independent implementation; that meeting is what R-22's "two implementations written from the spec, not from each other" buys, so DO NOT write the generator by reading the TS codec, and do not "fix" the TS codec to match the generator without first deciding, against §2, which side is wrong (a disagreement is a finding; record it in main.md and resolve it against the spec; if §2 itself is ambiguous, amend §2 first).

    WHAT THIS TASK DOES, ATOMICALLY. The v2-to-v3 flip must happen in ONE task so no per-task gate ever runs a mixed pair (operations' sequencing constraint; story-2 audio browser specs must stay green at this task's gate). After this task: the vectors are v3; `crates/media-protocol` is v3-only; `PROTOCOL_VERSION = 3`, so MC directs 3 and MH decodes 3 by derivation with no MC/MH source edit beyond test fixtures; the SDK encodes and decodes v3 only. Delete EVERY v2 path in the same change -- the Rust v2 encode/decode, the SDK's v2 path that task 5 (C-v3) kept transitionally, the v2 vectors file, v2-only fixtures -- so the transitional dual-encode cannot outlive the story (R-22: v3 is the only version; a peer on any other version is refused loudly as `unknown_version`; no allowlist, no floor). Declare this as an intentional wire break per `docs/protocol/CONVENTIONS.md` with predicted Layer-6 findings, if any.

    THE v3 LAYOUT (from ADR §2; restated so this prompt is self-contained -- §2 wins on any disagreement). Big-endian: version (1, = 3) | flags (1; bit0 independently-decodable, bit1 discardable, bit2 key-bearing, all others reject) | payload_length (4) | stream_sequence (4) | gop_counter (4, NEW) | wrapped transmit key (50, iff key-bearing: kek_generation 2, wrapped key 32, wrap tag 16) | ext_length (2) | TLV extensions | relay: stream_id (2), hop_sequence (4) | payload (opaque SFrame object) | Ed25519 signature (64). Fixed publisher prefix = 14 bytes (was 10); v3 adds ONLY `gop_counter` (wire-freeze Amendment 3: there is NO signed stream_number byte). Publisher region (signed and AEAD associated data) = all bytes before the relay region. `gop_counter` per (sender, stream): increments on every independently-decodable frame (audio: every frame), every frame of a group carries its keyframe's value, never wraps -- u32::MAX is the last legal value, after which the sender emits nothing more on that stream, counts the stop, and resumes only under a fresh stream number or sender id. Relay `stream_id` is HOP-LOCAL: on the uplink the publisher stamps MC's send id for the stream (`SendStream.stream_number`; audio 0, video 1 today), which is MH's ingress attribution label (publisher-set, unauthenticated, looked up in `source_streams`); on the downlink MH rewrites it to the receiving subscriber's slot id. Every byte is decoded or rejected; nothing is skipped.

    RUST CODEC (`crates/media-protocol`, protocol-owned):
    (1) v3 layout in `frame.rs` (field widths, offsets, `PUBLISHER_FIXED_PREFIX_SIZE`, the zero-copy view exposing `gop_counter`), `codec.rs` (all entry points), `PROTOCOL_VERSION = 3`. New decode reject `key_bearing_discardable` (both flags set) on every entry point incl. the header-prefix parse; keep reject-reason precedence documented and tested (`tests/reject_reasons.rs`). `encode_frame` refuses the same combination. Update the module docs' layout tables, the "Obligations this crate cannot enforce" section, and the hop-local relay-region semantics (uplink = MC send id, downlink = subscriber slot id), consistent with the comments task 1 (P1) already updated.
    (2) CUT-THROUGH SUPPORT for MH (R-21), consumed by task 8 (MH-A1, per-stream attribution of stream-carried frames by the uplink relay id) and task 9 (MH-A2, cut-through forwarding, edge state machine, per-hop downlink sequence rewrite), which wrap it with I/O, per-stream bounds and timeouts:
       (a) a header-prefix parse: given the first bytes of a stream-carried frame, returns Incomplete (with how many more bytes are needed, never allocating) or Complete with {header length up to and including the relay region, declared payload length, total frame length, flags, gop_counter, and the relay-region stream_id (MH's attribution label) and hop_sequence (which MH rewrites per hop on the downlink; there is no uplink loss accounting)}; it enforces `MAX_PAYLOAD_BYTES` against the length field BEFORE accepting anything (the trust boundary ADR §2 names: the defect passes fuzzing of a whole-frame decode while being wrong in the reader), validates every header byte (version, flag bits, key-bearing presence, extension grammar), and rejects `key_bearing_discardable`;
       (b) a header-slice relay rewrite that rewrites the relay region given only the header bytes, so the payload-plus-signature tail can stream through untouched;
       (c) an incremental chunk-fed header reader: a pure state machine (no I/O, no clock, no allocation beyond the bounded header) that accepts arbitrary chunk splits and yields the parsed header plus how many tail bytes follow;
       (d) fuzz targets in `crates/media-protocol/fuzz/fuzz_targets/` beside `codec_decode.rs`/`codec_roundtrip.rs`: one for the header-prefix parse, one driving the chunk-fed reader with arbitrary chunk splits of the same input and asserting the split never changes the outcome. Regenerate the fuzz corpus for v3 (`tests/corpus_seeds.rs`).
       Keep `decode_datagram` (audio, one frame per datagram, `trailing_bytes` reject) and `decode_stream_frame`/`peek_frame_len` coherent with the new parse -- one parser, several entry points, as today. Zero-copy and no-per-frame-allocation properties stay asserted.
    (3) Update `tests/byte_coverage.rs` (no-skipped-byte proof over the new fields), `frame_properties.rs`, `reject_reasons.rs`.

    GENERATOR AND VECTORS (`crates/media-vector-gen`, non-production; `proto/test-vectors/`):
    (4) Replace `proto/test-vectors/frame-v2.vectors.json` with `frame-v3.vectors.json` (`header_version: 3`), generated by `media-vector-gen` written from ADR §2. Rows, in addition to regenerating every existing v2 row at v3: a keyframe with gop_counter 0; gop_counter at u32::MAX (decodes; the sender-side stop is a spec statement, pinned in the row's comment); a key-bearing frame that is NOT the first of its group (key change mid-group, the "first N frames after any key change" rule); a tamper pair on `gop_counter` (signature verification fails); flipped/reserved flag bits; key-bearing plus discardable (rejected, `key_bearing_discardable`, has_vector true); a v2-labelled frame (refused, `unknown_version`); publisher rows carry the uplink relay `stream_id` = the stream's send id (audio 0, video 1), and a row demonstrating that the relay region is outside the signature (a changed relay stream_id still verifies); the insider-forgery row regenerated. Add to `reject_reasons`: `key_bearing_discardable`, and client's receiver tokens with `has_vector: false`: `self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch` (a two-frame property; no single row determines it). The over-max stream-reader case reuses `payload_length_exceeds_max`. The vectors file stays the single source of truth for the reject vocabulary (the SDK's `rejectReason.ts` checks set equality against it). The external SFrame-WG anchor (`proto/test-vectors/external/sframe-wg/`) is untouched and must stay green.
    (5) Rename every LIVE reference to the vectors file (guards, Rust and TS tests, `proto/test-vectors/README.md`, the protocol/test/security/operations/code-reviewer/dry-reviewer specialist INDEX files, `docs/observability/metrics/*.md` and `alerts.md` where they cite it, `docs/runbooks/devloop-validation.md`, ADR-0036's live text, `packages/sdk-core` sources and tests). Leave historical records (`docs/devloop-outputs/**`, completed story files) as they are.
    (6) Guard `scripts/guards/simple/validate-frame-vectors.sh`: retarget to v3. g3 (`header_version == PROTOCOL_VERSION`), g2 (max payload), g4 (legal flag mask), g8 (frame length), g11 (freshness), g13/g15 unchanged in intent, g14 (the cross-language flag): re-establish `gated_by.typescript: true` and `cross_language_property_established: true` ONLY after the TS v3 codec passes against these vectors in this task -- g14 hard-fails on drift between the flag and reality in either direction. Keep every PRECONDITION bail; there is no skip branch.

    AUDIO GOP COUNTER (lead Amendment R1; paired with client). v3 is the only encode path after this task, so every audio frame the SDK sends must carry a valid `gop_counter`. Add a NON-RESETTABLE per-(sender, stream) GoP counter on `TransmitKeyManager` (`packages/sdk-core/src/media/lifecycle/transmitKeys.ts`) beside `nextStreamSequence`, with the same ownership discipline that method's doc states: one manager per sender, never reset by teardown or key rotation, never per target (allocated once per frame above the multi-target fan-out in `pipeline/egress.ts::submit`, so every target carries identical sealed bytes). Audio advances it on every frame, because every audio frame is independently decodable (ADR §2 rule: increment on every independently-decodable frame; every frame of a group carries its keyframe's value). At u32::MAX it stops that stream, counted, and never wraps. Unit tests: monotonic per stream, independent across streams, survives rotation and teardown, identical across targets, the u32::MAX stop. This keeps the GoP invariant in one owner: task 13 (C-A2) reuses it for video keyframes and adds no second counter. About 20 lines plus tests; the counter is never logged, labelled or put on the test bus (R-30).

    SDK (paired with client): set `packages/sdk-core/src/media/frame/wireConstants.ts` `HEADER_VERSION = 3` and the new wire constants (the drift test `wireConstants.drift.test.ts` pins them against the vectors); run the TS codec's vector conformance against the new file; delete the v2 path task 5 (C-v3) kept; do NOT change `AudioPipeline.ts`'s uplink relay stamping here: stamping the MC-assigned send id through one function for audio and video is task 11 (C-A1). The SDK's rejection and counting of a directive whose `header_version` is not 3 belongs to task 11 (C-A1), not here.

    MC AND MH: no source change beyond what derivation gives (`crates/mc-service/src/media_signaling/directive.rs::header_version()` derives from `PROTOCOL_VERSION`; MH decodes through the codec). Update their test fixtures and frame builders to v3 mechanically (`crates/mh-service/tests/common/`, MH and MC unit tests, env-test fixtures that encode frames). MC's join-time refusal of a client that does not declare 3 is task 12 (MC-A).

    Acceptance criteria:
    - Workspace green through `scripts/layer-all.sh` at this task's gate, including Layer 7: the story-2 audio browser env-tests pass end to end on v3 (proves the flip is atomic and wired into the path that ships, not just the vectors).
    - The TS codec passes every v3 vector row; g14 flags re-established truthfully; the external anchor green.
    - `grep` for v2-only constants, the old vectors filename in live files and any v2 decode branch returns nothing.
    - Both fuzz targets build and run their smoke corpus; the chunk-split target proves split-invariance.
    - Header-prefix parse rejects an over-max length before requesting the payload; a unit test proves no allocation is sized from the length field.
    - Security reviews the reject vocabulary, the parse trust boundary, and that no text anywhere claims MH or the receiver authenticates the uplink stream label.
    - No env-test additions: the behaviours this task adds are codec-level; task 8 (MH-A1) owns the attribution and mute env-tests and task 9 (MH-A2) the cut-through and abandonment env-tests.

    Downstream: task 7 (T-A, test) builds video fixtures through this Rust codec; task 8 (MH-A1) and task 9 (MH-A2) consume the header-prefix parse, header-slice rewrite and chunk-fed reader; tasks 11/13 (C-A1/C-A2) and 14 (C-B) build the SDK video paths on the v3 codec. In repo text (code comments, docs, ADR) refer to these tasks by code only, never by number.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Vectors regenerated with rows for:
      - a GoP tamper pair;
      - counter 0 and u32::MAX;
      - a key-bearing non-first frame;
      - key-bearing + discardable rejected;
      - flipped flag bits;
      - v2 refused.
    - New tokens in `reject_reasons`, all has_vector false: `self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch`. There is no second client-side union. An over-maximum length on the stream reader reuses `payload_length_exceeds_max`.
    - The incremental chunk-fed header reader enforces MAX_PAYLOAD against the length field before acceptance and before any allocation. A fuzz target with arbitrary chunk splits covers the READER, not only decode.
    - The flip is atomic: v2 encode, decode and vectors are deleted in the same task, so dual-encode cannot outlive the story.
    - `gop_counter` at u32::MAX: the sender stops that stream, counted, and never wraps it or keeps sending silently.
    - The audio GoP counter (owned here) increments on every audio frame, is per (sender, stream), never resets while the sender id lives (stop/start and transmit-key rotation included), and lives on the same non-resettable owner as the stream sequence.
    - The generator and the TS codec stay independently authored.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-frame-header-v3-cutover
  tag: story-2026-10-07-see-each-other-task-6
- id: 7
  status: completed
  specialist: test
  deps:
  - 1
  - 3
  - 6
  prompt: |-
    Video test infrastructure for story 3 "see each other" (docs/user-stories/2026-10-07-see-each-other.md; ADR-0036 `docs/decisions/adr-0036-media-flow.md`). Requirements: R-28 (VP8 substrate probe, fail loud), R-32 (three-layer per-slot video verification helpers, an independent pattern detector, the injected-clock rule), and the shared fixtures that R-33's env-tests in other tasks consume. Test infrastructure only: fixtures and helpers with hermetic self-tests. No product code and no scenario env-tests; those ride in the consuming tasks listed below.

    Dependencies, and what each provides:
    - Task 1 (P1, protocol contract): the frozen wire shapes.
      - `MhServerMessage.keyframe_request { stream_number, gop_counter }`, where `stream_number` is the publisher's MC send id.
      - `MhClientMessage.recovery_request { slot_id, sender_id, gop_counter }`.
      - Both ride the bidi stream MH accepts after `MhConnectRequest`, with the same 4-byte length prefix, size-bounded.
      - The flat SlotState values (AVATAR_NO_VIDEO = 8, UNAVAILABLE = 9, AVATAR_CLIENT_MUTED = 10, AVATAR_SERVER_MUTED = 11; ACTIVE and FEWER_SOURCES_THAN_SLOTS kept).
      - The join's `publish_kinds` and `receive_capability`, the per-kind slot caps, and `CODEC_VP8`.
    - Task 3 (INF-A, infrastructure media config surface): the shared 9-key `media-codec-profile` ConfigMap source. The probe reads `MEDIA_VIDEO_CODEC`, `MEDIA_VIDEO_WIDTH`, `MEDIA_VIDEO_HEIGHT`, `MEDIA_VIDEO_FRAME_RATE_HZ` and `MEDIA_VIDEO_MAX_BITRATE_BPS` from it and never restates a literal.
    - Task 6 (P2, protocol v3 cutover), in `crates/media-protocol`: the v3-only Rust codec, the header-prefix parse, the chunk-fed incremental header reader, and the move of the existing `crates/env-tests/src/fixtures/media.rs` audio datagram onto v3. This task makes that audio fixture stamp real values: the uplink relay `stream_id` = the MC send id for audio (0 today; `media.rs` currently stamps 0xFFFF, which becomes `unmatched_stream` once MH attributes by relay id, and task 8 (MH-A1) relies on this task fixing it), and a per-frame `gop_counter`. Audio increments every frame because every audio frame is independently decodable.

    Wire facts are in the story file's Design §Wire freeze and are authoritative. Do not re-derive them. The ones this task encodes:
    - The v3 header adds only a u32 `gop_counter` after `stream_sequence`, making the fixed prefix 14 bytes.
    - `gop_counter` is per (sender, stream), increments on every independently-decodable frame, and stops at u32::MAX without wrapping.
    - Key-bearing plus discardable is a decode reject.
    - Stream ids are hop-local. The publisher stamps the uplink relay `stream_id` with MC's send id (audio 0, video 1 today). MH attributes by (connection-bound sender, uplink relay stream_id) looked up in MC's `source_streams`, and rewrites the relay id to the receiver's slot id on egress.

    Pairing:
    - Pair with client on the pattern spec and the test-bus projection the helpers consume. Client implements the generator in its publish-path work, task 11 (C-A1) or task 13 (C-A2) per client's cut. It implements the render-surface analysis and bus projection in task 14 (C-B) and task 17 (C-C1).
    - Pair with media-handler on the fixture behaviours and outcome table.
    - Pair with infrastructure on the `scripts/layer7.sh` lane edit, which is lane machinery.
    - Pair with security on the gating and no-identity rules.

    Deliverable 1: Rust env-test video fixtures, a new module `crates/env-tests/src/fixtures/video.rs`, registered beside `media.rs` and in its style. The fixture is keyless: payloads are opaque markers, as in `media.rs`. It holds and derives no key material.

    VideoPublisher, over the existing MH WebTransport fixture connection:
    - Groups: one unidirectional QUIC stream per group of pictures, written through the production `media-protocol` encoder (never a hand-rolled header). The first frame of each group is independently decodable, and the stream is finished when the next group begins. Frames go out at a fixture `frame_interval`, with a small default.
    - Group-start source: ONE knob with three values, default `None` (groups start only at stream start). Shape agreed with media-handler in planning; record the final names in the module docs.
      - `None`: never a group after the first. This is also the withhold-keyframes / never-keying source for task 10's (MH-B) pending gauge.
      - `OnKeyframeRequest { min_interval }` (task 10, MH-B). The publisher reads length-framed `MhServerMessage.keyframe_request { stream_number, gop_counter }` from its bidi control stream. It answers with a new group only if the request names its stream and `gop_counter` ≥ its current counter: finish the current uni stream, open a new one whose first frame is independently decodable, and increment `gop_counter`. A lower counter, another stream's number or an already-answered repeat produces nothing, so stale or repeated requests yield exactly one keyframe. `min_interval` is a fixture-side floor, default 0. The publisher records every request received, with its counter and arrival order, so tests can assert "a request was observed before the group start".
      - `Periodic(interval)` (task 9, MH-A2). A new group every `interval` on tokio time, so it is paused-time drivable in Rust integration tests too. The field doc and module doc state: fixture-only, never a production keyframe interval. ADR-0036 as amended has none; keyframes are on demand only. MH-A2 lands before MH-B adds keyframe requests, so its late-subscriber gate (first forwarded frame = the next periodic group start, never mid-group) uses this.
    - Two independent knobs:
      - (a) the LABEL, the uplink relay `stream_id`: by default the send id MC directed for the stream being sent; overridable to the other declared stream's id (relabelling) or an undeclared id.
      - (b) the transport: uni-stream-per-group or ≤MTU datagram.
      There is no key-id-stream knob (ruled out).
    - Modes:
      - keep-sending-while-muted: ignores withdrawal and mute. This is the patched client for R-16.
      - stall-mid-frame: writes a header and part of a payload, then stops (the R-21 stall bound).
      - reset-mid-group: R-12 abandonment.

    VideoSubscriber reads incoming uni streams with P2's chunk-fed header reader, enforcing the wire maximum payload before any allocation (overflow surfaces as the existing `payload_length_exceeds_max`). Per frame it reports:
    - the key-id sender;
    - the relay slot id;
    - the `gop_counter`;
    - whether the frame is independently decodable;
    - whether the stream ended by finish or by reset.
    It also reports whether the first frame on each new stream was a group start, so consumers can assert "never mid-group".

    Recovery-request sender: writes `recovery_request { slot_id, sender_id, gop_counter }` with every field explicit, so MH-B's outcomes are driven by value:
    - equal: forwarded;
    - lower: stale;
    - higher: counter_ahead, with no clamping;
    - sender_id not the slot's current source: source_mismatch, never forwarded upstream.

    Pin MH's expected outcome table verbatim in the module docs, as the expectation consumers assert. The publisher declares audio = send id 0 DATAGRAM and video = send id 1 STREAM_PER_GROUP.
    - An unknown uplink relay id, on either transport: dropped `unmatched_stream`.
    - A declared id on the wrong transport: dropped `transport_mismatch`.
    - Video relabelled as the audio id while video is server-muted, i.e. a patched client sending video bytes as ≤MTU datagrams: reaches AUDIO slots only, never a video slot.
    - Any frame from a sender whose every declared stream is DROP_AT_INGRESS: dropped `server_muted`.
    - A PAUSED (client-muted) stream: forwarded, and `paused_source_frames_total` increments.
    The docs name task 8 (MH-A1), task 9 (MH-A2) and task 10 (MH-B) as the asserting owners. The fixture's own tests do not assert MH behaviour.

    Hermetic self-tests, no cluster:
    - each knob produces exactly the bytes it claims, decoded through the production decoder;
    - stream framing round-trips with a read split at EVERY byte offset, the header split included;
    - `OnKeyframeRequest`'s counter rule against a scripted request sequence (equal or higher keys once; lower, other-stream and repeated do not), plus the request log and `min_interval`;
    - `Periodic` on paused time yields a group start every interval and none between; `None` yields none after the first;
    - recovery-request encoding round-trips through the prost types;
    - stall and reset modes behave as named.
    No wall-clock sleeps anywhere; use tokio paused time.

    Each consuming env-test file records in its header the sum, over its concurrently running test fns, of P × (N+M). `cargo test` runs one file's fns in parallel and the files sequentially. Task 16 (INF-C) sizes the Kind ceiling at about 120 against a largest per-file sum of 72 (`27_mc_slot_placement`). State this rule in `video.rs`'s module docs and in `crates/env-tests/README.md`.

    Deliverable 2: browser pattern detector, `packages/web-app/e2e/patternDetector.ts`, with its node-tier self-test `packages/web-app/tests/patternDetector.test.ts`.

    The pattern SPEC is written once in prose in `packages/web-app/e2e/README.md`. Client's generator restates it in its file header. The two share NO code and the detector never imports the generator, mirroring `testTone.ts` / `toneDetector.ts`. The frame is a grid of large flat blocks that VP8 at the Kind test profile cannot smear:
    - 16 ID blocks carrying the meeting-scoped `sender_id`, MSB first, black = 0 / white = 1. That is the full 16-bit space, so there are no collisions.
    - 4 guard blocks in a fixed checkerboard, catching blank, garbage and wrong-geometry frames.
    - 4 counter blocks holding a Gray-coded frame counter mod 16 that advances every CAPTURED frame.
    The only input is `sender_id`, never a durable identity (a visual watermark).

    The detector consumes, per video slot, the 24 block-mean lumas the test-build bus projects. They are sampled at the slot's RENDER surface (the canvas behind the SDK's `VideoSink` seam), never at the decoder output, so wrong-slot rendering is caught. It decides:
    - the decoded ID;
    - guard pass/fail;
    - liveness = the counter CHANGES across samples within the window. Never require "+1": abandonment (R-12) legitimately skips frames.
    Thresholds are named constants with documented margins. The self-test builds synthetic lumas from the README spec and covers:
    - every bit of a 16-bit id;
    - the guard failing on blank, inverted and shifted grids;
    - liveness with gaps (true) and frozen (false);
    - noise within margin accepted and beyond margin rejected;
    - a harness error, not a verdict, on malformed input.

    Deliverable 3: slot oracle, `packages/web-app/e2e/slotOracle.ts`, with node-tier test `packages/web-app/tests/slotOracle.test.ts`. R-32 layer 1 compares the key-id sender rendered in each ACTIVE video slot with the source the TEST expects from join order and fill rule. MC never names the source of a video ACTIVE slot: video ACTIVE carries no sender_id, while audio ACTIVE keeps it. So the test needs its own R-3, written from the story's rule text and sharing no code with MC's fill or MC's in-module oracle.

    R-3, the rule to implement: slots of each kind fill per subscriber by (1) the earliest joiner already holding a slot of the other kind but not this one, else (2) the earliest eligible joiner holding no slot of this kind.
    - Audio slots take audio publishers only.
    - Video slots take any participant sharing a connected handler with the subscriber, receive-only participants included (as AVATAR_NO_VIDEO), never the subscriber itself.
    - No join, leave, mute or redeclaration moves an occupied slot.
    - Over-cap declarations truncate to UNAVAILABLE.

    Inputs: join order, publish sets, per-kind N/M, caps and handler connectivity. Output per subscriber per slot: the expected occupant and state (AVATAR_* carries the named participant; AVATAR_SERVER_MUTED wins when both mutes apply), or fewer-sources / unavailable. The node test covers:
    - pair completion;
    - stability across a leave;
    - receive-only in a video slot;
    - truncation;
    - the accepted consequence that a no-camera participant holds a slot while a later camera goes unseen.

    Deliverable 4: cohort and evidence extensions. `packages/web-app/e2e/cohort.ts` and `cohortContexts.ts` gain a per-participant publish set (audio / video / both / receive-only) and per-kind N and M. Registration stays once per run within AC's auth-rate window, read from AC config.

    Video evidence helpers live beside `receiveEvidence.ts` and reuse `windowSampling.ts`'s stop rule. Per receiver, per ACTIVE video slot, against the oracle's expected sender:
    - layer 1: the key-id sender of rendered frames equals the expected occupant;
    - layer 2: the per-(slot, sender) verified-and-decrypted count advances;
    - layer 3: the detector reads the expected ID, the guard passes, and the slot is live.
    Separately they assert each slot's MC state (wire-name labels `active`, `avatar_no_video`, `avatar_client_muted`, `avatar_server_muted`, `fewer_sources`, `unavailable`) and what is actually rendered (video or avatar). Unreachable comes from the per-subscriber `unreachable_sender_ids`.

    Send-side helpers assert camera-track-live as exactly (directed AND NOT client-muted AND camera healthy). So:
    - an unwatched publisher's camera is off;
    - a server-muted participant's camera track has ended;
    - the mic stays open on client and server audio mute.

    Bus content is counts and booleans only (R-30). If the client projection is not yet present, self-test against the agreed shape; never invent a fallback source.

    Deliverable 5: VP8 substrate probe (R-28), one module `packages/web-app/e2e/vp8Probe.ts`. It runs in-page:
    - WebCodecs `VideoEncoder.isConfigSupported` and `VideoDecoder.isConfigSupported` for VP8 at the profile;
    - AND a real one-frame encode followed by a decode, checking that the decoded dimensions match. isConfigSupported alone can lie.
    It deliberately does NOT import the SDK's `probeVp8Support` (lead-ruled). It is an independent check, and the SDK's own fail-loud is tested in client's task.

    Two callers, both landing in this task; task 16 (INF-C) wires nothing for the probe:
    1. Layer 7: `packages/web-app/e2e/global-setup.ts` calls the probe ahead of cohort registration, following its every-check-throws-with-remediation rule.
       - The profile comes from the DEPLOYED `media-codec-profile` ConfigMap, so it is effective by construction once INF-C's Kind patch lands. layer7.sh holds kube access; if global-setup cannot run kubectl, layer7.sh exports the values.
       - A failure maps in `scripts/layer7.sh` to the Phase-1g PRECONDITION_FAILURE lane with `REASON=browser-vp8-unsupported`. That is non-green by construction: `scripts/lang/_common.sh:__status_rank()` ranks it above FAIL, the layer exits 2, and run-story halts.
       - Add the token to layer7.sh's REASON list and to `docs/runbooks/devloop-validation.md` with its remediation.
       - Add a hermetic `scripts/layer7.test.sh` case (the PATH-stub / `DEVLOOP_TEST` precedent) proving the routing.
    2. CI: a vitest BROWSER-MODE component test in `packages/web-app`, run by the existing `pnpm test:component` step of `ci-client.yml`'s Chromium job. There is no new workflow step and no new Playwright project, and a failure fails the job. The profile is read from the repo's base `media-codec-profile` source through `packages/web-app/e2e/configEnv.ts`, the suite's one config reader. If browser mode cannot read the file directly, inject the parsed values through the vitest config's `define`. Never restate them.
    A node-tier self-test (`packages/web-app/tests/vp8Probe.test.ts`) covers the verdict mapping: unsupported encoder, unsupported decoder, encode fails, dimension mismatch, pass. The probe never skips and never degrades the suite to audio-only.

    Consumers (each depends on this task; their env-tests ride in their own tasks, paired with test):
    - Task 8 (MH-A1): mixed audio+video attribution, transport_mismatch / unmatched_stream, the server-mute patched client, the relabel-while-server-muted case, and client mute still forwarding (PAUSED).
    - Task 9 (MH-A2): cut-through forwarding, a late subscriber's first frame at a group start (via `Periodic`), the mid-frame stall reset, and per-destination abandonment.
    - Task 10 (MH-B): late-joiner keyframe on request; the never-keying source's pending gauge as a per-meeting delta; recovery remap with stale / counter_ahead / source_mismatch; abandoned-subscriber resync. The switch race is MH-B integration-only on an injected clock.
    - Task 12 (MC-A):
      - `27_mc_slot_placement`: capability mix, truncation, pairing and stability, watched-only directives, two-handler video visibility;
      - `34_mc_kek_rotation`: the KEK reaches post-removal members while a video stream flows, and no video directive is re-issued. The publisher holds its stream open across the push.
    - Task 15 (MC-B), `35_mc_server_mute_teardown`: server video mute at every receiver, the patched client, audio/video mute independence, client video mute → PAUSED, the mute-storm push bound.
    - Task 17 (C-C1): the browser specs consume the detector, oracle, cohort and evidence helpers and the probe.

    Acceptance criteria:
    - Every new helper has a node-tier or hermetic Rust self-test (one per helper), passing in Layers 1–6 with no cluster.
    - The probe runs in this task's own Layer-7 run and passes on the devloop Chromium. Its precondition routing is proven by the layer7.test.sh case, and the component-tier caller passes.
    - No test or helper sleeps on wall time for a deadline (ADR-0036 §10). No helper gates on a wall-clock latency number (R-33).
    - The detector shares no code with the generator, the oracle shares none with MC, and the probe shares none with the SDK. Reviewers check the import graph.
    - Nothing in the fixtures, helpers or probe carries durable identity, key material, GoP values, frame sizes or keyframe timings into logs, labels or the bus (R-30, R-27).
    - No insecure browser flag anywhere (R-27). `playwright.config.ts` keeps its fake-media flags only, because the test-pattern camera replaces getUserMedia video in-app.
    - `crates/env-tests/README.md` and `packages/web-app/e2e/README.md` document the fixtures (including the fixture-only cadence), the pattern spec, the oracle's rule source, the per-file edge-sum header rule and the probe's REASON token.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - The VP8 test pattern and any patched-client harness, including the one used for the server-mute evasion env-test, are build-time gated. A production build throws and the bundle-content test proves absence. The pattern is derived from the meeting-scoped `sender_id`, never from user id, email or display name.
    - The pattern detector shares no code with the generator.
    - Test-bus projections carry no frame sizes, GoP values, key ids or per-stream timings.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-story3-video-test-infra
  tag: story-2026-10-07-see-each-other-task-7
- id: 8
  status: completed
  specialist: media-handler
  deps:
  - 1
  - 3
  - 6
  - 7
  prompt: |-
    MH-A1: per-stream ingress attribution against `source_streams`, per-stream server mute (DROP_AT_INGRESS) and client-mute pause (PAUSED), mixed-transport registrations, derived egress costs from the shared codec profile (docs/user-stories/2026-10-07-see-each-other.md; governing design docs/decisions/adr-0036-media-flow.md as amended by task 1 (P1); wire rulings in the story file's Design §Wire freeze). Covers R-16 (MH ingress enforcement of per-kind server mute), R-17 (PAUSED never drops), R-20 (attribution, mixed transports, per-stream echo), R-23 and R-24 (MH config reads, derived costs), R-30 (the series below), R-33 (MH-level env-tests on the datagram path), R-36 (MH-side comment corrections). Pair with protocol on `source_streams` semantics and the echo; pair with security on the slot-terms mute property and the apply-time rejects; pair with test on gates and env-tests. Covers: code, unit-tests, integration-tests, env-tests, deploy (config reads), observability.

    SCOPE BOUNDARY. This task establishes WHICH declared stream every ingress frame belongs to and what its disposition does. It does so on the datagram path, which exists today, and as a lookup that task 9 (MH-A2) calls from the new stream path. It does NOT accept unidirectional streams, forward video, or implement group-start gating; those are task 9. Video edges in a registration are accepted and installed here, and they deliver nothing until task 9 lands. That is a legal "forward nothing" state, not a half-applied invariant, because no ingress path for stream-per-group frames exists yet. The transport-seam extension for streams belongs to task 9, its first user.

    DEPENDENCIES.
    - Task 1 (P1, protocol), additive proto. `RegisterMeetingRequest.source_streams = 8`, a list of `SourceStream { sender_id, stream_number, transport_mode, disposition FORWARD|PAUSED|DROP_AT_INGRESS }`, where an UNSPECIFIED disposition or transport rejects. `EgressStream.transport_mode` is removed: an edge's transport derives from its candidate's declared SourceStream. The response echo is `applied_source_streams { sender_id, stream_number, transport_mode }`. P1 keeps the legacy `server_muted_sources`/`MutedSource` alongside until MC-B deletes them.
    - Task 3 (INF-A, infrastructure). The `media-codec-profile` ConfigMap (9 keys) and MH's required refs, incl. `MH_MAX_SOURCE_STREAMS_PER_MEETING`. `MH_STREAM_COST_AUDIO_BPS`, `MH_STREAM_COST_VIDEO_BPS` and `MH_MAX_MUTED_SOURCES_PER_MEETING` are made optional for two-deploy retirement.
    - Task 6 (P2, protocol). The v3-only Rust codec (14-byte fixed prefix: version, flags, payload_length, stream_sequence, gop_counter; key-bearing plus discardable rejected) with its header parse exposing the relay-region `stream_id`. The story-2 env-test fixtures are updated mechanically at the v3 flip.
    - Task 7 (T-A, test). The `fixtures/media.rs` audio fixture stamping uplink send id 0 (was 0xFFFF), and `crates/env-tests/src/fixtures/video.rs`: a publisher with an independent uplink-label override (default = send id) and a transport choice.

    ORDERING AND THE LEGACY PATH. Task 12 (MC-A) emits `source_streams` and mixed-mode video edges, and it depends on this task. Until MC-A lands, MC sends registrations with NO `source_streams`. Every intermediate commit must keep the story-2 audio env-tests green, so this task implements a bounded legacy path: a registration carrying an empty `source_streams` is treated as declaring stream 0 DATAGRAM for every sender, with each `server_muted_sources` sender DROP_AT_INGRESS. That reproduces story-2 behaviour exactly. The legacy path is one clearly marked function with a comment naming MC-B as its deleter. Task 15 (MC-B), paired with media-handler, deletes it with the superseded proto fields; do not let any other code depend on it. After that deletion, a registration carrying egress streams with an absent or empty `source_streams` is rejected as `rejected_transport_inconsistent` (every candidate is then undeclared, so the existing undeclared-candidate rule produces it with no new token); write the rule so it already holds for non-empty lists, and MC-B's test asserts the empty case once the legacy branch is gone. Name the legacy function's comment exactly: "transitional: deleted by MC-B with the superseded proto fields".

    1. ATTRIBUTION (R-20; `crates/mh-service/src/routing/mod.rs`, `media/forward.rs`). Stream ids are hop-local (Design §Wire freeze).
       - Each ingress frame is attributed by (the sender bound at attach, the UPLINK relay-region `stream_id`, read through the codec header parse), looked up in `source_streams`. The id is MC's send id: audio 0, video 1, pinned by MC as contract values; MH does not depend on the numbers.
       - The payload, including the SFrame key id, is never parsed.
       - Unknown id → drop `unmatched_stream`. The relay field is 16 bits while send ids are 8, so an id above 255 is simply unmatched.
       - A declared stream arriving on a transport other than its declared `transport_mode` → drop `transport_mismatch`, a cheap consistency check. Keep the two reasons separate: their remedies differ (declaration drift vs. sender or contract fault).
       - Attributed frames forward only along that source stream's edges, so audio never reaches a video slot and vice versa.
       - The lookup lives inside `MeetingRoutes`, preserving the module's meeting-scoping discipline: no lookup without a `MeetingKey`; indices are meeting-local. Expose it as the single function task 9 calls for stream frames.
       - The label is publisher-set and unauthenticated at MH. Write no comment implying MH authenticates or identifies a frame's true stream.

    2. APPLY-TIME REJECTS. The whole registration is rejected and nothing applied; count bounds are checked before any per-element work.
       - `rejected_invalid`: a duplicate (sender, stream_number) in `source_streams`, or an UNSPECIFIED disposition or transport.
       - `rejected_transport_inconsistent` (new `PolicyApplyOutcome`, added to `ALL` so it is zero-initialised; observability's alert A8 selects it): an egress candidate naming a stream absent from `source_streams`, or an egress whose candidates' declared transports differ.
       - Remove `PolicyRejection::HeterogeneousTransportModes`: one meeting legally mixes DATAGRAM audio and STREAM_PER_GROUP video.
       - Bound `source_streams` by the new required `MH_MAX_SOURCE_STREAMS_PER_MEETING` (default 1024, bounds 1..=4096). It replaces `MH_MAX_MUTED_SOURCES_PER_MEETING`; keep today's validated relationship to `MH_MAX_EGRESS_STREAMS_PER_MEETING`.
       - priority_group semantics: a lower number is higher priority (audio 1, video 2). Record this; task 9 maps it onto stream priority.
       - The echo `applied_source_streams` comes from the APPLIED snapshot, never the received one.

    3. DISPOSITIONS (R-16, R-17).
       - DROP_AT_INGRESS: frames of that source stream drop after attribution and before any routing work, counted `server_muted`.
       - Sender-level shortcut, checked FIRST: if every declared stream of the bound sender is DROP_AT_INGRESS, every frame from that sender drops as `server_muted`, unattributable ones included, so MH runbook scenario 19's signal stays right.
       - The R-16 guarantee holds in slot terms: a server-muted stream has no live route, and a relabelled frame must still match a declared (stream, transport), so it reaches only slots of the other stream's kind, where receivers decode by slot kind.
       - PAUSED (client mute): NEVER dropped at ingress, because a resume frame that beats MC's update must still reach subscribers. Arrivals are counted on `mh_media_paused_source_frames_total`, an overlay arrival counter, not a drop. The frames forward normally.
       - Task 9 adds the edge semantics of PAUSED: switch-completes-as-forward-nothing, and no keyframe wait. This task exposes the disposition in the routing snapshot for it.
       - MC keeps the edges of muted streams, so `egress_stream_id` and downlink hop sequences stay stable.
       - Re-push idempotency: an identical, or unrelated, re-push (MC re-pushes on every client-mute toggle) must not reset per-edge datagram hop sequences.

    4. DERIVED EGRESS COSTS (R-24; `crates/mh-service/src/config.rs:EgressAdmission`).
       - Formula: cost = MAX_BITRATE_BPS + per-frame overhead bits × FRAME_RATE_HZ, computed for BOTH audio and video from `media-codec-profile`.
       - Required reads: `MEDIA_AUDIO_MAX_BITRATE_BPS` (48000), `MEDIA_AUDIO_FRAME_RATE_HZ` (50), `MEDIA_VIDEO_MAX_BITRATE_BPS`, `MEDIA_VIDEO_FRAME_RATE_HZ`. `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` is read by task 10 (MH-B).
       - The overhead is derived from `media-protocol` size constants, never a literal: v3 fixed prefix, relay region, signature, SFrame overhead, and the wrapped key (every audio frame; amortised to zero for video).
       - No headroom knob. This derivation is interim; the ADR addendum's adaptive-capacity story replaces it.
       - Drop the reads of `MH_STREAM_COST_AUDIO_BPS` and `MH_STREAM_COST_VIDEO_BPS`. This closes docs/TODO.md "MH_STREAM_COST_AUDIO_BPS under-costs audio"; remove that entry.
       - Keep the type-blind worst-case-of-kinds costing. Keep `mh_media_egress_stream_ceiling` publishing the one derived value admission enforces.
       - `SUPPORTED_AUDIO_BITRATE_FLOOR_BPS`/`CEILING_BPS` and `AUDIO_FRAME_DURATION_MS` stay constants, because they size the datagram send buffer at compile time. MH refuses to boot if `MEDIA_AUDIO_MAX_BITRATE_BPS` is outside floor..=ceiling, or `MEDIA_AUDIO_FRAME_RATE_HZ` ≠ 1000 / `AUDIO_FRAME_DURATION_MS` (ADR §11 startup-validation fallback).
       - Re-derive `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` as P × (N+M) from named demo constants P=6, N=5, M=4, giving 54. It replaces `all_hear_all_streams(RECOMMENDED_MIN_DEMO_PEERS)`. `MIN_EGRESS_STREAM_CEILING` stays 2.
       - Unit tests pin: the exact derived audio cost (about 90 kbps); the derived video cost at the Kind test profile (about 0.265 Mbps at 320x180, 15 fps, 250 kbps) and at production defaults; and `base_placeholder_budget_is_below_recommended_min` (`MH_EGRESS_BUDGET_BPS=10000000` yields a ceiling below 54, so the configure-me nudge fires; infrastructure's config.env comment cites the test by name).

    5. CONFIG (R-23). Every key this task reads must be:
       - startup-validated;
       - logged absent-vs-defaulted per the existing convention;
       - documented in `infra/services/mh-service/config.env`;
       - covered by `crates/dt-guard/src/env_config.rs`. Task 3 lands the manifests first.
       Keys: `MH_MAX_SOURCE_STREAMS_PER_MEETING` plus the four `MEDIA_*` keys above. The stream-path keys and `MH_POD_MEMORY_LIMIT_BYTES` belong to task 9; the keyframe keys to task 10.

    6. FIXTURE PREREQUISITE. Attribution by the uplink relay id relies on task 7 (T-A) having changed `crates/env-tests/src/fixtures/media.rs` to stamp the audio send id 0 instead of 0xFFFF; do not re-edit it. Check any other integration-test frame builder MH ingests (in `crates/mh-service/tests/` and `crates/mh-test-utils/`) stamps the right send id, and fix those in this task.

    7. TELEMETRY (R-30; ADR §11). Handles are resolved at setup. No metric, log or span macro under `media/`. No media_kind or identity label. No stream id, size or timing in any log, label or span.
       - New `MediaDropReason` tokens, added to `ALL`, zero-initialised, direction Ingress: `unmatched_stream`, `transport_mismatch`.
       - `mh_media_paused_source_frames_total`.
       - The `rejected_transport_inconsistent` policy outcome.
       - A config gauge for `MH_MAX_SOURCE_STREAMS_PER_MEETING`.
       - Preserve the accounting identity: `mh_media_frames_forwarded_total{direction="ingress"}` is incremented once per ingress frame, never per edge, and `forwarded{ingress}` + ingress-direction drops = every frame received. Observability divides by it, so add no duplicate ingress counter.
       - Every new series lands IN THIS TASK with a catalog row in `docs/observability/metrics/mh-service.md` and at least a basic panel in `infra/grafana/dashboards/mh-media.json`. The `application-metrics` guard (`crates/dt-guard/src/application_metrics.rs`, checks 5 and 6) fails otherwise. Panels follow `docs/observability/dashboard-conventions.md`: explicit `editorMode`, counters as rates per ADR-0029. Task 20 (OBS-B) owns layout, alert panels and thresholds.
       - Correct `docs/observability/label-taxonomy.md` wherever it implies MH kind labels.

    8. COMMENT CORRECTIONS (R-36, MH side).
       - Rewrite the "PER-STREAM INGRESS MUTE IS FORECLOSED" and "exact while meetings are audio-only" text in `media/forward.rs` (and confirm task 1 did `internal.proto`) to the slot-terms property. It must say the uplink relay label is publisher-set and unauthenticated at MH, and that signing it would protect nothing, because the publisher sets both the label and the key id.
       - Update `docs/specialist-knowledge/media-handler/INDEX.md`.

    GATES (Tier 1a/1b, ADR §10; MetricAssertion per ADR-0032 for every counter claim):
    - the attribution outcome table below, on hand-built frames through `forward_one`;
    - a frame whose SFrame key-id stream disagrees with its relay label routes by the relay label (MH never reads the key id);
    - DROP_AT_INGRESS drops per kind while the other kind flows;
    - sender-level shortcut;
    - PAUSED forwarded and counted;
    - each apply-time reject fires with its outcome token and leaves the prior policy live;
    - a mixed-mode registration applies, and its echo lists every applied source stream;
    - the legacy path reproduces story-2 behaviour, including sender-level server mute;
    - the multi-meeting negative test extended: a (sender, stream id) declared in meeting A does not resolve in meeting B;
    - re-push preserves hop sequences;
    - the derived-cost and recommended-minimum tests;
    - boot refusal on out-of-envelope audio profile values.

    ATTRIBUTION OUTCOME TABLE (datagram path; publisher declared audio = id 0 DATAGRAM and video = id 1 STREAM_PER_GROUP; "label" = uplink relay stream_id). Task 9 extends it to the stream path.
    - label 0 datagram → forwarded to audio slots only;
    - label 1 datagram → `transport_mismatch`;
    - undeclared label (e.g. 7, or above 255) → `unmatched_stream`;
    - label 0 datagram while the sender's video stream is DROP_AT_INGRESS (a patched client smuggling video bytes as audio) → audio slots only, never a video slot;
    - sender with every declared stream DROP_AT_INGRESS → `server_muted`;
    - PAUSED stream → forwarded and counted.

    ENV-TESTS (Covers: env-tests). New `crates/env-tests/tests/<next free number>_mh_stream_attribution.rs`. Policy is driven through `crates/env-tests/src/fixtures/mh_grpc.rs` acting as MC; datagrams come through `fixtures/media.rs` and task 7's publisher. Record in the file header the sum, over concurrently running test fns, of P × (N+M), planned at most 30. Assert per-entity deltas, never shared absolutes. No wall-clock gates.
    - a registration mixing DATAGRAM audio and STREAM_PER_GROUP video edges applies and echoes every source stream;
    - per-kind server mute: an audio DROP_AT_INGRESS drops that sender's audio at every receiver while the stream stays declared;
    - the patched-client datagram case reaches audio slots only;
    - `transport_mismatch` and `unmatched_stream` counted on the datagram path;
    - a PAUSED stream still forwards;
    - the story-2 audio env-tests (26, 27, 28, 29, 32) stay green, both through the legacy path and with explicit `source_streams`.

    Out of scope: the stream transport seam, uni-stream ingress, cut-through, edge state machine, abandonment, stream limits and the memory check (all task 9); keyframe and recovery requests (task 10); proto edits (task 1); codec (task 6); dashboards and alerts beyond the basic panels (task 20); runbooks (tasks 4 and 19).

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Attribution is by (bound sender, uplink relay `stream_id`) against `source_streams`, with the arrival transport checked for consistency. No payload parse.
      - A frame with no match is dropped as `unmatched_stream`; a declared stream on the wrong transport as `transport_mismatch` (separate reasons).
      - Tested at MH: a frame relabelled with the sender's other declared id is forwarded only along that stream's edges.
    - Apply-time hard rejects, counted: a duplicate (sender, stream); an egress whose candidates declare different transport modes (`rejected_transport_inconsistent`); a candidate not in `source_streams`.
    - Server mute: disposition DROP_AT_INGRESS is checked after attribution and before routing, counted `server_muted`; the sender-level all-streams-muted shortcut runs first. PAUSED is never dropped at ingress.
    - Env-test: a patched client sending while video-server-muted, with frames labelled as video AND relabelled as its audio stream over datagrams, puts nothing into any subscriber's video slot. Its audio is unaffected.
    - Per-source-stream state, including the GoP state used by task 10 (MH-B), is bounded by `MH_MAX_SOURCE_STREAMS_PER_MEETING` and freed on unbind and on unassign.
    - All new code sits under `media/`, inside the directory-scoped telemetry deny. Only pod-level reason-labelled counts and gauges are emitted. No GoP value, frame size, timing or sender/stream identity appears in any log, label, span or exemplar.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-mh-stream-attribution
  tag: story-2026-10-07-see-each-other-task-8
- id: 9
  status: completed
  specialist: media-handler
  deps:
  - 3
  - 6
  - 7
  - 8
  prompt: |-
    MH-A2: stream-per-group cut-through video forwarding, edge state machine, per-destination abandonment with reset, stream limits, memory startup check (docs/user-stories/2026-10-07-see-each-other.md; governing design docs/decisions/adr-0036-media-flow.md as amended by task 1 (P1); wire rulings in the story file's Design §Wire freeze). Covers R-10 (forwarding half: group-start gating and switch semantics; keyframe requests are task 10), R-12 (MH-side per-destination abandonment and propagation), R-16 and R-20 (the stream path through task 8's attribution), R-17 (PAUSED edge semantics), R-21, R-23 (stream-path keys), R-30 (stream-path series), R-33 (MH-level env-tests). Pair with protocol on consuming the media-protocol parser pieces; pair with security on reset-never-finish and the stream-path limits; pair with test on the transport-seam shim, gates and env-tests. Covers: code, unit-tests, integration-tests, env-tests, deploy (config reads), observability.

    DEPENDENCIES.
    - Task 8 (MH-A1). The attribution lookup on (bound sender, uplink relay stream_id) against `source_streams`, with dispositions. The `unmatched_stream`, `transport_mismatch`, `server_muted` and paused-arrival accounting. Mixed-mode registrations applying. This task calls that lookup from the stream path and adds no second attribution rule.
    - Task 6 (P2, protocol). In `crates/media-protocol`:
      - the header-prefix parse, returning header length, payload length (with the maximum enforced against the length field before anything is accepted, rejecting with the existing `payload_length_exceeds_max`), flags, gop_counter, and the relay stream_id and hop;
      - the header-slice relay rewrite;
      - the incremental chunk-fed header reader, a pure state machine, plus its chunk-split fuzz target.
      This task wraps them with I/O, bounds and timeouts, and reimplements none of them.
    - Task 7 (T-A, test). `fixtures/video.rs`: a VideoPublisher opening one uni stream per group, with a uplink label override and a transport choice, plus T-A's single group-start-source knob set to `Periodic(interval)` (tokio time, millisecond granularity; fixture-only, never a production keyframe interval). This task's gating tests use it because keyframe requests arrive only with task 10. Also a VideoSubscriber reporting slot, gop_counter and independently-decodable per frame.
    - Task 3 (INF-A, infrastructure). The stream-path keys below as required refs, and `MH_POD_MEMORY_LIMIT_BYTES` via the downward API (`resourceFieldRef: limits.memory`, divisor 1), with infrastructure's guard failing any container that sources an env var from an undeclared limit.

    ORDERING. Task 12 (MC-A) needs task 8 for MH to accept its registrations, and this task for video frames to be delivered.

    1. TRANSPORT SEAM (ADR §10; `crates/mh-service/src/transport/mod.rs`, `webtransport/media_transport.rs`, shim in `crates/mh-test-utils/src/`; this task is the seam's first stream user).
       - Add an accept-unidirectional-stream method, a receive-stream type with chunked reads, and reset on the send stream. Hot-path per-connection I/O stays behind the trait, so cut-through runs with zero syscalls in tests.
       - The shim, built with test, must:
         - deliver inbound uni streams in caller-chosen chunks, splitting at every point including inside the header;
         - inject a publisher reset mid-frame and a publisher stall mid-frame;
         - expose send streams with controllable credit (a slow subscriber);
         - count stream creations and concurrent streams;
         - carry the MH↔client control envelope both ways (plumbing for task 10).
       - Deadlines use tokio paused time or an injected clock. No test sleeps on wall time.

    2. STREAM INGRESS (new files under `crates/mh-service/src/media/`, so the ADR §11 directory-scoped macro deny covers them).
       - Accept publisher uni streams. A stream carries one group, and its first frame is the group start. Each frame's header is read through the chunk-fed reader, then attributed through task 8's lookup using the uplink relay stream_id and the arrival transport (stream).
       - Attribution failures close the stream with RESET, counted under task 8's reasons:
         - label 0 over a uni stream → `transport_mismatch`;
         - an undeclared label → `unmatched_stream`;
         - a DROP_AT_INGRESS stream → `server_muted`. Keep reading and discarding that stream rather than stopping it, so a lifted mute resumes on the same group.
       - Count one `mh_media_frames_forwarded_total{direction="ingress"}` per attributed frame, at header completion, never per chunk or per edge. This preserves the accounting identity.
       - Per-connection stream-open rate limit at accept (`MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`). Refused streams are counted `stream_rate_limited`.
       - Concurrent uni streams stay bounded by the existing declared `MH_MAX_CONCURRENT_UNI_STREAMS` (64, quinn-enforced). There is no MH event to count, so publish the declared limit and the current concurrent ingress groups as gauges.
       - A stream ending mid-frame is `partial_frame_discard`, now reachable.
       - Correct `caps.rs`'s "creation-rate cap not enforceable" text.
       - Keep `stream_receive_window` 64 KiB and `receive_window` 256 KiB unchanged.

    3. CUT-THROUGH EGRESS (R-21).
       - For each attributed frame, write a per-subscriber rewritten header: the header-slice rewrite sets the relay stream_id to the subscriber's slot id and the hop to this edge's next number. The payload and signature then pass to each subscriber chunk by chunk as they arrive, as refcounted `Bytes`, zero-copy for the payload.
       - No whole-frame reassembly. Per incoming stream MH holds only a header and the chunk in flight.
       - Downlink hop sequence: per (subscriber connection, media stream), keyed on `egress_stream_id`. It continues across group streams and is never per QUIC stream. A deliberately unforwarded frame consumes no number (ADR §2).
       - One downlink uni stream carries exactly one slot. Every new group or source change opens a new stream.
       - priority_group maps onto quinn send-stream priority for stream-per-group egress, lower number higher (audio 1, video 2); datagrams already go ahead of stream data at packet assembly.

    4. EDGE STATE MACHINE (R-10, R-17). A pure state machine (`media/edge.rs`) on an injected clock, unit-testable with no QUIC.
       - Waiting: no group yet. A subscriber's first forwarded frame from any source is a group start, never mid-group.
       - Forwarding.
       - SwitchPending: the slot moved A→B. Keep forwarding A until B's group start, then finish A's egress stream and open B's. If B leaves before a group start, abandon the switch and keep A.
       - Paused: a switch to a PAUSED source completes immediately as "forward nothing". It cancels any group-start wait and stops forwarding the previous source. When frames from that source later arrive (a resume that beat MC's update), they forward from the next group start.
       - The rule is a behaviour, not a media type: a source whose every frame is independently decodable (audio) never waits.
       - Edge runtime state lives outside the immutable ArcSwap snapshot, keyed by `egress_stream_id` and reconciled on every apply. An identical or unrelated re-push preserves each edge's state and hop sequence (ADR §10 re-assert idempotency).
       - Expose a "waiting with source seen" signal that task 10 consumes for keyframe requests.
       - There is no give-up timeout while a source stays assigned (R-4).

    5. PER-DESTINATION ABANDONMENT (R-12, R-21). Every abandonment path RESETS, never FINISHes; a half-written frame must never look complete.
       - Each subscriber egress stream has a byte-capped backlog (`MH_EGRESS_STREAM_BACKLOG_BYTES`). Past the cap, MH resets that subscriber's stream, returns the edge to Waiting and counts `subscriber_backlog`. The publisher, the source's other subscribers and every subscriber's audio are unaffected.
       - A publisher stalled mid-frame beyond `MH_STREAM_MID_FRAME_STALL_MS` is reset downstream, counted `publisher_stall`.
       - A publisher reset of its group stream propagates as downstream resets (`publisher_reset`), with the edges returning to Waiting.
       - A publisher finish is a normal group end.
       - An ended source ends every downstream stream (`source_ended`).
       - An edge removed mid-group ends its stream (`edge_removed`).
       - Recovery always comes from the next hop's keyframe request, added in task 10 on top of the Waiting state. Until then a Waiting edge resumes at the publisher's next group start. MH never self-triggers anything on abandonment.

    6. MEMORY STARTUP CHECK (R-21). MH refuses to boot if backlog cap × derived egress stream ceiling + the existing receive-side bound × `MH_MAX_CONNECTIONS` exceeds a fixed, documented fraction of `MH_POD_MEMORY_LIMIT_BYTES`. It logs the effective limit at boot. Infrastructure's guard is the primary defence against an absent limit; add no further mechanism.

    7. CONFIG (R-23). Required keys:
       - `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`: default 10, bounds 1..=100.
       - `MH_EGRESS_STREAM_BACKLOG_BYTES`: default 262144, bounds 16384..=4194304.
       - `MH_STREAM_MID_FRAME_STALL_MS`: default 1000, bounds 100..=10000.
       - `MH_POD_MEMORY_LIMIT_BYTES`: no default.
       Each is startup-validated, logged absent-vs-defaulted, documented in `infra/services/mh-service/config.env`, and covered by `crates/dt-guard/src/env_config.rs`.

    8. TELEMETRY (R-30; ADR §11). Handles are resolved at setup. No macro under `media/`. No media_kind or identity label. No gop value, frame size or per-stream timing anywhere.
       - New `MediaDropReason` tokens, added to `ALL`, zero-initialised, each with its direction classified: `stream_rate_limited`, `partial_frame_discard`.
       - `mh_media_group_starts_total{direction=ingress|egress}`.
       - `mh_media_group_resets_total{reason=subscriber_backlog|publisher_reset|publisher_stall|source_ended|edge_removed}`, counting groups and kept out of the frame identity.
       - Gauges for concurrent ingress groups, the declared concurrent-stream limit, the backlog cap bytes, the stall bound and stream-transport sources active.
       - Stream-path latency: the existing decomposed histogram measures header-arrival → rewritten-header-written, sampled 1-in-N at random. Payload chunk timing is never observed.
       - Every new series lands IN THIS TASK with a catalog row in `docs/observability/metrics/mh-service.md` and a basic panel in `infra/grafana/dashboards/mh-media.json`. That is required by the `application-metrics` guard (checks 5 and 6); follow `docs/observability/dashboard-conventions.md`. Task 20 (OBS-B) owns layout and alerts.
       - Update `docs/specialist-knowledge/media-handler/INDEX.md` with the new files.

    GATES (Tier 1a/1b, ADR §10; MetricAssertion per ADR-0032; all deadlines on the injected clock):
    - zero-copy payload fan-out asserted on buffer refcount;
    - frames in equal frames out across every chunk split, header splits included;
    - a subscriber's first forwarded frame is a group start;
    - A is kept until B's group start; the switch is abandoned and A kept if B leaves;
    - a switch to PAUSED completes immediately and cancels the wait;
    - a DROP_AT_INGRESS video stream is discarded while the sender's audio flows;
    - the stream-path attribution rows below;
    - one slow subscriber never stalls a fast one, the publisher, or anyone's audio;
    - concurrent unfinished groups stay at or below the application bound and below the declared QUIC ceiling;
    - the mid-frame stall reset fires at the bound;
    - reset, never finish, on every abandonment path;
    - publisher reset propagates;
    - the downlink hop continues across group streams, and an unforwarded frame consumes no number;
    - re-push preserves edge state and hop;
    - `stream_rate_limited` and `partial_frame_discard` reachable;
    - the memory startup check refuses an oversize configuration.

    STREAM-PATH ATTRIBUTION ROWS (extending task 8's table; audio = id 0 DATAGRAM, video = id 1 STREAM_PER_GROUP):
    - label 1 uni stream → forwarded to video slots only, cut-through;
    - label 0 uni stream → `transport_mismatch`;
    - undeclared label uni stream → `unmatched_stream`;
    - label 1 uni stream while video is DROP_AT_INGRESS → `server_muted`, no video slot receives anything;
    - label 1 uni stream while video is PAUSED → forwarded and counted as a paused arrival.

    ENV-TESTS (Covers: env-tests). New `crates/env-tests/tests/<next free number>_mh_video_forwarding.rs`. Policy is driven through `fixtures/mh_grpc.rs` acting as MC; media comes through task 7's `fixtures/video.rs`. Record the concurrent edge sum in the file header, planned at most 30. Assert per-entity deltas. No wall-clock gates.
    - A mixed audio+video meeting forwards each kind only to its own slots, cut-through.
    - A late subscriber's first video frame is a group start: the fixture publisher's group-start source is `Periodic(interval)`, and the subscriber receives nothing until the next group start.
    - Server video mute drops video at ingress at every receiver while the same sender's audio forwards.
    - `transport_mismatch` for audio over a uni stream.
    - A PAUSED video stream still forwards.
    - A subscriber held below its backlog cap is reset and resumes at a later group while a second subscriber is unaffected.
    - The story-2 audio env-tests stay green.

    Out of scope: attribution rules and dispositions themselves (task 8); keyframe and recovery requests and pending metrics (task 10); proto (task 1); codec and parser (task 6); dashboards and alerts beyond the basic panels (task 20); runbooks (tasks 4 and 19).

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - MAX_PAYLOAD is checked against the length field before a frame is accepted. Only a header and the chunk in flight are held per incoming stream.
    - A mid-frame publisher stall past the configured bound, or a per-destination abandonment, RESETs the downstream stream and never finishes it (a half-written frame never looks complete). An ended or reset source ends every downstream stream.
    - Per-connection stream creation-rate and concurrent-stream limits apply before per-frame work, counted. QUIC max concurrent uni streams is declared explicitly.
    - The per-subscriber backlog is capped. Abandonment affects only that destination and never stalls other subscribers or audio.
    - Memory startup check: the backlog cap × streams is validated against the pod memory limit (downward API), and the pod refuses to boot when it exceeds the limit.
    - The per-hop downlink sequence rewrite is per (connection, media stream) and consumes no number for an unforwarded frame.
    - Same deny-scope and no-identity telemetry rule as task 8.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-mh-a2-stream-forwarding
  tag: story-2026-10-07-see-each-other-task-9
- id: 10
  status: completed
  specialist: media-handler
  deps:
  - 1
  - 3
  - 6
  - 7
  - 9
  prompt: |-
    MH-B: MH on-demand keyframes: MH-to-publisher KeyframeRequest, receiver RecoveryRequest remap and counter dedupe, pending visibility, and the amendment-4 cleanup of landed text and config (docs/user-stories/2026-10-07-see-each-other.md; governing design docs/decisions/adr-0036-media-flow.md as amended by task 1 (P1); wire rulings in the story file's Design §Wire freeze, including Amendment 4). Covers R-4 (pending is MH-internal, visible only as MH metrics, no give-up timeout while assigned), R-9 (MH consumption of the GoP counter), R-10 (MH requests a keyframe only for a source it has seen, at most once per (source stream, counter), never repeated), R-11 (MH side: a request names exactly one stream), R-12 (MH side: MH recovers a subscriber it abandoned by its ordinary request; the publisher keys itself toward a handler with no group in progress), R-13, R-23 (removal of the three MH keyframe/recovery keys), R-30 (request and pending series), R-33 (MH-level env-tests). Pair with client, the producer and consumer of both envelopes; pair with security for the R-11 bound and the request rules; pair with test for gates, env-tests and the fixture extension in item 9; pair with protocol, security, infrastructure and operations for the cleanup in item 8 (Covers: code, unit-tests, integration-tests, env-tests, deploy config, observability, docs).

    DEPENDENCIES. Task 9 (MH-A2, media-handler): the transport seam and shim including the MH-client control-envelope plumbing, stream ingress and cut-through, the pure edge state machine (Waiting / Forwarding / SwitchPending / Paused) with edge runtime state keyed by `egress_stream_id` and its "waiting with source seen" signal, and per-destination abandonment returning edges to Waiting (backlog reset, propagated publisher reset, mid-frame stall reset). Task 8 (MH-A1, media-handler): attribution of every ingress frame to a declared (sender, stream) against `source_streams` with dispositions FORWARD / PAUSED / DROP_AT_INGRESS. Task 1 (P1, protocol): `MhServerMessage { oneof { KeyframeRequest keyframe_request = 1 { stream_number, gop_counter } } }` with envelope trace tags 20/21 that MH leaves unset, and `MhClientMessage.recovery_request = 2` of `RecoveryRequest { slot_id, sender_id, gop_counter }`, both on the client-opened bidi control stream with the same 4-byte length prefix and size bound as `MhClientMessage`. Task 6 (P2, protocol): the v3 header field `gop_counter` (u32, per (sender, stream), incremented on every independently-decodable frame, so every audio frame; at u32::MAX the stream stops, never wraps) exposed by the header-prefix parse task 9 uses. Task 7 (T-A, test): `fixtures/video/` VideoPublisher with T-A's group-start source set to `OnKeyframeRequest { min_interval }` (reads `MhServerMessage.KeyframeRequest` from its control stream and answers with a new group only if the request names its stream and `gop_counter >= current`, with a fixture-side minimum interval and a record of every request received, so tests can assert a request preceded a group start), and the source `None` (never keys) for the never-sending pending case, plus a recovery-request sender with explicit `sender_id` and counter. Task 3 (INF-A, infrastructure): the MH keys and the shared profile's `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` as refs; item 8 removes three of those keys and item 6 removes the profile ref.

    RULINGS THAT BIND THIS TASK (story file Design §Wire freeze, Amendment 4). RecoveryRequest is `{ slot_id, sender_id, gop_counter }`. `sender_id` is a staleness guard only: it narrows matching and is NEVER used to choose a source or publisher and NEVER forwarded upstream. The counter rule is equality-based with no clamping. A keyframe request names exactly one stream (simulcast-ready). The publisher honours a request only for a stream it currently sends to that handler, only when `gop_counter >= current`, at most once per `max(directed min_keyframe_interval_ms, compiled floor)`, deferring at most one in-interval request per stream (counted `min_interval`, not refused). Separately, the publisher keys whenever a target has no group in progress (directed start, target added, abandonment of that target), under the same bound and the same single deferral. Whoever decides an abandonment repairs it: the publisher re-keys only after its own (backlog cap); a stream MH refuses or stops (STOP_SENDING: ingress refusal, stream-open limit) MH repairs by request. That is client behaviour; MH must not rely on anything stronger. Requests ride a reliable stream, so MH never repeats a (source stream, counter) and never retries; a later loss is a new event with a newer counter. "Pending" never appears on the wire; the MH-to-MC switch-completion report is deferred to story 5.

    1. PER-SOURCE KEYFRAME STATE (new per-source-stream state, keyed to the publisher connection's binding; inside `crates/mh-service/src/media/` so the §11 deny scope covers it). Track whether any frame has been received from the source stream since bind, the newest `gop_counter` read from EVERY frame header MH reads from that source, whether forwarded, server-muted, unrouted or paused, and the last counter MH requested for that source (the dedupe record for items 2 and 3). A missed group start must never leave MH holding a stale counter: the publisher ignores a request older than its current counter by design. State is bounded by `MH_MAX_SOURCE_STREAMS_PER_MEETING`, freed on unbind and on unassign, and keyed to the publisher's binding so a counter restart after a story-2 R-16 sender-id reissue starts clean.

    2. MH-ORIGINATED REQUESTS (R-10). When an edge enters Waiting (or SwitchPending toward B) and its source stream has been seen, MH requests a keyframe for that source stream carrying the newest counter, unless it has already requested that (source stream, counter); then it sends nothing. There is no retry and no timer: the same counter is never sent twice, whatever the clock does. A queued, unsent request is discarded on group start, edge removal, source PAUSED, source DROP_AT_INGRESS, or source unbind. A source never seen waits silently with no request; the publisher keys for every target with no group in progress, and MC's "watched implies directed" makes this finite. There is no give-up timeout while the source stays assigned (R-4). The pending gauges (item 7) and Video Slot Stuck Pending alert make a missing event loud. Type-blind by construction: audio frames are all independently decodable, so an audio edge never waits and never requests. Recovery after any abandonment (MH's own backlog reset, a publisher-side reset propagated by task 9 (MH-A2), a mid-frame stall reset) flows through this same Waiting path; nothing else triggers an MH-originated request. Streams MH refused or stopped are repaired only by MH's request once it wants the source again. MH never stops a stream before its counter is read (user ruling, amendment 4): change task 9 (MH-A2)'s stream-open limit path in `crates/mh-service/src/media/stream_ingress.rs` (~l.137–145) to read the fixed header prefix of an over-limit stream, record the source's counter in the per-source state, then stop it (still counted `stream_rate_limited`), so the ordinary once-per-counter request applies. Only a malformed header is stopped without a counter (non-conforming publisher, counted, no repair). No stall bound applies before the header completes, since nothing has been forwarded; the declared QUIC concurrent-stream limit bounds headerless streams. Verify whether the landed mid-frame stall timer fires before the header completes, and if so make it start at header completion. Pair with security on this DoS-surface change.

    3. RECEIVER RECOVERY REQUESTS (R-13). Replace the post-connect discard of the bidi receive side (`crates/mh-service/src/webtransport/connection.rs` around line 543, which reads probe bytes and drops them) with a bounded framed-message loop using the existing `MAX_MESSAGE_SIZE`. Per message: a fixed-size parse and a lookup, nothing else; volume is bounded by QUIC flow control. For each RecoveryRequest:
       - Resolve `slot_id` only within this subscriber connection's own edges. Unknown slot, or a slot whose edge has no forwarding stream-per-group source → `unmapped_slot`.
       - If `sender_id` is not the slot's current source sender → `source_mismatch`.
       - Otherwise compare `gop_counter` with MH's newest counter for that source stream: lower → `stale` (a newer group is already in flight); higher → `counter_ahead` (refused; MH never saw that group); equal and already requested (by any receiver or item 2) → `duplicate`; otherwise equal → `forwarded`.
       - Undecodable → `malformed`.
       MH never forwards the client's message verbatim; it emits its own KeyframeRequest with the source's stream_number and MH's newest counter.

    4. ONE DEDUPE RECORD PER SOURCE STREAM. MH-originated requests and forwarded receiver requests share item 1's per-source-stream record, so each (source stream, counter) yields at most one KeyframeRequest to the publisher. Repeated, multi-receiver and (with the publisher's counter rule) multi-handler requests therefore yield one keyframe.

    5. MH-TO-PUBLISHER CHANNEL. Write length-framed `MhServerMessage` on the send half of the publisher's client-opened bidi control stream, which today is discarded at `connection.rs:220` (`let (_, mut recv_stream) = connection.accept_bi()`). Writes are non-blocking with respect to the forward path. The send queue keeps only the newest request per stream: a newer request replaces a queued, unsent older one. A failed control-stream write is counted and not re-emitted; it never stalls media. No trace context is set and nothing is logged per request.

    6. CONFIG (R-23). This task adds no MH keyframe or recovery key: `MH_KEYFRAME_REQUEST_RETRY_MS`, `MH_KEYFRAME_REQUEST_AGGREGATION_MS` and `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC` are removed (item 8), and MH has no boot rule relating any key to `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`. MH has no use for that key (user ruling, amendment 4: it becomes MC-only config `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, moved by task 12 (MC-A)). Remove MH's hard ref from mh-0/mh-1, its assertion in env-test 01 (~l.398) and the mention in `crates/mh-service/src/config.rs` (~l.427); add no read. This must land before task 12 removes the key from the profile, or MH pods fail on a missing ConfigMap key. Every MH key stays covered by `crates/dt-guard/src/env_config.rs`.

    7. TELEMETRY (R-30, R-4; handles cached at setup; no media_kind or identity label; no GoP value, request timing or per-source event in any log, label or span; pending gauges are tick-sampled from session state, never per frame).
       - `mh_media_keyframe_requests_sent_total{trigger=initial|receiver_recovery}`: what was actually written to a publisher, after dedupe and queue replacement.
       - `mh_media_recovery_requests_received_total{outcome=forwarded|duplicate|stale|counter_ahead|source_mismatch|unmapped_slot|malformed}`.
       - `mh_media_edges_pending` (gauge) and `mh_media_edge_pending_oldest_seconds` (pod max).
       - `mh_media_edge_pending_duration_seconds` (histogram, observed at exit).
       - `mh_media_edge_pending_exits_total{outcome=group_start|abandoned_source_left|superseded|paused_forward_nothing|edge_removed}`.
       Every new series lands IN THIS TASK with a catalog row in `docs/observability/metrics/mh-service.md` and at least a basic panel in `infra/grafana/dashboards/mh-media.json` (the `application-metrics` guard, `crates/dt-guard/src/application_metrics.rs` checks 5 and 6, fails validation otherwise), following `docs/observability/dashboard-conventions.md` (explicit `editorMode`, counters as rates per ADR-0029); task 20 (OBS-B) owns layout, alert-linked panels and thresholds, including the stuck-pending threshold, which no longer derives from a retry interval.

    8. AMENDMENT-4 CLEANUP OF LANDED TEXT AND CONFIG. Line numbers were verified at prompt time; re-anchor by content.
       - `proto/dark_tower/signaling/v1/signaling.proto` (Guarded Shared Area; pair with protocol). The `min_keyframe_interval_ms` comment (~l.357–364): drop "read by MC AND MH" and "MH reads it to validate `MH_KEYFRAME_REQUEST_RETRY_MS`". The `RecoveryRequest` block (~l.1577–1629): replace the rate bound and aggregation (and its 100 ms distinction) with counter dedupe and `duplicate`; the receiver sends once per loss event, no timer or retry, taking `sender_id` and `gop_counter` from the failing frame's header (field comments ~l.1617–1627). The `KeyframeRequest` MH RULES (~l.1667–1673): replace retry, its boot rule and the aggregator with once per (source stream, counter), the newest-only queue, and the publisher's keying rules. Comments only.
       - `docs/decisions/adr-0036-media-flow.md` (pair with protocol and security). The §1 amendment (~l.116–150: MH retries, aggregation, "the publisher never keys itself on abandonment") and the §7 switch-table amendment (~l.1125–1132: "retrying until a group start arrives"). After each, add a blockquote `> **Amendment (2026-10-21, story 3 amendment 4 — event-driven keyframes and recovery; R-10, R-12, R-13).**` naming the superseded sentences, which stay visible per the ADR's convention, and what replaces them, including who-decides-repairs.
       - Remove the three MH keys in one deploy (pair with infrastructure): `infra/services/mh-service/config.env` (~l.770–802, entries and comments); the env refs in `infra/services/mh-service/mh-0-deployment.yaml` and `mh-1-deployment.yaml` (~l.225–239) and their profile comment (~l.256–265); the retry/aggregation sentences in `infra/services/media-codec-profile/profile.env` (~l.97–107); `crates/env-tests/tests/01_mh_deployment_config.rs` (key table ~l.356–370; both relations at ~l.622–643, including retry ≥ minimum interval); `.env.example` (~l.128–130); `docs/DEVELOPMENT.md` (~l.731–733).
       - Configuration Reference rows (operations reviews): delete the three rows in `docs/runbooks/mh-deployment.md` (~l.719–721) and delete the `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` row (~l.727), since MH no longer reads it; correct that row in `docs/runbooks/mc-deployment.md` (~l.875), which says MH-B boot-checks the retry key.
       Story and devloop-output history files are not edited.

    9. FIXTURE EXTENSION (landed in task 7 (T-A); pair with test). In `crates/env-tests/src/fixtures/video/`: add a `Fault` that resets the current group with `RESET_MID_GROUP_CODE` and keeps running, then self-keys the next group within `min_interval` through the existing single deferral in `schedule.rs`. Log that start as a new `GroupStartCause` (e.g. `TargetAbandoned`; the client's keyframes-produced cause for it is `abandonment`). The existing `Fault::ResetMidGroup` stops the publisher (the old rule): keep it as a publisher that ends, and correct its doc. Fix the stale "while MH retries keyframe requests" wording in the `GroupStartSource::None` docs (the module docs in `mod.rs`, ~l.31–36). Unit-test the new cause, including in-interval deferral.

    GATES (injected clock, MetricAssertion per ADR-0032):
       - a request is emitted only after a frame from that source has been seen;
       - a never-seen source never requests;
       - no (source stream, counter) is emitted twice: advancing the clock with an edge still Waiting emits nothing;
       - a queued unsent request is discarded on group start, edge removal, PAUSED, DROP_AT_INGRESS and unbind;
       - behind a blocked write, a newer counter replaces the queued older one and only the newer is written;
       - the counter is taken from forwarded, muted and unrouted frames alike;
       - repeated, multi-receiver and multi-handler requests yield one emission per (source stream, counter); later receiver requests, including one after MH's own request, count `duplicate`;
       - slot-to-source remap verified, with MH's own fields emitted;
       - the equality rule's forwarded / duplicate / stale / counter_ahead arms;
       - abandonment (backlog reset, propagated publisher reset, stall reset) produces a request via Waiting, and a later one at a newer counter a new request;
       - per-source state freed on unbind and unassign;
       - SWITCH-RACE: a slot moves A→B, then a RecoveryRequest carrying A's `sender_id` and A's newest counter is dropped as `source_mismatch`, B is asked for no keyframe and nothing is sent upstream to A; a second arm with B's `sender_id` and A's counter value is judged in B's counter namespace (stale or counter_ahead, forwarded only if equal to B's newest and not yet requested). Integration tests only, where timing is deterministic.

    ENV-TESTS (Covers: env-tests). New `crates/env-tests/tests/<next free number>_mh_video_keyframes.rs`, driving policy through `fixtures/mh_grpc.rs` as MC and media through test's `fixtures/video/`. Record in the file header the concurrent edge sum, planned at most 30. Assert per-entity deltas; no wall-clock gates.
       - A late-joining subscriber receives a group start only after a KeyframeRequest for exactly that stream is observed on the fixture publisher's control stream.
       - A never-sending source raises `mh_media_edges_pending` by the expected per-meeting delta and causes zero requests.
       - A receiver RecoveryRequest reaches the publisher remapped (slot to stream, MH's counter), with the source_mismatch / stale / counter_ahead / duplicate / forwarded outcomes counted, and the fixture publisher records exactly one request per (stream, counter).
       - A subscriber abandoned by its backlog cap recovers through a request.
       - Publisher→MH abandonment: with item 9's fault, the subscriber sees the propagated reset, then a group start whose cause the fixture logs as `TargetAbandoned`, within `min_interval`.
       - Env-test 01 still passes with the three keys and their relations removed.

    Out of scope: attribution and dispositions (task 8 (MH-A1)); forwarding, edge state and abandonment (task 9 (MH-A2)); proto field changes (comment cleanup in item 8 is in scope); client honouring, self-keying and sending of requests (client tasks 11 (C-A1), 13 (C-A2) and 14 (C-B)); dashboards and alerts beyond the basic panels (task 20 (OBS-B)); runbooks other than the Configuration Reference rows in item 8 (tasks 4 (OPS-A) and 19 (OPS-B)).

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - RecoveryRequest handling:
      - the slot is resolved only within that subscriber connection's own edges;
      - per-request work is a fixed-size parse under `MAX_MESSAGE_SIZE` and one lookup, with no allocation or upstream effect beyond it, and volume is bounded by QUIC flow control;
      - a `sender_id` that isn't the slot's current source is dropped as `source_mismatch`;
      - counter equal and not yet requested → forward, equal and already requested → `duplicate`, lower → `stale`, higher → `counter_ahead`, all counted;
      - at most one upstream request per (source stream, counter), so forwarded requests are bounded by groups produced, which the publisher's minimum interval bounds;
      - the upstream KeyframeRequest carries only MH's own fields, never the receiver's verbatim.
    - GoP state is scoped to the publisher's binding and freed on disconnect or unbind, so an R-16 sender-id reissue starts clean.
    - MH sends at most one KeyframeRequest per (source stream, counter) and never retries. The send queue holds only the newest unsent request per stream; a queued request is discarded on group start, edge removal, source unbind or PAUSED, and request state is freed on unassign.
    - Pending tracking is a pod-level count and oldest-age only, with no identity.
    - Deadline logic runs on an injected clock.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-mh-b-on-demand-keyframes
  tag: story-2026-10-07-see-each-other-task-10
- id: 11
  status: completed
  specialist: client
  deps:
  - 2
  - 3
  - 6
  prompt: |-
    Story 3 "see each other", task 11 (C-A1): SDK contract migration — the join publish set and per-kind receive declaration, the flat slot states, per-kind mute and send directives, MC send-id stamping, the uni-stream transport seam and MH control channel, and the slot env-var rename (docs/user-stories/2026-10-07-see-each-other.md; requirements R-1 (client half), R-2 (client half), R-3 (pairing view data), R-15 (client half), R-16 (directive half), R-17 (audio), R-18 (event), R-20 (send-id stamping), R-23 (slot counts), R-29 (series added here); wire spec: the story file's Design §Wire freeze). Specialist: client. Pair with security as reviewer of the R-18 fail-safe and the mute semantics, and with test as reviewer of the `MockWebTransport` extension's determinism.

    WHY THIS TASK LANDS BEFORE MC-A. Task 12 (MC-A) refuses a join that carries no `publish_kinds` and no `receive_capability`, and it stops emitting `SLOT_STATE_SOURCE_MUTED` in favour of the flat AVATAR_* values. MC-A's gate runs the story-2 browser specs. So the SDK must send the new join fields and understand the new slot values FIRST, while still working against the pre-MC-A controller that is live at THIS task's gate. Two transitional shims make that possible. Both are deleted by task 13 (C-A2), which runs after MC-A, and each carries a comment naming C-A2 as its deletion owner:
    - (a) the SDK still sends the initial post-join `ReceiveCapability`, identical to the join-carried one. It is a legal redeclaration and is a no-op once MC-A lands.
    - (b) the SDK maps a legacy `SLOT_STATE_SOURCE_MUTED` to `avatar_client_muted` or `avatar_server_muted` using the latest `ParticipantMuteUpdate` for that sender (server wins), so the UI and tests see only the flat vocabulary. A legacy or unknown state NEVER maps to `active`: with no mute update for that sender it maps to `avatar_client_muted` as the fail-closed default, and a server-muted cause always wins. Tests cover each case.

    DEPENDENCIES
    - Task 2 (OBS-A, observability) provides the per-kind client metric handles: `mediaMetricLabels(identity, kind)` with `media_kind`, `MediaMetrics` per kind, and session-level series kept kind-free. OBS-A owns the `media_kind` rule for every client series. You only ADD series on its handles.
    - Task 3 (INF-A, infrastructure) provides `MC_MAX_AUDIO_RECEIVE_SLOTS` (8, MC bounds 1..64) and `MC_MAX_VIDEO_RECEIVE_SLOTS` (4, bounds 0..64; 0 = video off, R-26) in `infra/services/mc-service/config.env`. `MC_MAX_RECEIVE_SLOTS` stays as a retired key with optional refs.
    - Task 6 (P2, protocol, client paired) provides:
      - the v3-only SDK codec (v2 deleted) and the regenerated `wireConstants.ts`;
      - the SDK declaring header version 3;
      - the non-resettable per-(sender, stream) GoP counter owner on `TransmitKeyManager`, with audio incrementing it every frame;
      - every binding below, from task 1 (P1).

    WIRE FACTS. The story file's Design §Wire freeze is authoritative; cite it, don't re-derive it.
    - `JoinRequest.publish_kinds = 8` and `receive_capability = 9`.
    - `JoinResponse.max_audio_receive_slots = 13` and `max_video_receive_slots = 14`. Tag 12 `max_receive_slots` is superseded and is deleted by MC-B, so stop reading it here.
    - Flat `SlotState`: `ACTIVE` (video carries no `sender_id`; audio names it), `AVATAR_NO_VIDEO = 8`, `UNAVAILABLE = 9`, `AVATAR_CLIENT_MUTED = 10`, `AVATAR_SERVER_MUTED = 11` (server wins), `FEWER_SOURCES_THAN_SLOTS`. `SWITCH_PENDING`, `WITHHELD_BY_CONGESTION` and `SOURCE_UNREACHABLE` are retained but never emitted. `ZERO_REQUESTED` is reserved. Unreachable stays on `unreachable_sender_ids`.
    - `MuteRequest` / `ServerMuteRequest` fields are `optional bool`; both absent is rejected. UNTIL MC-B lands absent = unchanged parsing, MC reads an absent kind as `false` (P1's interim shim), so this task sends BOTH kinds, explicitly, on every request (amended 2026-10-20, lead ruling at P1 review: sending only the changed kind before MC-B would unmute the other kind). Sending only the changed kind is MC-B's switch.
    - Send directive:
      - every published kind is always listed, and EMPTY targets mean send nothing (unwatched, or server-muted for that kind);
      - a published kind MISSING from `streams` is a protocol fault: a loud typed fault plus a counter, never read as a withdrawal;
      - client mute never changes the directive;
      - `header_version` must be 3, and any other value is refused and counted on an EXPORTED client series.
    - Send ids: audio `stream_number` 0, video 1 (pinned by MC-A).
    - `MhServerMessage { keyframe_request = 1 { stream_number, gop_counter } }` arrives on the MH connect bidi stream after `MhConnectRequest`, with the same 4-byte length prefix as `MhClientMessage`, size-bounded.
    - `MhClientMessage.recovery_request { slot_id, sender_id, gop_counter }` is sent by task 14 (C-B) through the control send you add here.
    - Uplink relay `stream_id` is HOP-LOCAL: the publisher stamps MC's send id for that stream (`SendStream.stream_number`), and MH attributes ingress by (bound sender, that id).

    WHAT TO BUILD

    (1) Send-id stamping (R-20). `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts:861` stamps `declaredSlotIds[0] ?? 0`, the subscriber's own first slot id, into the uplink. Replace it with ONE stamping function that takes the value only from the directive's `stream_number` for that stream. Video egress in task 13 (C-A2) must reuse the same function, and a test asserts the stamp never comes from a declared slot id.

    (2) Join and declaration (R-1, R-2).
    - `MeetingSession` takes a publish set and N audio plus M video slot counts. The publish set accepts AUDIO only in this task; video publishing is enabled by C-A2. A video request here is a typed "not yet supported" error, not a silent drop.
    - The join carries `publish_kinds` and the full `receive_capability`: audio slot ids `0..N-1`, then video ids `N..N+M-1`, declared in id order so MC's truncation keeps the low ids. Either N or M may be 0.
    - A local pre-check refuses "publishes nothing and declares 0 + 0" with a typed error and copy in `packages/web-app/src/lib/errorText.ts`. It is UX only; MC is the authority.
    - The publish set is held immutably on the session, and the reconnect path re-sends the ORIGINAL set. MC refuses a changed one.
    - Per-kind caps from `JoinResponse` are runtime-readable alongside effective N, M and the publish set.
    - RETIRE story 2's over-cap refusal: delete the throw in `session/mediaSelection.ts:receiveSlotIdsToDeclare` and the `ReceiveSlotsOverCap` path. Retire `dt_client_media_receive_slots_rejected_total` WITH its catalog rows in `docs/observability/metrics/client.md` (around lines 1186 and 1190) in the same change, because the `client-metrics-export` guard (G3) fails if they land apart. Over-cap slots then render UNAVAILABLE from the wire.

    (3) Slot env-var rename. The whole rename lands here.
    - `VITE_DT_RECEIVE_SLOTS` becomes `VITE_DT_AUDIO_RECEIVE_SLOTS` plus `VITE_DT_VIDEO_RECEIVE_SLOTS`, both ≥ 0, with no alias. The OLD name FAILS LOUDLY if set, naming the two new variables.
    - Update every live reader and writer: `packages/sdk-core/src/config/clientConfig.ts` (`parseReceiveSlots` accepts 0) and `session/events.ts`; `packages/web-app/src/lib/{config,session,testLevers,errorText}.ts`, `vite-env.d.ts`, `playwright.config.ts`, `e2e/fixtures.ts` and `e2e/README.md`; the tests `src/__tests__/{receiveSlotsConfig,errorText}.test.ts`; `scripts/dev-web.sh` and `scripts/dev-web.test.sh`; `docs/runbooks/client-dev-local.md` and `docs/runbooks/mc-incident-response.md`; `docs/observability/metrics/client.md`; and `infra/grafana/dashboards/client-media.json`'s description text. Completed story files stay untouched.
    - `scripts/dev-web.sh`:
      - exports both variables, with demo defaults N=3 and M=3, and echoes the effective values;
      - reads `MC_MAX_AUDIO_RECEIVE_SLOTS` and `MC_MAX_VIDEO_RECEIVE_SLOTS` from the same source it reads today (`MC_SHARED_CONFIGMAP`, i.e. `infra/services/mc-service/config.env`), and STOPS reading `MC_MAX_RECEIVE_SLOTS`;
      - keeps the CANNOT VERIFY hard failures for an absent file or an absent key;
      - turns the over-cap HARD FAIL into a NOTICE that excess slots will show unavailable (R-1);
      - reports a video cap of 0 as "video disabled";
      - updates its header comments (around lines 23, 130 and 633-670).
    - The test-pattern define and the multi-participant video demo docs are NOT this task's.

    (4) Flat slot states in the SDK (R-4).
    - Map the flat enum into `signaling/events.ts:StreamAssignmentEvent['slotState']` and `signaling/codecMap.ts`. The tokens are identical to MC's `slot_state_label()` and observability's labels: `active`, `avatar_no_video`, `avatar_client_muted`, `avatar_server_muted`, `fewer_sources_than_slots`, `unavailable`, plus the never-emitted `switch_pending`, `withheld_by_congestion` and `source_unreachable`, which consumers render fail-closed.
    - Remove `source_muted` and `zero_requested` from the union. Legacy `SOURCE_MUTED` is handled only by shim (b) above.
    - Video `active` carries no sender. Audio `active` and every avatar carry `senderId`.
    - Update `packages/web-app/src/lib/slotState.ts`'s exhaustive table with minimal copy so the build and the story-2 browser specs stay green; task 17 (C-C1) writes the final copy. Update `server-mute.spec.ts`'s slot-state assertion to `avatar_server_muted`.
    - Expose a participant pairing view keyed by sender id (participant → audio slot?, video slot?) from the assignment data. Live video pairing from verified frames is task 14 (C-B)'s addition.

    (5) Per-kind mute and directives (R-15, R-16, R-17, R-18).
    - `setAudioMuted` and `setVideoMuted` are independent, and `media/lifecycle/muteState.ts` gains video. A request about one kind never changes the other kind's state.
    - `SignalingClient.sendMuteRequest` and the host's server-mute request send BOTH kinds as explicit optional bools on every request, carrying the participant's current state for the unchanged kind (today `videoMuted` is hardcoded false). Not "only the changed kind" until MC-B (see above).
    - Server-mute state is tracked per kind from the self `ParticipantMuteUpdate`.
    - `session/mediaWiring.ts:sendDirectiveListener` becomes per kind: a missing published kind is a counted fault, and `header_version` other than 3 is refused and counted. Audio withdrawal keeps the mic OPEN, with nothing sent (no silence, no DTX), as today.
    - R-18 fail-safe, agreed with security:
      - whenever a kind's targets go from empty to non-empty while local state LAST RECORDED that kind as server-muted, the SDK emits a "capture resumed after server mute" event, and capture resumes unless the participant is client-muted, in which case there is no capture and no event;
      - the event keys on the state recorded before the resume, so it fires in either arrival order relative to the self update, and still fires if that update is lost; a later clearing update raises no second event;
      - the residual case is BY DESIGN and gets its own named test with a comment: if no server mute was ever recorded, a resume raises no notice, because the user was never told they were muted.
    - The UI notice itself is task 17 (C-C1)'s.

    (6) Transport seam and MH control channel.
    - `IWebTransport` gains `createUnidirectionalStream()` and `incomingUnidirectionalStreams`, implemented in `BrowserWebTransport`.
    - `packages/test-utils/src/MockWebTransport.ts` gains: uni streams in both directions; per-stream reset/abort injection; a slow-reader/backpressure knob; and send/receive of `MhServerMessage` and `MhClientMessage.recovery_request`.
    - `media/MediaTransport.ts`:
      - after `MhConnectRequest`, start a length-prefixed, size-bounded `MhServerMessage` read loop on the connect bidi stream;
      - expose a per-handler control send for `recovery_request`;
      - update the file header, which says "No frame I/O beyond the connect envelope".
    - With no video stream yet, every `keyframe_request` is refused and counted as `not_sending`. Malformed input is counted as `malformed` and never throws untyped. That makes the channel testable now; C-A2 adds acceptance.

    TELEMETRY (on OBS-A's per-kind handles, each with its row in `docs/observability/metrics/client.md` in this task):
    - `dt_client_media_capture_failures_total{media_kind, outcome}` (label `outcome`, not `reason`: taxonomy ruling at C-A1 Gate 1), with outcomes from the existing `CaptureFailure` vocabulary (`media/setup/captureFailure.ts`), generalised beyond microphone wording and extended with `ended`: `permission_denied`, `no_device`, `device_unavailable`, `ended`. Audio emits on it now (today audio failures surface only as events); C-A2 adds video.
    - `dt_client_media_keyframe_requests_received_total{media_kind, outcome}`, with all outcome values declared now (`honoured`, `honoured_counter_ahead`, `stale_counter`, `min_interval`, `not_sending`, `malformed`) and zero-initialised; C-A2 emits the honour paths.
    - Directive faults: published kind missing; header version refused.
    - No GoP values, sizes, timings, or sender, slot or stream identity in any label, log, span or test-bus field.

    TESTS AND GATES (node tier, `MockWebTransport`, injected clock, no wall-time sleeps):
    - the join carries the publish set and the declaration, in id order, with 0 allowed;
    - the reconnect re-sends the original publish set;
    - 0+0+nothing is refused locally;
    - caps are readable;
    - the over-cap throw and its counter are gone;
    - the old env name fails loudly in the web app and in `dev-web.sh` (extend `dev-web.test.sh` for: notice on over-cap, "video disabled" at 0, CANNOT VERIFY on an absent key);
    - slot-state mapping for every value, including shim (b) server-wins and the never-emitted fail-closed values;
    - per-kind mute independence;
    - both kinds are sent on every mute request, the unchanged one at its current state (until MC-B);
    - R-18 in both arrival orders, the lost-update case, and the residual no-notice case;
    - the directive fault and header-version refusal are counted;
    - send-id stamping comes from the directive and never from a slot id;
    - the `MhServerMessage` loop handles a malformed or oversized message by counting it, and an unknown stream as `not_sending`;
    - the recovery control send is framed correctly.

    Gates:
    - sdk-core, sdk-svelte, web-app and test-utils lint, typecheck and unit tests are green.
    - The story-2 browser specs stay green against the controller live at this gate, which is pre-MC-A; that is what the shims are for.
    - No new env-tests: the browser scenarios for these behaviours are task 17 (C-C1)'s.

    SECURITY ACCEPTANCE:
    - the R-18 event cannot be bypassed by message order or by a lost update;
    - one kind's mute never alters the other's;
    - the stamp is MC's send id only;
    - the `MhServerMessage` reader enforces its size bound before allocation;
    - the publish set is immutable after join.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Uplink relay `stream_id` is stamped with MC's send id (`SendStream.stream_number`), never a slot id, through one function for audio and video, which fixes the AudioPipeline.ts:861 bug.
    - Contract migration:
      - per-kind mute requests send presence-tracked fields only, never both absent;
      - the publish set and receive declaration use the closed enums;
      - unknown slot-state and enum values received from MC are treated as fail-safe (avatar or no media), never as ACTIVE.
    - MhServerMessage read loop: the message is parsed with a size bound. Until C-A2, every KeyframeRequest is refused and counted as `not_sending` or `malformed`; none is ever honoured, and none starts capture.
    - R-18 resume event: whenever capture resumes for a kind last recorded as server-muted, the event fires independent of message order. It is tested for both orders plus "update never arrives". A named test records the residual case (never recorded as muted → no event) as by design. The non-suppressible UI notice is task 17 (C-C1).
    - The `capture_failures{outcome}` series exists, emitted by audio, with no identity labels.
    - Transitional shims are clearly marked and deleted by C-A2: the post-join ReceiveCapability redeclaration, and the mapping of legacy SOURCE_MUTED to avatar tokens via ParticipantMuteUpdate (server-muted wins). No shim adds a path that accepts v2 beyond MC's directive, and no shim maps an unknown or legacy state to ACTIVE.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-21-c-a1-sdk-contract-migration
  tag: story-2026-10-07-see-each-other-task-11
- id: 12
  status: completed
  specialist: meeting-controller
  deps:
  - 1
  - 2
  - 3
  - 6
  - 7
  - 8
  - 9
  - 10
  - 11
  prompt: |-
    MC-A: video slots, the flat slot-state migration, video send directives, the join publish set with per-kind caps, and the per-stream source list (story 3 "see each other", `docs/user-stories/2026-10-07-see-each-other.md`). Requirements: R-1, R-2, R-3, R-4, R-8, R-20 (MC half), R-23 (MC knobs), R-24 (MC reads the shared profile), R-26, R-29 and R-30 (MC series), R-33 (MC env-test scenarios). Governing design: ADR-0036 §5, §6, §7 and §9 as amended by P1. The wire shapes, stream-id model and config single sources are in the story's Design §Wire freeze; cite it, do not restate it. Specialist: meeting-controller. Pair with test for every env-test and the model-check oracle. Security reviews the join validation and the self-exclusion and kind-crossing invariants; observability reviews the series.

    DEPENDENCIES
    - task 1 (P1): the additive wire. JoinRequest `publish_kinds = 8` and `receive_capability = 9`; JoinResponse `max_audio_receive_slots = 13` and `max_video_receive_slots = 14`; flat SlotState values `AVATAR_NO_VIDEO = 8`, `UNAVAILABLE = 9`, `AVATAR_CLIENT_MUTED = 10`, `AVATAR_SERVER_MUTED = 11`; `CODEC_VP8 = 5`; EncodingParameters `optional uint32 min_keyframe_interval_ms = 6`; internal `RegisterMeetingRequest.source_streams = 8` of `SourceStream {sender_id, stream_number, transport_mode, disposition}`; the per-stream echo `applied_source_streams`. P1 marks the superseded items (JoinResponse tag 12, `SOURCE_MUTED`, `MutedSource`, `EgressStream.transport_mode`), which MC-B (task 15) deletes.
    - task 2 (OBS-A): the Rust `MediaKind` metric vocabulary (`ALL` + `label()`; reuse it, do not define a second one) and kind-taking recorder signatures in `crates/mc-service/src/observability/metrics.rs`, with every existing call site passing audio and the catalog rows for those series. The recorders are: `record_slot_state(kind, state)`; `record_send_targets(kind, count)`, counted per stream; `set_receive_slot_cap(kind, cap)`, where OBS-A publishes audio only; `record_mute_request(kind, outcome)`; `record_server_mute_request(kind, action, outcome)`; and `set_server_muted_sources(kind, total)`. `mc_media_send_directives_total` and `mc_media_unreachable_senders_total` deliberately stay kind-free; do not add a label to them.
    - task 3 (INF-A): the config. `mc-service-config` gains required refs on mc-0 and mc-1 for `MC_MAX_AUDIO_RECEIVE_SLOTS` (8) and `MC_MAX_VIDEO_RECEIVE_SLOTS` (4). The shared `media-codec-profile` holds the profile keys listed in the Wire freeze, read by both MC and MH. AMENDMENT 4 (user, 2026-10-21) makes the minimum keyframe interval MC-only config, and THIS task, the first to read it, moves it, paired with infrastructure:
      - Rename `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` to `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, range 100..=10000, default 500.
      - Move it from `infra/services/media-codec-profile/profile.env` (which drops to 8 keys) to `infra/services/mc-service/config.env`, with the mc-0/mc-1 refs re-pointed at `mc-service-config`.
      - Update env-test 02, `.env.example`, `docs/DEVELOPMENT.md`, the `docs/runbooks/mc-deployment.md` row (~l.875, also dropping its MH-B retry boot-check clause) and the key name in the `signaling.proto` `min_keyframe_interval_ms` comment.
      - Task 10 (MH-B) has already removed MH's ref; confirm no `mh-*` deployment still references the key before deleting it from the profile.
      - MC still carries the value on the video directive as `min_keyframe_interval_ms` and publishes `mc_media_video_keyframe_min_interval_seconds`. The Kind patch overrides only the video resolution, frame rate and bitrate. `MC_MAX_RECEIVE_SLOTS` and `MC_AUDIO_{CODEC,MAX_BITRATE_BPS,FRAME_RATE_HZ}` move to optional refs (INF-A schedules their deletion); this task removes their code reads.
    - task 6 (P2): v3 is the only header version. `media_protocol::frame::PROTOCOL_VERSION` is 3, and MC's `header_version` stays derived from it.
    - task 7 (T-A): the Rust video fixtures `crates/env-tests/src/fixtures/video.rs` (keyless VideoPublisher and VideoSubscriber).
    - task 8 (MH-A1): MH accepts `source_streams`, mixed transports in one meeting, the dispositions and per-stream attribution. Attribution is (connection-bound sender, uplink relay `stream_id`) looked up in `source_streams`, where the uplink id IS MC's `SendStream.stream_number`. MH hard-rejects:
      - a duplicate (sender, stream);
      - an edge candidate missing from `source_streams`;
      - an egress whose candidates declare different modes (`rejected_transport_inconsistent`).
      MC must not emit video egress before this is deployed. Until MC-B, MH treats a registration with no `source_streams` as legacy (stream 0 DATAGRAM per sender). This task's registrations always carry `source_streams`, so that path is not exercised.
    - task 9 (MH-A2): stream-per-group forwarding. This task's env-test 34 needs video frames actually flowing through MH.
    - task 11 (C-A1): the SDK and web app already send `publish_kinds` and the initial `receive_capability` on the JoinRequest and render the flat slot-state values (avatar states instead of `SOURCE_MUTED`, and UNAVAILABLE). It lands first so the story-2 browser hear specs stay green when this task starts refusing joins without a publish set or receive declaration and stops emitting `SOURCE_MUTED`. If a story-2 browser spec fails here on either change, that is a C-A1 gap to report, not something to work around in MC.
      - C-A1 left ONE transitional tolerance that THIS task deletes (C-A1 Gate 3, test ruling): `packages/web-app/e2e/fixtures.ts:expectDeclaredReceiveSlots` accepts an UNKNOWN audio cap with a `transitional` annotation, because the controller before this task sends no `max_audio_receive_slots`/`max_video_receive_slots`. This task is the first to send them, so it restores the strict `serverCapState === 'known'` and cap >= N assertion and deletes the annotation branch (a client-file edit, paired with client).

    JOIN (R-2, R-22)
    - Where: the connection's join path in `crates/mc-service/src/webtransport/connection.rs`, before the actor admits.
    - `publish_kinds`: a closed set of `MEDIA_KIND_AUDIO` and `MEDIA_KIND_VIDEO_CAMERA`. UNSPECIFIED, `VIDEO_SCREEN`, unknown values and duplicates are rejected, never ignored.
    - `receive_capability`: goes through the same parser as the post-join `ReceiveCapability` (`crates/mc-service/src/media_signaling/capability.rs`). That parser must now reject `VIDEO_SCREEN` and unknown kinds; today it accepts video slots and silently ignores them.
    - Neither-publish-nor-receive: a participant that would publish nothing AND declares zero receive slots is refused at join.
      - The check runs on the declaration AS SENT, before truncation. Truncation is never a join failure (R-1), so a receive-only client whose video slots are all truncated by a video cap of 0 is admitted and sees every slot UNAVAILABLE.
      - A redeclaration that would produce the neither state is refused, and the previous declaration is kept.
    - Header version: refuse a join whose `supported_header_versions` lacks the current `PROTOCOL_VERSION`. There is no allowlist or floor; this is the counted reason for "old SDK meets new MC".
    - Publish set: stored on the actor's roster entry and fixed for the membership.
      - A reconnect presenting a different publish set is REFUSED, generically and counted. It is never adopted and never ignored.
      - A reconnect's receive declaration is treated as a redeclaration.
    - Refusal tokens:
      - on `mc_session_join_failures_total{error_type}`, zero-initialised: `publishes_and_receives_nothing`, `reconnect_publish_set_mismatch`, `header_version_unsupported`, `publish_kind_invalid`;
      - on `CapabilityOutcome` (`crates/mc-service/src/media_signaling/outcome.rs`): `media_kind_unsupported` and `would_publish_and_receive_nothing`.
      - Remove `SlotCountOverCap`, which truncation makes unreachable.
      - Client messages stay generic.

    PER-KIND CAPS (R-1, R-26)
    - `MC_MAX_RECEIVE_SLOTS` is replaced by two required keys in `crates/mc-service/src/config.rs`, startup-validated and logged like the existing media keys:
      - `MC_MAX_AUDIO_RECEIVE_SLOTS`, 1..=64;
      - `MC_MAX_VIDEO_RECEIVE_SLOTS`, 0..=64, where 0 is the video-off lever.
    - Truncate each kind BEFORE any per-slot work, keeping the first `cap` slots of that kind in declaration order. The client declares audio ids 0..N-1, then video N..N+M-1.
    - Each excess slot is reported `SLOT_STATE_UNAVAILABLE`: declared `slot_id` echoed, no `sender_id`, empty url, permanent for that declaration, no table position, no edge.
    - Populate both JoinResponse caps from the same parsed values that the cap gauge reads.
    - R-26: with the video cap at 0, every video slot is UNAVAILABLE, nothing is watched and no video directive has targets. Audio is untouched.
    - MC reads config at startup, so the lever applies after an MC restart, which reconnects everyone. Live slot vectors are not re-truncated in place; say so in the module doc so OPS-A's runbook wording matches.

    SLOT TABLE (R-3, `crates/mc-service/src/media_routing/slots.rs`)
    - Replace story 2's single audio vector with two independent per-kind vectors per subscriber S, sized by the truncated declaration.
    - Eligibility for an empty slot of kind K:
      - never S itself;
      - routable and sharing a connected handler with S (story 2 R-33 rule; unreachable stays on `unreachable_sender_ids` and consumes no slot);
      - for AUDIO, the candidate's publish set contains audio;
      - for VIDEO, any participant, so receive-only and audio-only participants render as AVATAR_NO_VIDEO.
    - Fill order for each empty slot of kind K:
      - (1) the earliest joiner (lowest `JoinRank`) holding a slot of the OTHER kind with S but none of kind K, which completes a pair;
      - (2) otherwise, the earliest eligible joiner holding no slot of kind K with S.
      - KIND ORDER (amended 2026-10-21, lead ruling at T-A planning): when S has empty slots of BOTH kinds at one event (its own join, a redeclaration, a refill after a leave or connectivity change), fill AUDIO first, then VIDEO, ONE slot at a time, repeating until no empty slot can be filled (a fixed point). The order changes the outcome (receive-only A earliest, B publishes both, N=M=1: audio-first gives audio=B, video=B; video-first would give video=A, audio=B), and rule (1) exists to complete pairs, which audio-first does. The independent test oracle pins the same order (`packages/web-app/e2e/slotOracle.ts:KIND_FILL_ORDER`).
      - NO-VIDEO PRECEDENCE (amended 2026-10-21, lead ruling at T-A Gate 3): a VIDEO slot whose occupant publishes no video is AVATAR_NO_VIDEO regardless of either mute, client or server; the mute avatars apply only to an occupant that declared video. The test oracle (`packages/web-app/e2e/slotOracle.ts`) pins the same.
    - STABILITY BEATS PAIRING. None of these ever change an occupied slot:
      - a join or a leave;
      - a mute of either kind or either cause;
      - a redeclaration of the other kind;
      - a connectivity change for an unrelated pair.
    - A leave frees only the leaver's slots and refills each by the fill rule. A redeclaration that keeps a kind's count keeps its occupants by ordinal.
    - Pairing is by publisher: every assignment except live video names the publisher.
    - `egress_stream_id`: pack the kind into it (`crates/mc-service/src/media_routing/assignment.rs`) so one subscriber's audio and video edges never collide. Keep the unpack beside the pack.
    - Stream-number constants: add `MAIN_VIDEO_STREAM_NUMBER: u8 = 1` beside `MAIN_AUDIO_STREAM_NUMBER: u8 = 0`.
      - Both are contract constants, not config: MC's send id IS the uplink relay `stream_id` the publisher stamps, and MH attributes by it. The pre-story SDK stamps 0 in practice, so audio must stay 0.
      - A test pins audio = 0 and video = 1 in both the directive and `source_streams`.
    - Model check: extend the module's deterministic exhaustive small-scope check. No proptest, no new dependency.
      - Scope: sequences of up to about 6 steps over up to 4 participants. Steps are a join with publish set in {A, V, AV, receive-only}, a leave, per-kind client and server mute flips, a redeclaration (counts in {0, 1, 2} and over cap), and connectivity changes.
      - Asserted after every step:
        - no self in any slot;
        - no sender twice within a kind for one subscriber;
        - audio slots hold only audio publishers;
        - each fill picks a rule-(1) candidate when one exists, else the earliest rule-(2) candidate;
        - an occupied slot changes only through its occupant's leave, the pair becoming unreachable, or that kind's redeclaration;
        - over-cap slots are UNAVAILABLE;
        - every edge's handler is in both parties' connected sets;
        - an edge's stream kind equals its slot's kind.
      - Add an INDEPENDENT expected-occupant oracle in the test module, written from the rule text above and sharing no code with the fill. Test writes a separate TypeScript oracle; neither is derived from the other.
      - If the enumeration is too slow, cap its length and keep named deeper cases. Never sample randomly.

    RENDER AND SNAPSHOT (R-20)
    - Video edges: candidate (sender, 1), `supersede_on_independent_frame = true`, priority group 2.
      - Add a `VIDEO_PRIORITY_GROUP` constant beside `AUDIO_PRIORITY_GROUP = 1`. A lower number means higher priority.
      - Audio edges are unchanged.
    - No edge exists for an AVATAR_NO_VIDEO, UNAVAILABLE, FEWER_SOURCES or unreachable slot.
    - Stop writing `EgressStream.transport_mode`. An edge's transport derives from its candidate's SourceStream.
    - `source_streams`, per handler snapshot:
      - One entry per (sender, stream) in EVERY participant's publish set. That is a superset of every directed stream, so nothing MC directs is ever unmatched at MH.
      - Transport is DATAGRAM for stream 0 and STREAM_PER_GROUP for stream 1.
      - Disposition, computed from the roster's existing flags: DROP_AT_INGRESS if that kind is server-muted, else PAUSED if it is client-muted, else FORWARD. MC-B makes the mute requests per kind and adds directive withdrawal.
    - Until MC-B deletes it, keep writing the old sender-only `server_muted_sources` exactly as story 2 computes it.
    - Hold MH's apply-time rejects as tested render invariants:
      - one entry per (sender, stream);
      - every edge candidate listed;
      - one transport per (sender, stream).
    - The generation advances only on rendered-output change.
    - Echo check (`crates/mc-service/src/media_routing/confirm.rs`): compare `applied_source_streams` per stream, unconditionally.
      - Mismatch arms: a sent stream absent from the echo, one echoed UNSPECIFIED, or one echoed with a different mode.
      - All three map to the EXISTING `transport_mode_mismatch` token on `mc_media_policy_pushes_total`, with the same precedence. It already pages via `MCMediaGenerationDivergence`.
      - The confirm-failure log line gains the arm (`absent_from_echo`, `echoed_unspecified`, `mode_differs`) and the stream_number as log fields, never as labels.
      - Update the catalog definition.

    SLOT-STATE VOCABULARY: THE WHOLE MIGRATION (R-4, R-5; `crates/mc-service/src/media_signaling/assignments.rs`)
    - This task owns every instance of the flat-enum rule, for both kinds.
    - Video slots:

      | Situation | State | Sender | Handler url |
      |---|---|---|---|
      | Occupant publishes video and is not muted | `ACTIVE` | absent; MC never names a live video source | the edge's handler |
      | Occupant does not publish video | `AVATAR_NO_VIDEO` | named | none |
      | Client-muted | `AVATAR_CLIENT_MUTED` | named | — |
      | Server-muted (wins over client-muted) | `AVATAR_SERVER_MUTED` | named | — |
      | Unfilled | `FEWER_SOURCES_THAN_SLOTS` | — | — |
      | Truncated | `UNAVAILABLE` | none | none |

    - Audio slots: `ACTIVE` names the sender. The two mute avatars, `FEWER_SOURCES_THAN_SLOTS` and `UNAVAILABLE` apply as for video. Audio never uses `AVATAR_NO_VIDEO`.
    - Nothing emits `SOURCE_MUTED` any more.
    - Replace `SourceMuteView`'s audio-only projection with a per-kind projection that keeps the client and server flags apart.
    - A mute change of EITHER kind re-emits to every holder, under the existing per-meeting flush bound, and only when that holder's view changed. Today a video-only self-mute "dirties nothing".
    - Update the module docs.
    - Labels follow the wire names through `slot_state_label()`: `active`, `avatar_no_video`, `avatar_client_muted`, `avatar_server_muted`, `fewer_sources_than_slots`, `unavailable`. Never "live".

    SEND DIRECTIVE (R-8; `crates/mc-service/src/media_signaling/directive.rs`)
    - Add `VideoEncoding` beside `AudioEncoding`, built only through a validating constructor. Every bound below refuses to boot when violated.

      | Field | Accepted values |
      |---|---|
      | codec | VP8 only |
      | width and height | 16..=1920 |
      | frame rate | 1..=60 |
      | bitrate | 100000..=4000000 |
      | `min_keyframe_interval_ms` | `MIN_KEYFRAME_INTERVAL_MS_FLOOR..=MIN_KEYFRAME_INTERVAL_MS_CEILING` |

    - Define `pub const MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 = 100;` and `pub const MIN_KEYFRAME_INTERVAL_MS_CEILING: u32 = 10_000;` as single-literal constants in this file.
      - The client's keyframe-floor sync guard (`scripts/guards/simple/validate-keyframe-floor-sync.sh`, client publish-path task) greps the floor and compares it with the SDK's compiled floor in `clientConfig.ts`.
      - The floor's comment names that guard, the SDK floor, and the client counter `keyframe_min_interval_clamped` as the drift signal.
    - Audio reads `MEDIA_AUDIO_*` and keeps the existing AUDIO_* bounds and their cross-reference comments to MH's `SUPPORTED_AUDIO_BITRATE_FLOOR/CEILING_BPS` and `AUDIO_FRAME_DURATION_MS`.
    - Video reads the six `MEDIA_VIDEO_*` keys. Drop the `MC_AUDIO_*` reads.
    - MH derives its costs from the same profile; that is R-24's single source. No MC-side guard.
    - Directive shape:
      - Every kind in the participant's publish set is ALWAYS listed, and an unpublished kind never is.
      - Video `SendStream`: `stream_number` 1, `MEDIA_KIND_VIDEO_CAMERA`. Codec, width, height, frame rate, max bitrate and `min_keyframe_interval_ms` are ALL set; the client refuses a video stream missing any of them.
      - Targets: the handlers owning that stream's out-edges, each with STREAM_PER_GROUP.
      - An unwatched stream (no out-edges) is listed with EMPTY targets. The last watcher's leave withdraws it, and a new watcher restores it, each within the same structural change.
      - `header_version` stays derived.
    - The module still has NO path to any mute state. MC-B narrows it to admit server mute only.

    METRICS (R-29, R-30)
    - Pass the real video kind at the new call sites: video slot states and video send targets.
    - Make the cap per kind and publish the `video` series. Video reading 0 is how an operator confirms R-26 applied.
    - New series:
      - `mc_media_send_directive_withdrawals_total{media_kind, reason}`, where `unwatched` is emitted here and `server_muted` by MC-B. The `mc_media_send_directives_total` outcome vocabulary is unchanged.
      - `mc_media_video_keyframe_min_interval_seconds`, a gauge published from the same parsed value MC directs. OBS-B's keyframe-storm alert reads it.
    - Every new or reshaped series:
      - carries `key_custody=operator` and no meeting, participant, sender, slot or stream identity;
      - is zero-initialised and covered by the present-at-zero render test;
      - lands in THIS task with a basic panel in `infra/grafana/dashboards/mc-media.json` and a row in `docs/observability/metrics/mc-service.md`, because the `application-metrics` guard's checks 5 and 6 fail otherwise. OBS-B owns layout and alerts afterwards.
    - There is no truncation counter; `slot_states{slot_state="unavailable"}` covers it.
    - No GoP counter, frame timing or mute history in MC logs or labels.

    TESTS
    - Unit:
      - the model check and oracle;
      - truncation order;
      - join rejects, the pre-truncation neither check, the header-version refusal and the reconnect mismatch;
      - `VideoEncoding` bounds;
      - the stream-number pins;
      - slot-state mapping for both kinds, including server-wins;
      - `egress_stream_id` free of cross-kind collisions;
      - the render invariants;
      - the echo arms.
    - Integration (`crates/mc-service/tests/`):
      - R-2 refusals at join and on redeclaration, with their tokens;
      - truncation to UNAVAILABLE with ids echoed;
      - R-8 watched-only directive, withdrawal on the last watcher's leave (`unwatched` counted) and restore on a new watcher;
      - R-26: with `MC_MAX_VIDEO_RECEIVE_SLOTS=0`, every video slot is UNAVAILABLE, no video targets exist, and audio is identical to a run without video. This is an integration test, not an env-test, because it needs MC restarted;
      - a disposition change advances the generation, and an identical re-render does not.
    - Env-tests (Layer 7, live Kind, Rust, paired with test, task 7 (T-A) fixtures):
      - Rules: per-entity evidence only (wire slot states and directives for the session under test), never shared-gauge values; no wall-clock gates or sleeps.
      - Extend `crates/env-tests/tests/27_mc_slot_placement.rs`:
        - publish mixes (A, V, AV, receive-only shown as AVATAR_NO_VIDEO; neither refused);
        - video over-subscription beyond the Kind cap of 4, giving UNAVAILABLE, with filled slots matching the oracle;
        - pairing and stability across join and leave;
        - watched-only video directive and withdrawal;
        - two-handler video visibility;
        - a v2-only client refused at join.
        - Record in the file header that the concurrent edge sum is at most 72 (Kind is sized at about 120).
      - Extend `crates/env-tests/tests/34_mc_kek_rotation.rs`: a leave while video is directed and flowing rotates the KEK to the post-removal members and does NOT re-issue, withdraw or restart any video directive. The browser half (decoding continues, no rotation keyframe) is the client's browser specs in task 17 (C-C1).

    ACCEPTANCE
    - Layers 1-7 are green, and the story-2 audio specs are green with M=0.
    - Nothing emits `SOURCE_MUTED`.
    - `directive.rs` has no path to client-mute state.
    - No code read of the retired MC_* keys remains, and `dt-guard env-config` is clean.
    - Update `docs/specialist-knowledge/meeting-controller/INDEX.md`.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Edges never cross kinds: an edge's stream derives from its slot's kind, asserted as a model-check invariant. No subscriber is assigned its own sender in any slot.
    - Join:
      - closed-enum validation;
      - neither-publish-nor-receive is refused generically and counted, both at join and on redeclaration;
      - truncation to the per-kind cap happens before any per-slot work;
      - a reconnect presenting a different publish set is refused and counted, not adopted.
    - Video ACTIVE slot state carries no sender identity; audio ACTIVE keeps it.
    - Directives carry `min_keyframe_interval_ms` from the required, validated shared-profile key.
    - Send ids are pinned (audio 0, video 1) and declared in `source_streams`.
    - No GoP value or stream timing appears in MC logs or labels.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  slug: 2026-10-22-mc-a-video-slots
  tag: story-2026-10-07-see-each-other-task-12
- id: 13
  status: pending
  specialist: client
  deps:
  - 11
  - 12
  prompt: |-
    Story 3 "see each other", task 13 (C-A2): the SDK video publish path — camera and test-pattern capture, VP8, stream-per-group egress with per-target abandonment, video keys, keyframe-request acceptance, and the keyframe-floor guard — plus removal of C-A1's two transitional shims (docs/user-stories/2026-10-07-see-each-other.md; requirements R-8, R-9 (video use), R-10 and R-11 (publisher half), R-12 (publisher half), R-14 (send half), R-16 and R-17 (camera), R-19, R-23 (client knobs), R-27 (generator), R-29/R-30 (send series); wire spec: the story file's Design §Wire freeze). Specialist: client. Pair with security as reviewer of key-bearing cadence, rotation triggers, keyframe-request bounds and test-pattern gating; with test as reviewer of the test-pattern spec; meeting-controller reviews the keyframe-floor guard.

    DEPENDENCIES
    - Task 11 (C-A1, client) provides:
      - the uni-stream transport seam and `MockWebTransport` (uni streams both ways, reset injection, slow reader);
      - the `MhServerMessage` read loop and the recovery control send;
      - the ONE send-id stamping function, which you reuse for video;
      - per-kind mute state and the R-18 resume event;
      - per-kind directive handling;
      - the per-kind join and declaration;
      - the flat slot states;
      - the declared `keyframe_requests_received_total` outcomes;
      - `capture_failures_total`;
      - the two shims you delete (below).
    - Task 12 (MC-A, meeting-controller) provides:
      - video directives: `stream_number` 1, `VIDEO_CAMERA`, `TRANSPORT_MODE_STREAM_PER_GROUP` targets, and `EncodingParameters` with `codec` VP8 (= 5), `width`, `height`, `frame_rate`, `max_bitrate_bps` and `min_keyframe_interval_ms` (= 6), ALWAYS set from the shared `media-codec-profile` (production 640×360, 30 fps, 800 kbps, 500 ms);
      - withdrawal as empty targets when unwatched;
      - the join-carried declaration and AVATAR_* emission (so the shims are dead);
      - `crates/mc-service/src/media_signaling/directive.rs::MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 = 100` and `MIN_KEYFRAME_INTERVAL_MS_CEILING: u32 = 10_000`.
    - Transitively, task 6 (P2) provides the v3 codec and the non-resettable per-(sender, stream) GoP counter owner on `TransmitKeyManager`, which audio already increments every frame and you reuse for video. Task 2 (OBS-A) provides the per-kind handles.

    WHAT TO BUILD

    (1) Delete C-A1's shims.
    - Remove the duplicate initial post-join `ReceiveCapability` send; the join-carried declaration is now the only initial one, and redeclaration stays.
    - Remove the legacy `SLOT_STATE_SOURCE_MUTED` mapping. After this task no SDK or web-app code references `SOURCE_MUTED` or `max_receive_slots`, so MC-B (task 15) can delete them from the proto. Grep `packages/` to prove it.
    - Enable video in the publish set.

    (2) Capture.
    - Camera via `getUserMedia({video})` at the DIRECTED width, height and frame rate.
    - The test-pattern source (R-27):
      - Vite define `__DT_TEST_PATTERN__`, opt-in `DT_TEST_PATTERN=1`, declared in `packages/web-app/vite/testDefines.ts` under the same single predicate as `__DT_TEST_TONE__`;
      - `false` in every production build and in the SDK library build, and it throws if reached in production;
      - extend `packages/web-app/tests/bundle-content.test.ts` with its marker tokens to prove absence.
    - The pattern derives ONLY from the meeting-scoped `sender_id`. It throws on 0 or out of range, like `testTone.ts`, and never uses a durable identity.
    - Frame layout (the shared SPEC agreed with test, written in this file's header; test writes the same spec in the e2e README; the e2e detector never imports the generator), on a grid of large flat blocks:
      - 16 ID blocks hold the `sender_id` bits MSB-first as black/white;
      - 4 guard blocks carry a fixed checkerboard;
      - 4 COUNTER blocks carry a Gray-coded frame counter mod 16, advanced every CAPTURED frame.
    - Route the substitution through the one selection point beside `session/mediaSelection.ts:selectCaptureSource`, and publish each participant's own pattern id where the tone is published today.
    - DEVICES: the camera track STOPS on ANY video withdrawal (empty targets: unwatched or server-muted) and on client video mute. It is re-acquired on restore with the existing device and permission, and never re-prompts in a loop. Camera light on ⟺ directed with non-empty targets AND not client-muted AND camera healthy.
    - R-19: a track that ends or is revoked mid-call, or a failed re-acquire:
      - presents the participant as video client-muted (`MuteRequest{video_muted: true}`, with `audio_muted` set to its current state until MC-B);
      - is counted on `capture_failures_total{media_kind="video", outcome}` (`permission_denied`, `no_device`, `device_unavailable`, `ended`);
      - has NO automatic retry; only the next directive restore or a user unmute tries again.

    (3) Codec.
    - New `media/setup/vp8.ts` exporting a pure `probeVp8Support(profile)`, mirroring `setup/opus.ts`. It runs `VideoEncoder.isConfigSupported` AND `VideoDecoder.isConfigSupported`.
    - When video is published or received and either fails, media start fails LOUDLY with a typed fault (R-8), never degrading to audio-only.
    - `CODEC_VP8` is declared in `supported_codecs` only when both pass.
    - Task 7 (T-A) has its own deliberately independent substrate probe (`packages/web-app/e2e/vp8Probe.ts`, its own runner). Do not touch it and do not converge it: an SDK capability-check defect must not pass or fail in lockstep with the SDK. If a DRY reviewer flags it, that is the answer.
    - The `VideoEncoder` has NO keyframe interval. `encode(frame, {keyFrame: true})` is driven only by a target with no group in progress (a directive start or resume, a target added to an active stream, or an abandonment the publisher itself decided; see (4)) and by an accepted keyframe request.

    (4) Video egress: new `media/pipeline/videoEgress.ts`, hot path inside the ADR §11 deny scope, no logging, pre-bound handles.
    - Encrypt and sign ONCE per frame, and send identical bytes to every target.
    - One unidirectional QUIC stream per group per target: the first frame is the keyframe, frames are back to back, and the stream is FINISHED when the next group starts.
    - The relay `stream_id` is stamped through C-A1's one function.
    - Per-target capped backlog in frames and bytes (R-12). When a target cannot keep up:
      - RESET that target's current group stream (never finish it);
      - send nothing more to that target until the next group;
      - count `dt_client_media_send_groups_abandoned_total`.
    - The encoder, other targets and audio never stall.
    - The publisher KEYS whenever a target it is directed to send to has no group in progress: a directed start or resume, a target added to an active stream, and an abandonment of that target the publisher decided itself (its per-target backlog cap above) (a browser RESET discards unsent bytes, so MH may never see the next group and has nothing to request against). There is one encoder, so that keyframe starts a new group on EVERY target of the stream. It is bounded by the same effective minimum interval as requests, max(directed `min_keyframe_interval_ms`, compiled floor 100 ms) (see (6)). A keyframe that would be too soon is HELD as the stream's single deferral, never dropped; the abandoned or added target receives nothing until it fires. MH→subscriber recovery stays request-driven on MH's side and is not this task's.
    - WHOEVER DECIDES AN ABANDONMENT REPAIRS IT. A group stream MH refuses or stops (STOP_SENDING, for example ingress refusal or MH's stream-open rate limit) is MH's decision: the publisher does NOT re-key for it, and that target does not count as "no group in progress" for self-keying, at trigger or at deferral fire time. The publisher sends nothing more to that target until the next group starts for another reason (MH's keyframe request when it wants the stream again, or a self-key owed to another target), which reaches it like every target. Otherwise a refused stream would loop at the minimum interval.
    - Hop sequence (ADR §2: it counts what the transmitter actually sent) is assigned per (target, stream) when a frame's header is WRITTEN, never at enqueue. Backlog drops consume no number, and the counter lives outside the QUIC stream object so it continues across groups.

    (5) GoP counter and keys (R-9, R-14).
    - Use P2's counter owner. Video increments it on every keyframe, including the one starting a resume. It never resets, and at u32::MAX the stream stops and never wraps.
    - Video rotates the session-wide generation on:
      - every `videoRotationPeriodMs` (new client config, separate from `audioRotationPeriodMs`, with its own floor);
      - every group start, whether requested, a resume, or keyed for a target with no group in progress;
      - every KEK receipt.
    - Each stream tracks frames since its key last changed. The wrap rides the FIRST N frames after ANY key change, whatever the trigger (configured N, default 3). Key-bearing frames are never discardable, and audio stays key-bearing every frame.
    - Neither T nor a KEK rotation forces a keyframe. The stream sequence keeps counting across withdraw and resume.
    - Update `lifecycle/transmitKeys.ts`'s rotation-trigger table comment.

    (6) Keyframe-request acceptance (R-10, R-11). For `keyframe_request { stream_number, gop_counter }` on handler H's connection:
    - Honour it only if the stream is directed with non-empty targets, H is in its CURRENT target set, and it is not client- or server-muted. A request never starts capture and never makes a muted or undirected stream send.
    - It needs `gop_counter >= current`. A counter above current is honoured and counted as `honoured_counter_ahead`.
    - The effective minimum interval is max(directed `min_keyframe_interval_ms`, compiled floor 100 ms). A directed value below the floor is clamped, counted on `dt_client_media_keyframe_min_interval_clamped_total`, and WARNed once, from lifecycle code.
    - A request inside the interval is DEFERRED (`min_interval`): at most ONE pending per stream, shared with a too-soon keyframe owed to a target with no group in progress (4) (a stream holds one deferral, whatever its cause; a later cause merges into it, never queues a second). It is discarded on withdrawal or mute, and a target-set change re-checks it, with EVERY condition re-checked at fire time: a held request needs its handler still targeted and its counter still >= current; a held self-key needs some current target still without a group in progress.
    - Only the named stream keys. The keyframe reaches every target, so multi-handler requests at one counter coalesce.
    - Count `dt_client_media_keyframes_produced_total{media_kind, source=directive_start|request|abandonment}`: `directive_start` for a directed start, resume or target added; `request` for an accepted keyframe request; `abandonment` for the publisher re-keying after an abandonment it decided itself. Never fold `abandonment` into `directive_start`: the split keeps a lossy-uplink keying loop distinguishable from a request storm. A keyframe is counted once; a merged deferral counts under the cause that created it.
    - A video stream missing any encoding field, `min_keyframe_interval_ms` included, is refused with a typed fault and counted. There is no client default for any video encoding value.

    (7) Keyframe-floor sync guard.
    - Add `scripts/guards/simple/validate-keyframe-floor-sync.sh`, following `validate-subdomain-regex-sync.sh` and `validate-slug-class-sync.sh`.
    - It reads MC's literal `MIN_KEYFRAME_INTERVAL_MS_FLOOR` and the SDK's compiled floor in `packages/sdk-core/src/config/clientConfig.ts`, and fails on inequality or on a literal not found.
    - Wire it into the simple-guard runner.
    - The SDK constant's comment names the MC constant, `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (MC-only config since amendment 4; task 12 (MC-A) renames it from `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS`), this guard and the clamp counter.

    (8) Client knobs (R-23). These are code-config in `clientConfig.ts`, validated and floored, logged once with absent-vs-defaulted distinguished, and outside the ConfigMap and env-config guard:
    - `videoRotationPeriodMs`;
    - the video key-bearing N;
    - the compiled keyframe floor (100);
    - the per-target backlog caps (frames and bytes).

    TELEMETRY (OBS-A handles, catalog rows in `docs/observability/metrics/client.md` in this task):
    - keyframes produced;
    - the honour paths of requests received;
    - min-interval clamped (panel only, no alert);
    - send groups abandoned;
    - `dt_client_media_encoder_errors_total`;
    - video capture failures;
    - video encoding-field faults.

    No GoP values, sizes, timings, or sender or stream identity anywhere.

    TESTS AND GATES (node tier, `MockWebTransport`, injected clock, no wall-time sleeps):
    - Keyframe requests:
      - a burst of stale or duplicate requests yields one keyframe;
      - counter-ahead is honoured and counted;
      - a wrong-handler, undirected or muted stream is `not_sending`;
      - deferral works: one pending, discarded on withdrawal, mute or a target change, and re-checked at fire;
      - a request and a too-soon self-key share the one deferral and yield one keyframe, counted once;
      - each cause counts under its own `source` value, with a publisher-decided abandonment counted as `abandonment`, never `directive_start`;
      - the effective interval is max(directed, floor), with the clamp counted;
      - only the named stream keys.
    - A publisher-decided abandonment resets one target only, with no stall; the publisher then keys a new group on every target, held as the single deferral if inside the effective interval and never dropped.
    - A target added to an active stream gets a keyframe-started group (all targets start a new group), bounded by the effective interval.
    - A stream MH stops or refuses (STOP_SENDING injected on the mock) triggers NO keyframe, at once or at a later deferral fire; that target receives the next group started by an MH request.
    - A directed start keys with no request, within the effective interval; a stream whose targets all have a group in progress never self-keys.
    - The hop sequence has no gaps across abandonment and groups.
    - Rotation triggers for both kinds:
      - key-bearing on the first N frames after each trigger, never discardable;
      - a nonce-uniqueness property test across audio and video;
      - the GoP counter is never reset and increments on resume.
    - The camera stops on every video-off path and is re-acquired on restore; the mic stays open.
    - R-19 is reported and counted, with no retry loop.
    - A missing encoding field is refused.
    - The video stamp comes from the directive.
    - The bundle-absence test passes.
    - `probeVp8Support` fails loudly.
    - The floor guard passes, and fails when either literal is edited.
    - `packages/` has zero `SOURCE_MUTED` / `max_receive_slots` references.

    Gates: sdk-core, sdk-svelte, web-app and test-utils lint, typecheck and unit tests green; story-2 browser specs green. No env-tests: the browser video scenarios are task 17 (C-C1)'s.

    SECURITY ACCEPTANCE:
    - keyframe requests honoured only for a directed, non-muted stream whose targets include the carrying handler, never starting capture;
    - every keyframe, requested or keyed for a target with no group in progress, at most one per effective interval;
    - no re-key for a stream MH refused or stopped, so MH refusal cannot drive a keyframe loop;
    - one deferral per stream, whatever its cause, re-checked at fire and never dropped while still owed;
    - the requested keyframe rotates the key;
    - the wrap on the first N frames after any key change, never discardable;
    - the test pattern build-gated with absence proven, from the meeting-scoped sender id only;
    - camera re-acquire never prompt-loops.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Keys:
      - video transmit keys rotate every T (separate config from audio), at every group start and on every KEK receipt;
      - key-bearing on the first N frames after any key change, and never flagged discardable;
      - stream sequence and GoP counter never reset while the sender id lives, across withdraw and resume;
      - encrypt once, fan out to all targets.
    - Camera and capture:
      - under client mute nothing of that kind leaves the device;
      - the camera stops on any video withdrawal or mute;
      - re-acquire reuses the granted device and permission with no prompt loop;
      - a re-acquire failure presents as video client-muted, counted in `capture_failures{outcome}`, with no automatic retry (R-19).
    - KeyframeRequest:
      - honoured only when the stream is directed, its target set includes the carrying handler, and it is not muted;
      - at most one keyframe per max(directed `min_keyframe_interval_ms`, compiled 100 ms floor), with the floor sync guard;
      - only one deferral is held per stream, shared by requests and keyframes owed to a target with no group in progress;
      - a held request is discarded on withdrawal, mute of that kind, or handler removal, and re-checked at fire time;
      - a request never starts capture;
      - a counter above current is honoured within the bound and counted as an anomaly;
      - malformed and unknown-stream requests are refused and counted;
      - a video stream directed without `min_keyframe_interval_ms` is refused.
    - Every C-A1 transitional shim is deleted; none survives the task.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-13
- id: 14
  status: pending
  specialist: client
  deps:
  - 11
  prompt: |-
    Story 3 "see each other", task 14 (C-B): the SDK video receive path — uni-stream reader, one receive policy, M reusable slot pipelines, live/avatar render decision, receiver recovery requests (docs/user-stories/2026-10-07-see-each-other.md; requirements R-4, R-5, R-6, R-12 (receiver half), R-13 (client half), R-14 (receive half), R-21 (client reader), R-29/R-30 (receive series), R-32 (SDK evidence hooks); wire spec: the story file's Design §Wire freeze). Specialist: client.
    - Pair with security as reviewer of the receive-policy function, the decode-before-live exception and replay handling.
    - Pair with test as reviewer of the injected-clock tests, the reader corpus and the evidence hooks.

    DEPENDENCIES
    - Task 11 (C-A1, client) provides:
      - the uni-stream transport seam on `IWebTransport`/`MockWebTransport` (incoming streams, reset injection, slow reader);
      - the MH bidi read loop and the per-handler control send you use for `recovery_request`;
      - the flat slot states in `StreamAssignmentEvent`, and the assignment-based pairing view you extend;
      - OBS-A's per-kind handles already in use.
    - Transitively, task 6 (P2) provides the v3-only codec and the client reject tokens in the vectors (`self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch`, all `has_vector: false`). Task 2 (OBS-A) provides the per-kind handles.
    - This task does NOT depend on task 13 (C-A2). The node-tier tests build v3 frames directly with the codec and SFrame stack and feed them into mock uni streams, and decoding runs through an injected `VideoDecoder` seam, like the audio decoder seam, because the unit tier has no WebCodecs. Sending-path behaviour is C-A2's.

    WIRE FACTS (story file Design §Wire freeze; summarised for the receive side):
    - Video arrives MH→client on unidirectional QUIC streams, ONE STREAM PER FORWARDED GROUP. Each downlink stream belongs to exactly one of this subscriber's slots (MH guarantees this). Its frames carry relay `stream_id` = this subscriber's slot id (hop-local; MH rewrites it). Frames are back to back, delimited by `payload_length`.
    - MH forwards cut-through, so a frame's bytes arrive across many reads.
    - An MH stream RESET means the group was abandoned (R-12). A FINISH is a normal group end.
    - Slot states: the flat enum. Video ACTIVE names no sender; avatars name the participant with a reason, and server wins. Never-emitted values are fail-closed. Unreachable stays on `unreachable_sender_ids`.
    - `recovery_request { slot_id, sender_id, gop_counter }`:
      - `sender_id` and `gop_counter` come from the FAILING frame's own header, never from the last verified frame, which after a switch names the previous source. `sender_id` is a staleness guard only: MH drops a mismatch as `source_mismatch`.
      - MH dedupes by counter, not time: it forwards an equal counter once per (source stream, counter); a duplicate, a lower counter (`stale`) or a higher one (`counter_ahead`) is counted and not forwarded.

    WHAT TO BUILD (video hot path under `packages/sdk-core/src/media/pipeline/`, inside ADR §11's deny scope; lifecycle and setup are siblings)

    (1) Incremental uni-stream reader (R-21, client half).
    - Parse the v3 header incrementally. Enforce `MAX_PAYLOAD_BYTES` against `payload_length` BEFORE allocating, using the existing `payload_length_exceeds_max` token; where it was counted gives reader vs decode.
    - Buffer the WHOLE frame before verification. Never verify, decrypt or decode a partial payload; cut-through is MH-only.
    - Bound concurrent incoming streams with a configured cap ≥ 2×M + a small constant. At a group boundary the old stream may still drain while the new one opens. Validate cap × MAX_PAYLOAD_BYTES at config time against a configured memory ceiling, and refuse to start if over.
    - A stream whose frames carry differing relay `stream_id`s is aborted and counted as `stream_slot_mismatch`.
    - A frame on an undeclared slot id is dropped and counted.
    - A reset mid-frame discards the partial frame and is counted as `stream_partial_reset`, and the slot loses decodability (see 5).
    - Restate the receive accounting identity at the PARSE boundary per media kind: frames received = accepted + Σ drops by reason, for video. This resolves the "NOTE FOR THE VIDEO STORY" in `pipeline/ingress.ts`; update that note. Audio's datagram identity is unchanged.
    - Reader fuzzing: there is no fast-check in the repo. Build a deterministic corpus from the v3 vectors: truncation at every offset; every length-field extreme (0, max, max+1, u32::MAX); single-bit flips over the header; arbitrary chunk splits of valid multi-frame streams. Every case must end in a typed outcome, never an untyped throw or an unbounded allocation.

    (2) ONE receive-policy-by-kind function (R-5), with a branch-table unit test covering every branch.
    - AUDIO: unchanged from story 2. An unassigned sender is `sender_not_assigned`, dropped BEFORE open.
    - VIDEO:
      - Resolve the identity key from `key_id.sender_id` only. An unrostered sender is the existing no-roster drop.
      - Verify the signature.
      - Then replay-check per (KEK generation, sender, stream).
      - Then unwrap, decrypt and decode, in ANY slot state. This is the video decode-before-live exception, so the frame is ready when the slot turns ACTIVE.
    - Render-side guards, applied before a frame is shown in an ACTIVE slot:
      - the subscriber's own sender id is never rendered: drop and count `self_sender`;
      - a sender whose video is server-muted per the latest `ParticipantMuteUpdate` is never rendered: drop and count `sender_server_muted`.
      - `sender_server_muted` is the receiver-side signal that MH failed to enforce a mute. Observability's alert A5 and the MH 19 runbook select on `dt_client_media_frames_dropped_total{reason="sender_server_muted", media_kind="video"}`.
    - Media kind always comes from the transport and slot (datagrams feed audio lanes, uni streams feed video pipelines), never from the frame. A video frame can never reach an audio lane or the reverse, by construction.
    - All new tokens must be members of `RejectReason` and pass the `ALL_REJECT_REASONS` set-equality test against the vectors file, where P2 adds them as `has_vector: false`: `self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch`. No second client-side union.

    (3) Exactly M video slot pipelines (R-6), one per declared video slot, each a jitter buffer plus ONE WebCodecs `VideoDecoder` for the slot's lifetime.
    - Use only the native WebCodecs VP8 decoder; add no third-party codec.
    - Configure the decoder from VP8 and the slot's configured receive bounds (max coded width, height and frame rate), never from bitstream or extension values. Decoded output above the bounds is dropped and counted as `decode_over_bound`.
    - Decode resources are bounded by M, never by sender count.
    - A NEW SOURCE ON A SLOT reuses that slot's pipeline: no new decoder, and NO jitter-buffer flush. The old source's queued frames play out, then the new source plays from its keyframe. The new source's presentation timestamps (inside the ciphertext) are re-based onto the current playout timeline so relative delay is unchanged. Catching up a grown buffer is future work and out of scope.
    - Fault isolation: a decoder error closes and reconfigures ONLY that slot's decoder. It waits for a keyframe (see 5) and never stalls another slot or audio.
    - Expose decoder-construction count through a TEST SEAM (not the e2e bus).

    (4) Render decision and VideoSink seam (R-4, R-5). Each slot keeps its latest decoded frame tagged with its verified key-id sender. The SDK emits a per-slot render state:
    - `active(sender)`, for an ACTIVE slot once a frame has decoded;
    - `avatar(participant, reason)`, for AVATAR_NO_VIDEO / AVATAR_CLIENT_MUTED / AVATAR_SERVER_MUTED, naming the slot's `sender_id`;
    - `fewer_sources`;
    - `unavailable`;
    - fail-closed for the never-emitted states.

    Transitions:
    - ACTIVE→avatar switches AT ONCE and ignores trailing frames.
    - avatar→ACTIVE keeps the avatar until the first frame decodes. A live slot with no decoded frame yet keeps showing what it showed before, with no spinner.

    Rules:
    - Attribution of live video is the verified key-id sender. MC never names the live source.
    - Frames are drawn through a `VideoSink` seam (`drawFrame(VideoFrame)`), supplied by the embedder. Frame delivery NEVER passes through reactive store updates, and every `VideoFrame` is closed exactly once.
    - Provide a participant PAIRING VIEW keyed by sender id: participant → { audio slot?, video slot? }. Audio slots name their sender; an ACTIVE video slot pairs by the verified sender of its latest rendered frame. R-3 pairs by publisher, never by slot index.

    (5) Receiver-driven recovery (R-13, client half), EVENT-DRIVEN. A video slot that loses decodability sends ONE `recovery_request { slot_id, sender_id, gop_counter }` per loss event on its handler's MH connection, through C-A1's control send. The causes are:
    - a failed verify or decrypt of a frame mid-group;
    - a decoder error;
    - a local jitter-buffer overflow drop;
    - an MH-reset group.

    Rules:
    - No pacing timer and no retry: requests ride a reliable stream, and a later loss is a new event with a newer counter.
    - A loss event is the first failure on a slot at a given (`sender_id`, `gop_counter`). Later failures at the same pair (the rest of a broken group, a decoder error loop) send nothing, so requests are bounded by groups received, never by frames.
    - Both fields come from the failing frame's header. For a reset group, use a header already parsed on that stream; a reset stream with no parsed header sends nothing, since MH knows its own abandonment and recovers it (R-12).
    - A newer request REPLACES an older one still queued and unsent for the same slot (at most one per slot), placed in front of C-A1's control send so nothing below it holds an obsolete request.
    - It carries no other identity.
    - A slot that receives nothing from its new source sends nothing: it is MH's to recover (R-10/R-12).
    - Count `dt_client_media_recovery_requests_sent_total{media_kind="video", reason=verify_failed|decrypt_failed|decoder_error|local_drop|group_abandoned}`.

    (6) Keys and replay (R-14, receive half). Replay state, high-water marks and cached transmit keys are scoped per (KEK generation, sender, stream) for video exactly as for audio (ADR-0036 §4 amendment invariant 3). A tail of an older group arriving after a newer generation is dropped. A replayed earlier video group is rejected and never re-rendered. A non-key-bearing frame for an unknown key id stays a decode-reject, as an invariant violation. The story-2 rebind purge (keys only, replay retained) covers video.

    (7) Configuration (R-23). These are code-config knobs in `packages/sdk-core/src/config/clientConfig.ts`, each validated with a floor and logged once:
    - the incoming-stream concurrency cap (floor 2×M + c);
    - the reader memory ceiling;
    - the per-slot jitter-buffer bounds;
    - the video decode bounds.

    They are SDK config, outside the ConfigMap and env-config guard.

    TELEMETRY (R-29, R-30), on OBS-A's per-kind handles. Each new series lands with its row in `docs/observability/metrics/client.md`:
    - the drop reasons above;
    - the recovery counter;
    - a distinct-active-video-sources gauge fed from DECODE activity, never from slot state (story 2 R-28 precedent);
    - two latency histograms, which are open objectives and never gated: media start → first RENDERED video frame, and slot turns ACTIVE → first rendered frame.
    - the UNFULFILLED-SLOT series (R-30): a video slot MC reports ACTIVE that shows neither decodable video nor an avatar. It enters when ACTIVE with no decoded frame from its current source (before the first decode, or after a loss per (5)), and leaves on its next rendered frame, when MC moves it out of ACTIVE, or on teardown (slot removal, leave, disconnect, SDK disposal). EVERY exit, teardown included, observes the histogram and decrements the gauge, so neither leaks. Emit a time-in-state histogram observed on exit, with buckets extending past 100 s (10× the 10000 ms maximum keyframe interval), and a currently-unfulfilled gauge (named `dt_client_media_slot_unfulfilled_duration_ms` (histogram) and `dt_client_media_slots_unfulfilled` (gauge, panel only), each with `media_kind="video"`; these exact names are referenced by task 20 (OBS-B) and task 21 (T-B), so do not rename them; add both catalog rows). NO identity and NO slot ids; injected clock; updated on transitions only. It feeds a dashboard panel and alert threshold, and the R-35 overlay via task 18 (C-C2): expose per-slot unfulfilled state and time-in-state as a local SNAPSHOT getter for it, never exported.

    Rules:
    - No GoP values, frame sizes, keyframe timings, or sender, slot or stream identity in any label, log, span or e2e-bus field (ADR §11).
    - Per-frame code in `pipeline/` has no logging and no metric-name construction; handles are pre-bound.

    EVIDENCE HOOKS for R-32 (the web app wires them to the e2e bus in task 17 (C-C1); here, expose them from the SDK):
    - per video slot: the MC state vs the actual render state;
    - per (slot, sender) layer-2 counters: verified+decrypted vs dropped-by-reason, extending the story-2 `ReceiveVerificationRecorder`;
    - the verified sender of the last rendered frame.

    Counts and booleans only.

    TESTS AND GATES (node tier with the mock transport, and an injected clock for every deadline; no test sleeps on wall time):
    - A source switch on a slot constructs ZERO new decoders, does not flush, plays the old source out, and re-bases the new source's timestamps. Assert this via the test seam with synthetic timestamps.
    - R-5 in BOTH orders, deterministically:
      - video decoded before the slot turns ACTIVE renders only after ACTIVE;
      - ACTIVE before the first frame holds the previous render until the first decode;
      - ACTIVE→avatar drops trailing frames immediately.
    - A fault on one slot never stalls or corrupts another slot or audio.
    - Exactly M pipelines whatever the sender count.
    - One recovery request per loss cause, fields from the failing frame's header (including just after a switch); a broken group or decoder error loop at one counter sends one; a newer request replaces a queued one; no timer, no retry; nothing for a slot with nothing from its new source, or a reset stream with no parsed header.
    - The unfulfilled-slot gauge and histogram follow every entry and exit path, teardown included (the gauge returns to 0 after teardown), the histogram has a bucket above 100 s, and no label carries identity or a slot id.
    - The reader rejects an oversized length before allocation, and the deterministic corpus passes.
    - `self_sender` and `sender_server_muted` are never rendered and are counted.
    - A video leaver cannot open post-rotation video. This is a client integration test: a departed browser tears down, so it can't be an env-test.
    - A replayed earlier group is rejected.
    - The accounting identity holds at the parse boundary under every drop path.
    - The hot-path layout test stays green with the new pipeline files.
    - Gate: sdk-core lint, typecheck, unit and integration tests are green; the vectors conformance suite is green.

    No env-tests in this task: browser end-to-end scenarios for this behaviour are task 17 (C-C1)'s, because they need the UI and the cluster tasks. Everything here is proven at the node tier.

    SECURITY ACCEPTANCE: a single receive policy, with every branch tested; whole-frame buffering before verify; pre-allocation length check; native decoder only, configured from declared bounds; at most M pipelines; recovery requests carry only slot, sender and counter from the failing frame's header, one per loss event, never retried; replay scoping per (KEK generation, sender, stream).

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Receive policy is ONE function with a branch-table test:
      - audio from an unassigned sender: `sender_not_assigned`, dropped before open;
      - video: verify against the rostered key first; an unrostered sender is dropped; a rostered sender is decoded in any slot state but rendered only in an ACTIVE slot, attributed to the verified key-id sender;
      - `self_sender` and `sender_server_muted` are dropped and counted;
      - media kind always comes from the slot.
    - Stream reader:
      - MAX_PAYLOAD is checked against the length field before allocation (`payload_length_exceeds_max`);
      - the whole frame is buffered, verified, then decrypted and decoded; there is never streamed decrypt or decode;
      - a partial frame on reset is discarded (`stream_partial_reset`);
      - undeclared slot ids are dropped and mixed-slot streams aborted (`stream_slot_mismatch`);
      - the reader is fuzzed;
      - concurrent streams × MAX_PAYLOAD is validated against a configured ceiling.
    - Decoder: native WebCodecs VP8 only, no third-party codec dependency. It is configured from the slot's bounds, never from bitstream or extension values. Over-bound output is dropped (`decode_over_bound`). There are exactly M pipelines.
    - Replay window and generation high-water are scoped per (KEK generation, sender, stream) for video. A replayed earlier video group is rejected and never re-rendered. A stale group tail after a newer generation is dropped. A client integration test shows a leaver cannot open post-rotation video.
    - RecoveryRequest carries `slot_id` and the sender and GoP counter from the failing frame's own header, nothing else; at most one per slot per (sender, counter), no retry, a newer one replacing a queued one. Those header values may be unverified (for example after a verify failure); that is accepted because MH forwards only on an exact counter match for the slot's current source and `source_mismatch` catches the rest. Security confirms this at this task's Gate 1.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-14
- id: 15
  status: pending
  specialist: meeting-controller
  deps:
  - 7
  - 8
  - 9
  - 10
  - 11
  - 12
  - 13
  - 14
  prompt: |-
    MC-B: four independent mute controls at MC for audio and video, server-mute directive withdrawal, client mute as MH policy, and deletion of the superseded proto fields (story 3 "see each other", `docs/user-stories/2026-10-07-see-each-other.md`). Requirements: R-15, R-16, R-17, R-18 (MC half), R-25 (in-story deletion), R-30 (MC mute series), R-33 (MC mute scenarios), R-36 (ADR-0036 §5 text). Wire shapes are in the story's Design §Wire freeze. Specialist: meeting-controller. Pair with:
    - security, for the authority ordering and per-kind request semantics;
    - test, for every env-test;
    - media-handler, for the superseded-field deletion, which edits MH too.

    DEPENDENCIES
    - task 12 (MC-A) provides:
      - the per-kind slot table;
      - the flat slot states (`AVATAR_CLIENT_MUTED = 10` and `AVATAR_SERVER_MUTED = 11`, server wins), already emitted for both kinds from the roster's existing flags;
      - `source_streams` with dispositions FORWARD / PAUSED / DROP_AT_INGRESS;
      - the always-listed directive, with empty targets for an unwatched stream;
      - `mc_media_send_directive_withdrawals_total{media_kind, reason}`, with `unwatched` already emitted;
      - the pinned stream numbers (audio 0, video 1).
    - task 8 (MH-A1) provides per-stream enforcement.
      - DROP_AT_INGRESS drops everything attributed to that (sender, stream) as `server_muted`. A sender whose every declared stream is DROP_AT_INGRESS is dropped wholesale.
      - PAUSED never drops ingress. A switch to a paused stream completes at once as "forward nothing" and cancels any keyframe wait.
      - Attribution is (connection-bound sender, uplink relay `stream_id`) looked up in `source_streams`. An unknown id drops as `unmatched_stream`, and a declared stream arriving on the wrong transport as `transport_mismatch`.
      - The label is publisher-set and unauthenticated. R-16 still holds in slot terms: a server-muted stream has no live route, and receivers decode by slot kind. A relabelled frame can reach only slots of the OTHER stream's kind, never a slot of the muted kind.
    - task 9 (MH-A2): stream-per-group video forwarding, which the patched-client env-test needs so video actually flows.
    - task 10 (MH-B): on-demand keyframes and recovery. No keyframe requests go to a paused or DROP_AT_INGRESS stream. After it lands, MH has no reader left for the superseded fields.
    - task 11 (C-A1), task 13 (C-A2) and task 14 (C-B): the SDK publish and receive paths. They migrate every TypeScript consumer off `SLOT_STATE_SOURCE_MUTED` (to the avatar values) and off `JoinResponse.max_receive_slots` (to the per-kind caps), and adopt the per-kind optional mute fields.
      - They MUST be landed first, because this task deletes both items and regenerates `packages/proto-gen`.
      - Before deleting, grep `packages/` for both names and their generated identifiers and confirm zero references. If any remain, stop and report; do not edit client code here.
    - task 1 (P1), transitively: `MuteRequest` and `ServerMuteRequest` per-kind fields are `optional bool` (tag-preserving).

    SERVER MUTE PER KIND (R-15, R-16)
    - Replace `ServerMuteAction::from_request(audio, video)` in `crates/mc-service/src/media_signaling/outcome.rs`. Today it folds both bools into one mute/unmute, so a request about audio can silently clear a video mute.
    - New request semantics:
      - each kind is independent;
      - an ABSENT field means unchanged for that kind;
      - a request with BOTH fields absent is rejected and counted, with a generic client message.
    - Keep story 2's ordering exactly; it is the security property:
      - authority first: requester from the authenticated connection, host role from the token, refusal before any target lookup;
      - then the per-connection `server_mute_limiter`;
      - then the target.
      The connection and the actor both check authority.
    - `handle_server_mute` in `crates/mc-service/src/actors/meeting.rs` stays the single writer of `audio_server_muted`, `video_server_muted` and `server_muted_by`. Clear `server_muted_by` only when neither kind remains muted.
    - `UnmuteRequest` only notifies the host and never clears a mute.
    - A server mute of kind K is one structural change in one actor turn:
      - set K's stream to DROP_AT_INGRESS;
      - empty K's directive targets, counting the withdrawal as `reason="server_muted"`;
      - re-render and re-push (the generation advances);
      - confirm the echo;
      - re-emit to every holder as AVATAR_SERVER_MUTED.
    - The other kind's directive, disposition, edges and slot states stay byte-identical.
    - A server unmute of K restores K's targets and disposition in the same structural change, REGARDLESS of client mute. Security and client agreed this: the directive stays blind to client mute (ADR §5), and a client-muted client keeps capture off locally, which is where R-18's "unless client-muted" is met.
    - A server-muted stream KEEPS its slot and edges (R-3 stability). Its edges stay in the snapshot with DROP_AT_INGRESS, so MH counts the drops as `server_muted`.
    - The mute lives on the roster entry. It survives a re-assert, an MH restart (the next snapshot restates it) and a reconnect within grace, and is not carried into a fresh rejoin.

    DIRECTIVE BOUNDARY (ADR §5)
    - `crates/mc-service/src/media_signaling/directive.rs` has no path to mute state by construction. Narrow that invariant; do not drop it.
    - The builder gains a per-kind SERVER-mute input and still has NO import path to client-mute state. A client mute/unmute cycle altering a directive therefore stays a compile error.
    - Rewrite its module doc to "server mute and watch state, never client mute", and update the matching sentence in `assignments.rs`.

    CLIENT MUTE PER KIND (R-17)
    - `handle_self_mute` takes per-kind optional fields: an absent field means unchanged, and both absent is rejected and counted. The participant always comes from the connection.
    - A change is a structural change:
      - the disposition flips between FORWARD and PAUSED, never DROP_AT_INGRESS (client mute alone never drops ingress, so a resume keyframe that beats MC's update still reaches subscribers);
      - the generation advances;
      - re-push, then confirm the echo;
      - holders are re-emitted as AVATAR_CLIENT_MUTED, or AVATAR_SERVER_MUTED if the server mute is also on.
    - The directive is never touched by client mute.
    - A toggle that changes nothing stays a no-op before the actor hop and the limiter, and does not advance the generation.
    - Churn is bounded by the existing per-connection `mute_limiter` (`crates/mc-service/src/webtransport/connection.rs`) plus the per-handler latest-wins push worker (`crates/mc-service/src/media_routing/pusher.rs`): at most one push in flight and one pending per handler. No new config key.
    - Prove the bound with an integration test: N rapid toggles across several participants keep RegisterMeeting pushes per handler within the bound, and the final applied snapshot equals the final state.

    ORDERING (R-18; Wire freeze)
    - In the actor turn applying a server mute or unmute, send the self `ParticipantMuteUpdate` to every participant (target included) BEFORE the target's directive change and the holders' `StreamAssignments`, on the same ordered per-participant path.
    - The client keys its non-suppressible "a host resumed your camera/mic" notice on the pair: K is no longer server-muted in the latest self update, AND K's targets went from empty to non-empty.
    - Pin the order with a test.
    - `server_mute_replay` carries per-kind state unchanged.

    METRICS (R-30)
    - Task 2 (OBS-A) already changed the recorders (`record_mute_request(kind, outcome)`, `record_server_mute_request(kind, action, outcome)`, `set_server_muted_sources(kind, total)`) and their zero-init and catalog rows. This task adds no labelling; it supplies the per-kind semantics and the real kinds.
    - Counting:
      - both mute counters increment ONCE PER KIND THE REQUEST NAMES, request-level refusals included;
      - a both-absent rejection names no kind and counts on the generic path.
    - The muted-sources gauge comes from a per-kind census (`MutedSourceCensus`, `crates/mc-service/src/actors/meeting_media.rs`).
    - This task emits `mc_media_send_directive_withdrawals_total{reason="server_muted"}`.
    - `mc_media_send_directives_total` stays kind-free.
    - Any series added or reshaped here lands with a basic `mc-media.json` panel and an `mc-service.md` row (the `application-metrics` guard's checks 5 and 6).
    - `key_custody=operator`, no identity labels, and no mute history in logs or labels. The lifecycle log may name participants on the span, as today.

    SUPERSEDED-FIELD DELETION (R-25 confirmed 2026-10-08: the story ships as one coordinated cutover, config, then MH, then MC, then client, and rolls back as a whole)
    - Delete:
      - from `proto/dark_tower/internal/v1/internal.proto`: `MutedSource` and `RegisterMeetingRequest.server_muted_sources` (reserve tag 7 and the name), and `EgressStream.transport_mode` (reserve tag 6 and the name);
      - from `proto/dark_tower/signaling/v1/signaling.proto`: `JoinResponse.max_receive_slots` (reserve tag 12 and the name) and `SLOT_STATE_SOURCE_MUTED` (reserve value 2 and the name).
    - Follow `docs/protocol/CONVENTIONS.md` reserve rules.
    - Update every comment that referenced the deleted items, including the old "per-stream ingress mute is foreclosed" text in `internal.proto` and `crates/mh-service/src/media/forward.rs`, which R-16's slot-stated property supersedes. Agree the wording with protocol.
    - Delete MC's writes and MH's remaining reads, with media-handler paired. That INCLUDES MH-A1's transitional legacy path: a registration carrying no `source_streams` is treated as "stream 0 DATAGRAM for every sender, `server_muted_sources` senders DROP_AT_INGRESS", which kept intermediate commits green before MC-A emitted `source_streams`. After this task, a registration with egress present and an absent or empty `source_streams` is a reject, not a legacy fallback. Every candidate is then undeclared, so MH's existing rule rejects it as `rejected_transport_inconsistent` (no new token). A test pins that, and MH-A1 labels the legacy function "transitional: deleted by MC-B with the superseded proto fields" so it is easy to find.
    - Regenerate the Rust (`crates/proto-gen`) and TypeScript (`packages/proto-gen`) bindings, and update `crates/proto-gen/tests/internal_roundtrip.rs` and the codegen oracle.
    - Keep `scripts/guards/simple/validate-internal-proto-no-key-material.sh` green.
    - `buf breaking` reports the deletion. Record it as a declared intentional break, the way P1 records the v3 break; never weaken the gate.
    - In-repo text refers to tasks by code only (for example "deleted by MC-B").

    ADR (R-36, §5)
    Amend `docs/decisions/adr-0036-media-flow.md` §5, coordinating placement with P1's bundle (which carries §6). Use a dated amendment in the ADR's convention, with the superseded sentence kept visible. It must state:
    - there are four independent controls that compose;
    - server mute withdraws that kind's directive (empty targets: capture stops, and the camera light goes off) AND MH drops at ingress as the backstop;
    - client mute keeps the directive and is enforced at capture, never by an ingress drop;
    - the directive never depends on client mute, which keeps "MC has not asked you to send" distinct from "you muted yourself";
    - plainly: a host's server unmute can resume a participant's camera or microphone unless they have client-muted, and the participant is notified at that moment;
    - the mic stays open on audio mute of either cause; only a video mute stops the camera.

    TESTS
    - Unit:
      - per-kind parsing (absent means unchanged; both absent rejected);
      - once that parsing lands, switch the SDK's `SignalingClient.sendMuteRequest` and the host's server-mute request to send only the changed kind (client reviews), and test that a single-kind request leaves the other kind's state untouched (amended 2026-10-20: C-A1 sends both kinds until this task);
      - one kind never changes the other's directive, disposition or slot state;
      - server-wins;
      - client mute never yields DROP_AT_INGRESS and never alters a directive;
      - server unmute restores targets while client-muted.
    - Integration (`crates/mc-service/tests/server_mute_integration.rs` and siblings):
      - R-15: host-only per kind; non-host refused before target lookup; generic counted refusals; rate bound;
      - R-16: a video server mute empties the video targets and sets DROP_AT_INGRESS with audio untouched, and the reverse; survival across a re-push and a reconnect within grace;
      - the R-17 churn bound;
      - the R-18 ordering pair.
    - Env-tests (Layer 7, live Kind, Rust, paired with test, task 7 (T-A) fixtures), extending `crates/env-tests/tests/35_mc_server_mute_teardown.rs`:
      - Rules: per-entity evidence only; no wall-clock gates or sleeps.
      - Host server-mutes a publisher's video: its directive lists video with empty targets, and every receiver's video slot for it reads AVATAR_SERVER_MUTED while its audio slots stay ACTIVE.
      - A PATCHED mock publisher that keeps sending video, including relabelled or re-transported frames, reaches no subscriber's video slot. Evidence: no video frames from it at any subscriber, and MH's `server_muted` drops for it rising (an attributable single-entity delta).
      - A server audio mute leaves video flowing, and the reverse.
      - Client video mute: AVATAR_CLIENT_MUTED at every holder, PAUSED in the applied snapshot, directive unchanged.
      - Host unmute restores the targets.
      - A mute-toggle storm stays within the push bound.
      - Record a concurrent edge sum of at most 46 in the file header.

    ACCEPTANCE
    - Layers 1-7 are green, story-2 audio server-mute specs included. Their wire assertions use the avatar values that MC-A already emits.
    - No reference remains to the deleted fields or to `ServerMuteAction::from_request`.
    - `directive.rs` admits server mute only.
    - The §5 amendment has landed.
    - `docs/specialist-knowledge/meeting-controller/INDEX.md` is updated for the per-kind mute paths.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Server mute per kind:
      - host-only from the token, requester from the connection;
      - the authorisation → limiter → target-read order is kept;
      - refusals are generic and counted;
      - a request about one kind never changes the other, with a test for each cross-kind arm;
      - `ServerMuteAction::from_request` is replaced.
    - A server-muted kind gets its directive withdrawn AND (sender, stream) set to DROP_AT_INGRESS, with the edge kept. The mute survives re-assert, MH restart and reconnect within grace.
    - Server unmute restores the directive regardless of client mute; the client keeps capture off. MC sends the self ParticipantMuteUpdate BEFORE the directive change, with an ordering test. This obligation does NOT lapse with the flat slot states.
    - Client mute: the participant comes from the connection; the existing rate bound applies; idempotent toggles don't bump the generation; churn cannot drive unbounded re-pushes. Client mute sets PAUSED and never DROP_AT_INGRESS.
    - Field deletion, paired with media-handler (`MutedSource`, `EgressStream.transport_mode`, SOURCE_MUTED): `validate-internal-proto-no-key-material.sh` still passes, and no deleted field leaves a reader that silently defaults.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-15
- id: 16
  status: pending
  specialist: infrastructure
  deps:
  - 3
  - 8
  - 11
  - 13
  prompt: |-
    Prove video in the Kind dev cluster for story 3, "See Each Other" (docs/user-stories/2026-10-07-see-each-other.md; covers R-24 (Kind half), R-27 (dev-web.sh half), R-28 (Kind test codec profile and budget); ADR-0036 §11 "Derive rather than guard" and "Admission control is keyed on egress bandwidth"; ADR-0038 §2). This is task 16 (INF-C), infrastructure.

    DEPENDENCIES, and what each provides:
    - Task 3 (INF-A, infrastructure): the shared `media-codec-profile` ConfigMap (base `infra/services/media-codec-profile/profile.env`, eight `MEDIA_*` codec keys consumed by MC and MH — amendment 4 moved the minimum keyframe interval to MC's ConfigMap as `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` (task 12 (MC-A)); the "both services read it" membership rule is a header comment, not a guard), the per-kind caps `MC_MAX_AUDIO_RECEIVE_SLOTS`=8 and `MC_MAX_VIDEO_RECEIVE_SLOTS`=4, and the retired-but-present `MC_MAX_RECEIVE_SLOTS` and `MH_STREAM_COST_{AUDIO,VIDEO}_BPS` (optional refs).
    - Task 8 (MH-A1, media-handler): MH derives each per-stream cost as MAX_BITRATE_BPS + (code-derived per-frame overhead bits × FRAME_RATE_HZ) from the profile, for audio and video, logs the derived costs at startup, and publishes the derived stream ceiling as `mh_media_egress_stream_ceiling` (the value admission enforces and advertises to GC); `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` re-derived for P×(N+M)=54.
    - Task 11 (C-A1, client): the client slot-variable rename `VITE_DT_AUDIO_RECEIVE_SLOTS` / `VITE_DT_VIDEO_RECEIVE_SLOTS` (the old `VITE_DT_RECEIVE_SLOTS` throws if set) INCLUDING `scripts/dev-web.sh`'s slot export and cap preflight (reads the per-kind caps from `infra/services/mc-service/config.env`; over-cap N or M is a notice that excess slots show UNAVAILABLE, not a hard fail; video cap 0 reported as "video disabled"; `scripts/dev-web.test.sh` updated). A rename owns every reader in one change; do NOT redo it here — build on it.
    - Task 13 (C-A2, client; depends on task 11 (C-A1) and task 12 (MC-A)): the SDK video publish path (VP8 at the MC-directed profile) and the build-time synthetic test-pattern camera, define `__DT_TEST_PATTERN__`, opt-in `DT_TEST_PATTERN=1`, defined in `packages/web-app/vite/testDefines.ts` beside `__DT_TEST_TONE__`; confirm its location in the tree before documenting it.
    Not yours, already landed in task 7 (T-A, test): the VP8 probe and all its wiring (`packages/web-app/e2e/vp8Probe.ts`, called from Playwright global-setup against the DEPLOYED `media-codec-profile` and mapped to layer 7's Phase-1g `PRECONDITION_FAILURE` lane with `REASON=browser-vp8-unsupported`; plus a vitest browser-mode component test under the existing `ci-client.yml` component step). INF-C registers and wires nothing for the probe; your only obligation is item 1's: the Kind test-profile patch is the ConfigMap the probe reads.

    WHAT TO BUILD

    1. Kind test codec profile (R-28; lead ruling: APPLIED, not conditional on evidence). Add a strategic-merge patch on the `media-codec-profile` ConfigMap in the Kind overlay (e.g. `infra/kubernetes/overlays/kind/services/media-codec-profile/` with a `kustomization.yaml` referencing the base and a `configmap-test-profile-patch.yaml`; wire it into `infra/kubernetes/overlays/kind/services/kustomization.yaml` in place of the bare base reference INF-A added). It changes exactly four values: `MEDIA_VIDEO_WIDTH=320`, `MEDIA_VIDEO_HEIGHT=180`, `MEDIA_VIDEO_FRAME_RATE_HZ=15`, `MEDIA_VIDEO_MAX_BITRATE_BPS=250000`. Every `MEDIA_AUDIO_*` key is unchanged, and the minimum keyframe interval is not in this profile (MC config, default 500). Patch values only, never add a key: a strategic merge is additive by key name, so a renamed base key would leave a dead patch key while the base value wins in Kind — say so in the patch header as the egress-budget patch does, and cover it with the env-test in 4. Header: why Kind runs a lower profile (env-tests run a config-driven lower profile per R-28, so a cohort of headless tabs can encode and decode VP8 on a devloop host and CI runner; capacity is verified as evidence by task 21 (T-B)), that the profile change flows to MC's directive and MH's derived cost from this ONE place, that it rolls MC and MH together (content-addressed), that the dev-web.sh demo inherits it, and that Layer 7's VP8 probe (T-A) reads this deployed profile, so the probe tests exactly what the env-tests run. Values must be inside MC's bounds (`crates/mc-service/src/media_signaling/directive.rs`); cite them, do not restate them.

    2. Production-quality demo: there is NO override mechanism — no new deploy.sh input, no dev-web.sh flag, no second render path (user preference for existing tooling over new mechanism). Getting production quality is a documented one-step recovery with existing tooling: remove or edit the one Kind test-profile patch (name the file), then run `infra/kind/scripts/deploy.sh` (devloop container: `dev-cluster deploy`); restoring is `git checkout` of that file plus a deploy. State this in the patch header and in dev-web.sh, including that the switch rolls MC and MH (content-addressed) and drops live meetings, and that the patch must be restored before running Layer 7. The env-test in 4 is what keeps this loud.

    3. Kind egress budget re-size (R-24). Re-size `MH_EGRESS_BUDGET_BPS` in `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` against MH's DERIVED ceiling at the Kind test profile. Re-derive the two-sided window in that file's comment, replacing story 2's audio-only (N+1)×N reasoning:
       - Lower side, the largest load that must fit on one handler: max of the demo (P=6, N=5, M=4 → 6×(5+4) = 54 edges) and the largest per-file concurrent edge sum in the env-tests (test reports 72, `27_mc_slot_placement` by formula, ≈60 actually emitted; cargo runs test binaries sequentially, so only fns within one file overlap — each video env-test file records its Σ P×(N+M) in its header), PLUS the retained-edge floor story 2 task 20 observed (disconnected participants hold edges through `MC_DISCONNECT_GRACE_PERIOD_SECONDS` while suites run back to back; 39 edges at that time). That is roughly 111.
       - Upper side, unchanged in kind: the S9 exhaustion test (`crates/env-tests/tests/28_mh_egress_admission.rs`; limit and rule in `crates/env-tests/src/fixtures/egress_admission.rs`) must still be able to drive a registration past the ceiling within its participant limit: 2 × ceiling < MAX_S9_PARTICIPANTS × (the slots S9 actually declares per subscriber, now read from the per-kind cap keys it uses).
       - Target the middle of the window (planning estimate: a ceiling of roughly 120–140, i.e. a budget near 32–37 Mbps at MH's estimated ≈ 0.265 Mbps video cost). Derive the final value from the DEPLOYED derived cost (MH-A1 logs the derived costs at startup and publishes the ceiling gauge; read them, do not trust this paragraph's estimate). If the window is empty or narrower than the retained-edge floor, STOP and escalate with the numbers: do not shrink the floor, shorten the grace period, or serialise env-tests to make it fit (all three were considered and rejected in story 2 or in this story's planning).
       - Comment rules carried from story 2: no restated ceiling or cost figure (they go stale when the overhead constant moves); cite the enforcing env-test and the formula; say why under-sizing is dangerous (it presents as "some receivers miss some senders", indistinguishable from a routing bug); keep the GUARD COVERAGE paragraph (this patch changes a value, never a key).

    4. Env-tests (Covers: env-tests; pair with test). Use `crates/env-tests/src/fixtures/kube.rs:configmap_key()`/`resolve_generation()`, `fixtures/metrics.rs` (instance-pinned metric reads), `eventual.rs:assert_eventually()`; no wall-clock sleeps, no wall-clock thresholds.
       - Switch the Kind window test (`01_mh_deployment_config.rs::test_kind_egress_stream_ceiling_meets_demo_requirement`), `28_mh_egress_admission.rs` and `fixtures/egress_admission.rs` from re-deriving the ceiling out of ConfigMap costs (which are retired and no longer read by MH) to reading `mh_media_egress_stream_ceiling` from each MH instance, and from `MC_MAX_RECEIVE_SLOTS` to the per-kind cap keys. Keep its distinct "overlay did not apply" failure. Assert both sides of the window from 3 against the gauge; keep the lower-side constants (54, the per-file maximum, the retained floor) in ONE place in the fixture with a comment naming their sources, and make the per-file maximum a value the test reads or cites, not a second copy that drifts — if the per-file sums live only in file headers, add a hermetic check (Layer 3 script self-test or a unit test in env-tests) that the fixture's maximum is ≥ every recorded header sum.
       - New: the live `media-codec-profile` values equal the committed Kind test-profile patch values (read the patch from the repo, read the live ConfigMap generation MC and MH pods resolve), so a dead patch key or an overlay that did not apply fails loudly; the failure message names the Kind test-profile patch file and says to restore it (`git checkout`) and redeploy.
       - Remove `MC_MAX_RECEIVE_SLOTS` and `MH_STREAM_COST_{AUDIO,VIDEO}_BPS` from every env-test read that this task touches, leaving the retiring keys' ref-existence assertions (INF-A's) intact until the scheduled step-2 deletion.

    5. `scripts/dev-web.sh` multi-participant video demo documentation (R-27). On top of C-A1's slot-variable rename and preflight: document the see-and-hear demo bring-up — N+1 distinct accounts, one per browsing context, separate browser profiles recommended; that Kind runs the TEST profile (320×180, 15 fps) and how to get production quality: remove or edit the one test-profile patch and run `infra/kind/scripts/deploy.sh`, restore it with `git checkout` plus a deploy before Layer 7 (a profile switch rolls MC and MH and drops live meetings); the test-pattern camera opt-in `DT_TEST_PATTERN=1` (a Vite build-time define, absent from production bundles, per participant and changing every frame) as the way to run several participants on one machine; real-camera bring-up, including that one physical webcam generally cannot be opened by several browser profiles at once (notably on the WSL2/Windows host), so a real-camera demo uses one real camera plus test-pattern participants or several machines; that the R-7 local loopback self-check is a client debug option; and that `MC_MAX_VIDEO_RECEIVE_SLOTS=0` means video is disabled cluster-wide. No insecure browser flag anywhere (`dt-guard no-insecure-browser-flags` stays green; `--use-fake-device-for-media-stream` / `--use-fake-ui-for-media-stream` are already sanctioned for Playwright only and are NOT needed for the test pattern, which replaces getUserMedia video in-app). Extend `scripts/dev-web.test.sh` for anything you add that has behaviour (e.g. a profile line in the preflight banner read from the live ConfigMap); documentation-only text needs no test. Update `docs/runbooks/client-dev-local.md` only to cross-reference dev-web.sh; operations owns the camera-permission and no-video ladder there (task 19 (OPS-B)).

    GATES AND ACCEPTANCE
    - `scripts/layer-all.sh` green with the Kind cluster at the TEST profile; every story-2 audio env-test and browser spec stays green (R-33: story-2 audio specs green with M=0 are owned by T-A and C-C1; this task must not regress them).
    - The window env-test passes against the live gauge on both MH instances and fails (verified once by a scratch, uncommitted mis-sizing) when the budget is moved outside the window on either side.
    - The profile-equality env-test fails loudly, naming the patch file, when the patch is not applied (verify once with a scratch, uncommitted removal of the patch from the overlay's kustomization), and passes on the committed tree. Record both runs in the devloop output.
    - No new value has two homes: the test profile exists only in the Kind patch, production defaults only in `profile.env`, the ceiling only as MH's gauge.
    - Fail loudly: no skip, no `continue-on-error`, no widened tolerance to make a run pass; an empty sizing window is an escalation, not a tuning exercise.

    Pair with test (window env-test, profile-equality test, per-file edge sums), media-handler (derived cost and gauge semantics), client (dev-web.sh rename built in C-A1; test-pattern define built in C-A2), and operations (the profile switch's blast radius, which their runbook states). Dependencies: task 3 (INF-A), task 8 (MH-A1), task 11 (C-A1), task 13 (C-A2). Provides: a Kind cluster whose profile and budget the browser env-tests (task 17 (C-C1)) and the capacity evidence and manual test plan (task 21 (T-B)) run against.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - No insecure browser flag anywhere (existing guard). The test codec profile is a Kind config patch only, changed by editing or removing the patch and redeploying. There is no runtime or dev-web.sh override path.
    - `dev-web.sh` demo docs use the build-gated test pattern; they never instruct disabling certificate checks.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-16
- id: 17
  status: pending
  specialist: client
  deps:
  - 7
  - 10
  - 12
  - 13
  - 14
  - 15
  - 16
  prompt: |-
    Story 3 "see each other", task 17 (C-C1): the demonstrable see-each-other grid — Svelte adapter, paired video grid, slot-state copy, four mute controls and host affordances, the R-18 notice, the publish-set chooser, the per-slot e2e evidence bus, and the client-owned browser env-tests including the story-2 reruns (docs/user-stories/2026-10-07-see-each-other.md; requirements R-1, R-2, R-3, R-4, R-5 (UI), R-15 (UI), R-16, R-17, R-18 (UI), R-19 (UI), R-28 (UI surfacing), R-29 (read-back), R-32 (bus and assertions), R-33 (client-owned scenarios); wire spec: the story file's Design §Wire freeze). Specialist: client. Pair with test as reviewer of every env-test, the bus shape and fixture discipline; security reviews the R-18 notice and the bus gating.

    The debug options (R-7 loopback self-check, R-35 stats overlay) are task 18 (C-C2), not this task.

    DEPENDENCIES
    - Task 14 (C-B, client) provides the receive path; task 13 (C-A2) the video publish path; task 11 (C-A1) the contract migration. The SDK surface you consume:
      - per-slot render states `active(sender)`, `avatar(participant, reason)`, `fewer_sources`, `unavailable`, plus fail-closed;
      - the `VideoSink` seam (`drawFrame(VideoFrame)`) and the participant pairing view;
      - independent `setAudioMuted`/`setVideoMuted`, per-kind server-mute state, and the "capture resumed after server mute" event;
      - the publish-set and N/M join options, with runtime-readable N, M, caps and publish set;
      - the layer-2 evidence recorder and render-state hooks;
      - the test-pattern define `__DT_TEST_PATTERN__` (`DT_TEST_PATTERN=1`);
      - the exported per-kind client series.
    - Task 7 (T-A, test) provides:
      - the pattern detector `packages/web-app/e2e/patternDetector.ts`, which shares no code with the generator;
      - the slot oracle predicting each slot's expected source from join order and the fill rule;
      - the extended cohort (N audio + M video);
      - the independent VP8 substrate probe (`e2e/vp8Probe.ts`), which never imports `probeVp8Support`;
      - the Rust video fixtures.
    - Task 10 (MH-B, media-handler) provides on-demand keyframes (one request per (source stream, counter), never retried), recovery remap with counter-based dedupe, and pending visibility, on top of MH-A1/MH-A2's stream-per-group forwarding (tasks 8, 9).
    - Task 12 (MC-A) provides video slots, directives and the flat slot states.
    - Task 15 (MC-B, meeting-controller) provides the four mute controls, per-kind server mute with directive withdrawal, and the self `ParticipantMuteUpdate` delivery.
    - Task 16 (INF-C, infrastructure) provides:
      - the Kind test codec profile (320×180, 15 fps, 250 kbps, delivered through the shared `media-codec-profile` Kind patch; there is no dev-web.sh override);
      - the egress budget resized for N audio + M video edges per subscriber;
      - the dev-web.sh video demo docs.

    SLOT STATES: the flat enum per the story file's Design §Wire freeze. The SDK tokens are identical to MC's `slot_state_label()`: `active`, `avatar_no_video`, `avatar_client_muted`, `avatar_server_muted` (server wins), `fewer_sources_than_slots`, `unavailable` (truncated over the per-kind cap, permanent, never a join failure), plus the never-emitted `switch_pending`, `withheld_by_congestion` and `source_unreachable`. Unreachable peers come from `unreachable_sender_ids`. Video `active` names no sender, so the UI attributes it from the SDK's verified render tag.

    WHAT TO BUILD

    (1) Svelte adapter (`packages/sdk-svelte/src/`).
    - `stores/MediaStore.svelte.ts` gains per-video-slot render state and the per-participant pairing view, with fine-grained reactivity:
      - a slot's state change re-renders only that tile;
      - a participant's mute change re-renders only their tile and row;
      - decoded frames NEVER touch the store.
    - Prove the granularity in `__tests__/MediaStore.test.ts`, extending story 2's re-render granularity proof.
    - A `VideoTile.svelte` component binds a canvas as its slot's `VideoSink` and draws at playout time.
    - `stores/bindMeetingSession.ts` wires the new SDK events.

    (2) In-meeting view (`packages/web-app/src/views/InMeeting.svelte`, `ParticipantRow.svelte`, `lib/participantState.ts`, `lib/slotState.ts`).
    - An M-tile video grid paired with the N audio rows by participant (R-3): one participant's audio and video show as one participant, paired by publisher sender id, never by slot index.
    - `lib/slotState.ts` stays EXHAUSTIVE by type over the SDK union, with no default arm and no index signature. Write user-facing copy for each state as a complete statement about the source:
      - avatars name the participant and give the reason (camera off by choice / turned off by the host / not sending video);
      - unavailable ("this meeting allows fewer video slots");
      - fewer sources;
      - the never-emitted states render fail-closed copy that says the state is unexpected.
    - The DOM keeps `data-slot-state` = the raw wire token.
    - A live slot with no decoded frame yet keeps its previous rendering, with no spinner for a steady state.
    - Four mute indicators per participant (client audio, client video, server audio, server video), visibly distinct.
    - The host affordance: server-mute and unmute per kind per participant. The host role is unchanged from story 2 R-8.
    - R-18: when the SDK's "capture resumed after server mute" event fires, show a NON-SUPPRESSIBLE notice ("the host turned your camera/microphone back on") with an inline client-mute button that takes effect at once. It cannot be dismissed before it is acknowledged and cannot be disabled by any setting.
    - R-19: a participant whose camera failed is shown to themselves with the failure reason and to others as video-muted.
    - Join view (`views/JoinMeeting.svelte`): a publish-set chooser (microphone / camera) whose result is the join's fixed publish set. A choice that would neither publish nor receive shows the SDK's refusal text from `lib/errorText.ts`, never a silent empty meeting.
    - Slot counts come from `VITE_DT_AUDIO_RECEIVE_SLOTS` and `VITE_DT_VIDEO_RECEIVE_SLOTS`, as renamed by task 11 (C-A1). Replace C-A1's minimal `slotState.ts` copy with the final copy.

    (3) E2E evidence bus (`lib/e2eBus.ts`, `lib/e2eAnalysis.ts`, under `__E2E_HOOKS__`, absent from production; extend the bundle-content test). Use test's agreed shape, counts and booleans only. Never GoP values, frame sizes or keyframe timings. Per video slot:
    - slot id and kind;
    - MC state;
    - what is actually rendered (video vs avatar);
    - the verified key-id sender of the last RENDERED frame;
    - frames decoded and frames rendered;
    - the 24 block-mean lumas sampled from the RENDER SURFACE (the canvas), not the decoder output, so wrong-slot rendering is caught; test's node-side detector decides ID, guard and liveness;
    - layer-2 counters per (slot, sender): verified+decrypted vs dropped-by-reason.

    Send side:
    - camera-track-live and mic-track-open booleans;
    - per sent stream, frames sent and keyframes produced by cause;
    - each participant's own pattern id.

    (4) Browser env-tests (Layer 7 in `packages/web-app/e2e/`, live Kind, run on the test codec profile).
    - Paired with test. No test gates on a wall-clock number, and every deadline uses `windowSampling.ts`'s stop rule, never a sleep.
    - Assert camera light as the camera track's `readyState`, with live ⟺ (directed AND NOT client-muted AND camera healthy).
    - Each scenario names its R-numbers in its header:
      - `multi-party-see.spec.ts`, see-and-hear by content at THREE layers per receiver per sender (R-1, R-5, R-32, R-33):
        - layer 1: the last rendered frame's verified key-id sender equals the source the slot oracle expects in that slot from join order and the fill rule (MC does not name live sources);
        - layer 2: the frame verified and decrypted under that sender's roster key, by the layer-2 counters;
        - layer 3: the canvas pattern decodes to that sender's id, guard blocks pass, and the counter changes across samples;
        - plus each slot's rendered state, and story-2 tone detection for audio in the same cohort.
      - `publish-mixes.spec.ts` (R-2, R-3, R-4):
        - audio-only, video-only, both, and receive-only participants;
        - receive-only and audio-only publishers occupy video slots as `avatar_no_video`;
        - a publish-nothing-receive-nothing join is refused with the generic reason;
        - N=0 and M=0 receivers each work.
      - `video-over-subscription.spec.ts` (R-1): a declaration above MC's video cap is truncated, excess slots show `unavailable`, and the join succeeds.
      - `client-mute-video.spec.ts` (R-17, R-15):
        - client video mute ends the muted client's camera track, and every other receiver shows `avatar_client_muted`;
        - client audio mute keeps the mic track open, sends nothing, and leaves video flowing;
        - unmute resumes video.
      - `server-mute-video.spec.ts` (R-16, R-18, R-15):
        - the host server-mutes a participant's video; the muted client's camera track ends; every receiver shows `avatar_server_muted`; audio keeps flowing;
        - the host UI affordance per kind is exercised through the DOM;
        - server unmute resumes capture, and the participant sees the non-suppressible notice;
        - with the participant client-muted, unmute does NOT resume capture.
        - The patched-client ingress-drop half belongs to media-handler's Rust env-tests and is not duplicated here.
      - `late-joiner-video.spec.ts` (R-10, R-33): a participant joining a meeting with video already flowing sees it, with no keyframe interval configured anywhere. This proves on-demand keyframes end to end.
      - `kek-rotation-video.spec.ts` (R-14): a participant leaves, video keeps rendering at the remaining receivers across the KEK rotation, and no slot loses decodability for longer than the stop rule allows.
      - Story-2 audio specs green with M=0 (R-33): run the existing `multi-party-hear`, `partial-connectivity`, `server-mute`, `kek-rotation`, `over-subscription` and `solo-participant` specs with `VITE_DT_VIDEO_RECEIVE_SLOTS=0` and audio-only publishing. Use T-A's cohort for the renamed slot counts.
      - R-29 read-back: read a VIDEO client series (e.g. `dt_client_media_frames_received_total{media_kind="video"}`) back out of Prometheus, via the existing story-2 read-back helper and `clientMetricNames.ts`.
      - R-28: the browser layer fails loudly, never skips or degrades, when VP8 is unavailable. T-A's preflight localises it; here assert the SDK's media-start fault surfaces in the UI.
    - Two-handler visibility for video (R-33): the Rust half stays with the meeting-controller/media-handler task that owns it. Here, extend `partial-connectivity` to video slots: video from a sender sharing no handler never renders, and that peer appears as unreachable.
    - Diagnostics on failure: dump MC and MH counters (`mcMetrics.ts`, `instanceCounters.ts`) to separate MH budget rejection or pending slots from client faults.

    GATES AND ACCEPTANCE:
    - web-app, sdk-svelte and sdk-core lint, typecheck and unit tests green.
    - The bundle-content test proves `__DT_TEST_PATTERN__` and `__E2E_HOOKS__` absent from production.
    - Every scenario above passes on the Kind cluster through `scripts/layer7.sh`.
    - No `test.skip` or conditional skip anywhere: a missing capability fails loudly.
    - The env-test catalog in `packages/web-app/e2e/README.md` gains each new spec with its R-numbers.
    - Demonstrable at this task: N+1 browsers on `dev-web.sh` with the test pattern see each other's video in a paired grid.

    SECURITY ACCEPTANCE:
    - The R-18 notice cannot be suppressed.
    - The test pattern and the bus are absent from production builds.
    - The bus carries counts and booleans only: no GoP values, sizes or timings.
    - No insecure browser flag in any Playwright or dev config.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - All four mute states render distinctly. The host has a per-kind affordance. The R-18 notice cannot be suppressed by UI state.
    - Server-muted `reason` text is truncated and rendered as text, never markup.
    - The e2e bus whitelist carries no frame sizes, GoP values, key ids or per-stream timings. The bundle-content test proves the test pattern and patched-client harness are absent from production.
    - The env-test for the leaver lockout with video flowing is included or cross-referenced.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-17
- id: 18
  status: pending
  specialist: client
  deps:
  - 13
  - 14
  prompt: |-
    Story 3 "see each other", task 18 (C-C2): local-only debug options — the R-7 camera and microphone loopback self-check and the R-35 statistics overlay (docs/user-stories/2026-10-07-see-each-other.md; requirements R-7, R-35). Specialist: client. Pair with security as reviewer of the local-only guarantee; observability reviews the "never exported" test.

    DEPENDENCIES (deliberately narrow; this task is off the critical path and does NOT depend on task 17, C-C1):
    - Task 13 (C-A2, client) provides the camera capture (directed constraints and device reuse) and the send-side snapshot getters: per sent stream frames, keyframes by cause, and backlog depth.
    - Task 14 (C-B, client) provides the receive-side snapshot getters: per slot frames decoded and dropped by reason, jitter-buffer depth, and packet and byte arrival counts.
    - If a getter the overlay needs does not exist, add it to the SDK as a SNAPSHOT getter, sampled on a timer and never on the forward path, mirroring `AudioPipeline.frameCounts`. Never add per-frame callbacks or logging in `media/pipeline/`.

    WHAT TO BUILD

    (1) R-7 self-check, OFF by default.
    - A participant can check their own camera and microphone by LOCAL loopback: render the local camera track in a `<video>` element and play or meter the local microphone, straight from the local capture.
    - Never through the encoder, the packetiser, the SFrame stack or any handler, and nothing is sent.
    - Working before join is desirable but not required.
    - If it opens its own capture, it releases it when closed. If it reuses the session's capture, it never stops that track: the publish path owns the camera lifecycle.
    - It must not change the camera-light rule (camera live ⟺ directed AND not client-muted AND healthy) for the session's own publish.

    (2) R-35 statistics overlay, HIDDEN by default.
    - Per video and audio slot: packet rate, bit rate, jitter-buffer depth, decode count, drop counts by reason, and keyframes seen.
    - Per sent stream: packet rate, bit rate, keyframes produced by cause, and groups abandoned.
    - Rates are computed in the overlay from successive snapshots.
    - It is LOCAL to the client and NEVER exported: it never touches the MetricsSink, uses no `dt_client_*` names, and never writes to the e2e bus. Its per-slot bit rates are per-stream size series, which ADR §11 forbids exporting.

    (3) Mounting.
    - Both live in a self-contained `packages/web-app/src/views/DebugPanel.svelte`, behind one toggle mounted from `InMeeting.svelte` and (for the self-check) `JoinMeeting.svelte`.
    - Keep the mount to a single line in each view, so it rebases trivially against C-C1's concurrent edits to the same views.
    - Toggling is a runtime UI action, persisted only in `localStorage` as a per-viewer convenience, wrapped in try/catch.

    TESTS AND GATES:
    - A unit test asserts the overlay and self-check modules import no MetricsSink and no e2e-bus module, and contain no `dt_client_` string. Observability's "never exported" property is proven mechanically, not by review.
    - A unit test asserts the self-check performs no network or transport call: assert against the `IWebTransport` and `MediaTransport` seams via injection.
    - A component test covers default-hidden, toggling, and the rates computed from synthetic snapshot sequences on an injected clock.
    - web-app, sdk-svelte and sdk-core lint, typecheck and unit tests are green.
    - No env-tests: both features are local-only and never cluster-observable by design.

    SECURITY ACCEPTANCE:
    - Both features are off by default, local only, and send nothing.
    - Overlay data reaches neither telemetry nor the bus.
    - No new capture path bypasses the camera-light rule.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - The R-35 overlay and the R-7 loopback are local only. Both are off by default.
    - The overlay never touches the MetricsSink, exporter or e2e bus, with a unit test for that. Its per-slot rates are per-stream size series (ADR §11).
    - Loopback never passes through the encoder, packetiser or handler.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-18
- id: 19
  status: pending
  specialist: operations
  deps:
  - 4
  - 6
  - 8
  - 9
  - 10
  - 11
  - 12
  - 13
  - 14
  - 15
  prompt: |-
    Video incident scenarios, extended media scenarios, a `client-dev-local` ladder, and the 24-hour watch list (docs/user-stories/2026-10-07-see-each-other.md, R-31; operator surface for R-2, R-4, R-11, R-12, R-13, R-16, R-17, R-18, R-19, R-21, R-26; ADR-0036 §11 "Eight media runbook scenarios are required", which this task completes for keyframe storm and stream-credit stall). Task 19 (OPS-B), specialist operations. In anything you write into the repo, refer to other tasks by CODE only (for example "shipped by MH-B"), never by task number.

    Dependencies (scenarios describe shipped behaviour and cite shipped series, so every one must have landed):
    - task 4 (OPS-A): deploy order, the `#mixed-transport-rollback-hazard` subsection, the video-off lever, the directive wiring and the premise sweep. Do not duplicate them; link to them.
    - task 6 (P2): the v3 cutover (adds only `gop_counter`; 14-byte fixed prefix) and the `unknown_version` token.
    - task 8 (MH-A1): per-stream attribution (`unmatched_stream`, `transport_mismatch`), server mute and PAUSED (`server_muted`, `mh_media_paused_source_frames_total`), derived costs and recommended-min, MH config reads, the backlog-vs-memory startup check.
    - task 9 (MH-A2): stream-per-group cut-through forwarding, the edge state machine, per-destination abandonment and `mh_media_group_resets_total{reason}`, group starts, the stream-open and partial-frame drops.
    - task 10 (MH-B): once-per-counter keyframe requests, recovery remap and counter dedupe by outcome, pending visibility (`mh_media_edges_pending`, `mh_media_edge_pending_oldest_seconds`), and removal of the three MH keyframe/recovery keys (Amendment 4).
    - task 12 (MC-A): video slots and the flat slot-state migration, per-kind caps and truncation to UNAVAILABLE, send directives and withdrawals for unwatched publishers, join refusal tokens (neither-publish-nor-receive, a reconnect with a different publish set, missing header version 3). MC 8 and MC 18 cite these.
    - task 15 (MC-B): four mute controls, directive withdrawal on server mute, and superseded field deletion.
    - tasks 11 and 13 (C-A1, C-A2): the SDK video publish path, including keying any target with no group in progress, the keyframe-request refusal reasons, camera-failure presentation and the v2-directive refusal token.
    - task 14 (C-B): the SDK video receive path, its client drop tokens, and (Amendment 4) the unfulfilled-slot histogram `dt_client_media_slot_unfulfilled_duration_ms` and gauge `dt_client_media_slots_unfulfilled` (panel only).

    Task 20 (OBS-B) DEPENDS ON THIS TASK. It owns every alert rule (A1–A9) and needs each anchor below to resolve when its alert lands. So: you own NO alert rule. Do not add or edit `infra/docker/prometheus/rules/*`. Create every anchor below with EXACTLY these headings, so the slugs match what OBS-B was told.

    Pair with observability for the series names, thresholds and panel references, with media-handler and meeting-controller for token semantics, with client for the client-side tokens, and with security for the mute-authority and privacy wording. Before writing, confirm every token below against the tree, since the implementing tasks are authoritative. Where a name differs, use the tree's and note the difference in the devloop output. Scenario numbers verified free at planning: MH 20–23 (1–19 used) and MC 22–23 (1–21 used). `client-dev-local` continues from F21 (F16–F20 used). Re-verify, because another task may have taken one.

    NEW SCENARIOS

    1. `docs/runbooks/mh-incident-response.md` "### Scenario 20: Video Keyframe Storm" (anchor `#scenario-20-video-keyframe-storm`; alert A1 `MHVideoKeyframeStorm`, a warning on `mh_media_group_starts_total{direction="ingress"}` per active source stream above 0.5 / the client minimum keyframe interval for 10m. It uses the server-observed series because a patched client can neither inflate nor hide it).
       Context: there is no keyframe interval (R-9). Keyframes are event-driven (Amendment 4):
       - the publisher keys any directed target with no group in progress (start, target added, its own abandonment); one keyframe reaches every target (R-12). It never re-keys for a stream MH refused or stopped (STOP_SENDING): whoever decides an abandonment repairs it, so MH requests when it wants the stream again;
       - MH requests once per (source stream, counter), only for a seen source, never retrying (R-10);
       - receivers send one unpaced recovery request per loss event; MH remaps slot→source stream and forwards one per (source stream, counter), counting the rest by outcome (R-13);
       - the publisher honours a request only for a stream it sends that handler, counter at least its current one. Every keyframe is bounded by max(MC-directed `min_keyframe_interval_ms`, published as `mc_media_video_keyframe_min_interval_seconds`, compiled client floor), a too-soon one held as the single deferral, never dropped (R-10–R-12). So forwards are bounded by groups, and groups by that interval.
       Arms, forking FIRST on MH pod age:
       (a) a burst right after an MH roll or restart is expected in this story. Every subscriber reconnects and is held for a group start; resume and keyframe jitter after a mass reconnect is story 4's (ADR addendum). State this, do not tune it away;
       (b) a lossy-path recovery loop (R-12): a path below the fixed bitrate abandons groups and recovers repeatedly. This is accepted and bounded by the minimum interval. Visible as `mh_media_group_resets_total{reason="subscriber_backlog"}` rising alongside; a lossy uplink re-keys on its own abandonment, visible as keyframes produced by cause `abandonment`;
       (c) a recovery-request flood (R-13) cannot raise the keyframe rate, since one forward per counter serves every receiver. Read recovery-request outcomes: high `duplicate` is shared loss; sustained `counter_ahead` or `source_mismatch` is a client or contract fault. There is no rate-limit drop;
       (d) a patched publisher keying without cause: group starts above what forwarded requests and target changes explain. Read the client `keyframes produced by cause` series, `source` = `request`, `directive_start`, `abandonment` (the diagnosis panel OBS-B, task 20, adds; take the series name from the tree). A publisher-side bound is client-enforced, so a patched client is visible here and as egress pressure (MH 18).
       Resolution notes: no runtime lever except the R-26 video-off lever (cost per OPS-A). MH has no keyframe-request or recovery tuning key; do not offer one.

    2. "### Scenario 21: Slow Video Subscriber (Stream-Credit Stall)" (anchor `#scenario-21-slow-video-subscriber-stream-credit-stall`; alert A2, fleet-level only: `mh_media_group_resets_total{reason="subscriber_backlog"}` above 10% of egress group starts across the fleet for 15m. One bad path cannot reach 10% of the fleet, so firing means a systemic capacity problem and is the leading indicator for egress exhaustion).
       Context:
       - MH forwards video cut-through (R-21): header read with the maximum payload enforced against the length field, a per-subscriber header rewrite, payload passed as it arrives;
       - each subscriber stream has a capped backlog, `MH_EGRESS_STREAM_BACKLOG_BYTES`. Past it MH resets that destination's group stream only, and the subscriber returns to waiting for a group start (R-12).
       Rows:
       - `subscriber_backlog` resets (slow subscriber; per-path recovery is a panel, not a page, per R-12);
       - `mh_media_group_resets_total{reason="publisher_stall"}` (a publisher stalled mid-frame past `MH_STREAM_MID_FRAME_STALL_MS`, reset downstream);
       - the other reset reasons `publisher_reset`, `source_ended`, `edge_removed`, which are normal lifecycle;
       - drop reason `stream_rate_limited` (stream opens above `MH_STREAM_OPENS_PER_CONNECTION_PER_SEC`, refused at accept);
       - drop reason `partial_frame_discard`;
       - concurrency: NO event counter exists, because quinn flow control prevents the peer opening past the declared limit. Triage from the declared-limit config gauge beside the concurrent-ingress-groups gauge, and say so explicitly so nobody hunts for a counter;
       - memory: R-21 validates backlog cap × streams against `MH_POD_MEMORY_LIMIT_BYTES` at startup, so exhaustion is a counted reset, never an OOM kill. Point to the backlog-in-use vs cap panel, and to the existing `MHHighMemory` as the only memory alert.
       Cross-reference MH 18 and MH 20(b).

    3. "### Scenario 22: Video Slot Stuck Pending" (anchor `#scenario-22-video-slot-stuck-pending`; alert A3 `MHVideoEdgeStuckPending`, a warning when `mh_media_edge_pending_oldest_seconds` exceeds 10 × `mc_media_video_keyframe_min_interval_seconds` for 5m (no MH retry interval exists any more); count series `mh_media_edges_pending`).
       Context: "pending" (source assigned, no group start yet) is MH-internal. There is no client slot state for it and no give-up timeout while assigned (R-4). MH's gauge is the only server-side detector. The client keeps showing what the slot showed before (R-4/R-5), so users see a frozen avatar, not a spinner. Nothing retries (Amendment 4), so stuck pending is a MISSING-EVENT BUG (a keyframe or request someone owed), never a slow retry; the gauge and A3 make it loud.
       Also triage from `dt_client_media_slot_unfulfilled_duration_ms` and `dt_client_media_slots_unfulfilled` (an ACTIVE slot showing neither decodable video nor an avatar; no identity). Rising with MH pending is this scenario; with MH pending flat, MH believes it forwards, so the receive half is MC 23.
       Fork in this order:
       (a) is the publisher directed? Read MC directive outcomes and withdrawals (OPS-A's wiring). An undirected source cannot answer a keyframe request (R-11);
       (b) did the publisher key, or refuse? A directed target with no group in progress gets a keyframe within the minimum interval, unrequested (R-12); an unseen source correctly gets no MH request. Read keyframes produced by cause and the client refusal reasons (C-A1/C-A2); no refusal should leave a slot pending. Persisting "not sending to that handler": MC's directive and MH's edge disagree, or the start did not key. Persisting stale counter: the newer group is not reaching this handler (the publisher abandoned toward it with no `abandonment` keyframe, R-12, or fork e). Minimum interval is a deferral; pending past one interval means it was lost;
       (c) capture failure not presented as muted: R-19 requires a failed or revoked camera to present as video-muted. A pending source whose client reports camera failure is a client bug;
       (d) recovery-request outcomes `source_mismatch` (a request after a source change; normal in small numbers) and `counter_ahead` (a counter higher than MH has seen; a client or contract fault), per wire-freeze ruling 2. A slot receiving nothing from its new source is MH's to recover (R-13);
       (e) ingress refused: `stream_rate_limited`, `unmatched_stream` or `transport_mismatch` drops for that publisher's streams, and `unknown_version` (a v2 sender; see OPS-A's skew table). The publisher does not re-key a stream MH refused or stopped (STOP_SENDING); MH requests when it wants it again, so pending after a refusal that clears without an MH request is an MH missing-event bug;
       (f) a server-muted source showing as pending is an MC bug. It should be AVATAR_SERVER_MUTED with the directive withdrawn (R-16), so go to MC 21.
       Note that an MH→MC switch-completion report does not exist until story 5.

    4. "### Scenario 23: Client Sending While Client-Muted" (anchor `#scenario-23-client-sending-while-client-muted`; alert A4, a warning on `mh_media_paused_source_frames_total` above 1% of ingress for 15m).
       Frame it as a CLIENT PRIVACY DEFECT, not a mute-enforcement failure. Under client mute nothing of that kind may leave the device (R-17). MH never drops on client mute (R-17), which is why this is an arrival counter, not a drop. A short burst at unmute is expected, because a resume keyframe can beat MC's policy update (R-17).
       Forks: SDK version or stale bundle; capture path not stopping (video mute must stop the camera track; audio mute keeps the mic open and stops sending); a debug loopback (R-7) or test-pattern camera (R-27) wrongly wired into the send path.
       Escalate to client and security. There is no operator remedy beyond identifying the build, because no operator path to an individual's media exists.

    5. `docs/runbooks/mc-incident-response.md` "### Scenario 22: Client Video Decode Failures" (anchor `#scenario-22-client-video-decode-failures`; alert A6 `ClientVideoDecodeErrorRatio`, a warning above 1% for 10m; client-reported alerts anchor in the MC runbook by precedent).
       Forks:
       - before vs after decrypt: verify or decrypt rejects are key or identity problems, so go to MC 16;
       - VP8 decoder support or browser (R-8 says unsupported fails loudly at media start, so mid-call errors are not that);
       - one publisher vs fleet-wide (one publisher points at that sender's build or encoder; fleet-wide points at a new SDK rollout or a codec-profile change);
       - `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch` and `self_sender` client drop tokens (take semantics from C-B).

    6. `docs/runbooks/mc-incident-response.md` "### Scenario 23: Client Video Slot Unfulfilled" (anchor `#scenario-23-client-video-slot-unfulfilled`; alert A9 `ClientVideoSlotUnfulfilled`, a warning when the p95 of `dt_client_media_slot_unfulfilled_duration_ms` exceeds 10 × the minimum keyframe interval for 15m; `dt_client_media_slots_unfulfilled` is panel only). Client-reported, so corroborating and client-inflatable.
       Context: a slot MC reports ACTIVE shows neither decodable video nor an avatar (Amendment 4). Each episode should end within one minimum interval of a keyframe.
       Forks:
       - MH pending also high: the MH half, so go to MH 22;
       - MH pending flat (MH forwarding): decode drops go to MC 22, verify, decrypt or missing-key rejects to MC 16. A receiver sends one recovery request per loss event, taking sender and counter from the failing frame; a stream reset before any header sends nothing and is MH's to recover;
       - accepted residual: a directed camera whose capture never begins without reporting a mute or capture failure (for example a permission prompt left open) stays unfulfilled until A3 or A9 fires. The fix is the client's capture-state reporting (R-19); escalate to client;
       - one publisher vs fleet-wide, as in MC 22.

    EXTENDED SCENARIOS (keep each existing anchor stable)
    - MH 19 "Server Mute Not Taking Effect At Ingress": add a VIDEO arm.
      - Enforcement is per declared stream: SourceStream disposition DROP_AT_INGRESS, attributed by (connection-bound sender, uplink relay-region `stream_id` = the MC send id) looked up in `source_streams`, with no payload parse (wire-freeze Amendment 3). The uplink label is publisher-set and unauthenticated. A relabelled frame reaches only slots of the other stream's kind, and receivers take the decoder from the slot kind, which is why R-16 still holds. R-16 is stated in terms of slots: nothing from a sender whose video is server-muted reaches any subscriber's video slot, however labelled or transported, while audio keeps flowing.
      - Proof of enforcement is MH's own `server_muted` ingress drop counter rising.
      - The receive-side corroboration is `dt_client_media_frames_dropped_total{reason="sender_server_muted", media_kind="video"}` (alert A5, warning 10m). It is client-reported and therefore corroborating, not proof.
      - Add two sub-arms. "Camera light still on after server mute": MC withdraws that kind's directive (R-16), so a client still capturing means the withdrawal was lost or ignored, while MH's backstop still drops. "Video not resuming after server unmute": a server unmute restores capture unless the participant is client-muted (R-18), and the participant gets a non-suppressible notice. Check client mute state first.
    - MH 18 "Media Egress Budget Exhaustion": real video bytes now flow.
      - MH derives each stream cost from `media-codec-profile` bitrate plus code-derived per-frame overhead × frame rate. The ceiling uses the larger cost (MH stays type-blind), and `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` is now P×(N+M)=54.
      - Name the R-26 video-off lever as relief, with its MC-restart cost (link OPS-A).
      - Name A2's fleet backlog-reset alert as the leading indicator.
    - MC 21 "Server Mute Not Enforced": per kind. Four independent controls (R-15); a request about one kind never changes the other; host-only; requester taken from the connection; refusals counted and rate-bounded.
    - MC 18 "A Participant Hears Only Part of the Roster": rename the heading only if no `runbook_url` targets it (otherwise add a "sees or hears" line under it). Add:
      - per-kind caps (`MC_MAX_AUDIO_RECEIVE_SLOTS`, `MC_MAX_VIDEO_RECEIVE_SLOTS`);
      - truncation of over-cap declarations to UNAVAILABLE (R-1);
      - receive-only participants occupying video slots as AVATAR_NO_VIDEO (R-3);
      - the fill rule (pair-completion first, then earliest eligible joiner) and stability (no join, leave, mute or redeclaration moves an occupied slot);
      - cap 0 = video off;
      - unreachable still via `unreachable_sender_ids`.
    - MC 8 "Join Failures": refusal arms for neither publishing nor receiving (R-2, also on a later redeclaration), a reconnect with a different publish set (refused and counted, wire-freeze ruling 4), a declaration naming an unserved kind or unknown value (R-2), and an old SDK lacking header version 3 (OPS-A skew table). Cite MC's refusal tokens.
    - MC 16 "Missing Key Material": a per-kind note (A7 is re-derived per `media_kind`). Video-only missing key points at R-14's rotation at every group start and every T, with the key carried in-band on the first frames after a key change. Audio-only points at story 2's paths.

    `docs/runbooks/client-dev-local.md` LADDER, from F21:
    - camera permission denied;
    - "no video in a tab", forking on: VP8 unsupported (fails loudly at media start, R-8; the browser substrate is probed per R-28); test-pattern camera not built in (a build-time gate, absent from production, R-27); video cap 0 or a slot UNAVAILABLE; slot AVATAR_* reasons;
    - a v2 directive refused (a stale MC vs a v3 SDK);
    - a live slot showing neither video nor an avatar (unfulfilled; shown in the R-35 overlay): point at MC 23;
    - the R-7 local loopback self-check and the R-35 debug stats overlay as LOCAL triage tools, never exported to telemetry.
    Point F-entries at MC 22 rather than duplicating its forks.

    24-HOUR WATCH LIST
    Write it into the post-deploy checklists of `mh-deployment.md` (24-hour check) and `mc-deployment.md`, in the order things rise first:
    1. MC `transport_mode_mismatch` and MH `rejected_transport_inconsistent`, both expected at zero;
    2. the MH admission rejection ratio vs its threshold gauge;
    3. MH backlog-in-use vs cap, and MH memory vs `MH_POD_MEMORY_LIMIT_BYTES`;
    4. `mh_media_edges_pending` and `mh_media_edge_pending_oldest_seconds`;
    5. the fleet `subscriber_backlog` reset ratio;
    6. ingress group starts per source vs 1 / min interval;
    7. recovery-request outcomes (`duplicate`, `stale`, `counter_ahead`, `source_mismatch`);
    8. MH `server_muted` drops (rising after a mute is healthy; rising while clients still capture means the withdrawal is not honoured);
    9. the MC policy-push rate under mute churn (R-17 bounds it);
    10. client video decode drops;
    11. the `dt_client_media_slot_unfulfilled_duration_ms` p95 and the `dt_client_media_slots_unfulfilled` panel;
    12. that a client video series is present in Prometheus at all (R-29). A dead client pipe leaves every client alert green.

    GATES AND ACCEPTANCE
    - Every heading above exists with the exact text given, and its slug matches the anchor OBS-B uses. Grep `runbook_url` across `infra/docker/prometheus/rules/` and confirm no existing anchor broke. `dt-guard` alert/runbook resolution passes. Layer validation passes (`scripts/layer-all.sh`).
    - Every series and token cited exists in the tree, or the devloop output lists it as introduced by OBS-B with its owner. None is invented.
    - No identity (meeting, participant, sender or stream id), frame size, keyframe timing or GoP value appears in any example query or log filter (ADR-0036 §11; R-30).
    - No end-to-end, zero-trust or forward-secrecy claim.
    - `MH_KEYFRAME_REQUEST_RETRY_MS`, `MH_KEYFRAME_REQUEST_AGGREGATION_MS` and `MH_RECOVERY_REQUESTS_PER_CONNECTION_PER_SEC` appear nowhere in `docs/runbooks/`, least of all as levers (MH-B removes them and the MH `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` row; MC-A renames the MC row to `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, amendment 4). Remove any survivor and note it.
    - Each scenario carries Detection / Diagnosis / Resolution / Escalation in the runbook's existing shape.
    - Gate-1 tier: light (documentation). Review is required by observability (anchors and series), media-handler and meeting-controller (tokens), and security (MH 19, MH 23 and the mute wording).

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - The server-mute scenario uses the client `sender_server_muted` counter as the receiver-side signal of MH enforcement failure, alongside MH `server_muted`. It includes "camera light still on after server mute".
    - No runbook step asks an operator to log, dump or export per-stream sizes, timings or GoP values. Per-participant investigation uses MC's assignment state.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-19
  tier: light
  tier_reason: docs-only runbook scenarios; no code, contract or security surface
- id: 20
  status: pending
  specialist: observability
  deps:
  - 2
  - 4
  - 8
  - 9
  - 10
  - 11
  - 12
  - 13
  - 14
  - 15
  - 19
  prompt: |-
    Story 3 "see each other", task OBS-B: video dashboards, alerts A1–A9, the send-directive alert, catalogs, inventories and SLO open objectives (docs/user-stories/2026-10-07-see-each-other.md, requirements R-30 and R-31's alert half, plus R-29's alert re-derivation; governing design docs/decisions/adr-0036-media-flow.md §11 and §"A control's coverage must be demonstrated, not asserted"; alert authorship per ADR-0031 §Ownership split). Implementing specialist: observability. Pair with operations as reviewer of every `runbook_url` and annotation (operations owns the runbooks, observability owns every rule — exactly one owner per alert, and operations owns none of these rules), with media-handler, meeting-controller and client as reviewers of the panels and selectors over their series, and with security as reviewer of A5 and of the identity-freedom of every new panel.

    DEPENDENCIES (task id, code, what it provides). Task 2 (OBS-A) — the `media_kind` split: client per-kind handles, MC kind labels, the collector/GC/guard hops. Task 4 (OPS-A) — directive wiring: the rewritten `docs/runbooks/mc-incident-response.md` send-directive section (around the old line 2007 query) that reads withdrawals as healthy, and its anchor, which the directive rule below links to. Task 8 (MH-A1) — per-stream attribution drops (`unmatched_stream`, `transport_mismatch`), per-stream server mute and PAUSED (`server_muted` drops, `mh_media_paused_source_frames_total`), `rejected_transport_inconsistent`, and the once-per-ingress-frame `mh_media_frames_forwarded_total{direction="ingress"}` identity on the stream path. Task 9 (MH-A2) — stream-per-group cut-through forwarding series: group starts and resets, stream drops (`stream_rate_limited`, `partial_frame_discard`, the rewrite entry-point split), backlog cap and in-use, concurrent ingress groups and the declared limit. Task 10 (MH-B) — keyframe requests sent, recovery requests received (counter-deduped), edge pending (count, oldest age, duration, exits) and the stream-sources-active gauge. Task 12 (MC-A) — video slots, flat slot-state migration, withdrawals counter, per-kind slot cap, `mc_media_video_keyframe_min_interval_seconds`, widened `transport_mode_mismatch`. Tasks 11 (C-A1) and 13 (C-A2) — SDK video publish series (keyframes produced by `source`: `request`, `directive_start` (start, resume, target added), `abandonment` (re-key after its own abandonment), keyframe requests received, send groups abandoned, encoder errors, capture failures, min-interval clamps, v2-directive rejects) and the build-time keyframe-floor equality guard. Task 14 (C-B) — SDK video receive series, the new receive drop reasons, render-latency histograms, the unfulfilled-slot histogram `dt_client_media_slot_unfulfilled_duration_ms` and gauge `dt_client_media_slots_unfulfilled`, and the `media_kind="video"` Prometheus read-back. Task 15 (MC-B) — four mute controls and per-kind mute semantics; removal of `source_muted`. Task 19 (OPS-B) — the video incident scenarios whose anchors every alert below links to. Read each emitting task's catalog rows (`docs/observability/metrics/mc-service.md`, `mh-service.md`, `client.md`) for the exact landed names before writing a selector; where a landed name differs from this prompt, the catalog wins and you record the difference. A selector naming a series that does not exist yields an empty result rather than an error, so every selector below must be checked against a real emitted series (`crates/env-tests/tests/33_alert_rules_loaded.rs` and the `application-metrics` guard's check 4). In anything that lands in the repo (rule comments, docs, panel descriptions), refer to tasks by code only, never by number.

    WHAT THE EMITTING TASKS ALREADY LANDED, AND WHAT IS YOURS. The `application-metrics` guard (`crates/dt-guard/src/application_metrics.rs`) fails validation on any `mc_*`/`mh_*` metric that is on no dashboard (check 5) or in no catalog (check 6), so MH-A1, MH-A2, MH-B, MC-A and MC-B each landed a basic panel and a catalog row with every series they added, and every client task landed its catalog row and collector allowlist entry (`client-metrics-export` G1/G3). This task owns: the layout and video rows of the three media dashboards, every alert-linked panel (with the alert's threshold drawn as a line, per `docs/observability/dashboard-conventions.md`), the alert rules, the alert and dashboard inventories (`docs/observability/alerts.md`, `dashboards.md`), the SLO open objectives, and the ADR-0031 structured blocks. Do not duplicate a panel an emitting task already landed; move, regroup and annotate it.

    ALERT DESIGN RULES (apply to every rule). All are `severity: warning` — none pages: A5, A6 and A9 are client-reported and therefore client-inflatable, and R-12 explicitly accepts repeated recovery on paths below the fixed bitrate in this story. Thresholds are DERIVED from server-side config gauges wherever one exists, never literals copied from config (CLAUDE.md single source of truth; ADR §11 "derive rather than guard"); write the provenance in the rule's comment block. Ratios carry the non-zero-denominator guard (`docs/observability/alert-conventions.md` §Non-Zero-Denominator Guard). `for:` stays below any `increase()` window. Every rule carries the full ADR-0031 block in `infra/docker/prometheus/rules/*.yaml`, including the two-half table: does it FIRE (inject the adverse condition) and does it APPLY (the selector matches a real series on a real pod in the Kind deployment). No rule groups or matches on any identity label; client rules never group by `instance` (`client-metrics-export` Rule 2). Every `runbook_url` must resolve to a file and anchor (alert validation enforces it; OPS-B wrote these anchors).

    THE ALERTS.
    - A1 `MHVideoKeyframeStorm` (`mh-alerts.yaml`). Fleet ingress group starts per active stream source exceed half the directed minimum keyframe rate: `sum(rate(mh_media_group_starts_total{direction="ingress"}[10m])) / sum(mh_media_stream_sources_active) > 0.5 / max(mc_media_video_keyframe_min_interval_seconds)`, guarded by `sum(mh_media_stream_sources_active) > 0`, `for: 10m`. Provenance: the publisher never keys more often than the directed minimum interval (R-11; the client enforces max(directed, compiled 100 ms floor), and a build-time guard landed in C-A1/C-A2 keeps the lower bound of MC's `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` equal to that floor), so a fleet average near half that ceiling means publishers are being driven to their cap — a request storm, or a self-key loop (the publisher keys whenever a target has no group in progress, including after abandoning it, so a sub-bitrate uplink re-keys at the interval; R-12 accepts one such path, not half the fleet). The interval comes from MC's server-side gauge, never a client-reported one: client gauges have no per-browser identity and collapse to last-writer-wins at the collector, which would let a patched client suppress this alert. A burst right after an MH roll is expected (resume/keyframe jitter after a mass reconnect is story 4's); the annotation says check pod age first. Runbook: `docs/runbooks/mh-incident-response.md#scenario-20-video-keyframe-storm` (it cites the client's `dt_client_media_keyframes_produced_total` by `source`, `request` against `abandonment`, as a diagnosis panel — put that panel on client-media).
    - A2 `MHVideoSubscriberBacklogResets`. Fleet-level only: `sum(rate(mh_media_group_resets_total{reason="subscriber_backlog"}[15m])) / sum(rate(mh_media_group_starts_total{direction="egress"}[15m])) > 0.10`, guarded, `for: 15m`. One slow path cannot reach 10% of the fleet's egress groups; firing means a systemic capacity problem and is the leading indicator for egress exhaustion. Per-path recovery stays a panel (R-12 accepts it). Runbook: `mh-incident-response.md#scenario-21-slow-video-subscriber-stream-credit-stall`.
    - A3 `MHVideoEdgeStuckPending`. `max(mh_media_edge_pending_oldest_seconds) > 10 * max(mc_media_video_keyframe_min_interval_seconds)`, `for: 5m`. Pending (assigned, no group start yet; MH-internal per R-4, with no give-up timeout while assigned) has no other detector; it covers new edges, newly connected subscribers and post-abandonment resyncs, not only switches. With no retry (amendment 4), it is what makes a lost event loud. Provenance: each exit needs one group start, which the publisher produces within one minimum interval (it keys any target with no group in progress, and holds an early request as the single deferral); ten intervals (5 s at the default) is that bound plus transit margin, still short of a tile visibly frozen. A directed capture that never begins and is not reported as a mute or capture failure (an open permission prompt) stays pending until A3 or A9 fires: an accepted residual, loud not masked, owned by client capture-state reporting; never widen the threshold for it. A sub-bitrate uplink re-enters pending (R-12) — hence warning. Runbook: `mh-incident-response.md#scenario-22-video-slot-stuck-pending`.
    - A4 `MHPausedSourceIngress`. Frames arriving from a source MC's policy marks paused (client-muted), as a fraction of ingress: `sum(rate(mh_media_paused_source_frames_total[15m])) / sum(rate(mh_media_frames_forwarded_total{direction="ingress"}[15m])) > 0.01`, guarded, `for: 15m`. These frames are counted, NOT dropped (R-17: client mute never makes MH drop ingress, so a resume keyframe that beats MC's update still reaches subscribers); the annotation must state that the 1%/15m tolerates that legitimate race. Sustained, it means media is leaving a device the user muted — a client privacy defect. The denominator is the EXISTING `mh_media_frames_forwarded_total{direction="ingress"}`, incremented once per ingress frame (never per edge or per chunk — MH-A1 keeps that identity on the stream path, counting at header completion); `forwarded{ingress}` plus the ingress-direction drop reasons is every frame received. Do not add a second ingress counter. Runbook: `mh-incident-response.md#scenario-23-client-sending-while-client-muted`.
    - A5 `ClientVideoServerMutedSenderFrames` (`client-alerts.yaml`). `sum(rate(dt_client_media_frames_dropped_total{reason="sender_server_muted", media_kind="video"}[5m])) > 0`, `for: 10m`. Counts only the server-muted reason: decode-before-live in client-muted and no-video avatar slots is legitimate (R-5) and must never feed this. A transient at mute onset (in-flight trailing frames) is expected and the 10m covers it. It is corroborating evidence of a video server-mute leak (R-16), not proof; the proof is MH's own `mh_media_frames_dropped_total{reason="server_muted"}` — say so in the annotation. Runbook: `mh-incident-response.md#scenario-19-server-mute-not-taking-effect-at-ingress` (video arm).
    - A6 `ClientVideoDecodeErrorRatio` (`client-alerts.yaml`). `sum(rate(dt_client_media_decoder_errors_total{media_kind="video"}[10m])) / sum(rate(dt_client_media_frames_accepted_total{media_kind="video"}[10m])) > 0.01`, guarded, `for: 10m`. Runbook: `docs/runbooks/mc-incident-response.md#scenario-22-client-video-decode-failures` (client-reported alerts anchor in the MC runbook, as `ClientKekRetentionViolation` and `MCMediaMissingKeyMaterial` do).
    - A7 `MCMediaMissingKeyMaterial` (existing, `mc-alerts.yaml`) — re-derive per kind: numerator and denominator become `sum by (media_kind)`, so a video-only key failure is not diluted by audio's higher frame rate. Re-derive the threshold for video against this story's key rules (R-14: video transmit keys rotate every T, at every group start and on every KEK receipt; the new key rides the first N frames after any key change; key-bearing frames are never discardable) and against W, and record the provenance beside the existing one; keep the anchor `#scenario-16-missing-key-material` (OPS-B adds the per-kind note). Update `packages/web-app/e2e/clientMetricNames.ts` and its drift test `packages/web-app/tests/clientMetricNames.test.ts` if the selected-name set or label matching changes.
    - A8 `MHTransportInconsistent` (`mh-alerts.yaml`). Either arm: `sum(rate(mh_media_policy_applies_total{outcome="rejected_transport_inconsistent"}[5m])) > 0` or `sum(rate(mh_media_frames_dropped_total{reason="transport_mismatch"}[5m])) > 0`, `for: 15m`. `rejected_transport_inconsistent` fires when an egress's candidates declare different transport modes or a candidate names an undeclared source stream (mixed meetings are legal; `EgressStream.transport_mode` was removed — an edge's transport derives from its candidate's declared stream); `transport_mismatch` is a declared stream arriving on the wrong transport. Warning with 15m because the annotation's scope is skew within the v3 era plus a mis-ordered cutover (rollback across the v2→v3 break is all three components together). MC's own per-stream echo mismatch (`mc_media_policy_pushes_total{outcome="transport_mode_mismatch"}`, its meaning widened to: a sent stream absent from `applied_source_streams`, echoed UNSPECIFIED, or echoed with a different mode) is NOT duplicated here: it already PAGES through `MCMediaGenerationDivergence` (negated selector `outcome!~"match|handler_id_mismatch"`), which operations confirmed is correct as a page. Refresh that rule's description for the per-stream meaning and the mis-ordered-cutover case, and say MC's confirm-failure log line carries the arm (`absent_from_echo`, `echoed_unspecified`, `mode_differs`) and stream number. Runbook: `docs/runbooks/mh-deployment.md#mixed-transport-rollback-hazard` (cross-linked by OPS-B with MC scenario 15's `transport_mode_mismatch` row).
    - A9 `ClientVideoSlotUnfulfilled` (`client-alerts.yaml`). C-B's histogram `dt_client_media_slot_unfulfilled_duration_ms` (milliseconds, so the seconds gauge is scaled): `histogram_quantile(0.95, sum by (le) (rate(dt_client_media_slot_unfulfilled_duration_ms_bucket[15m]))) > 10 * 1000 * max(mc_media_video_keyframe_min_interval_seconds)`, guarded by `sum(rate(dt_client_media_slot_unfulfilled_duration_ms_count[15m])) > 0`, `for: 15m`. Provenance: an episode ends with one keyframe produced within one minimum interval, so the bound is A3's; A9 sees the receiver's half (MH forwards, nothing renders), which A3 cannot. p95, because capture start, not the interval, bounds the tail. C-B's buckets extend past 100 s (10× the key's 10000 ms maximum; `histogram_quantile` caps at the top finite bucket) and every exit is observed, including teardown; verify both and raise any gap. The gauge `dt_client_media_slots_unfulfilled` is panel-only: client gauges collapse to last-writer-wins (A1). Runbook: the A9 scenario OPS-B writes in `mc-incident-response.md` (it points to MH scenario 22 for the MH half); take its slug from the tree.
    - Directive rule — closes `docs/TODO.md` "`mc_media_send_directives_total` has no alert rule" (filed 2026-09-04; trigger fired 2026-09-08). Classify EVERY `DirectiveOutcome` token in the ADR-0031 block partition, so a token added later is visibly unclassified: normal — `emitted`, `emitted_empty_targets`; MC defect — `unknown_stream_number`, `transport_mode_unspecified`, `handler_url_unresolved`; environmental — `meeting_state_unavailable`, `assignment_failed`; plus any token MC-A/MC-B added (read the enum). Withdrawals are healthy and are read from `mc_media_send_directive_withdrawals_total{media_kind, reason=unwatched|server_muted}`, never inferred from the outcome counter. The rule fires on any non-normal outcome via a NEGATED selector (so an unclassified new token fires rather than falling silently outside), severity warning unless you show from the token meanings that a defect token silences a connection for its whole life, in which case argue page in the block and get operations' agreement. Runbook: the send-directive section OPS-A rewrote in `mc-incident-response.md`. Close the TODO entry in the same change, pointing at the rule.

    NO ALERT, BY DECISION (record each in `alerts.md` with the reason): stream-path drops per path (R-12 accepts recovery); MH recovery `duplicate` (many receivers losing one group is normal; dedupe is the bound working, and junk is bounded by QUIC flow control); `honoured_counter_ahead` and MH `counter_ahead` (diagnostic; client-reported on one side); `dt_client_media_keyframe_min_interval_clamped_total` (a build-time equality guard in C-A1/C-A2 makes it non-zero only across a rollout skew); the version-skew signals (R-25 is confirmed: rollback across the v3 cutover is all components together) (MH `unknown_version`, MC's refusal of an SDK lacking header version 3, the client's v2-directive reject series) — expected during a rollout, so panels only, listed for operations' rollout scenario; capture failures (user environment); MH memory (R-21's startup validation of backlog cap × streams against the pod limit makes backlog OOM structurally impossible; the existing `MHHighMemory` suffices — add a backlog-in-use beside backlog-cap panel).

    DASHBOARDS. `infra/grafana/dashboards/mc-media.json`: per-kind send targets and withdrawals by reason; slot states by `media_kind` with the flat tokens (`active`, `avatar_no_video`, `avatar_client_muted`, `avatar_server_muted`, `fewer_sources_than_slots`, `unavailable`; `switch_pending`, `withheld_by_congestion`, `source_unreachable` are retained-but-unemitted and stay in the domain; `source_muted` was removed by MC-B and must appear in no query; `zero_requested` is reserved), with unreachable read from `mc_media_unreachable_senders_total`, NOT from the slot-state counter; client vs server mute by kind as separate panels — never one panel summing the two (`label-taxonomy.md` `action` contract); per-kind slot cap (video at 0 = R-26 lever applied); `mc_media_video_keyframe_min_interval_seconds`; join refusals including neither-publish-nor-receive and reconnect publish-set mismatch. `mh-media.json`: a video stream-path row — group starts by direction; group resets by reason (`subscriber_backlog`, `publisher_reset`, `publisher_stall`, `source_ended`, `edge_removed`); `mh_media_edges_pending`, `mh_media_edge_pending_oldest_seconds` (A3 threshold line), the `mh_media_edge_pending_duration_seconds` histogram and pending exits by outcome; keyframe requests sent by trigger (`initial`, `receiver_recovery`); recovery requests received by outcome (`forwarded`, `duplicate`, `stale`, `counter_ahead`, `unmapped_slot`, `malformed`, `source_mismatch`); paused-source arrivals with A4's ratio; stream drops (`unmatched_stream`, `transport_mismatch`, `stream_rate_limited`, `partial_frame_discard`, the `rewrite_relay_region` entry-point split); concurrent ingress groups against the declared limit; backlog cap and in-use; the config gauges (stall bound, stream sources active). Every MH panel is type-blind: no panel title or legend may claim a series is "video" from an MH label; title by behaviour (stream transport) and note in the description that MC's assignment is what makes stream-transport traffic video. `client-media.json`: split existing panels by `media_kind`; a video row — keyframes produced by `source` (`directive_start`, `request`, `abandonment`), keyframe requests received by outcome (`honoured`, `honoured_counter_ahead`, `stale_counter`, `min_interval`, `not_sending`, `malformed`), recovery requests sent by reason, send groups abandoned, encoder errors, capture failures by kind and reason (`permission_denied`, `no_device`, `device_unavailable`, `ended`), the new receive drop reasons (`self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch`; an oversized stream payload reuses the existing `payload_length_exceeds_max` token), the two render-latency histograms (media start → first rendered video frame; slot turns active → first rendered frame from its source), distinct active video sources, `dt_client_media_slot_unfulfilled_duration_ms` (A9 line) and `dt_client_media_slots_unfulfilled`, min-interval clamps, and the v2-directive reject series. Counters render as rates or increases per ADR-0029; every target sets `editorMode` and `range`/`instant` (guard check 7). Update `docs/observability/dashboards.md`.

    SLO. In `docs/observability/slos.md`, add two OPEN objectives beside "Join-to-first-media: an objective, never a gate": join-to-first-rendered-video and slot-active-to-first-render, naming their histograms, their measurement points, and that no test gates on a wall-clock number (R-30, ADR §10). State that the target is open and what evidence would close it (Kind and real-network measurements; story 8 performance). Add the MH pending-exit duration histogram as the server-side decomposition of the second.

    CATALOGS AND INVENTORIES. Verify (do not re-author) each emitting task's catalog rows exist and match what is emitted; add the alert cross-references to each row an alert reads; update `docs/observability/alerts.md` with A1–A9, the directive rule, the refreshed `MCMediaGenerationDivergence`, and the no-alert decisions above.

    HYGIENE (R-30, ADR §11). Confirm, and record evidence for, the applies-half: MH's stream forward and re-framing code lives under `crates/mh-service/src/media/` (the `media-telemetry-deny` scope, `scripts/guards/simple/media-telemetry-deny.yaml`) and the SDK's video encode/decode/receive loops under `packages/sdk-core/src/media/pipeline/`; `crates/env-tests/tests/32_media_metric_hygiene.rs` covers every new series (no identity label, `key_custody=operator`, no `media_kind` on any `mh_*` series); no frame sizes, keyframe timings, GoP values or sender/stream identity appear in any label, log, span or the test bus. If any fails, it is a defect in the emitting task — raise it, do not paper over it in a panel.

    TESTS AND GATES. `crates/env-tests/tests/33_alert_rules_loaded.rs` asserts every new rule is loaded by the deployed Prometheus with the committed expression; extend its list. For each alert, demonstrate FIRE where it can be induced on Kind without a wall-clock gate (A3 by a publisher fixture that sends continuation frames but never a group start; A9 by an authenticated OTLP histogram post on the `31_gc_telemetry.rs` path; A8 by a registration fixture with mixed candidate modes, A5 by the patched-client server-mute scenario the MH/MC env-tests already drive) and APPLY for all (the selector returns a series from a real pod). No test gates on a wall-clock latency number. `dt-guard alert-rules`, `application-metrics`, `client-metrics-export` (Rule 2: every label an alert matches survives the client pipeline — `media_kind` does since OBS-A) and `layer-all.sh` green.

    ACCEPTANCE. A1–A9 and the directive rule exist with derived thresholds, ADR-0031 blocks and resolving runbook anchors; `MCMediaGenerationDivergence` description refreshed; the directive TODO closed; three dashboards carry the video rows with threshold lines; inventories, catalogs cross-referenced and SLO open objectives written; hygiene applies-evidence recorded; env-test 33 extended; all guards green.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - Every series carries `key_custody=operator` and no meeting, sender, stream, slot or participant identity. Catalogued tokens:
      - client: `self_sender`, `sender_server_muted`, `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch`;
      - MH: `unmatched_stream`, `transport_mismatch`, `source_mismatch`, `stale`, `counter_ahead`, `duplicate`.
    - Alerts pass `validate-alert-rules.sh` (no URL exfiltration, annotation hygiene). A sustained `sender_server_muted` rate pages toward the server-mute runbook.

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-20
- id: 21
  status: pending
  specialist: test
  deps:
  - 16
  - 17
  - 18
  - 19
  - 20
  prompt: |-
    Capacity evidence and the see-each-other manual test plan, written AND run, for story 3 (docs/user-stories/2026-10-07-see-each-other.md; ADR-0036 `docs/decisions/adr-0036-media-flow.md`). Requirements: R-28 (cohort size, CPU load and Kind capacity for the video cohort verified as evidence, not assumed) and R-34 (a manual test plan for the demo, with real camera and test pattern, all four mute controls and publish mixes, written and run before close). It also gives a human eye to R-5, R-7, R-15 through R-19, R-26, R-27 and R-35, which automation reads only through wire tokens and the dev-only bus.

    This is the story's last task. It runs against the fully landed story. Dependencies, and what each provides:
    - Task 16 (INF-C, infrastructure): the Kind `media-codec-profile` patch to 320×180 / 15 fps / 250000 bps, the Kind egress ceiling re-sized to about 120, and the dev-web.sh multi-participant video demo docs.
    - Task 17 (C-C1, client): the demonstrable grid UI (video tiles, avatars by reason, the host per-kind mute affordances, the server-unmute notice) and its browser specs, which consume task 7's helpers.
    - Task 18 (C-C2, client): the debug options, the R-7 self camera/mic loopback and the R-35 stats overlay, which phase 9 exercises.
    - Task 19 (OPS-B, operations): the video incident scenarios, the extended server-mute / egress / rollout scenarios, the `client-dev-local` ladder entries for camera permission and no-video, and the 24h watch list.
    - Task 20 (OBS-B, observability): the video dashboards (including the client unfulfilled-slot panel and MH's pending panels, amendment 4), alerts A1–A8, catalogs and open-objective SLO panels.
    Transitively it relies on every implementing task (tasks 1–15).

    Pair with:
    - operations, on the triage cross-references and the plan's runbook pointers;
    - observability, on which dashboard panels the plan walks (titles verified in `infra/grafana/dashboards/`);
    - client, on UI affordance names;
    - security, on the wording of the server-unmute and host-resumes-capture notice.

    Part 1: capacity evidence (R-28). This is a recorded run, not a gate.
    - Run the browser video cohort shapes in a Layer-7 run on the devloop host, at the Kind test profile read back from the deployed `media-codec-profile` ConfigMap (never restated):
      - see-all: P=4, N=3, M=3, 24 edges;
      - over-subscription: P=4, N=2, M=2, 16 edges.
    - Record per browsing context: CPU (process-level, sampled over the scenario window), decoded and rendered video frames per slot, and whether any slot lost liveness under the plan-7 detector.
    - Record recovery activity over the scenario window, per publisher and per receiver: send groups abandoned, keyframes produced by cause, MH group resets by reason, MH's pending count, and the client's currently-unfulfilled video-slot gauge (amendment 4) at the end of the window. Under amendment 4 there is no MH retry or aggregation traffic; recovery load is self-keyed and requested keyframes, bounded by the minimum keyframe interval. Series names are read from `docs/observability/metrics/`, not recalled.
    - Record from MH: installed edges against `mh_media_egress_stream_ceiling`, plus headroom.
    - Record the Rust side: the largest per-file concurrent edge sum, read from each video env-test file's header. `27_mc_slot_placement` ≤72 by formula, about 60 emitted; the others are ≤46. Compare it with the deployed ceiling.
    - Record the same CPU figures for the CI Chromium job's component-tier VP8 probe run.
    - Write the evidence into a "Capacity evidence" section of the manual test plan document below, with the run date, host description and commit.
    - If the evidence shows the cohort does not fit (CPU saturation causing liveness loss, a slot still unfulfilled at the end of the window, or edges within 10% of the ceiling), that is a loud story-blocking finding, reported to the lead with the numbers. It is never fixed by quietly shrinking the cohort or re-tuning the profile in place, because the profile and ceiling are infrastructure's configuration with their own owner.

    Part 2: the manual test plan document, `docs/user-stories/2026-10-07-see-each-other-manual-test-plan.md`. Follow the structure and voice of `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md`:
    - a header with story link, status and purpose;
    - "What the demo proves — and what it does not";
    - a cold-start phase;
    - numbered phases, where every step names its automated equivalent (spec or env-test file and the assertion), so a human-only failure can be told apart from a regression the suite should have caught;
    - dashboards to read;
    - triage on failure, pointing at OPS-B's scenarios and `client-dev-local` entries by number;
    - promote-to-E2E verdicts;
    - a run record.

    State plainly what the demo cannot prove. Every participant runs the identical TypeScript SFrame stack and MH is keyless, so a self-consistent crypto error is invisible here. The cross-language vectors (task 6, P2) and the external SFrame-WG anchor remain the only independent crypto check.

    Phases, at minimum:
    - (0) Cold start via dev-web.sh: a multi-participant video demo with distinct accounts, one per browsing context, separate profiles recommended. Use the test-pattern camera (build-time gated) to avoid N cameras.
    - (1) See and hear by content. Each participant's tile shows the expected participant's pattern and tone.
      - The pattern changes every frame, so a frozen tile is visible.
      - Slot fill follows join order and the pair-completion rule. The plan states the expected grid per participant, derived from R-3.
    - (2) Publish mixes: one participant audio-only, one video-only, one both, one receive-only.
      - Receive-only and audio-only participants show as avatar "no video" in video slots.
      - Audio-only participants hold no video publisher precedence.
      - Receive-only participants take no audio slot.
      - A participant that would neither publish nor receive is refused at join with a generic message.
    - (3) Over-subscription and truncation: declare more video slots than the cap. Excess slots render as unavailable and never as a spinner. Fewer-sources renders as an under-filled grid.
    - (4) Four mute controls, each alone and composed:
      - client audio mute: the mic stays open and nothing is sent;
      - client video mute: the camera light goes off and peers see avatar client-muted;
      - server audio mute and server video mute by the host, where the camera light goes off and peers see avatar server-muted;
      - server unmute while client-muted: capture stays off;
      - server unmute while not client-muted: capture resumes AND the participant sees the non-suppressible notice and can client-mute at once.
      A non-host's mute attempt is refused. A request about one kind never changes the other.
    - (5) Watched-only directives: when no one holds a participant in a video slot, that participant's camera light is off, and it comes on when a watcher joins.
    - (6) Leave and rotation with video flowing: no visible stall on remaining tiles. A late joiner sees everyone, and their first frame is a clean picture, never a smeared mid-group frame.
    - (7) Camera revoked mid-call (browser permission revoked, or device unplugged): peers see video-muted, never a perpetually pending tile.
    - (8) Video cap 0 (the operator lever, R-26), via the documented ConfigMap change and MC pickup: every video slot is unavailable, no camera light, and audio unaffected. Restore afterwards.
    - (9) Debug options: the self camera/mic loopback (R-7) renders locally with no handler involvement, and the stats overlay (R-35) is hidden by default and shows per-slot and per-stream rates, buffer depth, decode and drop counts and keyframes. Confirm in the collector/Prometheus that no overlay value is exported.
    - (10) Production profile pass. There is no dev-web.sh override for the codec profile. To run production quality, edit or remove the Kind `media-codec-profile` patch so the base profile (production defaults, 640×360 / 30 fps / 800000 bps) applies, then redeploy. The plan names the patch file and the redeploy command, verified against the tree. Run phases 1 and 4 with REAL cameras on two machines or two cameras, with headphones. Restore the patch and redeploy afterwards.
    - (11) Story-2 audio regression: with M=0 the hear-each-other demo behaves as in story 2's plan, phases 1–4.
    - (12) Recovery after an induced publisher→MH abandonment (R-12, R-30, amendment 4). Hold one publisher's uplink below the profile's fixed bitrate until it abandons its group toward its handler, then lift the constraint.
      - The plan names the induction method, verified against the tree and needing no insecure browser flag. Examples: host-level traffic shaping on the publisher's machine, or a dev build with a lowered per-target backlog cap (task 13 (C-A2)'s client knobs).
      - Observe that the publisher's send-groups-abandoned counter rises.
      - Observe that each peer's tile for that publisher resumes with a clean picture, never a smeared mid-group frame, with no reload or rejoin. The publisher's keyframes-by-cause series shows the self-keyed group start.
      - Observe that MH's pending count, and each receiver's currently-unfulfilled video-slot gauge (dashboard panel and R-35 overlay), return to zero.
      - Recovery is observed as state returning to zero, never timed. Repeated abandonment while the constraint holds is accepted (R-12); the pass condition applies after it is lifted.
      - Automated equivalents: task 13 (C-A2)'s abandonment test and task 10 (MH-B)'s keyframe env-test, by name from the tree. A failure triages through OPS-B's Video Slot Stuck Pending scenario, by number from the tree.
      - If the tree offers no human-reachable induction, the step says so, names its automated equivalent, and the gap goes to the lead. It is never dropped silently.

    Running the plan:
    - Run the automated equivalents of every phase in this task's own Layer-7 run, and record them in the run record with spec names and outcomes.
    - The human pass on a real browser (Windows Chrome over mirrored networking, as story 2) is run by the user before /close-story. Mark it "NOT RUN — pending before /close-story" until the user records it. Never mark it run on the basis of automation.
    - Record every step that fails, with the triage pointer used. A failure is fixed in its owning task's area or raised to the lead, never papered over in the plan text.

    Acceptance criteria:
    - The capacity evidence section exists with real numbers from a run on this commit.
    - The manual plan exists with every phase above, and each step names its automated equivalent.
    - The automated run record is filled in.
    - Dashboard titles and runbook scenario numbers are verified against the tree, not recalled.
    - No step uses an insecure browser flag.
    - The test-pattern camera and test tone appear only in dev builds.
    - No step asks a human to judge a wall-clock latency as pass/fail. Join-to-first-video and assignment-to-first-render are observed on the open-objective panels, never gated.

    SECURITY ACCEPTANCE CRITERIA (from the security specialist's per-task list; security reviews this task against them at Gate 3; where they add to the criteria above, they apply):
    - The manual plan includes:
      - a patched or non-cooperating-client attempt under server video mute (or a reference to the MH-A1 env-test);
      - host unmute resuming the camera with the notice visible;
      - the camera light going off on withdrawal and on mute;
      - a leaver losing video after rotation.
    - Evidence artifacts (traces, HARs) are not promoted to the repo, since they contain live tokens (`packages/web-app/e2e/README.md`).

    WIRE FREEZE: where this prompt and the story file's Design §Wire freeze (docs/user-stories/2026-10-07-see-each-other.md) disagree, the story file wins; stop and report rather than improvising.
  tag: story-2026-10-07-see-each-other-task-21
  tier: light
  tier_reason: evidence capture and a manual test plan; no code or contract change
```

---

## Revisions

- **2026-10-13 — final review revision (user-approved).** Fixed seven cross-prompt contradictions:
  - MH retry is 500, not 250;
  - the shared profile has nine keys;
  - the VP8 probe runner is owned by T-A;
  - a C-v3 mismatch line left over from the old wire design is removed;
  - T-A's fixture gains a group-start source;
  - OBS-B token cleanup;
  - no dev-web.sh profile override.

  Other changes:
  - Removed the stale "pending R-25" text.
  - Renumbered so that plan numbers equal manifest ids, with repo text naming tasks by code.
  - Split MH-A, C-A and C-C, giving 21 tasks; C-A1 lands before MC-A.
  - P2 owns the audio GoP counter.
  - Dropped uplink hop-sequence loss accounting and the profile-membership guard.
  - Manifest reset and re-emitted (all tasks pending).

- **2026-10-21 — amendment 4 (user), from the MH-B Gate-1 escalation.** Keyframes and recovery are event-driven:
  - the publisher keys for any target with no group in progress, including after abandoning it;
  - MH requests once per (source, counter) with no retry, and dedupes recovery by counter;
  - three MH keys are removed;
  - receivers send one recovery request per loss event, taking the sender and counter from the failing frame;
  - a client unfulfilled-slot series is added.

  R-10, R-12, R-13, R-23 and R-30 are amended, and the prompts for tasks 10, 13, 14, 17, 19, 20 and 21 are revised. Task 10's planning-phase attempt was discarded and the task restarts clean.
  Further user rulings the same day:
  - MH reads every stream's header before refusing it;
  - the minimum keyframe interval becomes MC-only `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`, moved by task 12 (which now depends on task 10).
  Prompts for tasks 12 and 16 also revised.
