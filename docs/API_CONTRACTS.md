# Dark Tower - API Contracts

This document defines the interfaces and communication patterns between Dark Tower components.

## Overview

```
┌──────────┐
│  Client  │
└─────┬────┘
      │
      │ HTTP/3: Meeting management
      ▼
┌──────────────────┐
│ Global Controller│
└─────┬────────────┘
      │
      │ Internal API: Meeting discovery
      ▼
┌────────────────────┐
│ Meeting Controller │◄─────┐
└─────┬──────────────┘      │
      │                     │ WebTransport: Media control
      │ WebTransport:       │
      │ Signaling           │
      ▼                     │
┌──────────┐          ┌─────┴──────┐
│  Client  │◄────────►│Media Handler│
└──────────┘          └────────────┘
   WebTransport: Media streams
```

## 1. Client ↔ Global Controller

**Transport**: HTTP/3 (for transactional requests)

### 1.1 Create Meeting

**Endpoint**: `POST /api/v1/meetings`

**Request**:
```json
{
  "display_name": "Team Standup",
  "max_participants": 100,
  "settings": {
    "enable_e2e_encryption": true,
    "require_auth": false,
    "recording_enabled": false
  }
}
```

**Response** (201 Created):
```json
{
  "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
  "meeting_url": "https://darktower.example.com/m/abc123",
  "join_token": "eyJhbGciOiJIUzI1NiIs...",
  "meeting_controller_url": "https://us-west-1.darktower.example.com:4433/wt",
  "created_at": "2025-01-16T12:00:00Z"
}
```

### 1.2 Get Meeting Info

**Endpoint**: `GET /api/v1/meetings/{meeting_id}`

**Response** (200 OK):
```json
{
  "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
  "display_name": "Team Standup",
  "participant_count": 5,
  "max_participants": 100,
  "created_at": "2025-01-16T12:00:00Z",
  "meeting_controller_region": "us-west-1",
  "meeting_controller_url": "https://us-west-1.darktower.example.com:4433/wt"
}
```

### 1.3 List Meetings

**Endpoint**: `GET /api/v1/meetings?user_id={user_id}&active=true`

**Response** (200 OK):
```json
{
  "meetings": [
    {
      "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
      "display_name": "Team Standup",
      "participant_count": 5,
      "created_at": "2025-01-16T12:00:00Z"
    }
  ],
  "total": 1
}
```

### 1.4 Authentication

**Endpoint**: `POST /api/v1/auth/token`

**Request**:
```json
{
  "grant_type": "client_credentials",
  "client_id": "...",
  "client_secret": "..."
}
```

**Response** (200 OK):
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIs...",
  "token_type": "Bearer",
  "expires_in": 3600
}
```

## 2. Client ↔ Meeting Controller

**Transport**: WebTransport (QUIC) for bidirectional signaling

### 2.1 Connection Establishment

1. Client obtains `meeting_controller_url` from Global Controller
2. Client establishes WebTransport connection with join token
3. Meeting Controller validates token and creates session

**Initial Handshake**:
```
Client → Meeting Controller: JoinRequest (protobuf)
Meeting Controller → Client: JoinResponse (protobuf)
```

### 2.2 Signaling Messages (Protocol Buffers)

#### JoinRequest
```protobuf
message JoinRequest {
  string meeting_id = 1;
  string join_token = 2;
  string participant_name = 3;
  ParticipantCapabilities capabilities = 4;
  string correlation_id = 5;              // ADR-0023 session recovery
  string binding_token = 6;               // ADR-0023 session recovery
  bytes identity_public_key = 7;          // raw Ed25519, 32 bytes (ADR-0036 §4)
}

message ParticipantCapabilities {
  reserved 1 to 4;                        // was: video_codecs, audio_codecs,
  reserved "video_codecs", "audio_codecs",  //      supports_simulcast,
           "supports_simulcast", "max_video_streams";
  repeated Codec supported_codecs = 5;
  repeated uint32 supported_header_versions = 6;
}

