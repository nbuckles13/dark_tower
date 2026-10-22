//! Video media-path test fixtures (story 3; ADR-0036 §1, §2, §6, §7).
//!
//! A keyless, black-box VIDEO publisher and subscriber over the existing MH
//! WebTransport fixture connection ([`super::mc_session::MhSession`]), in the
//! style of [`super::media`]: payloads are opaque markers, frames are built and
//! parsed ONLY through the production `media-protocol` codec, and nothing here
//! holds, derives or verifies key material (no crypto crate is imported).
//!
//! # Layout
//!
//! Every rule is a PURE core; the wtransport loops only shovel bytes, so every
//! self-test runs hermetically on tokio paused time with no QUIC:
//!
//! * [`schedule`] — [`GroupStartSource`] and the [`GroupScheduler`] state
//!   machine (counter rule, `min_interval` deferral, periodic cadence, the
//!   ordered [`PublisherEvent`] log).
//! * [`encode`] — one frame through `media_protocol::codec::encode_frame`.
//! * [`reader`] — the chunk-fed stream parser over
//!   `media_protocol::reader::HeaderReader`.
//! * [`transport`] — the [`UplinkTransport`]/[`UplinkStream`] seam (implemented
//!   for `wtransport::Connection`).
//! * [`control`] — keyframe requests in, recovery requests out, on the MH
//!   carrier stream.
//! * [`publisher`] / [`subscriber`] — the drivers. [`VideoSubscriber::next`]
//!   returns [`Next::Quiet`] only while the connection is LIVE; once it reports
//!   [`SubscriberEvent::ConnectionClosed`], a further quiet read panics, so an
//!   absence assertion can never pass on a dead subscriber.
//!
//! # The knobs (final names, agreed with media-handler at T-A planning)
//!
//! * [`GroupStartSource`] — ONE knob, default `None`:
//!   * `None`: a group starts only at stream start. This is ALSO the
//!     withhold-keyframes / never-keying source for MH-B's pending gauge (a late
//!     subscriber's edge waits pending, and MH sends ONE request per (stream,
//!     counter) that is never answered — MH does not retry). A publisher that
//!     is bound but never started is the different
//!     "never-sending, zero requests answerable" source.
//!   * `OnKeyframeRequest { min_interval }` (MH-B, task 10): answers
//!     `MhServerMessage.keyframe_request { stream_number, gop_counter }` read
//!     from the carrier stream, only if `stream_number` is the DIRECTED send id
//!     (never the label knob) and `gop_counter >= current`. A lower counter,
//!     another stream's number, or an already-answered repeat (its counter is
//!     now lower) produces nothing. `min_interval` (default 0) mirrors the
//!     client: one in-interval request per stream is DEFERRED, never refused,
//!     and re-checked when it fires.
//!   * `Periodic(interval)` (MH-A2, task 9): a new group every `interval` on
//!     tokio time. **FIXTURE-ONLY — never a production keyframe interval.**
//!     ADR-0036 as amended has none: keyframes are on demand only. It exists so
//!     MH-A2's late-subscriber gate (first forwarded frame = the next group
//!     start, never mid-group) is drivable before MH-B adds requests.
//! * [`UplinkLabel`] — the uplink relay `stream_id`: by default the send id MC
//!   directed ([`super::media::VIDEO_SEND_ID`]); `Override(AUDIO_SEND_ID)` is
//!   the relabel case, any other value an undeclared id. The payload's key-id
//!   `stream` byte is ALWAYS the directed send id, so a relabelled frame carries
//!   key-id stream 1 and relay label 0 — exactly MH-A1's "a frame whose key-id
//!   stream disagrees with its relay label routes by the relay label".
//! * [`VideoTransport`] — uni-stream-per-group, or one ≤MTU datagram per frame
//!   (sized against `max_datagram_size()` at send time; too large FAILS LOUDLY,
//!   never truncated or split). Label × transport compose freely.
//! * [`PublisherMode`] — `Compliant` sends only while [`VideoPublisher::set_directed`]
//!   says so: a withdrawal finishes the group stream, requests while withdrawn
//!   are `NotSending`, and re-direction starts a NEW group (`Resume`, next
//!   counter) — never a mid-group resume, since MH forwards a paused or waiting
//!   source from the next group start. `KeepSendingWhileMuted` ignores
//!   withdrawal and mute (the patched client for R-16).
//! * MH's STOP_SENDING (a stream-path attribution drop or a stream-rate
//!   refusal closes the stream) is an OBSERVABLE outcome, never a fixture
//!   panic: [`FrameOutcome::StoppedByPeer`] and
//!   [`PublisherEvent::StreamStoppedByPeer`]; frames are then withheld until
//!   the next decided group start, which opens a fresh stream.
//! * [`Fault`] — `StallMidFrame` writes a header and part of a payload, then
//!   PARKS the stream handle until [`VideoPublisher::release_stall`] (dropping
//!   it would FINISH it — R-21's stall bound); `ResetMidGroup` calls
//!   `reset(`[`RESET_MID_GROUP_CODE`]`)` explicitly and the publisher then ENDS;
//!   `AbandonMidGroup` resets the same way ONCE and keeps running, self-keying
//!   the next group (story 3 amendment 4: a publisher keys whenever a target
//!   has no group in progress — under every [`GroupStartSource`], inside
//!   `min_interval` through the single deferral), logged as
//!   `GroupStartCause::TargetAbandoned`. A stream MH stopped never self-keys:
//!   that abandonment is MH's to repair by request. A group rollover calls
//!   `finish()` explicitly.
//!
//! There is no key-id-stream knob (ruled out at story design).
//!
//! # GoP counter convention
//!
//! Mirrors the SDK's `nextGopCounter`: the first group is 0, each later group
//! start takes the next value via `checked_add`, every frame of a group carries
//! the counter of the frame that started it, and at `u32::MAX` the next group
//! start STOPS the stream ([`PublisherEvent::Exhausted`]) — never a wrap
//! (ADR-0036 §2). Group-start frames are independently decodable, not
//! key-bearing (keyless fixture), not discardable; other frames set no flag.
//!
//! # Payload
//!
//! `key_id(8) || marker fill`, the key id in `media_protocol::frame`'s layout
//! (`sender_id | stream | generation`), packed from the `KEY_ID_*` constants
//! with checked conversions. `generation` is a constant 0 PLACEHOLDER, not a
//! real generation. Unlike [`super::media`]'s marker-only audio, the key id is
//! needed because a 16-bit sender id does not fit a one-byte marker. The key id
//! carries sender identity: it never appears in a log, panic string or `Debug`.
//!
//! # No telemetry (R-30)
//!
//! This module emits nothing: no `tracing`, no `println!`/`eprintln!`. GoP
//! counters, stream and slot ids, key-id senders and lengths exist only in
//! returned values. Panic text names the phase, slot/sender and outcome, never
//! a counter, length or key id.
//!
//! # MH's expected outcomes — the table consumers assert
//!
//! The publisher declares audio = send id 0 DATAGRAM and video = send id 1
//! STREAM_PER_GROUP. Pinned verbatim from the T-A task; the FIXTURE'S OWN TESTS
//! ASSERT NONE OF IT. Asserting owners: MH-A1 (task 8) for the datagram rows,
//! MH-A2 (task 9) for the stream-path extensions, MH-B (task 10) for recovery.
//!
//! | Input | MH outcome |
//! |-------|-----------|
//! | An unknown uplink relay id, on either transport | dropped `mh_media_frames_dropped_total{reason="unmatched_stream"}` |
//! | A declared id on the wrong transport | dropped `mh_media_frames_dropped_total{reason="transport_mismatch"}` |
//! | Video relabelled as the audio id while video is server-muted (a patched client sending video bytes as ≤MTU datagrams) | reaches AUDIO slots only, never a video slot |
//! | Any frame from a sender whose every declared stream is DROP_AT_INGRESS | dropped `mh_media_frames_dropped_total{reason="server_muted"}` |
//! | A PAUSED (client-muted) stream | forwarded, and `mh_media_paused_source_frames_total` increments |
//!
//! Additions from MH-A1 §3 and MH-A2's stream rows, so consumers do not assert
//! the wrong thing:
//!
//! * A frame on a SINGLE `DROP_AT_INGRESS` stream (e.g. video muted, audio
//!   FORWARD) is dropped `server_muted` AFTER attribution; the sender's audio is
//!   unaffected.
//! * Precedence: the sender-level all-`DROP_AT_INGRESS` shortcut runs FIRST — for
//!   such a sender even an undeclared label or a wrong transport counts
//!   `server_muted`, never `unmatched_stream` / `transport_mismatch`.
//! * The stream path: a label-1 uni stream is forwarded cut-through to video
//!   slots only; while PAUSED it is forwarded and counted as a paused arrival;
//!   label 0 on a uni stream is `transport_mismatch`.
//!
//! Recovery requests ([`recovery_request_frame`]; MH-B), every field explicit:
//!
//! | `recovery_request { slot_id, sender_id, gop_counter }` | MH outcome |
//! |-------|-----------|
//! | counter EQUAL to MH's newest for that source stream | forwarded (re-sent upstream as MH's own `KeyframeRequest`, fields remapped) |
//! | counter LOWER | `stale` |
//! | counter HIGHER | `counter_ahead` (no clamping) |
//! | `sender_id` not the slot's current source | `source_mismatch`, never forwarded upstream |
//! | a slot with no stream-per-group source (e.g. an audio slot) | `unmapped_slot` |
//!
//! Until MH-B lands, MH's carrier hold-open loop reads and discards these
//! bytes, so a recovery request is harmless and has no effect.
//!
//! # Per-file edge-sum header rule (R-33 consumers)
//!
//! Each consuming env-test FILE records in its header the sum, over its
//! CONCURRENTLY running test fns, of P × (N+M) — participants times the audio
//! plus video slots each declares. `cargo test` runs one file's fns in
//! parallel and the files sequentially, so the per-file sum is the load the
//! Kind cluster carries at once. INF-C (task 16) sizes the Kind ceiling at
//! about 120 against the PLANNED largest per-file sum, 72
//! (`27_mc_slot_placement`); each file's ACTUAL sum is the `Edge-sum` line in
//! its own header. A file that would exceed it serialises its fns or raises the budget in the
//! same change.

pub mod backlog;
pub mod control;
pub mod encode;
pub mod probe;
pub mod publisher;
pub mod reader;
pub mod schedule;
pub mod subscriber;
pub mod transport;

pub use control::{
    recovery_request_frame, send_recovery_request, CarrierRequests, KeyframeRequestMsg,
    KeyframeRequestSource, NoRequests,
};
pub use encode::{encode_video_frame, key_id_sender, pack_key_id, VideoFrameSpec};
pub use probe::{until_video_on_slot, BackgroundPublisher};
pub use publisher::{
    Fault, FrameOutcome, PublisherMode, UplinkLabel, VideoPublisher, VideoPublisherConfig,
    VideoTransport, RESET_MID_GROUP_CODE,
};
pub use reader::{ReadError, ReceivedFrame, ReceivedStream, StreamEnd, StreamFrameReader};
pub use schedule::{
    Decision, GroupScheduler, GroupStartCause, GroupStartSource, PublisherEvent, RequestOutcome,
};
pub use subscriber::{Next, SubscriberEvent, VideoSubscriber};
pub use transport::{DownlinkRead, DownlinkStream, UplinkError, UplinkStream, UplinkTransport};
