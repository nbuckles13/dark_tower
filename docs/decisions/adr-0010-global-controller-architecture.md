# ADR-0010: Global Controller Architecture

**Status**: Accepted
**Date**: 2025-12-04
**Deciders**: Multi-agent debate (GC, MC, AC, Database, Test, Security specialists)
**Debate Rounds**: 3
**Final Consensus**: 93.3% average satisfaction

## Context

Dark Tower requires a Global Controller (GC) to serve as the HTTP/3 API gateway for meeting management, MC assignment, and multi-tenancy. The GC must:

1. Assign users joining the same meeting to the same Meeting Controller (MC)
2. Coordinate with MCs across multiple regions
3. Remain stateless (recoverable from scratch)
4. Handle scale, latency, security, and operability requirements

### Key Problems Identified

**Problem 1: Intra-Region Meeting-to-MC Mapping**
- Multiple GC instances in a region may receive join requests for the same meeting simultaneously
- Need consistent assignment to prevent meeting fragmentation

**Problem 2: Inter-Region MC Discovery**
- Users in different regions joining the same meeting need their local MCs to discover each other
- MCs in different regions must establish signaling connections

## Decision

### 1. Meeting Controller Assignment Flow

When GC receives a meeting join request, it must handle both new meetings and stale assignments (where the previously assigned MC is now unhealthy):

```rust
async fn handle_meeting_join(meeting_id: &str, region: &str, gc_id: &str) -> Result<McAssignment> {
    // Step 1: Check for existing HEALTHY assignment
    let existing = sqlx::query!(
        r#"
        SELECT ma.meeting_controller_id, mc.grpc_endpoint, mc.webtransport_endpoint
        FROM meeting_assignments ma
        JOIN meeting_controllers mc ON ma.meeting_controller_id = mc.id
        WHERE ma.meeting_id = $1
          AND ma.region = $2
          AND ma.ended_at IS NULL
          AND mc.status = 'healthy'
          AND mc.last_heartbeat > NOW() - INTERVAL '30 seconds'
        "#,
        meeting_id, region
    ).fetch_optional(&pool).await?;

    if let Some(row) = existing {
        // Found healthy assignment, use it
        return Ok(McAssignment {
            mc_id: row.meeting_controller_id,
            grpc_endpoint: row.grpc_endpoint,
            webtransport_endpoint: row.webtransport_endpoint,
        });
    }

    // Step 2: No healthy assignment. Select candidate MC via load balancing.
    let candidates = sqlx::query!(
        r#"
        SELECT id, grpc_endpoint, webtransport_endpoint,
               (current_meetings::float / NULLIF(max_meetings, 0)) AS load_ratio
        FROM meeting_controllers
        WHERE status = 'healthy'
          AND region = $1
          AND current_meetings < max_meetings
          AND last_heartbeat > NOW() - INTERVAL '30 seconds'
        ORDER BY load_ratio ASC, last_heartbeat DESC
        LIMIT 5
        "#,
        region
    ).fetch_all(&pool).await?;

    if candidates.is_empty() {
        return Err(Error::NoHealthyMcAvailable);
    }

    let selected_mc = weighted_random_select(&candidates);

    // Step 3: Atomic operation - end any unhealthy assignment AND insert new one
    // This prevents race conditions when multiple GCs detect unhealthy MC simultaneously
    let assignment = sqlx::query!(
        r#"
        WITH ended AS (
            UPDATE meeting_assignments
            SET ended_at = NOW()
            WHERE meeting_id = $1
              AND region = $2
              AND ended_at IS NULL
              AND meeting_controller_id IN (
                  SELECT id FROM meeting_controllers
                  WHERE status != 'healthy'
                     OR last_heartbeat < NOW() - INTERVAL '30 seconds'
              )
            RETURNING meeting_id
        ),
        inserted AS (
            INSERT INTO meeting_assignments (meeting_id, meeting_controller_id, region, assigned_by_gc_id)
            SELECT $1, $3, $2, $4
            WHERE NOT EXISTS (
                -- Only insert if no healthy assignment exists
                SELECT 1 FROM meeting_assignments ma
                JOIN meeting_controllers mc ON ma.meeting_controller_id = mc.id
                WHERE ma.meeting_id = $1
                  AND ma.region = $2
                  AND ma.ended_at IS NULL
                  AND mc.status = 'healthy'
                  AND mc.last_heartbeat > NOW() - INTERVAL '30 seconds'
            )
            ON CONFLICT (meeting_id, region) DO NOTHING
            RETURNING meeting_controller_id
        )
        SELECT meeting_controller_id FROM inserted
        "#,
        meeting_id, region, selected_mc.id, gc_id
    ).fetch_optional(&pool).await?;

    match assignment {
        Some(row) => {
            // We won the race - write to outbox for cross-region discovery
            sqlx::query!(
                "INSERT INTO meeting_peer_events_outbox
                 (meeting_id, mc_id, region, event_type, grpc_endpoint, org_id)
                 VALUES ($1, $2, $3, 'peer_joined', $4, $5)",
                meeting_id, selected_mc.id, region, selected_mc.grpc_endpoint, org_id
            ).execute(&pool).await?;

            Ok(McAssignment {
                mc_id: selected_mc.id,
                grpc_endpoint: selected_mc.grpc_endpoint,
                webtransport_endpoint: selected_mc.webtransport_endpoint,
            })
        }
        None => {
            // Another GC won the race - re-query to get their assignment
            let winner = sqlx::query!(
                r#"
                SELECT ma.meeting_controller_id, mc.grpc_endpoint, mc.webtransport_endpoint
                FROM meeting_assignments ma
                JOIN meeting_controllers mc ON ma.meeting_controller_id = mc.id
                WHERE ma.meeting_id = $1
                  AND ma.region = $2
                  AND ma.ended_at IS NULL
                "#,
                meeting_id, region
            ).fetch_optional(&pool).await?;

            winner.map(|r| McAssignment {
                mc_id: r.meeting_controller_id,
                grpc_endpoint: r.grpc_endpoint,
                webtransport_endpoint: r.webtransport_endpoint,
            }).ok_or(Error::AssignmentRaceConditionFailed)
        }
    }
}
```