enum Codec {
  CODEC_UNSPECIFIED = 0;                  // never valid in a directive
  CODEC_OPUS = 1;
  CODEC_VP9 = 2;
  CODEC_AV1 = 3;
  CODEC_H264 = 4;
}
```

`video_codecs`/`audio_codecs` collapse into one `supported_codecs` list: media
kind is a property of the codec's own identity, so encoding it a second time
positionally lets the two representations disagree (`audio_codecs: [VP9]` was
well-typed and meaningless). `supports_simulcast` goes because simulcast is out
of scope in ADR-0036, and `max_video_streams` because `ReceiveCapability`'s
explicit slot list supersedes it — a scalar maximum and a per-slot list are two
representations of one constraint, and the list is strictly more expressive.

#### JoinResponse
```protobuf
message JoinResponse {
  string participant_id = 1;
  reserved 2, 5;                                // was: user_id, encryption_keys
  reserved "user_id", "encryption_keys";
  repeated Participant existing_participants = 3;
  repeated MediaServerInfo media_servers = 4;   // Multiple handlers
  string correlation_id = 6;                    // ADR-0023 session recovery
  string binding_token = 7;                     // ADR-0023 session recovery
  optional uint32 sender_id = 8;                // 16-bit semantics; 1..=65535
  bytes meeting_kek = 9;                        // AES-256, exactly 32 bytes
  uint32 kek_generation = 10;                   // u16 semantics
}

message Participant {
  string participant_id = 1;
  string name = 2;
  repeated MediaStream streams = 3;
  uint64 joined_at = 4;
  optional uint32 sender_id = 5;
  bytes identity_public_key = 6;                // raw Ed25519, 32 bytes
}

message MediaServerInfo {
  string media_handler_url = 1;
  reserved 2;                                   // was: connection_token
}
```

**`sender_id` (ADR-0036 §2).** The joiner's per-meeting numeric sender id, and the
first half of the attribution chain. A `uint32` on the wire carrying 16-bit
semantics, because the 64-bit SFrame key id allots 16 bits to the sender
(`sender_id(16) | stream(8) | generation(40)`).

> The `JoinResponse.sender_id` comment in `proto/dark_tower/signaling/v1/signaling.proto`
> is normative; the bullets below summarise it. On any disagreement — especially
> the `NonZeroU16` enforcement clause, which is an unimplemented instruction to
> story task 10 — the proto wins.

- **Valid range 1..=65535. Zero is never valid**, and there is no zero sentinel:
  absence is field presence, not a magic value. Absent means MC has not assigned
  one.
- MC is allocator **and** enforcement point, using `NonZeroU16` — not a bare
  `u16::try_from`, which accepts the reserved-invalid zero. Fail closed on
  violation: refuse the join or the assignment, never truncate or wrap. An
  out-of-range value does not fail loudly on its own — it truncates into the key
  id and surfaces as a signature failure against the *wrong* participant's key.
- Consumers MUST NOT coerce absence to 0. Two senders sharing zero collide on the
  key id, hence on the derived AEAD wrap nonce under one KEK — AES-GCM
  authentication-key recovery, not merely a confidentiality loss.
- Per-meeting and MC-allocated. **Not** a durable user identity: the field it
  replaces (`user_id`, a hardcoded-zero `uint64`) would have made participants
  linkable across meetings.

**`meeting_kek` / `kek_generation` (ADR-0036 §4).** Every participant MC admits
receives the meeting key-encryption key; there is no other condition and no group
protocol. Senders wrap per-stream transmit keys under it and carry the wrapped key
in their own frames, so a joiner needs no round trip and no existing member is
touched. Empty KEK means not-yet-provisioned; consumers fail closed and never wrap
or unwrap under a key of any length other than 32.

The accurate claim, in ADR-0036 §4's own words: **media is encrypted between
clients; MH, transport and storage cannot read it; MC can.** This is accepted
operator custody, recorded as the user's risk decision. It must never be described
as end-to-end against the operator or as zero-trust; telemetry carries
`key_custody=operator` in place of any end-to-end boolean — the label's definition,
its permitted values, and which telemetry surfaces must carry it are specified in
`docs/observability/label-taxonomy.md` §Key custody, which is canonical for those
mechanics; the accuracy claim and the prohibition above are stated here as contract.
No key material crosses the MC→MH contract, and none rides on the roster.

**`identity_public_key` (ADR-0036 §3, §4).** The participant's raw Ed25519 signing
public key — 32 bytes, not PEM/JWK/base64/SPKI — sent on `JoinRequest` and
republished on the roster. **This story performs no attestation check**: MC does
not verify it against the meeting token's `cnf` thumbprint, so it is trust on
first use. A verified frame signature therefore proves only that every frame came
from the same keyholder, never *who* that keyholder is, and for a guest never more
than a pseudonym. The client-validated AC attestation closes this in story 2;
when it lands, the attestation wins over the roster on disagreement.

Keys are scoped to one meeting, never reused across meetings — a reused key makes
a participant linkable by public key regardless of display name.

#### ParticipantJoined (Server → Client)
```protobuf
message ParticipantJoined {
  Participant participant = 1;
}
```

#### ParticipantLeft (Server → Client)
```protobuf
message ParticipantLeft {
  string participant_id = 1;
  LeaveReason reason = 2;
}

