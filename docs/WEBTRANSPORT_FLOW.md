# Dark Tower - WebTransport Connection Flow

This document details the WebTransport connection establishment and communication patterns used in Dark Tower.

## Overview

Dark Tower uses WebTransport (QUIC) for two primary connection types:
1. **Signaling Connection**: Client ↔ Meeting Controller (Protocol Buffer messages)
2. **Media Connection**: Client ↔ Media Handler (Proprietary binary protocol)

## Connection Type 1: Signaling (Client ↔ Meeting Controller)

### 1.1 Connection Establishment

```
┌────────┐                                    ┌──────────────────┐
│ Client │                                    │Meeting Controller│
└───┬────┘                                    └────────┬─────────┘
    │                                                  │
    │ 1. Obtain meeting_controller_url                │
    │    from Global Controller via HTTPS             │
    │                                                  │
    │ 2. WebTransport Connect                         │
    │    URL: https://mc.region.dark.com/wt           │
    │─────────────────────────────────────────────────>│
    │                                                  │
    │ 3. TLS 1.3 Handshake + QUIC Connection          │
    │<────────────────────────────────────────────────>│
    │                                                  │
    │ 4. WebTransport Session Established             │
    │<─────────────────────────────────────────────────│
    │                                                  │
    │ 5. Send JoinRequest (bidirectional stream)      │
    │    Stream ID: 0                                 │
    │─────────────────────────────────────────────────>│
    │                                                  │
    │                                      6. Validate │
    │                                         token    │
    │                                      7. Create   │
    │                                         session  │
    │                                                  │
    │ 8. Receive JoinResponse                         │
    │<─────────────────────────────────────────────────│
    │                                                  │
    │ 9. Signaling stream ready                       │
    │                                                  │
```

### 1.2 Initial Handshake Details

**Step 1: Obtain Connection Info**

```http
GET /api/v1/meetings/550e8400-e29b-41d4-a716-446655440000 HTTP/3
Host: api.dark.com
Authorization: Bearer <jwt_token>
```

Response:
```json
{
  "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
  "meeting_controller_url": "https://mc-us-west-1.dark.com:4433",
  "join_token": "eyJhbGc...",
  "expires_at": "2025-01-16T12:05:00Z"
}
```

**Step 2-4: WebTransport Connection**

```javascript
// Client-side code
const url = "https://mc-us-west-1.dark.com:4433";
const transport = new WebTransport(url);

await transport.ready;
console.log("WebTransport session established");
```

**Step 5: Send JoinRequest**

```javascript
// Create bidirectional stream for signaling
const stream = await transport.createBidirectionalStream();
const writer = stream.writable.getWriter();
const reader = stream.readable.getReader();

// Encode JoinRequest protobuf
const joinRequest = {
  meeting_id: "550e8400-e29b-41d4-a716-446655440000",
  join_token: "eyJhbGc...",
  participant_name: "Alice",
  capabilities: {
    video_codecs: ["VP9", "AV1", "H264"],
    audio_codecs: ["Opus"],
    supports_simulcast: true,
    max_video_streams: 4
  }
};

const encoded = JoinRequest.encode(joinRequest).finish();

// Send length-prefixed message
const lengthPrefix = new Uint8Array(4);
new DataView(lengthPrefix.buffer).setUint32(0, encoded.length, false);
await writer.write(lengthPrefix);
await writer.write(encoded);
```

**Step 8: Receive JoinResponse**

```rust
// Server-side code (Rust)
async fn handle_join_request(
    stream: &mut WebTransportBiStream,
    request: JoinRequest,
) -> Result<()> {
    // Validate token
    let claims = validate_jwt(&request.join_token)?;

    // Create participant session
    let participant_id = create_participant_session(
        &request.meeting_id,
        &request.participant_name,
        &request.capabilities
    ).await?;

    // Get existing participants
    let existing_participants = get_meeting_participants(&request.meeting_id).await?;

    // Allocate media handler
    let media_server = allocate_media_handler(&participant_id).await?;

    // ADR-0036 §4: hand the joiner the meeting KEK. There is no per-participant
    // key generation and no group protocol — if MC admits you, you get the KEK,
    // and senders carry their wrapped transmit keys in their own frames.
    let (meeting_kek, kek_generation) = meeting_actor.current_kek().await?;

    // Send response
    let response = JoinResponse {
        participant_id: participant_id.to_string(),
        existing_participants,
        media_server: Some(media_server),
        // `None` until MC's allocator assigns one (range/zero rules: see the
        // normative `JoinResponse.sender_id` proto comment).
        sender_id,
        meeting_kek,
        kek_generation,
    };

    send_proto_message(stream, &response).await?;

    Ok(())
}
```