**Key design points**:
- Step 1 only returns assignments with **healthy** MCs (status + recent heartbeat)
- Step 3 uses a CTE to atomically end unhealthy assignments AND insert the new one
- The `NOT EXISTS` check prevents inserting if another GC already assigned a healthy MC
- Race conditions between GCs are handled by `ON CONFLICT DO NOTHING` + re-query
- Outbox write enables cross-region discovery (see Section 5)

### 2. MC Load Balancing Algorithm

GC selects an MC using **weighted round-robin with capacity scoring**:

```sql
SELECT
    mc.id,
    mc.endpoint,
    mc.region,
    -- Capacity score: lower is better (0.0 = empty, 1.0 = full)
    (mc.current_meetings::float / NULLIF(mc.max_meetings, 0)) AS load_ratio
FROM meeting_controllers mc
WHERE mc.status = 'healthy'
  AND mc.region = $1  -- prefer same region as requesting user
  AND mc.current_meetings < mc.max_meetings
  AND mc.last_heartbeat > NOW() - INTERVAL '30 seconds'
ORDER BY
    load_ratio ASC,           -- least loaded first
    mc.last_heartbeat DESC    -- most recently healthy as tiebreaker
LIMIT 5;
```

**Selection from top 5**: Weighted random selection where probability is inversely proportional to `load_ratio`. This prevents thundering herd to a single MC while still preferring less-loaded instances.

```rust
fn select_mc(candidates: &[McCandidate]) -> &McCandidate {
    // Weight = 1.0 - load_ratio (so 0% loaded = weight 1.0, 90% loaded = weight 0.1)
    let weights: Vec<f64> = candidates.iter()
        .map(|mc| 1.0 - mc.load_ratio.min(0.99))
        .collect();

    // Weighted random selection
    let total: f64 = weights.iter().sum();
    let mut rng = rand::thread_rng();
    let mut choice = rng.gen::<f64>() * total;

    for (i, weight) in weights.iter().enumerate() {
        choice -= weight;
        if choice <= 0.0 {
            return &candidates[i];
        }
    }
    &candidates[0]
}
```

### 3. Meeting Assignment Cleanup

**When are `meeting_assignments` rows removed?**

Assignments are marked ended (soft delete) when:

1. **Meeting ends normally**: Last participant leaves, MC notifies GC
2. **Meeting timeout**: No participants for 1 hour (background job)
3. **MC failure**: MC marked unhealthy, assignments migrated or ended

```sql
-- Soft delete: mark ended_at instead of DELETE
UPDATE meeting_assignments
SET ended_at = NOW()
WHERE meeting_id = $1 AND ended_at IS NULL;

-- Hard delete: background job removes old assignments (configurable, default 7 days)
DELETE FROM meeting_assignments
WHERE ended_at < NOW() - INTERVAL '7 days';
```

**Why soft delete?**: Preserves audit trail for debugging. Hard delete via configurable background job (default 7 days, shorter for dev/staging) keeps table size manageable.

### 4. GC-MC Communication Architecture

**Connection Model**: Each MC maintains a persistent gRPC connection to a **GC service endpoint** (not individual GC instances).

```
┌─────────────────────────────────────────────────────────────┐
│                        Region: us-west-1                     │
│                                                              │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐                      │
│  │  GC-1   │  │  GC-2   │  │  GC-3   │  (stateless)         │
│  └────┬────┘  └────┬────┘  └────┬────┘                      │
│       │            │            │                            │
│       └────────────┼────────────┘                            │
│                    │                                         │
│            ┌───────┴───────┐                                │
│            │ Load Balancer │  gc-internal.us-west-1.dark.io │
│            └───────┬───────┘                                │
│                    │                                         │
│       ┌────────────┼────────────┐                           │
│       │            │            │                            │
│  ┌────┴────┐  ┌────┴────┐  ┌────┴────┐                      │
│  │  MC-1   │  │  MC-2   │  │  MC-3   │  (stateful)          │
│  └─────────┘  └─────────┘  └─────────┘                      │
└─────────────────────────────────────────────────────────────┘
```

**MC discovers GC endpoint via**:
- Environment variable: `GC_GRPC_ENDPOINT=gc-internal.us-west-1.dark.io:50051`
- Or service discovery (Kubernetes DNS, Consul, etc.)

**Authentication layers**:
1. **mTLS**: Mutual TLS authenticates service identity (MC cert → GC, GC cert → MC)
2. **Service token**: AC-issued JWT in gRPC metadata for authorization

```rust
// MC connecting to GC
let channel = Channel::from_static("https://gc-internal.us-west-1.dark.io:50051")
    .tls_config(mtls_config)?
    .connect()
    .await?;

// Add service token to each request
let mut client = GlobalControllerServiceClient::with_interceptor(
    channel,
    |mut req: Request<()>| {
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", service_token).parse().unwrap(),
        );
        Ok(req)
    },
);
```

### 4a. Meeting Assignment Notification (GC → MC)

When GC assigns a meeting to an MC, it proactively notifies the MC rather than waiting for client connection. This allows MC to reject assignments when at capacity (backpressure) and to receive MH assignments from GC.

**Assignment Flow**:
```
T=0    Client → GC: JoinMeeting { meeting_id }

T=1    GC: Selects MC-1 (load balancing per Section 2)
       GC: Selects MH-A, MH-B for this region (per ADR-0023 Section 5)

T=2    GC → MC-1: AssignMeeting {
           meeting_id,
           mh_assignments: [MH-A, MH-B],
           requesting_gc_id
       }

T=3    MC-1: Accepts or rejects

T=4    If accepted:
           GC records assignment (Section 1 atomic INSERT)
           GC → Client: JoinResponse { mc_endpoint }
           Client connects to MC-1

       If rejected:
           GC selects MC-2, repeats from T=2
           (Max 3 retries before returning 503)
```