enum LeaveReason {
  LEAVE_REASON_UNSPECIFIED = 0;
  reserved 1 to 4;                    // was: VOLUNTARY..TIMEOUT at 0..4
  LEAVE_REASON_VOLUNTARY = 5;
  LEAVE_REASON_KICKED = 6;
  LEAVE_REASON_CONNECTION_LOST = 7;
  LEAVE_REASON_MEETING_ENDED = 8;
  LEAVE_REASON_TIMEOUT = 9;
}
```

#### PublishStream (Client → Server)
```protobuf
message PublishStream {
  string stream_id = 1;
  reserved 2, 3;                      // was: stream_type, metadata
  MediaKind media_kind = 4;
  EncodingParameters encoding = 5;
}

message MediaStream {
  string stream_id = 1;
  reserved 2, 3;
  MediaKind media_kind = 4;
  EncodingParameters encoding = 5;
}

// Collapses the former duplicate StreamType and MediaType enums.
enum MediaKind {
  MEDIA_KIND_UNSPECIFIED = 0;
  MEDIA_KIND_AUDIO = 1;
  MEDIA_KIND_VIDEO_CAMERA = 2;
  MEDIA_KIND_VIDEO_SCREEN = 3;
}

// The single encoding vocabulary. Replaces StreamMetadata / VideoMetadata /
// SimulcastLayer; simulcast is out of scope in ADR-0036.
message EncodingParameters {
  Codec codec = 1;
  uint32 max_bitrate_bps = 2;
  uint32 width = 3;
  uint32 height = 4;
  uint32 frame_rate = 5;
}
```

#### StreamPublished (Server → Client)
```protobuf
message StreamPublished {
  string participant_id = 1;
  MediaStream stream = 2;
}
```

#### ReceiveCapability (Client → Server)

**Capability, not layout.** The client declares what it can **decode and render**
— a list of slots, each with a media kind and an optional pin — and MC composes
the experience. It does not specify who appears where. The predecessor
(`SubscribeToLayout` / `UpdateLayout` / `UnsubscribeLayout` / `LayoutConfig` /
`LayoutType`) had the client specifying grid geometry, which inverts that;
geometry was never the server's business. This is also a security property:
resource-amplification-by-request becomes structurally impossible rather than
rate-limited.

```protobuf
message ReceiveCapability {
  repeated ReceiveSlot slots = 1;     // capped by server-side configuration
}