### 1.3 Message Exchange Pattern

After connection establishment, the signaling stream operates in bidirectional mode:

```
Client                                    Meeting Controller
  │                                              │
  │ PublishStream (new camera stream)           │
  │─────────────────────────────────────────────>│
  │                                              │
  │                         StreamPublished ACK  │
  │<─────────────────────────────────────────────│
  │                                              │
  │                 ParticipantJoined (Bob)     │
  │<─────────────────────────────────────────────│
  │                                              │
  │                 StreamPublished (Bob's cam)  │
  │<─────────────────────────────────────────────│
  │                                              │
  │ SubscribeStream (Bob's camera)              │
  │─────────────────────────────────────────────>│
  │                                              │
  │                         Subscription ACK     │
  │<─────────────────────────────────────────────│
  │                                              │
```

### 1.4 Message Framing

All Protocol Buffer messages are length-prefixed:

```
┌─────────────────────────────┐
│ Length (4 bytes, big-endian)│
├─────────────────────────────┤
│ Protobuf Message (variable) │
└─────────────────────────────┘
```

### 1.5 Heartbeat Mechanism

```
Client                                    Meeting Controller
  │                                              │
  │ [Every 15 seconds]                           │
  │                                              │
  │ Heartbeat (via QUIC keep-alive)             │
  │─────────────────────────────────────────────>│
  │                                              │
  │                                   Update Redis│
  │                           presence timestamp │
  │                                              │
```

If no heartbeat received for 30 seconds, the participant is marked as disconnected.

---

## Connection Type 2: Media (Client ↔ Media Handler)

### 2.1 Connection Establishment

```
┌────────┐                                    ┌──────────────┐
│ Client │                                    │Media Handler │
└───┬────┘                                    └──────┬───────┘
    │                                                │
    │ 1. Receive the MediaHandlerSet from the       │
    │    Meeting Controller; authenticate below     │
    │    with the entry's media_dial token          │
    │                                                │
    │ 2. WebTransport Connect                       │
    │    URL: https://mh.region.dark.com:4434       │
    │───────────────────────────────────────────────>│
    │                                                │
    │ 3. TLS 1.3 Handshake + QUIC Connection        │
    │<──────────────────────────────────────────────>│
    │                                                │
    │ 4. WebTransport Session Established           │
    │<───────────────────────────────────────────────│
    │                                                │
    │ 5. Send Auth Message (with token)             │
    │    Datagram or unidirectional stream          │
    │───────────────────────────────────────────────>│
    │                                                │
    │                                   6. Validate  │
    │                                      token in  │
    │                                      Redis     │
    │                                                │
    │ 7. Auth Success                                │
    │<───────────────────────────────────────────────│
    │                                                │
    │ 8. Ready to send/receive media                │
    │                                                │
```

> **Corrected 2026-10-13; updated 2026-11-10 (story 4 MH-1).** Steps 5–7 are
> schematic. The client authenticates by opening a **bidirectional stream** and
> sending `MhClientMessage{connect_request: MhConnectRequest{media_dial_token}}` as
> its first length-framed message. The `media_dial` token is minted by the
> meeting's MC (ADR-0039 §5) and delivered per handler in every
> `MediaHandlerSet`; it is the only connect credential (`MhConnectRequest` tag 1,
> the meeting JWT, was deleted by story 4 P3 and is reserved). MH verifies the
> token locally, before holding anything, in a fixed order: size cap; header `alg` must be `EdDSA` (`kid`/`jwk`
> headers never select a key, and MH never fetches one); the `meeting_id` claim
> selects that meeting's Ed25519 public key from its `RegisterMeeting` (no
> registration = `DECLINED` at once, nothing parked); the signature; the claims
> (`meeting_id`, `token_type = media_dial`, `exp`/`iat` within a configured skew,
> non-empty `sub`); then `aud` must equal this MH's `handler_id`, else
> `WRONG_HANDLER`. Every earlier failure is `DECLINED`. A token is reusable for any
> number of dials within its TTL. AC's JWKS is not involved, and tokens are not
> looked up in Redis.