**Protocol**:
```protobuf
service McAssignment {
    rpc AssignMeeting(AssignMeetingRequest) returns (AssignMeetingResponse);
    rpc RequestMhReplacement(MhReplacementRequest) returns (MhReplacementResponse);
}

message AssignMeetingRequest {
    string meeting_id = 1;
    repeated MhAssignment mh_assignments = 2;
    string requesting_gc_id = 3;
}

message MhAssignment {
    string mh_id = 1;
    string webtransport_endpoint = 2;
    MhRole role = 3;
}

enum MhRole {
    MH_ROLE_UNSPECIFIED = 0;
    MH_ROLE_PRIMARY = 1;
    MH_ROLE_BACKUP = 2;
}

message AssignMeetingResponse {
    bool accepted = 1;
    RejectionReason rejection_reason = 2;  // Only set if accepted=false
}

enum RejectionReason {
    REJECTION_REASON_UNSPECIFIED = 0;
    REJECTION_REASON_AT_CAPACITY = 1;
    REJECTION_REASON_DRAINING = 2;
    REJECTION_REASON_UNHEALTHY = 3;
}

// MC requests replacement when assigned MH rejects or fails
message MhReplacementRequest {
    string meeting_id = 1;
    string failed_mh_id = 2;
    MhReplacementReason reason = 3;
}

enum MhReplacementReason {
    MH_REPLACEMENT_REASON_UNSPECIFIED = 0;
    MH_REPLACEMENT_REASON_CAPACITY_EXCEEDED = 1;
    MH_REPLACEMENT_REASON_UNHEALTHY = 2;
    MH_REPLACEMENT_REASON_CONNECTION_FAILED = 3;
}

message MhReplacementResponse {
    MhAssignment new_mh = 1;
}
```

**Why MC can reject**:
- MC knows its true capacity better than GC's heartbeat-based view (heartbeats can lag)
- Provides backpressure during load spikes
- Prevents overload cascades

**Order of operations**: GC notifies MC BEFORE writing to database. This ensures MC has accepted before the assignment is recorded. If MC rejects, GC selects another MC without creating a failed assignment row.

**MH assignments via GC**: GC assigns MHs alongside MC assignment rather than having MC discover MHs directly. This allows:
- Cross-region MH coordination (MH registry synced via existing GC-to-GC infrastructure)
- Centralized MH load balancing with global visibility
- MC can request MH replacement if assigned MH fails (via `RequestMhReplacement`)

See ADR-0023 Section 5 for detailed MH selection algorithm and cross-region meeting MH coordination.

### 5. Inter-Region MC Discovery

**How do MCs in different regions hosting the same meeting find each other?**

**Answer**: Direct MC-to-Bus architecture with GC as write-only publisher and regional DB isolation.

> **Design Evolution**: Initial proposals used GC-pushed gRPC streaming (complexity concerns with GC tracking MC subscriptions) and assumed cross-region Redis or PostgreSQL (architecturally impossible - both are regional only). The final design uses a **Direct MC-to-Bus** pattern achieving 83.2% consensus.

**Key Principles**:
- **Regional DB isolation** - Each region has its own PostgreSQL; no cross-region DB queries
- **MC subscribes directly to bus** - No GC subscription tracking overhead
- **Read/Write separation** - GC writes to bus, MC reads from bus (enforced via ACLs)
- **Transactional outbox pattern** - Atomic DB write + bus publish consistency
- **Blind cross-region broadcast** - GC notifies all regions via gRPC without querying remote DBs

**Architecture**:
```
┌─────────────────────────────────────────────────────────────────────┐
│                         Region: us-west                              │
│                                                                      │
│  ┌────────────┐         ┌─────────────────┐                         │
│  │ PostgreSQL │◄───────►│  GC (writer)    │                         │
│  │ (regional) │         │                 │                         │
│  └────────────┘         └────────┬────────┘                         │
│        │                         │                                   │
│        │ outbox                  │ publish                          │
│        ▼                         ▼                                   │
│  ┌────────────┐         ┌─────────────────┐                         │
│  │ Publisher  │────────►│ Redis Streams   │◄──────┐                 │
│  │ (bg job)   │         │ (regional)      │       │ subscribe       │
│  └────────────┘         └─────────────────┘       │                 │
│                                                    │                 │
│                          ┌────────────────────────┴───┐             │
│                          │                            │             │
│                     ┌────▼────┐                 ┌────▼────┐         │
│                     │  MC-10  │◄───────────────►│  MC-11  │         │
│                     └─────────┘  direct gRPC    └─────────┘         │
│                          │                                          │
└──────────────────────────┼──────────────────────────────────────────┘
                           │ gRPC (cross-region peer connection)
                           ▼
                     ┌─────────┐
                     │  MC-1   │ (eu-west)
                     └─────────┘
```

**Discovery Flow** (~200ms latency):
```
1. Alice (us-west) joins meeting-123
   → Client → GC-us-west: POST /v1/meetings/meeting-123/join
   → GC-us-west in single transaction:
     a) INSERT INTO meeting_assignments(meeting-123, MC-10, us-west)
     b) INSERT INTO meeting_peer_events_outbox(peer_joined, MC-10, us-west)
   → GC returns: { mc_endpoint: "mc-10.us-west...", peer_mcs: [] }

2. Publisher (us-west) processes outbox:
   → SELECT ... FROM outbox LIMIT 1 FOR UPDATE SKIP LOCKED
   → XADD meeting:meeting-123:events {peer_joined, MC-10, us-west, grpc_endpoint}
   → gRPC broadcast to all other region GCs (eu-west, asia)
   → UPDATE outbox SET published_at = NOW() WHERE id = ...

3. Remote GC (eu-west) receives gRPC notification:
   → Blindly writes to local Redis: XADD meeting:meeting-123:events {...}
   → No DB lookup needed (handles race conditions with late joiners)

4. Bob (eu-west) joins meeting-123 (seconds later)
   → Client → GC-eu-west: POST /v1/meetings/meeting-123/join
   → GC-eu-west in single transaction:
     a) INSERT INTO meeting_assignments(meeting-123, MC-1, eu-west)
     b) INSERT INTO outbox(peer_joined, MC-1, eu-west)
   → GC returns: { mc_endpoint: "mc-1.eu-west..." }

5. MC-1 (eu-west) subscribes to Redis Streams:
   → XREADGROUP meeting:meeting-123:events
   → Sees: [MC-10@us-west] (from step 3 blind write)
   → Initiates gRPC connection to MC-10

6. Publisher (eu-west) processes outbox:
   → Publishes to local Redis
   → gRPC broadcast to us-west, asia

7. MC-10 (us-west) receives from Redis Streams:
   → Sees: [MC-1@eu-west]
   → MC-10 already has connection from MC-1, or initiates its own if none exists
     (The lower-MC-ID rule only applies to deduplication when both connect simultaneously)

8. MC-10 and MC-1 complete handshake
   → Total discovery latency: ~200ms
```