message ReceiveSlot {
  uint32 slot_id = 1;                 // subscriber-chosen, 16-bit semantics
  MediaKind media_kind = 2;
  optional uint32 pinned_sender_id = 3;
}
```

**Every constraint is an upper bound**, so any subset is a valid fulfilment and an
unsatisfiable request cannot be expressed. **Pins are a per-slot optional
parameter, never a parallel list** — which makes "seven pins into six slots"
unrepresentable rather than merely invalid.

`slot_id` is **subscriber-chosen and scoped to one subscriber's connection, not
globally unique**: a media handler's routing table must be keyed on
`(subscriber, slot_id)`. It must be unique within one declaration — MC rejects
duplicates rather than last-write-wins — and MC validates the 16-bit range and the
slot cap, rejecting the whole declaration on violation (not a `debug_assert!`,
which release profiles compile out; not clamp-or-truncate). It is the same value
space as the frame's relay-region stream id, which is what lets a receiver
validate an arriving `stream_id` against its own declared slots.

#### StreamAssignments (Server → Client)

```protobuf
message StreamAssignments {
  repeated StreamAssignment assignments = 1;
}

message StreamAssignment {
  uint32 slot_id = 1;
  optional uint32 sender_id = 2;
  MediaKind media_kind = 3;
  string media_handler_url = 4;
  SlotState slot_state = 5;
  optional uint64 switch_command_id = 6;   // iff slot_state == SWITCH_PENDING
}
```

**Attribution chain**: an arriving frame's key id yields `sender_id`, `sender_id`
yields the roster entry, and the roster entry's `identity_public_key` verifies the
frame's Ed25519 signature. Attribution comes from **the signature verifying**,
never from this assignment or the roster mapping itself — a compromised MC can
mis-map either.

**Slot state is explicit on the wire, because absence of frames is not a signal.**
*Withheld by congestion*, *fewer sources than slots* and *source unreachable* are
indistinguishable to a client — all present as no media — and render completely
differently.

```protobuf
enum SlotState {
  SLOT_STATE_UNSPECIFIED = 0;
  SLOT_STATE_ACTIVE = 1;                        // steady
  SLOT_STATE_SOURCE_MUTED = 2;                  // steady: present, far-end muted
  SLOT_STATE_WITHHELD_BY_CONGESTION = 3;        // transient; the only MH-observed one
  SLOT_STATE_FEWER_SOURCES_THAN_SLOTS = 4;      // steady
  SLOT_STATE_ZERO_REQUESTED = 5;                // steady
  SLOT_STATE_SOURCE_UNREACHABLE = 6;            // structurally persistent
  SLOT_STATE_SWITCH_PENDING = 7;                // transient; carries command id
}
```

Switch completion is reported **by command identifier, never by state
description**: if MC switches a slot to B, reverts to A, then switches to B again,
a report reading "A→B complete" is ambiguous across the first and third commands.

#### SendDirective (Server → Client)

**MC directs both where a client sends and what it produces** (ADR-0036 §5).

```protobuf
message SendDirective {
  repeated SendStream streams = 1;
  uint32 header_version = 2;          // meeting-wide frame header version (§2)
}

message SendStream {
  uint32 stream_number = 1;           // 8-bit semantics (key id's stream field)
  MediaKind media_kind = 2;
  EncodingParameters encoding = 3;
  repeated SendTarget targets = 4;    // empty target set = send nothing
}

message SendTarget {
  string media_handler_url = 1;
  TransportMode transport_mode = 2;   // NOT priority group — see below
}