#### 2.1.1 The client↔MH control stream (story 3)

The connect stream stays open for the life of the connection and carries
length-framed protobuf in **both** directions — a 4-byte big-endian length prefix,
then the message bytes, bounded by MH's maximum message size:

```
Client                                         Media Handler
  │ MhClientMessage{connect_request}  (first, once) │
  │────────────────────────────────────────────────>│
  │ MhClientMessage{recovery_request}  (any time)   │
  │────────────────────────────────────────────────>│
  │            MhServerMessage{keyframe_request}    │
  │<────────────────────────────────────────────────│
```

- `RecoveryRequest{slot_id, sender_id, gop_counter}` — a receiver whose slot lost
  decodability asks its handler for a fresh group. `slot_id` is the receiver's
  downlink relay id; `sender_id` is a staleness guard only. MH remaps it to the
  source stream and aggregates; it never forwards the message verbatim.
- `KeyframeRequest{stream_number, gop_counter}` — MH asks a publisher for a
  keyframe on one stream (`stream_number` = the publisher's uplink relay id, MC's
  send id). The publisher honours it only under the bounds stated on the message.

Any message other than `connect_request` as the first message is an invalid
connect. The rules for both requests are normative in `signaling.proto` and in
ADR-0036 §1 (2026-10-13 amendment); they are not restated here.

### 2.2 Media Transport via QUIC Datagrams

> **Superseded in part (ADR-0036 §1).** "All media frames are sent using QUIC
> datagrams" is true of **audio** only: video is carried one group of pictures per
> unidirectional QUIC stream (`TRANSPORT_MODE_STREAM_PER_GROUP`), and MC assigns
> the transport per stream. In the diagram below, `stream_id` is the relay-region
> id, which is **hop-local** (ADR-0036 §2 *Header v3*): on the uplink it is the
> publisher's MC send id (audio 0, video 1), on the downlink the receiver's slot
> id; and no frame carries a `sender_id` field — the sender is bound to the
> connection at MH and named inside the SFrame key id. The diagram's values are
> illustrative only.

**All media frames are sent using QUIC datagrams**, not streams. This provides:
- Lower latency (no head-of-line blocking)
- Unreliable delivery (acceptable for real-time media)
- Automatic flow control at the connection level

Control messages (receive-capability declarations, send directives, slot
assignments) use bidirectional streams.

```
Client                                    Media Handler
  │                                              │
  │ Send Datagram: MediaFrame (audio)           │
  │ [sender_id=0x123, stream_id=0x001]          │
  │───────────────────────────────────────────>│
  │                                              │
  │ Send Datagram: MediaFrame (video)           │
  │ [sender_id=0x123, stream_id=0x002]          │
  │───────────────────────────────────────────>│
  │                                              │
  │                                    Route to  │
  │                                    subscribers│
  │                                              │
  │          Receive Datagram: MediaFrame       │
  │          [sender_id=0x456, stream_id=0x003] │
  │<─────────────────────────────────────────────│
  │                                              │
  │          Receive Datagram: MediaFrame       │
  │          [sender_id=0x456, stream_id=0x004] │
  │<─────────────────────────────────────────────│
  │                                              │
```

### 2.3 Datagram Flow Control

QUIC provides datagram-level flow control:

- Maximum datagram size: Configurable per connection (typically 1200-1500 bytes)
- No per-stream flow control (datagrams bypass stream mechanisms)
- Configurable QUIC retransmission for lost datagrams
- Connection-level congestion control

**Configuration**:
```rust
// QUIC datagram configuration
let mut transport_config = quinn::TransportConfig::default();

// Enable datagrams
transport_config.datagram_receive_buffer_size(Some(10_000_000)); // 10 MB
transport_config.datagram_send_buffer_size(10_000_000);

// Configure retransmission for media
transport_config.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
```

