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
  "meeting_controller_url": "wss://us-west-1.darktower.example.com/ws",
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
  "meeting_controller_url": "wss://us-west-1.darktower.example.com/ws"
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
}

message ParticipantCapabilities {
  repeated string video_codecs = 1;  // e.g., ["VP9", "AV1", "H264"]
  repeated string audio_codecs = 2;  // e.g., ["Opus", "AAC"]
  bool supports_simulcast = 3;
  uint32 max_video_streams = 4;
}
```

#### JoinResponse
```protobuf
message JoinResponse {
  string participant_id = 1;
  repeated Participant existing_participants = 2;
  MediaServerInfo media_server = 3;
  EncryptionKeys encryption_keys = 4;
}

message Participant {
  string participant_id = 1;
  string name = 2;
  repeated MediaStream streams = 3;
  uint64 joined_at = 4;
}

message MediaServerInfo {
  string media_handler_url = 1;
  string connection_token = 2;
}

message EncryptionKeys {
  bytes public_key = 1;
  string key_id = 2;
}
```

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
  VOLUNTARY = 0;
  KICKED = 1;
  CONNECTION_LOST = 2;
  MEETING_ENDED = 3;
}
```

#### PublishStream (Client → Server)
```protobuf
message PublishStream {
  string stream_id = 1;
  StreamType stream_type = 2;
  StreamMetadata metadata = 3;
}

enum StreamType {
  AUDIO = 0;
  VIDEO_CAMERA = 1;
  VIDEO_SCREEN = 2;
}

message StreamMetadata {
  string codec = 1;
  uint32 max_bitrate = 2;
  VideoMetadata video = 3;  // Only for video streams
}

message VideoMetadata {
  uint32 width = 1;
  uint32 height = 2;
  uint32 framerate = 3;
  repeated SimulcastLayer simulcast_layers = 4;
}

message SimulcastLayer {
  string layer_id = 1;
  uint32 width = 2;
  uint32 height = 3;
  uint32 max_bitrate = 4;
}
```

#### StreamPublished (Server → Client)
```protobuf
message StreamPublished {
  string participant_id = 1;
  MediaStream stream = 2;
}

message MediaStream {
  string stream_id = 1;
  StreamType stream_type = 2;
  StreamMetadata metadata = 3;
}
```

#### SubscribeStream (Client → Server)
```protobuf
message SubscribeStream {
  string stream_id = 1;
  SubscriptionOptions options = 2;
}

message SubscriptionOptions {
  string preferred_layer = 1;  // For simulcast
  uint32 max_bitrate = 2;
}
```

#### UnsubscribeStream (Client → Server)
```protobuf
message UnsubscribeStream {
  string stream_id = 1;
}
```

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

1. Client receives `media_handler_url` and `connection_token` from Meeting Controller
2. Client establishes WebTransport connection to Media Handler
3. Client opens bidirectional streams for each media stream

### 3.2 Media Protocol

**Frame Format** (Binary):

```
┌─────────────────────────────────────────────────────────┐
│ Version (1 byte)                                        │
├─────────────────────────────────────────────────────────┤
│ Frame Type (1 byte)                                     │
│ 0x00 = Audio, 0x01 = Video Key, 0x02 = Video Delta     │
├─────────────────────────────────────────────────────────┤
│ Stream ID (16 bytes - UUID)                            │
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
│ Payload (variable length)                              │
└─────────────────────────────────────────────────────────┘

Total header size: 46 bytes
```

### 3.3 Flow Control

- Each QUIC stream has independent flow control
- Media Handler sends STREAM_QUALITY_UPDATE messages via Meeting Controller
- Client adjusts encoding parameters based on feedback

## 4. Meeting Controller ↔ Media Handler

**Transport**: Internal gRPC or WebTransport

### 4.1 Register Participant

**Request**:
```protobuf
message RegisterParticipant {
  string participant_id = 1;
  string meeting_id = 2;
  repeated MediaStream streams = 3;
}
```

**Response**:
```protobuf
message RegisterParticipantResponse {
  string connection_token = 1;
  string media_handler_url = 2;
}
```

### 4.2 Route Media

**Command**:
```protobuf
message RouteMediaCommand {
  string source_stream_id = 1;
  repeated string destination_participant_ids = 2;
  RoutingOptions options = 3;
}

message RoutingOptions {
  bool transcode = 1;
  string target_codec = 2;
  uint32 target_bitrate = 3;
  bool mix_audio = 4;  // Mix multiple audio streams
}
```

### 4.3 Telemetry (Media Handler → Meeting Controller)

**Stream**:
```protobuf
message MediaTelemetry {
  string stream_id = 1;
  uint64 bytes_sent = 2;
  uint64 bytes_received = 3;
  float packet_loss = 4;
  uint32 bitrate = 5;
  uint32 jitter_ms = 6;
  uint64 timestamp = 7;
}
```

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