**Outbox Schema** (with backoff and cancellation support):
```sql
CREATE TABLE meeting_peer_events_outbox (
    id BIGSERIAL PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    mc_id TEXT NOT NULL,
    region TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('peer_joined', 'peer_left')),
    grpc_endpoint TEXT NOT NULL,
    org_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,           -- NULL = pending
    cancelled BOOLEAN NOT NULL DEFAULT FALSE,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Fast lookup for pending events ready to process
CREATE INDEX idx_outbox_pending
ON meeting_peer_events_outbox(next_attempt_at)
WHERE published_at IS NULL;
```

**Publisher Implementation** (batched with ack/nack and backoff):
```rust
struct Publisher {
    pool: PgPool,
    redis: RedisClient,
    peer_gcs: Vec<PeerGcClient>,
    cancel_token: CancellationToken,
    batch_size: i64,  // e.g., 100
}

impl Publisher {
    async fn run(&self) -> Result<()> {
        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!("Publisher shutting down gracefully");
                    return Ok(());
                }
                result = self.process_batch() => {
                    if let Err(e) = result {
                        error!("Publisher error: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }

    async fn process_batch(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Grab batch of events ready for processing (respects backoff)
        let events = sqlx::query_as!(
            OutboxEvent,
            "SELECT * FROM meeting_peer_events_outbox
             WHERE published_at IS NULL
               AND next_attempt_at <= NOW()
             ORDER BY next_attempt_at
             LIMIT $1
             FOR UPDATE SKIP LOCKED",
            self.batch_size
        ).fetch_all(&mut *tx).await?;

        if events.is_empty() {
            tx.commit().await?;
            tokio::time::sleep(Duration::from_millis(50)).await;
            return Ok(());
        }

        // 1. Batch write to local Redis (pipeline for efficiency)
        let mut pipe = redis::pipe();
        for event in &events {
            pipe.xadd(
                &format!("meeting:{}:events", event.meeting_id),
                &[
                    ("event_type", &event.event_type),
                    ("mc_id", &event.mc_id),
                    ("region", &event.region),
                    ("grpc_endpoint", &event.grpc_endpoint),
                    ("org_id", &event.org_id),
                ]
            );
        }
        pipe.query_async(&mut self.redis).await?;

        // 2. Single batched gRPC to each remote region (parallel)
        let notify_futures: Vec<_> = self.peer_gcs.iter()
            .filter(|gc| gc.region != self.my_region)
            .map(|gc| {
                let events = events.clone();
                async move {
                    match timeout(
                        Duration::from_secs(10),
                        gc.notify_meeting_events_batch(&events)
                    ).await {
                        Ok(Ok(_)) => Ok(gc.region.clone()),
                        Ok(Err(e)) => Err((gc.region.clone(), e.to_string())),
                        Err(_) => Err((gc.region.clone(), "timeout".to_string())),
                    }
                }
            })
            .collect();

        let results = futures::future::join_all(notify_futures).await;
        let failures: Vec<_> = results.iter().filter_map(|r| r.as_ref().err()).collect();

        if failures.is_empty() {
            // All succeeded - mark batch as published
            let ids: Vec<_> = events.iter().map(|e| e.id).collect();
            sqlx::query!(
                "UPDATE meeting_peer_events_outbox SET published_at = NOW() WHERE id = ANY($1)",
                &ids
            ).execute(&mut *tx).await?;
            tx.commit().await?;
        } else {
            // Some failed - apply exponential backoff (50ms → 100ms → ... → 5s max)
            warn!("Batch notification failed: {:?}", failures);
            let ids: Vec<_> = events.iter().map(|e| e.id).collect();
            sqlx::query!(
                "UPDATE meeting_peer_events_outbox
                 SET attempt_count = attempt_count + 1,
                     next_attempt_at = NOW() + (LEAST(POWER(2, attempt_count) * 50, 5000) || ' milliseconds')::interval
                 WHERE id = ANY($1)",
                &ids
            ).execute(&mut *tx).await?;
            tx.commit().await?;  // Commit the backoff update, release locks
        }

        Ok(())
    }
}
```

**Meeting End Cleanup** (mark outbox rows as cancelled, no lock contention):
```sql
-- When GC learns meeting ended (from MC notification)
UPDATE meeting_peer_events_outbox
SET published_at = NOW(), cancelled = true
WHERE meeting_id = $1 AND published_at IS NULL;
```

**Batched Cross-Region gRPC**:
```protobuf
service GlobalControllerPeer {
    rpc NotifyMeetingEventsBatch(NotifyMeetingEventsBatchRequest)
        returns (NotifyMeetingEventsBatchResponse);
}

message NotifyMeetingEventsBatchRequest {
    repeated MeetingEvent events = 1;
}

message MeetingEvent {
    string meeting_id = 1;
    string event_type = 2;
    string mc_id = 3;
    string region = 4;
    string grpc_endpoint = 5;
    string org_id = 6;
}

message NotifyMeetingEventsBatchResponse {
    bool success = 1;
    string error_message = 2;  // Only set if success=false
}
```

**MC Redis Subscription**:
```rust
// MC subscribes when it starts hosting a meeting
async fn subscribe_to_meeting_events(meeting_id: &str) {
    // Create consumer group if not exists
    redis.xgroup_create_mkstream(
        &format!("meeting:{}:events", meeting_id),
        "mc-consumers",
        "0"  // Read from beginning (catch up on missed events)
    ).await.ok();  // Ignore if already exists

    loop {
        // Read new events (blocking)
        let events = redis.xreadgroup(
            "mc-consumers",
            &format!("mc-{}", my_mc_id),
            &[&format!("meeting:{}:events", meeting_id)],
            ">"  // Only new messages
        ).await?;

        for event in events {
            if event.mc_id != my_mc_id && event.region != my_region {
                // Discovered a peer MC in another region
                self.on_peer_discovered(PeerInfo {
                    mc_id: event.mc_id,
                    region: event.region,
                    grpc_endpoint: event.grpc_endpoint,
                }).await?;
            }
        }
    }
}
```