### 2.4 Media Frame Format

**The frame format is specified once, in ADR-0036 §2 *Header v3*** (normative
table, decode order and reject vocabulary), with the cross-language vectors file
winning over prose.

> **Superseded.** The 42-byte layout below is the pre-ADR-0036 (ADR-0028 §4)
> header and does not describe the wire. It is kept only so the change is
> visible; do not implement from it.

```
┌─────────────────────────────────────────────────────────┐
│ Version (1 byte)                                        │
├─────────────────────────────────────────────────────────┤
│ Frame Type (1 byte)                                     │
├─────────────────────────────────────────────────────────┤
│ User ID (8 bytes - participant identifier)             │
├─────────────────────────────────────────────────────────┤
│ Stream ID (4 bytes - subscriber-chosen identifier)     │
├─────────────────────────────────────────────────────────┤
│ Timestamp (8 bytes)                                     │
├─────────────────────────────────────────────────────────┤
│ Sequence Number (8 bytes)                               │
├─────────────────────────────────────────────────────────┤
│ Payload Length (4 bytes)                                │
├─────────────────────────────────────────────────────────┤
│ Flags (2 bytes)                                         │
├─────────────────────────────────────────────────────────┤
│ Reserved (6 bytes)                                      │
├─────────────────────────────────────────────────────────┤
│ Payload (variable length - encrypted with SFrame)      │
└─────────────────────────────────────────────────────────┘

Total header size: 42 bytes
```

**Stream ID Semantics** (superseded with the layout above):
- **User ID (8 bytes)**: Assigned by Meeting Controller when participant joins. This is a random 64-bit number that uniquely identifies the participant within the meeting. Shared with all participants via roster distribution.
- **Stream ID (4 bytes)**: Chosen by the subscriber for their own local bookkeeping. The Meeting Controller maintains a mapping between subscriber's local stream_id and the actual media source.

### 2.5 Bandwidth Adaptation

Since media is sent via unreliable datagrams, bandwidth adaptation is critical:

**Client-Side Adaptation**:
1. Client monitors packet loss and RTT
2. Adjusts encoding bitrate dynamically
3. Switches between simulcast layers if needed
4. Drops frames if necessary to maintain real-time performance

**Server-Side Feedback**:
Meeting Controller sends bandwidth estimates via control stream (Protocol Buffer messages):

```protobuf
message BandwidthEstimate {
  uint64 estimated_bandwidth_bps = 1;  // bits per second
  float packet_loss_rate = 2;          // 0.0 to 1.0
  uint32 rtt_ms = 3;                   // round-trip time
  uint64 timestamp = 4;                // when measured
}
```

Client uses this feedback to adjust encoding parameters before sending the next frame.

---

## Error Handling

### Signaling Connection Errors

```protobuf
message ErrorMessage {
  ErrorCode code = 1;      // See API_CONTRACTS.md
  string message = 2;
  map<string, string> details = 3;
}
```

**Error Scenarios**:

1. **Invalid Token**:
   ```
   code: UNAUTHORIZED
   message: "Invalid or expired join token"
   ```

2. **Meeting Full**:
   ```
   code: CAPACITY_EXCEEDED
   message: "Meeting has reached maximum participants"
   ```

3. **Meeting Ended**:
   ```
   code: NOT_FOUND
   message: "Meeting has ended"
   ```

### Media Connection Errors

**Stream-level errors**:
- QUIC RESET_STREAM frame with error code
- Error codes:
  - `0x00`: No error (graceful close)
  - `0x01`: Invalid stream format
  - `0x02`: Authentication failed
  - `0x03`: Quota exceeded
  - `0x04`: Internal server error

### MH session close (ADR-0039 §7)

MH ends every client session it closes **on purpose** — a declined credential or
media session, a released meeting, and (from MH-3) the drain-deadline shed — with
exactly one numeric `MhCloseCode`. The numbers are defined ONCE, in
`proto/dark_tower/signaling/v1/signaling.proto` (`enum MhCloseCode`); nothing else
restates them.

**On the wire** (MH side, `crates/mh-service/src/webtransport/close.rs`, through the
vendored wtransport patch, `vendor/patches/wtransport/PATCH.md`):

