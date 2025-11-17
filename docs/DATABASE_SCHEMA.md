# Dark Tower - Database Schema

This document defines the data models for PostgreSQL (persistent storage) and Redis (ephemeral storage).

## PostgreSQL Schema

### 1. Users Table

Stores user account information.

```sql
CREATE TABLE users (
    user_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255),  -- NULL for OAuth users
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT true,
    metadata JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_created_at ON users(created_at);
```

### 2. OAuth Providers Table

Links users to OAuth providers.

```sql
CREATE TABLE oauth_providers (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    provider VARCHAR(50) NOT NULL,  -- 'google', 'github', etc.
    provider_user_id VARCHAR(255) NOT NULL,
    access_token_encrypted BYTEA,
    refresh_token_encrypted BYTEA,
    token_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(provider, provider_user_id)
);

CREATE INDEX idx_oauth_user_id ON oauth_providers(user_id);
```

### 3. Meetings Table

Stores meeting metadata and configuration.

```sql
CREATE TABLE meetings (
    meeting_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_by_user_id UUID REFERENCES users(user_id) ON DELETE SET NULL,
    display_name VARCHAR(255) NOT NULL,
    meeting_code VARCHAR(20) UNIQUE NOT NULL,  -- Short human-readable code
    max_participants INTEGER NOT NULL DEFAULT 100,

    -- Settings
    enable_e2e_encryption BOOLEAN NOT NULL DEFAULT true,
    require_auth BOOLEAN NOT NULL DEFAULT false,
    allow_recording BOOLEAN NOT NULL DEFAULT false,
    waiting_room_enabled BOOLEAN NOT NULL DEFAULT false,

    -- State
    status VARCHAR(20) NOT NULL DEFAULT 'scheduled',  -- 'scheduled', 'active', 'ended'
    scheduled_start_time TIMESTAMPTZ,
    actual_start_time TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,

    -- Assignment
    assigned_controller_id VARCHAR(255),  -- Which meeting controller is handling this
    assigned_region VARCHAR(50),

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX idx_meetings_created_by ON meetings(created_by_user_id);
CREATE INDEX idx_meetings_meeting_code ON meetings(meeting_code);
CREATE INDEX idx_meetings_status ON meetings(status);
CREATE INDEX idx_meetings_scheduled_start ON meetings(scheduled_start_time);
CREATE INDEX idx_meetings_controller ON meetings(assigned_controller_id);
```

### 4. Meeting Participants Table

Tracks who joined which meetings.

```sql
CREATE TABLE meeting_participants (
    id BIGSERIAL PRIMARY KEY,
    meeting_id UUID NOT NULL REFERENCES meetings(meeting_id) ON DELETE CASCADE,
    participant_id UUID NOT NULL,  -- Generated session ID
    user_id UUID REFERENCES users(user_id) ON DELETE SET NULL,  -- NULL for guests
    display_name VARCHAR(255) NOT NULL,

    -- Session info
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    leave_reason VARCHAR(50),  -- 'voluntary', 'kicked', 'connection_lost', 'meeting_ended'

    -- Connection details
    ip_address INET,
    user_agent TEXT,
    client_version VARCHAR(50),

    -- Stats (updated periodically)
    total_duration_seconds INTEGER DEFAULT 0,

    UNIQUE(meeting_id, participant_id)
);

CREATE INDEX idx_participants_meeting_id ON meeting_participants(meeting_id);
CREATE INDEX idx_participants_user_id ON meeting_participants(user_id);
CREATE INDEX idx_participants_joined_at ON meeting_participants(joined_at);
```

### 5. Recordings Table

Metadata for meeting recordings.

```sql
CREATE TABLE recordings (
    recording_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meeting_id UUID NOT NULL REFERENCES meetings(meeting_id) ON DELETE CASCADE,
    started_by_user_id UUID REFERENCES users(user_id) ON DELETE SET NULL,

    -- Recording info
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    duration_seconds INTEGER,
    file_size_bytes BIGINT,

    -- Storage
    storage_path TEXT NOT NULL,  -- S3/object storage path
    storage_bucket VARCHAR(255) NOT NULL,

    -- Format
    video_codec VARCHAR(50),
    audio_codec VARCHAR(50),
    resolution VARCHAR(20),  -- e.g., "1920x1080"

    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'recording',  -- 'recording', 'processing', 'ready', 'failed'
    error_message TEXT,

    -- Access
    is_public BOOLEAN NOT NULL DEFAULT false,
    access_token VARCHAR(255),  -- For sharing

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_recordings_meeting_id ON recordings(meeting_id);
CREATE INDEX idx_recordings_started_by ON recordings(started_by_user_id);
CREATE INDEX idx_recordings_status ON recordings(status);
```

### 6. Meeting Controllers Table

Tracks registered meeting controllers.