**Redis ACLs** (enforced read/write separation):
```redis
# GC service: write-only to meeting event streams
ACL SETUSER gc-service on >gc_password ~meeting:*:events +XADD -XREAD -XREADGROUP

# MC service: read-only from meeting event streams
ACL SETUSER mc-service on >mc_password ~meeting:*:events -XADD +XREAD +XREADGROUP +XACK
```

**Credential Rotation** (dual-password pattern):
```redis
# Redis ACL supports multiple passwords per user for zero-downtime rotation:
# 1. Add new password (both old and new now work)
ACL SETUSER gc-service >new_gc_password
ACL SETUSER mc-service >new_mc_password

# 2. Deploy services with new credentials (rolling update)
# 3. Remove old password after all instances updated
ACL SETUSER gc-service <old_gc_password
ACL SETUSER mc-service <old_mc_password

# Verify rotation complete
ACL LIST
```

**Message Retention**:
- Redis Streams retain messages for 24 hours (configurable)
- Late-joining MCs replay from stream start to catch up
- Older messages trimmed automatically: `XTRIM meeting:*:events MAXLEN ~ 10000`

**Why Blind Remote Write**:
```
Problem: If remote GC queries DB before writing to Redis, race condition occurs:

Timeline WITHOUT blind write:
  T=0:    GC-us-west assigns MC-10, writes to outbox
  T=10ms: Publisher broadcasts to GC-eu-west
  T=20ms: GC-eu-west queries DB: "any MCs in eu-west for meeting-123?" → NO
  T=25ms: GC-eu-west decides NOT to write to Redis (no local MC)
  T=30ms: GC-eu-west assigns MC-1 (Bob just joined)
  T=35ms: MC-1 subscribes to Redis Streams → EMPTY (missed MC-10 notification!)

Timeline WITH blind write:
  T=0:    GC-us-west assigns MC-10, writes to outbox
  T=10ms: Publisher broadcasts to GC-eu-west
  T=20ms: GC-eu-west BLINDLY writes to local Redis: "MC-10 joined"
  T=30ms: GC-eu-west assigns MC-1 (Bob just joined)
  T=35ms: MC-1 subscribes to Redis Streams → Sees MC-10! ✓
```

**GC Peer Registry** (static configuration):
```yaml
# Each GC cluster has this config
gc_peer_regions:
  us-west:
    grpc_endpoint: "gc-internal.us-west.dark.io:50051"
  eu-west:
    grpc_endpoint: "gc-internal.eu-west.dark.io:50051"
  asia:
    grpc_endpoint: "gc-internal.asia.dark.io:50051"
```

**Performance Characteristics**:
| Metric | Value |
|--------|-------|
| Discovery latency | ~200ms (local) to ~350ms (cross-region) |
| Outbox throughput | ~6 events/sec at 10K concurrent meetings |
| PostgreSQL load | ~12 queries/sec (trivial for single region) |
| Redis Streams | Supports 100K+ concurrent streams |
| Row lock duration | ~10ms per event (minimal crash impact) |

**Crash Recovery**:
- **Publisher crashes**: Row lock released via TCP keepalive (~25s). Another publisher instance picks up pending events via `FOR UPDATE SKIP LOCKED`.
- **Redis down**: Publisher retries with exponential backoff. Events remain in outbox until Redis recovers.

**MC-to-MC gRPC Protocol** (direct peer communication):
```protobuf
service MeetingControllerPeer {
    rpc PeerHandshake(HandshakeRequest) returns (HandshakeResponse);
    rpc StateSync(stream StateSyncMessage) returns (stream StateSyncMessage);
    rpc PeerHealthCheck(HealthCheckRequest) returns (HealthCheckResponse);
}
```

**Connection Deduplication** (MC-to-MC):
Both MCs initiate connections in parallel for fast establishment. Deterministic rule resolves duplicates.

**Rule**: **Keep the connection initiated by the lower MC ID (lexicographic)**

**Both-Sided Initiation Flow**:
```
T=0ms:  MC-1 learns about MC-10 → INITIATES connection to MC-10
        MC-10 learns about MC-1 → INITIATES connection to MC-1

T=80ms: Both connections established (race condition)
        MC-1 has: outgoing→MC-10, incoming←MC-10
        MC-10 has: outgoing→MC-1, incoming←MC-1

T=81ms: Deduplication runs on both sides independently:
        MC-1:  "MC-1" < "MC-10" → I'm lower → CLOSE incoming, KEEP outgoing
        MC-10: "MC-1" < "MC-10" → I'm higher → CLOSE outgoing, KEEP incoming

T=82ms: Result: Single connection MC-1 → MC-10
```

**Fallback Polling** (safety net):
- Interval: 60 seconds with 0-10s random jitter
- MC queries GC API `GetMeetingPeers()` to discover any missed peers
- Used when: Redis message lost, publisher crashed, stream subscription gap

**Three-Region Example** (Alice@us-west, Bob@eu-west, Carol@asia):
```
T=0:    Alice joins → MC-10@us-west assigned
        Publisher: writes to us-west Redis, gRPC to eu-west & asia
        → eu-west Redis now has: [MC-10]
        → asia Redis now has: [MC-10]

T=45s:  Bob joins → MC-1@eu-west assigned
        MC-1 subscribes → reads [MC-10] from Redis
        MC-1 initiates connection to MC-10 (MC-1 < MC-10)
        Publisher: writes to eu-west Redis, gRPC to us-west & asia
        → us-west Redis now has: [MC-10, MC-1]
        → asia Redis now has: [MC-10, MC-1]

T=46s:  MC-10 reads from Redis → sees MC-1
        Connection already established (MC-1 initiated)

T=120s: Carol joins → MC-20@asia assigned
        MC-20 subscribes → reads [MC-10, MC-1] from Redis
        MC-1 initiates to MC-20 (MC-1 < MC-20)
        MC-10 initiates to MC-20 (MC-10 < MC-20)
        Full mesh: 3 bidirectional connections
```