1. ONE HTTP/3 DATA frame on the session's CONNECT stream carrying ONE
   `CLOSE_WEBTRANSPORT_SESSION` capsule (draft-ietf-webtrans-http3; RFC 9297 capsule
   format): type `0x2843`, length 4, the code as a 32-bit big-endian integer, and an
   **empty** message. Minimal varints (RFC 9000 §16): for code 4 the bytes are
   `00 07 68 43 04 00 00 00 04`.
2. FIN on the CONNECT stream.
3. MH waits for the peer to react (its FIN, reset, or connection close), then the
   transport closes QUIC with `H3_NO_ERROR` (0x100) and an empty reason. The capsule
   WRITE (which waits on the peer's flow-control credit) and this wait share ONE
   bound, 1 s (`CLOSE_SESSION_LINGER`); past it MH closes QUIC with `H3_NO_ERROR`
   itself. Never a QUIC close carrying the code.

A browser reads that as `WebTransport.closed` RESOLVING with
`WebTransportCloseInfo.closeCode`.

**No code means transport loss.** Every other end — crash, QUIC idle timeout, a
client close, a stream failure, and server shutdown until MH-3 — carries no capsule
and no CONNECT FIN: MH closes QUIC explicitly (`H3_NO_ERROR`) before dropping the
connection, because a dropped connection can FIN the CONNECT stream, which a browser
would read as a clean close with code 0 (a contract fault). Expected browser reading
of a capsule-less QUIC close: `closed` rejects (loss); story 4 task C1 verifies it in
a browser.

**Security**: a code other than `DECLINED` is sent only after the credential fully
validated (`WRONG_HANDLER` = a fully valid `media_dial` token whose `aud` is another
handler); every credential failure and pre-validation exit is the one generic
`DECLINED`; every code concerns only the caller's own validated meeting; no reason
strings.

### Connection Loss Handling

```
Client                                    Server
  │                                          │
  │ Connection lost                          │
  │ (network issue)                          │
  │                                          │
  │ [Automatic QUIC retransmission]          │
  │                                          │
  │ [If fails after 10s]                     │
  │                                          │
  │ Close connection                         │
  │ Trigger reconnection logic               │
  │                                          │
  │ 1. Obtain new join token (HTTP)         │
  │──────────────────────────────────────────>│
  │                                          │
  │ 2. New WebTransport connection           │
  │──────────────────────────────────────────>│
  │                                          │
  │ 3. Resume session                        │
  │    (same participant_id if within TTL)   │
  │                                          │
```

---

## Connection Lifecycle

### Signaling Connection Lifecycle

```
[DISCONNECTED]
      │
      │ WebTransport.connect()
      ▼
[CONNECTING]
      │
      │ ready event
      ▼
[AUTHENTICATING]
      │
      │ JoinRequest/JoinResponse
      ▼
[CONNECTED]
      │
      │ ◄───── Normal operation
      │        - Send/receive messages
      │        - Heartbeats
      │
      │ close() or error
      ▼
[DISCONNECTING]
      │
      │ cleanup
      ▼
[DISCONNECTED]
```

### Media Connection Lifecycle

```
[IDLE]
      │
      │ Receive MediaHandlerSet entry
      ▼
[CONNECTING]
      │
      │ WebTransport ready
      ▼
[AUTHENTICATING]
      │
      │ Send auth token
      ▼
[READY]
      │
      ├──> [SENDING_MEDIA]    // Open outgoing streams
      │         │
      │         │ Encode frames with WebCodec
      │         │ Send via unidirectional streams
      │         │
      │    [ACTIVE]
      │         │
      ├──> [RECEIVING_MEDIA]  // Accept incoming streams
      │         │
      │         │ Receive frames
      │         │ Decode with WebCodec
      │         │
      │    [ACTIVE]
      │
      │ close() or error
      ▼
[CLOSING]
      │
      │ Close all streams
      │ Send FIN
      ▼
[CLOSED]
```

---

## Performance Optimizations

### 1. Connection Pooling

Meeting Controllers maintain connection pools to Media Handlers:

```rust
struct ConnectionPool {
    connections: HashMap<String, Vec<WebTransportSession>>,
    max_per_handler: usize,
}
```

### 2. Stream Multiplexing

Multiple media streams share single QUIC connection:
- Reduces connection overhead
- Better congestion control
- Simplified firewall traversal

### 3. Datagram API Usage

Client-side JavaScript for sending media via datagrams:

```javascript
// Get datagram writer
const datagramWriter = transport.datagrams.writable.getWriter();

// Encode media frame (media-protocol v2 header — ADR-0036 §2 + Appendix;
// header v3, landed by P2, adds a 32-bit gopCounter to the publisher region).
// The header splits into a publisher region (signed and used as the AEAD
// associated data) and a relay region the media handler rewrites per
// subscriber. There is no media-type field: the independently-decodable flag
// replaces it, because MH must not learn audio from video (§7).
const encodedFrame = encodeMediaFrame({
  flags: { independentlyDecodable: true, discardable: false, keyBearing: true },
  streamSequence: streamSequence++,   // per (sender, stream); NEVER reset — AEAD nonce input
  wrappedTransmitKey: wrapped,        // present iff keyBearing (§4)
  extensions: [],                     // publisher-set TLV (§7); audio carries none today
  streamId,                           // relay region, hop-local: on this uplink, MC's send id
                                      // (SendStream.stream_number); MH rewrites it to the slot id
  hopSequence: hopSequence++,         // relay region: per (connection, media stream)
  payload: sframeObject,              // opaque SFrame object
  signature                           // Ed25519 over publisher region then payload (§3)
});

// Send via datagram
await datagramWriter.write(encodedFrame);
```

**Benefits**: Lower latency, no head-of-line blocking
**Trade-off**: Unreliable delivery (acceptable for real-time media)

---

## Security Considerations

### 1. TLS 1.3 Required

All WebTransport connections use TLS 1.3:
- Forward secrecy
- Strong cipher suites only
- Certificate pinning (optional)

### 2. Token-Based Authentication

- Signaling (MC): the AC-issued meeting JWT (EdDSA, verified against AC's JWKS).
- Media (MH): the MC-minted `media_dial` token (EdDSA with a per-meeting Ed25519
  key, `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS`, default 300 s), reusable within its TTL,
  verified against the key in the meeting's `RegisterMeeting` (ADR-0039 §5; see
  §2.1).

### 3. Media Encryption

> **Corrected 2026-10-13.** This section was headed "E2E Encryption" and ended
> "Server cannot decrypt media content". Both overstate: under ADR-0036 §4 media
> is encrypted between clients, and **MH, transport and storage cannot read it;
> MC can**, because MC issues the meeting KEK. That is accepted operator custody
> (`key_custody=operator`), and nothing may describe it as end-to-end against the
> operator or as zero-trust. The original text is kept below so the change is
> visible.

Media payloads are encrypted client-side before transmission:

```
┌──────────────────────────────────┐
│ Application Layer (E2E)          │  <-- Client encrypts
├──────────────────────────────────┤
│ Media Protocol (Proprietary)     │
├──────────────────────────────────┤
│ WebTransport/QUIC                │
├──────────────────────────────────┤
│ TLS 1.3                          │  <-- Transport encryption
├──────────────────────────────────┤
│ UDP                              │
└──────────────────────────────────┘
```

Server cannot decrypt media content.

---

## Testing Strategies

### Unit Tests

- Mock WebTransport connections
- Test message encoding/decoding
- Validate error handling

### Integration Tests

- Local QUIC server for testing
- Simulate network conditions (latency, packet loss)
- Test reconnection logic

### Load Tests

- Concurrent connection handling
- Stream multiplexing limits
- Bandwidth saturation behavior

---

## Monitoring & Observability

### Key Metrics

1. **Connection Metrics**:
   - Connection establishment time
   - Active connections count
   - Connection failures by reason

2. **Stream Metrics**:
   - Streams per connection
   - Stream creation rate
   - Stream errors

3. **Performance Metrics**:
   - Round-trip time (RTT)
   - Packet loss rate
   - Bandwidth utilization
   - Frame delivery latency

### Distributed Tracing

All connections tagged with:
- `meeting_id`
- `participant_id`
- `region`
- `controller_id` / `handler_id`

Enables end-to-end request tracing through the system.