```sql
CREATE TABLE meeting_controllers (
    controller_id VARCHAR(255) PRIMARY KEY,
    region VARCHAR(50) NOT NULL,
    endpoint TEXT NOT NULL,

    -- Capacity
    max_meetings INTEGER NOT NULL,
    current_meetings INTEGER NOT NULL DEFAULT 0,
    max_participants INTEGER NOT NULL,
    current_participants INTEGER NOT NULL DEFAULT 0,

    -- Health
    health_status VARCHAR(20) NOT NULL DEFAULT 'healthy',  -- 'healthy', 'degraded', 'unhealthy'
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Metadata
    version VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX idx_controllers_region ON meeting_controllers(region);
CREATE INDEX idx_controllers_health ON meeting_controllers(health_status);
CREATE INDEX idx_controllers_heartbeat ON meeting_controllers(last_heartbeat_at);
```

### 7. Media Handlers Table

Tracks registered media handlers.

```sql
CREATE TABLE media_handlers (
    handler_id VARCHAR(255) PRIMARY KEY,
    region VARCHAR(50) NOT NULL,
    endpoint TEXT NOT NULL,

    -- Capacity
    max_streams INTEGER NOT NULL,
    current_streams INTEGER NOT NULL DEFAULT 0,
    max_bitrate_mbps INTEGER NOT NULL,
    current_bitrate_mbps INTEGER NOT NULL DEFAULT 0,

    -- Health
    health_status VARCHAR(20) NOT NULL DEFAULT 'healthy',
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Capabilities
    supports_transcoding BOOLEAN NOT NULL DEFAULT true,
    supports_mixing BOOLEAN NOT NULL DEFAULT true,
    supported_codecs TEXT[] DEFAULT ARRAY['VP9', 'AV1', 'H264', 'Opus'],

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX idx_handlers_region ON media_handlers(region);
CREATE INDEX idx_handlers_health ON media_handlers(health_status);
```

### 8. Audit Logs Table

Comprehensive audit trail.

```sql
CREATE TABLE audit_logs (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(50) NOT NULL,  -- 'user_login', 'meeting_created', 'participant_joined', etc.
    actor_user_id UUID REFERENCES users(user_id) ON DELETE SET NULL,
    actor_type VARCHAR(50) NOT NULL,  -- 'user', 'system', 'api'

    -- Target
    target_type VARCHAR(50),  -- 'meeting', 'user', 'recording', etc.
    target_id VARCHAR(255),

    -- Details
    action VARCHAR(100) NOT NULL,
    result VARCHAR(20) NOT NULL,  -- 'success', 'failure'
    error_message TEXT,

    -- Context
    ip_address INET,
    user_agent TEXT,
    metadata JSONB DEFAULT '{}'::jsonb,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_event_type ON audit_logs(event_type);
CREATE INDEX idx_audit_actor ON audit_logs(actor_user_id);
CREATE INDEX idx_audit_target ON audit_logs(target_type, target_id);
CREATE INDEX idx_audit_created_at ON audit_logs(created_at);

-- Partition by month for better performance
CREATE TABLE audit_logs_y2025m01 PARTITION OF audit_logs
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');
-- Add more partitions as needed
```

### 9. API Keys Table

For programmatic access.

```sql
CREATE TABLE api_keys (
    key_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    key_hash VARCHAR(255) NOT NULL UNIQUE,  -- Hashed API key
    key_prefix VARCHAR(20) NOT NULL,  -- First few chars for identification

    name VARCHAR(255) NOT NULL,
    scopes TEXT[] NOT NULL,  -- ['meetings:create', 'meetings:read', etc.]

    is_active BOOLEAN NOT NULL DEFAULT true,
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash);
```

---

## Redis Data Structures

### 1. Active Meeting State

**Key Pattern**: `meeting:{meeting_id}`

**Type**: Hash

**Fields**:
```
{
  "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
  "display_name": "Team Standup",
  "status": "active",
  "participant_count": 5,
  "started_at": "1705411200",
  "controller_id": "mc-us-west-1-001",
  "media_handler_id": "mh-us-west-1-003"
}
```

**TTL**: Expires 24 hours after meeting ends

---

### 2. Participant Sessions

**Key Pattern**: `participant:{participant_id}`

**Type**: Hash

**Fields**:
```
{
  "participant_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
  "user_id": "123e4567-e89b-12d3-a456-426614174000",
  "display_name": "Alice",
  "joined_at": "1705411200",
  "connection_state": "connected",
  "last_heartbeat": "1705414800",
  "streams": "[\"stream1\", \"stream2\"]"
}
```

**TTL**: 5 minutes (refreshed on heartbeat)

---

### 3. Meeting Participants Set

**Key Pattern**: `meeting:{meeting_id}:participants`

**Type**: Set

**Values**: List of participant IDs

```
SADD meeting:550e8400-e29b-41d4-a716-446655440000:participants f47ac10b-58cc-4372-a567-0e02b2c3d479
```

**TTL**: Same as meeting

---

### 4. Media Stream Metadata

**Key Pattern**: `stream:{stream_id}`

**Type**: Hash