enum TransportMode {
  TRANSPORT_MODE_UNSPECIFIED = 0;     // fail-closed: MC MUST assign
  TRANSPORT_MODE_DATAGRAM = 1;
  TRANSPORT_MODE_STREAM_PER_GROUP = 2;
}
```

**Different encodings are different streams**, not one stream with divergent
per-target encodings. That is what keeps the encrypt-once invariant true by
construction: different encodings mean different plaintext, so there is no single
ciphertext to fan out, and modelling it otherwise would force either
per-destination encryption (nonce reuse) or two counters per frame.

**The directive carries transport mode and NOT priority group.** Priority group is
an MC→MH egress property on the internal registration contract (ADR-0036 §7), and
those groups *bound* how far a self-declared salience signal can promote a
participant. A client able to name its own priority group could promote itself past
that bound — turning "hold the meeting's audio floor" into a one-line client patch.

**`header_version` is meeting-wide and MC-directed**, because it sits in the signed
publisher region: a relay cannot translate between header versions without breaking
verification. MC selects one version for the whole meeting from participants'
declared `supported_header_versions`, applying a server-side allowlist and a
minimum floor. A declaration is an upper bound on what a client can do, never a
lower bound on what the meeting accepts; a client declaring nothing at or above the
floor is rejected at join rather than the meeting being downgraded, and an empty
list is not "accept anything". **Enforcement is not yet allocated to a story
task** (neither task 14 nor task 10 carries the allowlist/floor); it is latent
while only header version 2 exists and is tracked in `docs/TODO.md`. Do not read
it as covered.

#### MeetingKekUpdate (Server → Client)

```protobuf
message MeetingKekUpdate {
  bytes meeting_kek = 1;              // AES-256, exactly 32 bytes
  uint32 kek_generation = 2;          // u16 semantics
}
```

Pushed to every member when MC rotates the KEK (debounced on participant
departure). Defined additively and unused as of this contract revision. Receivers
retain the previous KEK for a bounded window so frames in flight, and frames from
senders that have not yet re-wrapped, still open.

#### ServerMuteRequest (Client → Server)

Renamed from `HostMuteRequest`; same message, same tag, no behaviour change.
ADR-0036 §5 avoids *host mute* as a term because it presumes a role model this
system has not defined. Two distinct things are called muting and the enforcement
point follows from who decided:

| | Decided by | Enforced at | Protects |
|---|---|---|---|
| **Client mute** (`MuteRequest`) | the participant, about themselves | the client, at capture | the user, from the server |
| **Server mute** (`ServerMuteRequest`) | meeting policy, about someone else | MH, at ingress | the meeting, from a patched client |

Under client mute **no media leaves the device**, enforced client-side and never
dependent on the server honouring it. MC keeps the send directive active while a
client reports itself muted, which is what makes unmute instantaneous and keeps
*"MC has not asked you to send"* distinguishable from *"you have muted yourself"*.


#### StreamQualityUpdate (Bidirectional)
```protobuf
message StreamQualityUpdate {
  string stream_id = 1;
  uint32 available_bitrate = 2;
  float packet_loss = 3;
  uint32 rtt_ms = 4;
}
```

## 3. Client ↔ Media Handler

**Transport**: WebTransport (QUIC) for media streams using proprietary protocol

### 3.1 Connection Establishment

1. Client receives `media_handler_url` from the Meeting Controller
2. Client establishes a WebTransport connection to the Media Handler and
   authenticates with its **meeting JWT** (ADR-0020) — validated by
   `MhJwtValidator::validate_meeting_token`
3. Client opens streams for each media stream

> **Superseded 2026-09-01.** This step previously read "receives `media_handler_url`
> and `connection_token`". There is no `connection_token`: the client authenticates
> to the Media Handler with the meeting JWT, not a second credential. The
> client-facing half of that field was removed from `signaling.proto`'s
> `MediaServerInfo` on 2026-04-13 (`reserved 2; reserved "connection_token";`), and
> the server-side half — `internal.proto`'s `RegisterResponse.connection_token` —
> was deleted with the ADR-0036 internal-contract reshape. Corrected in place rather
> than silently edited, because a reader who trusted the old text would have built
> the credential.

### 3.2 Media Protocol

**Frame Format** (Binary):

```
┌─────────────────────────────────────────────────────────┐
│ Version (1 byte)                                        │
├─────────────────────────────────────────────────────────┤
│ Frame Type (1 byte)                                     │
│ 0x00 = Audio, 0x01 = Video Key, 0x02 = Video Delta     │
├─────────────────────────────────────────────────────────┤
│ User ID (8 bytes - participant identifier)             │
├─────────────────────────────────────────────────────────┤
│ Stream ID (4 bytes - subscriber-chosen identifier)     │
├─────────────────────────────────────────────────────────┤
│ Timestamp (8 bytes - microseconds since epoch)         │
├─────────────────────────────────────────────────────────┤
│ Sequence Number (8 bytes)                              │
├─────────────────────────────────────────────────────────┤
│ Payload Length (4 bytes)                               │
├─────────────────────────────────────────────────────────┤
│ Flags (2 bytes)                                        │
│ Bit 0: End of frame                                    │
│ Bit 1: Discardable                                     │
│ Bits 2-15: Reserved                                    │
├─────────────────────────────────────────────────────────┤
│ Reserved (6 bytes)                                     │
├─────────────────────────────────────────────────────────┤
│ Payload (variable length - encrypted with SFrame)      │
└─────────────────────────────────────────────────────────┘