**Split-Brain Detection** (CRITICAL):
```prometheus
# MC reports known peer regions
mc_meeting_known_regions{meeting_id="...", mc_id="...", region="eu-west"} 1

# Compare against outbox events to detect missed discoveries
alert: MeetingSplitBrain
expr: |
  (count by (meeting_id) (meeting_peer_events_outbox_published) > 1)
  and
  (min by (meeting_id) (mc_meeting_peer_count) == 0)
for: 60s
severity: critical
```

**Security**:
- **mTLS**: All GC-GC, GC-MC, and MC-MC gRPC connections
- **Redis ACLs**: Enforce read/write separation (GC write-only, MC read-only)
- **Service tokens**: Services authenticate via AC-issued JWT
- **No per-meeting authorization**: Message bus contains only metadata (meeting_id, mc_id, region, endpoint)

**Failure Handling**:
- Regional Redis down → Events queue in outbox until recovery
- Cross-region gRPC fails → Remote region uses 60s fallback poll
- Publisher crashes → Row lock releases (~25s), another instance picks up
- MC misses events → Reads from stream start or uses fallback poll

**Discovery SLOs**:
```yaml
# P95 peer discovery latency (notification to handshake complete)
mc_peer_discovery_duration_seconds{quantile="0.95"} < 0.5  # 500ms

# P99.9 peer discovery latency
mc_peer_discovery_duration_seconds{quantile="0.999"} < 5.0  # 5 seconds

# Missing connection time (multi-region meeting without all peers connected)
mc_peer_connection_missing_seconds / meeting_duration_seconds < 0.01  # <1%
```

### 6. JWT Claims Usage

Each claim serves a specific purpose:

| Claim | Purpose | Used By |
|-------|---------|---------|
| `sub` | User/service identity (UUID) | All services for identifying the principal |
| `org_id` | Tenant isolation | GC validates against subdomain; DB queries filter by org_id |
| `jti` | Token ID for revocation | Denylist lookup: `SELECT 1 FROM token_denylist WHERE jti = $1` |
| `aud` | Intended audience | GC rejects if `"gc"` not in audience; MC rejects if `"mc"` not in audience |
| `scope` | Authorization | GC checks `user.read.gc` for GET, `user.write.gc` for POST/DELETE |
| `iat` | Issued-at timestamp | Reject tokens issued too far in future (clock skew attack) |
| `exp` | Expiration | Reject expired tokens |

**Validation flow in GC**:
```rust
fn validate_token(token: &str, expected_audience: &str, subdomain_org_id: &str) -> Result<Claims> {
    // 1. Verify signature against AC's JWKS
    let claims = jwks_validator.verify(token)?;

    // 2. Check expiration
    if claims.exp < now() {
        return Err(TokenExpired);
    }

    // 3. Check issued-at (reject future tokens beyond 5min clock skew)
    if claims.iat > now() + 300 {
        return Err(TokenFromFuture);
    }

    // 4. Check audience
    if !claims.aud.contains(expected_audience) {
        return Err(InvalidAudience);
    }

    // 5. Check org_id matches subdomain
    if claims.org_id != subdomain_org_id {
        return Err(OrgMismatch);
    }

    // 6. Check denylist
    if is_denied(&claims.jti).await? {
        return Err(TokenRevoked);
    }

    // 7. Check scopes (done per-endpoint)
    Ok(claims)
}
```

### 7. Token Denylist

**Renamed from "blacklist" to "denylist"** per security terminology best practices.

**Purpose**: Emergency revocation of tokens before their natural expiration.

**Use cases**:
1. User logs out (revoke their current token)
2. User changes password (revoke all tokens)
3. Service credential compromised (revoke all tokens for that service)
4. Admin force-revokes a user's access

**Schema**:
```sql
CREATE TABLE token_denylist (
    jti TEXT PRIMARY KEY,
    revoked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,  -- Token's original expiry
    reason TEXT NOT NULL,             -- 'logout', 'password_change', 'admin_revoke'
    revoked_by TEXT                   -- user_id or 'system'
);

-- Auto-cleanup: tokens past their expiry can be deleted
CREATE INDEX idx_denylist_expires ON token_denylist(expires_at);
```

**Ownership**:
- **AC writes**: `/v1/admin/tokens/revoke` endpoint
- **GC reads**: Check denylist before accepting user tokens

#### Meeting-Scoped Tokens (MC/MH)

MC and MH do **not** check the token denylist. Instead, they use short-lived **meeting-scoped tokens** that eliminate the need for denylist lookups:

```
1. User joins meeting via GC:
   → GC validates user JWT, checks denylist
   → GC requests meeting token from AC: POST /v1/internal/meeting-tokens
      { user_id: "...", meeting_id: "...", scopes: ["meeting.participant"] }
   → AC issues short-lived meeting token (5-15 min)
   → GC returns meeting token + MC endpoint to client

2. Client connects to MC:
   → WebTransport connection to MC with meeting token
   → MC validates signature via JWKS (no denylist check)
   → MC extracts: user_id, meeting_id, scopes from token

3. Token refresh (seamless):
   → MC monitors token expiry, sends signaling message: TOKEN_EXPIRING_SOON
   → Client requests new meeting token from GC (with original user JWT)
   → GC validates user JWT, checks denylist, issues new meeting token
   → Client sends signaling message: UPDATE_TOKEN { new_token: "..." }
   → MC validates new token, continues session

4. Revocation propagates via refresh:
   → Admin revokes user's JWT (adds to denylist)
   → User's next refresh attempt fails at GC (denylist hit)
   → MC never learns about revocation directly
   → User disconnected when meeting token expires (max 15 min)
```

**Why no denylist at MC/MH**:
- Meeting tokens are short-lived (5-15 min) - revocation propagates via refresh
- MC/MH are latency-sensitive - denylist lookup adds ~1-2ms per media packet
- Simpler architecture - MC/MH only need JWKS, not database access
- Reduced blast radius - compromised meeting token only works for that meeting

**Performance optimization**: Cache denylist in Redis with short TTL (60s).

