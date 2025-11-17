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
    │    URL: wss://mc.region.dark.com/ws             │
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

    // Generate encryption keys for E2E
    let encryption_keys = generate_encryption_keys(&participant_id)?;

    // Send response
    let response = JoinResponse {
        participant_id: participant_id.to_string(),
        existing_participants,
        media_server: Some(media_server),
        encryption_keys: Some(encryption_keys),
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
    │ 1. Receive media_handler_url &                │
    │    connection_token from Meeting Controller   │
    │                                                │
    │ 2. WebTransport Connect                       │
    │    URL: wss://mh.region.dark.com:4434         │
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

### 2.2 Media Stream Mapping

Each media stream (audio/video) uses a dedicated QUIC stream:

- **Outgoing Media**: Client opens unidirectional stream per media source
- **Incoming Media**: Media Handler opens unidirectional streams to client

```
Client                                    Media Handler
  │                                              │
  │ Open UniStream (Stream ID: 4)               │
  │ Tag: audio, stream_id=abc123                │
  │───────────────────────────────────────────>│
  │                                              │
  │ MediaFrame 1 ────────────────────────────>│
  │ MediaFrame 2 ────────────────────────────>│
  │ MediaFrame 3 ────────────────────────────>│
  │                                              │
  │                                    Route to  │
  │                                    subscriber│
  │                                              │
  │                  UniStream (Stream ID: 5)   │
  │                  Tag: video, stream_id=def456│
  │<─────────────────────────────────────────────│
  │                                              │
  │<──────────────────────────────── MediaFrame 1│
  │<──────────────────────────────── MediaFrame 2│
  │<──────────────────────────────── MediaFrame 3│
  │                                              │
```

### 2.3 Stream Initialization

Each media stream starts with a header message:

```
┌─────────────────────────────────────────────────┐
│ Stream Header (32 bytes)                        │
├─────────────────────────────────────────────────┤
│ Magic (4 bytes): 0x44415254 ("DART")           │
├─────────────────────────────────────────────────┤
│ Version (1 byte): 0x01                          │
├─────────────────────────────────────────────────┤
│ Stream Type (1 byte): 0x00=Audio, 0x01=Video   │
├─────────────────────────────────────────────────┤
│ Stream ID (16 bytes): UUID                      │
├─────────────────────────────────────────────────┤
│ Codec (4 bytes): FourCC code                    │
├─────────────────────────────────────────────────┤
│ Reserved (6 bytes)                              │
└─────────────────────────────────────────────────┘
```

### 2.4 Media Frame Format

After the header, each frame follows the format defined in API_CONTRACTS.md:

```
┌─────────────────────────────────────────────────────────┐
│ Version (1 byte)                                        │
├─────────────────────────────────────────────────────────┤
│ Frame Type (1 byte)                                     │
├─────────────────────────────────────────────────────────┤
│ Stream ID (16 bytes)                                    │
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
│ Payload (variable)                                      │
└─────────────────────────────────────────────────────────┘
```

### 2.5 Flow Control

QUIC provides automatic flow control per stream:

```rust
// Server-side flow control
const MAX_STREAM_BUFFER: usize = 1_048_576; // 1MB
const MAX_CONNECTION_BUFFER: usize = 10_485_760; // 10MB

// Configure QUIC transport
let mut config = quinn::ServerConfig::default();
config.transport.stream_receive_window(MAX_STREAM_BUFFER as u64);
config.transport.receive_window(MAX_CONNECTION_BUFFER as u64);
```

Client adapts to available bandwidth using feedback from Meeting Controller.

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
      │ Receive media_handler_url
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

### 3. Datagram Support (Future)

For ultra-low latency, use QUIC datagrams:

```javascript
// Send unreliable audio frame (datagram mode)
transport.datagrams.writable.getWriter().write(audioFrame);
```

Trade-off: Unreliable delivery vs. lower latency

---

## Security Considerations

### 1. TLS 1.3 Required

All WebTransport connections use TLS 1.3:
- Forward secrecy
- Strong cipher suites only
- Certificate pinning (optional)

### 2. Token-Based Authentication

- Short-lived tokens (5 min TTL)
- Single-use for media connections
- Cryptographically signed (HMAC-SHA256)

### 3. E2E Encryption

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