Total header size: 42 bytes
```

**Note**: User ID (8 bytes) identifies the participant, Stream ID (4 bytes) is chosen by the subscriber for local routing.

### 3.3 Flow Control

- Each QUIC stream has independent flow control
- Media Handler sends STREAM_QUALITY_UPDATE messages via Meeting Controller
- Client adjusts encoding parameters based on feedback

## 4. Meeting Controller ↔ Media Handler

**Transport**: Internal gRPC or WebTransport

### 4.1 Register Meeting — the MC→MH control plane

`MediaHandlerService` has **exactly one RPC**, by design. ADR-0036 §8 makes
meeting registration the control plane: it gains *fields* rather than sibling
RPCs. Authoritative shape, with the normative rules on every field:
`proto/dark_tower/internal/v1/internal.proto`.

The request carries the meeting identity, MC's callback endpoint, the complete
forwarding policy as a repeated self-contained `EgressStream` (subscriber slot +
candidate sources + priority group + supersede-on-independent-frame + transport
mode), meeting-level `SelectionRules`, and a `policy_generation` derived from MC's
assignment-output change. The response carries `accepted` (received-and-parsed
only — **not** evidence of application), the **applied** generation, `handler_id`,
`process_start_epoch_ms`, and the echoed `transport_mode`.

> **Superseded 2026-09-01 — three RPCs retired.** This section previously
> specified `RegisterParticipant`/`RegisterParticipantResponse{connection_token}`
> (§4.1), `RouteMediaCommand` + `RoutingOptions{transcode, target_codec,
> target_bitrate, mix_audio}` (§4.2), and `MediaTelemetry{..., jitter_ms}` (§4.3).
> All three are deleted from `internal.proto`, and the text is replaced rather
> than dropped so a reader who relied on it learns why:
>
> - **Route Media** described a transcoding/mixing relay. It cannot exist: the
>   Media Handler holds no keys (ADR-0036 §4) and is type-blind (§7), so it
>   cannot decode media to transcode or mix it.
> - **Register Participant** issued a second client credential parallel to the
>   meeting JWT that actually authenticates. See the §3.1 note above.
> - **Telemetry** carried per-participant and per-stream identity with byte
>   counts and timestamps — the per-stream time-ordered size sequence ADR-0036
>   §11 names as *the voice-activity trace* — and §11 replaces it with Media
>   Handler self-monitoring histograms. Its `jitter_ms` measured an ADR-0011
>   objective the ADR-0036 amendment table strikes as unmeasurable, because the
>   handler forwards and does not buffer.

## 5. Global Controller ↔ Meeting Controller

**Transport**: Internal gRPC

### 5.1 Register Meeting Controller

**Request**:
```protobuf
message RegisterMeetingController {
  string controller_id = 1;
  string region = 2;
  string endpoint = 3;
  ControllerCapacity capacity = 4;
}