```rust
async fn is_denied(jti: &str) -> Result<bool> {
    // 1. Check Redis cache first
    if let Some(cached) = redis.get(&format!("denylist:{}", jti)).await? {
        return Ok(cached == "1");
    }

    // 2. Check database
    let denied = sqlx::query!(
        "SELECT 1 FROM token_denylist WHERE jti = $1 AND expires_at > NOW()",
        jti
    )
    .fetch_optional(&pool)
    .await?
    .is_some();

    // 3. Cache result (negative cache too, to avoid repeated DB hits)
    redis.setex(&format!("denylist:{}", jti), 60, if denied { "1" } else { "0" }).await?;

    Ok(denied)
}
```

### 8. Database Schema

```sql
-- Meeting Controller registry
CREATE TABLE meeting_controllers (
    id TEXT PRIMARY KEY,
    region TEXT NOT NULL,
    grpc_endpoint TEXT NOT NULL,
    webtransport_endpoint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'healthy', 'unhealthy', 'draining')),
    current_meetings INTEGER NOT NULL DEFAULT 0,
    max_meetings INTEGER NOT NULL DEFAULT 1000,
    last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_mc_healthy ON meeting_controllers(region, status, current_meetings)
WHERE status = 'healthy';

-- Meeting-to-MC assignments (atomic via UNIQUE constraint)
CREATE TABLE meeting_assignments (
    meeting_id TEXT PRIMARY KEY,
    meeting_controller_id TEXT NOT NULL REFERENCES meeting_controllers(id),
    region TEXT NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    assigned_by_gc_id TEXT NOT NULL,
    ended_at TIMESTAMPTZ  -- NULL = active, set when meeting ends
);

CREATE INDEX idx_assignments_by_mc ON meeting_assignments(meeting_controller_id)
WHERE ended_at IS NULL;

CREATE INDEX idx_assignments_by_region ON meeting_assignments(meeting_id, region)
WHERE ended_at IS NULL;

-- State transitions for debugging
CREATE TABLE meeting_state_transitions (
    id BIGSERIAL PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    old_state TEXT,
    new_state TEXT NOT NULL,
    triggered_by TEXT NOT NULL,
    reason TEXT,
    transitioned_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_transitions_by_meeting ON meeting_state_transitions(meeting_id, transitioned_at DESC);

-- Token denylist (written by AC, read by all)
CREATE TABLE token_denylist (
    jti TEXT PRIMARY KEY,
    revoked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL,
    revoked_by TEXT
);

CREATE INDEX idx_denylist_expires ON token_denylist(expires_at);
```

### 9. MC-to-GC Heartbeat System

| Tier | Interval | Purpose |
|------|----------|---------|
| Fast | 10s | Lightweight capacity snapshot |
| Comprehensive | 30s | Full health metrics (CPU, memory, per-meeting stats) |

**Failure Detection**: MC marked unhealthy after 30s without heartbeat.

### 10. Performance SLOs

| Operation | Target |
|-----------|--------|
| Create Meeting | P95 ≤ 40ms |
| Get Meeting Info | P95 ≤ 15ms |
| MC Assignment | P95 ≤ 20ms |
| Token Validation | P99 ≤ 2ms |

### 10a. Observability and Trace Propagation

**W3C Trace Context** must be propagated across all GC service boundaries for end-to-end distributed tracing (per ADR-0011):

```
traceparent: 00-{trace-id}-{parent-id}-{flags}

Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
```

**Propagation flow**:
```
Client → GC:  traceparent in HTTP headers
GC → AC:      traceparent in token validation requests
GC → MC:      traceparent in gRPC metadata (AssignMeeting, etc.)
GC → GC:      traceparent in cross-region gRPC (NotifyMeetingEventsBatch)
GC → DB:      trace_id in query comments for correlation
```

**Implementation**:
```rust
// Extract from incoming HTTP request
let parent_context = TraceContextPropagator::extract(&request.headers());

// Create span with parent
let span = tracing::info_span!(
    "gc.http.request",
    trace_id = %parent_context.trace_id(),
    method = %request.method(),
);

// Propagate to outgoing gRPC
let mut metadata = tonic::metadata::MetadataMap::new();
TraceContextPropagator::inject(&span.context(), &mut metadata);
```

**Current state**: GC has basic `#[instrument]` macros but not W3C Trace Context propagation. Requires:
- `tracing-opentelemetry` layer
- OTLP exporter configuration
- Header extraction/injection middleware

See ADR-0011 for full observability framework and ADR-0023 Section 11 for MC-specific trace propagation.

### 11. Chaos Engineering Requirements

Must test and handle:
1. MC Registry Unavailable → 503
2. DB Pool Exhausted → 429 with backpressure
3. Redis Cache Down → DB fallback
4. Cascading MC Failures → <15% session redistribution
5. Auth Service Unreachable → reject new requests, cache existing

## Consequences

### Positive

- **No race conditions**: PostgreSQL atomicity guarantees consistent MC assignment
- **Security boundaries respected**: AC owns all token issuance
- **Fast failure detection**: 10s heartbeat enables quick recovery
- **Testable**: Complete gRPC contracts and SLOs enable comprehensive testing
- **Observable**: State audit table provides debugging capability

### Negative

- **PostgreSQL dependency**: MC assignment requires database availability (mitigated by read replicas)
- **Complexity**: gRPC streaming adds implementation complexity vs direct Redis writes
- **AC dependency**: GC cannot operate without AC for token validation

### Neutral

- **mTLS overhead**: ~1-2ms per connection (acceptable for internal traffic)
- **Message signing**: ~1ms per Pub/Sub message (acceptable for cross-region events)

## Implementation Status