**Fields**:
```
{
  "stream_id": "b8f3c5a2-1234-5678-9abc-def012345678",
  "participant_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
  "stream_type": "video_camera",
  "codec": "VP9",
  "max_bitrate": "2500000",
  "subscribers": "3",
  "created_at": "1705411200"
}
```

**TTL**: 1 hour after stream ends

---

### 5. Participant Subscriptions

**Key Pattern**: `participant:{participant_id}:subscriptions`

**Type**: Set

**Values**: Stream IDs this participant is subscribed to

```
SADD participant:f47ac10b-58cc-4372-a567-0e02b2c3d479:subscriptions stream1 stream2
```

---

### 6. Rate Limiting

**Key Pattern**: `ratelimit:{endpoint}:{identifier}:{window}`

**Type**: String (counter)

**Example**:
```
ratelimit:api:meetings:create:user:123:1705411200
```

**Value**: Number of requests in this window

**TTL**: 60 seconds (for per-minute limits)

---

### 7. Presence/Heartbeat

**Key Pattern**: `presence:{participant_id}`

**Type**: String (timestamp)

**Value**: Last heartbeat timestamp

**TTL**: 30 seconds (if no heartbeat, considered disconnected)

---

### 8. Meeting Controller Capacity

**Key Pattern**: `controller:{controller_id}:capacity`

**Type**: Hash

**Fields**:
```
{
  "max_meetings": "1000",
  "current_meetings": "245",
  "max_participants": "10000",
  "current_participants": "3456",
  "health": "healthy",
  "last_update": "1705414800"
}
```

**TTL**: 5 minutes (refreshed on heartbeat)

---

### 9. JWT Token Blacklist

**Key Pattern**: `token:blacklist:{token_hash}`

**Type**: String (reason)

**Value**: Reason for blacklisting

**TTL**: Until token expiration

---

### 10. WebTransport Connection Tokens

**Key Pattern**: `wt:token:{token}`

**Type**: Hash

**Fields**:
```
{
  "participant_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "meeting_id": "550e8400-e29b-41d4-a716-446655440000",
  "created_at": "1705411200",
  "connection_type": "media"
}
```

**TTL**: 5 minutes (one-time use for connection establishment)

---

### 11. Cache Layer

**Key Pattern**: `cache:{entity_type}:{entity_id}`

**Type**: String (JSON)

**Example**:
```
cache:meeting:550e8400-e29b-41d4-a716-446655440000
```

**Value**: Serialized entity data from PostgreSQL

**TTL**: 5-60 minutes depending on entity type

---

## Data Flow Examples

### Example 1: User Joins Meeting

1. **Client** → **Global Controller**: GET `/api/v1/meetings/{meeting_id}`
   - Global Controller queries PostgreSQL `meetings` table
   - Returns meeting info including `meeting_controller_url`

2. **Client** → **Meeting Controller**: WebTransport connection with `JoinRequest`
   - Meeting Controller validates token (JWT)
   - Creates entry in Redis: `participant:{participant_id}`
   - Adds to Redis set: `meeting:{meeting_id}:participants`
   - Updates Redis hash: `meeting:{meeting_id}` (increment participant_count)
   - Inserts into PostgreSQL: `meeting_participants` table (async)

3. **Meeting Controller** → **Client**: `JoinResponse` with existing participants
   - Fetches from Redis: `meeting:{meeting_id}:participants`
   - For each participant, fetches from Redis: `participant:{participant_id}`

### Example 2: Meeting Ends

1. **Meeting Controller**: Detects last participant left
   - Updates PostgreSQL: `meetings` table (status = 'ended', ended_at = NOW())
   - Updates Redis: `meeting:{meeting_id}` (status = 'ended')
   - Sets TTL on Redis keys for cleanup

2. **Async Worker**: Processes completed meeting
   - Aggregates statistics from `meeting_participants`
   - Updates any analytics tables
   - Triggers cleanup of old data

---

## Migration Strategy

### Initial Setup

```sql
-- migrations/001_initial_schema.sql
-- Contains all CREATE TABLE statements above

-- migrations/002_add_indexes.sql
-- Contains all CREATE INDEX statements

-- migrations/003_add_triggers.sql
-- Triggers for updated_at timestamps, etc.
```

### Future Migrations

- Use a migration tool like `sqlx` or `diesel` for Rust
- Version control all schema changes
- Support rollback for failed migrations
- Test migrations in staging before production

---

## Data Retention

| Entity | Retention Period | Cleanup Method |
|--------|-----------------|----------------|
| Users | Indefinite | Manual deletion or GDPR request |
| Meetings | 90 days after end | Batch job |
| Meeting Participants | 90 days | Cascade delete with meeting |
| Recordings | Configurable (default: 1 year) | Batch job |
| Audit Logs | 1 year | Drop old partitions |
| API Keys | Until revoked/expired | Manual or automated |
| Redis Session Data | Real-time TTL | Automatic Redis expiration |
| Redis Cache | 5-60 minutes | Automatic Redis expiration |