message ControllerCapacity {
  uint32 max_meetings = 1;
  uint32 current_meetings = 2;
  uint32 max_participants = 3;
  uint32 current_participants = 4;
}
```

### 5.2 Heartbeat

**Request**:
```protobuf
message Heartbeat {
  string controller_id = 1;
  ControllerCapacity capacity = 2;
  HealthStatus health = 3;
}

enum HealthStatus {
  HEALTHY = 0;
  DEGRADED = 1;
  UNHEALTHY = 2;
}
```

### 5.3 Meeting Assignment

**Request** (Global → Meeting Controller):
```protobuf
message AssignMeeting {
  string meeting_id = 1;
  MeetingConfig config = 2;
}

message MeetingConfig {
  string display_name = 1;
  uint32 max_participants = 2;
  bool enable_e2e_encryption = 3;
  bool recording_enabled = 4;
}
```

## Error Handling

All APIs use standard error responses:

### HTTP/3 Errors

```json
{
  "error": {
    "code": "MEETING_NOT_FOUND",
    "message": "Meeting 550e8400-e29b-41d4-a716-446655440000 does not exist",
    "details": {}
  }
}
```

Common error codes:
- `INVALID_REQUEST` - Malformed request
- `UNAUTHORIZED` - Authentication failed
- `FORBIDDEN` - Authorization failed
- `NOT_FOUND` - Resource not found
- `CONFLICT` - Resource conflict
- `RATE_LIMITED` - Too many requests
- `INTERNAL_ERROR` - Server error

#### Meeting-creation refusal codes (`POST /api/v1/meetings`)

Creation can be refused for three unrelated reasons. Each has its own code, so a
client (or an automated runner) can tell a busy organization from a broken one
without parsing the human-readable `message`:

| Code | Status | Meaning | Caller action |
|------|--------|---------|---------------|
| `ORGANIZATION_MEETING_LIMIT_EXCEEDED` | 403 | The organization is at its concurrent-meeting cap | Retry once a meeting ends; a routine capacity refusal |
| `ORGANIZATION_INACTIVE` | 403 | The organization exists but is deactivated | Do not retry; contact an administrator |
| `ORGANIZATION_NOT_PROVISIONED` | 500 | A valid token names an organization with no record | Do not retry; server-side state fault, operator action required |

`FORBIDDEN` on this endpoint now means role denial **only** — the caller lacks
one of `user`, `admin`, `org_admin`. Before this split it also carried cap
exhaustion.

Note that `INTERNAL_ERROR` remains in use on this endpoint for meeting-code
collision exhaustion and CSPRNG failure; `ORGANIZATION_NOT_PROVISIONED` is
deliberately distinct from it so the two cannot be confused.

### WebTransport/Protobuf Errors

```protobuf
message ErrorMessage {
  ErrorCode code = 1;
  string message = 2;
  map<string, string> details = 3;
}

enum ErrorCode {
  UNKNOWN = 0;
  INVALID_REQUEST = 1;
  UNAUTHORIZED = 2;
  FORBIDDEN = 3;
  NOT_FOUND = 4;
  CONFLICT = 5;
  INTERNAL_ERROR = 6;
  CAPACITY_EXCEEDED = 7;
  STREAM_ERROR = 8;
}
```

## Rate Limiting

All endpoints implement rate limiting:

- Global Controller: 100 req/min per client
- Meeting Controller: 1000 messages/min per participant
- Media Handler: Bandwidth-based limiting

Rate limit headers (HTTP/3):
```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1705411200
```

## Versioning

- HTTP APIs: `/api/v1/...` in URL path
- Protobuf: Version field in each message
- Media Protocol: Version byte in header

Breaking changes require new API version.