| Section | Component | Status | Commit/PR | Notes |
|---------|-----------|--------|-----------|-------|
| 1 | MC Assignment Flow | ✅ Done | ffa5c93 | Load balancing + atomic assignment |
| 2 | Load Balancing Algorithm | ✅ Done | ffa5c93 | Weighted random from top 5 |
| 3 | Assignment Cleanup - Soft Delete | ✅ Done | pending | `end_assignment()` via gRPC |
| 3 | Assignment Cleanup - Hard Delete | ✅ Done | pending | Background task |
| 4 | GC-MC Communication - Registration | ✅ Done | 2b2a5ab | gRPC `RegisterMc` |
| 4 | GC-MC Communication - Heartbeat | ✅ Done | 2b2a5ab | gRPC `Heartbeat` |
| 4a | GC→MC AssignMeeting RPC | ✅ Done | 921ebb6 | MC accepts/rejects meeting |
| 4a | MC Rejection Handling in GC | ✅ Done | 921ebb6 | Retry with different MC (max 3) |
| 4a | MH Registry in GC | ✅ Done | 921ebb6 | MH registration + load reports |
| 4a | Wire MH/MC Components | ✅ Done | pending | Wire MhService, health checker, assign_meeting_with_mh into handlers/main.rs |
| 4a | Captcha Validation for Guest Access | ❌ Pending | | Implement reCAPTCHA v3 or Cloudflare Turnstile validation in `get_guest_token` endpoint to prevent automated abuse (currently accepts any non-empty token) |
| 4a | env-tests for MH Registry + Assignment RPC | ❌ Pending | | End-to-end tests with real GC/MC/MH: MH registration, load reports, assignment with MH selection, MC accept/reject |
| 4a | MH Cross-Region Sync | ❌ Pending | | Sync MH registry via GC-to-GC |
| 4a | RequestMhReplacement RPC | ❌ Pending | | MC requests MH replacement |
| 5 | Outbox Table Schema | ❌ Pending | | `meeting_peer_events_outbox` |
| 5 | Publisher Background Job | ❌ Pending | | Outbox → Redis + cross-region gRPC |
| 5 | Cross-Region GC gRPC | ❌ Pending | | `NotifyMeetingEventsBatch` |
| 5 | Redis Streams Integration | ❌ Pending | | MC subscription to events |
| 5 | MC-to-MC gRPC | ❌ Pending | | `MeetingControllerPeer` service |
| 6 | JWT Claims Validation | ✅ Done | 5bbd003 | Full claim validation in auth middleware |
| 7 | Token Denylist Table | ❌ Pending | | Schema + Redis cache |
| 7 | Denylist Check in Validation | ❌ Pending | | |
| 7 | Meeting-Scoped Tokens | ✅ Done | 9e477dc | AC endpoints + GC integration |
| 8 | Database Schema - MCs | ✅ Done | 2b2a5ab | `meeting_controllers` table |
| 8 | Database Schema - Assignments | ✅ Done | ffa5c93 | `meeting_assignments` table |
| 8 | Database Schema - Transitions | ❌ Pending | | `meeting_state_transitions` (audit) |
| 8 | Database Schema - Denylist | ❌ Pending | | `token_denylist` table |
| 9 | Heartbeat System | ✅ Done | 2b2a5ab | Health checker background task |
| 10 | Performance SLOs | ⏸️ Deferred | | Targets defined, not instrumented |
| 10a | W3C Trace Context Propagation | ❌ Pending | | `tracing-opentelemetry` + OTLP exporter |
| 11 | Chaos Engineering | ⏸️ Deferred | | Requirements defined, not implemented |

## Debate Summary

### Initial Debate (Rounds 1-3): GC Architecture

**Round 1** - Key concerns: Redis SETNX race conditions, missing JWT claims, no mTLS

**Round 2** - Revisions: PostgreSQL atomic INSERT, gRPC streaming, two-tier heartbeat, mTLS

**Round 3** - Consensus: 93.3% average

### Follow-up Debates (Rounds 4-5): Inter-Region MC Discovery

The initial Section 5 design proposed GC-pushed gRPC streaming with Redis Pub/Sub, but this was architecturally problematic:
1. Redis Pub/Sub assumed cross-region connectivity (impossible - Redis is regional only)
2. GC tracking MC subscriptions added state management complexity to "stateless" GC
3. Cross-region discovery required querying other regions' PostgreSQL (violates regional DB isolation)

**Round 4 Design** (GC-pushed, 93.8% consensus): Solved the Redis Pub/Sub problem but still had GC subscription tracking overhead and cross-region DB assumption.

**Round 5 Design - Direct MC-to-Bus** (83.2% consensus):
A subsequent debate addressed remaining concerns with a fundamentally simpler architecture:

- **MC subscribes directly to Redis Streams** - No GC subscription tracking
- **Read/write separation** - GC writes to bus, MC reads (enforced via Redis ACLs)
- **Transactional outbox pattern** - Atomic DB write + bus publish consistency
- **Regional DB isolation** - Each region has own PostgreSQL, no cross-region queries
- **Blind cross-region broadcast** - GC notifies all regions via gRPC without querying remote DBs
- **Blind remote write** - Remote GC writes to local Redis without DB lookup (handles race conditions)

| Specialist | Score | Key Concern Remaining |
|------------|-------|----------------------|
| MC | 85% | Consumer group cleanup complexity |
| GC | 82% | Publisher as separate process adds ops overhead |
| Database | 88% | Outbox table growth (mitigated with cleanup job) |
| Security | 78% | mTLS + ACLs acceptable but no per-meeting authorization |
| Protocol | 83% | Redis Streams adequate but NATS would be cleaner |

**Average**: 83.2% - Consensus achieved (below 90% target but all concerns mitigable).

**Why lower consensus is acceptable**:
- All remaining concerns are operational, not architectural
- No specialist blocked the design
- Concerns can be addressed during implementation phase

## Related ADRs

- [ADR-0003: Service Authentication](adr-0003-service-authentication.md) - JWT format and scopes
- [ADR-0007: Token Lifetime Strategy](adr-0007-token-lifetime-strategy.md) - Denylist implementation
- [ADR-0009: Integration Test Infrastructure](adr-0009-integration-test-infrastructure.md) - Test harness
- [ADR-0023: Meeting Controller Architecture](adr-0023-meeting-controller-architecture.md) - MC receives meeting/MH assignments from GC

## Files to Create/Modify

1. `proto/gc_mc_internal.proto` - gRPC service definitions
2. `migrations/NNNN_gc_schema.sql` - Database schema
3. `crates/global-controller/` - GC implementation
4. AC changes: Add `jti`, `org_id` to JWT claims

---

*Generated by multi-agent debate on 2025-12-04*
