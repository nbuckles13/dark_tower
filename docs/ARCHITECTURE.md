# Dark Tower - System Architecture

This document provides a comprehensive overview of the Dark Tower video conferencing platform architecture.

## Table of Contents

1. [High-Level Overview](#high-level-overview)
2. [Component Architecture](#component-architecture)
3. [Deployment Architecture](#deployment-architecture)
4. [Data Flow](#data-flow)
5. [Scaling Strategy](#scaling-strategy)
6. [Security Architecture](#security-architecture)
7. [Observability](#observability)
8. [Failure Scenarios](#failure-scenarios)

---

## High-Level Overview

Dark Tower is a distributed video conferencing platform designed for global scale, low latency, and high performance.

### Design Principles

1. **Performance First**: Sub-250ms join-to-media latency
2. **Global Scale**: Multi-region deployment with geographic routing
3. **Fault Tolerance**: No single points of failure
4. **E2E Security**: End-to-end encrypted media streams
5. **Open Standards**: WebTransport, WebCodec, Protocol Buffers
6. **Observable**: Comprehensive metrics and tracing

### System Diagram

```
                                  ┌─────────────────────┐
                                  │   Global DNS        │
                                  │  (GeoDNS routing)   │
                                  └──────────┬──────────┘
                                             │
                ┌────────────────────────────┼────────────────────────────┐
                │                            │                            │
          ┌─────▼──────┐             ┌──────▼──────┐             ┌───────▼─────┐
          │  us-west-1 │             │  us-east-1  │             │  eu-west-1  │
          │   Region   │             │   Region    │             │   Region    │
          └────────────┘             └─────────────┘             └─────────────┘

Each Region Contains:

┌─────────────────────────────────────────────────────────────────┐
│                         Region                                  │
│                                                                 │
│  ┌──────────────────┐                                          │
│  │ Load Balancer    │                                          │
│  │  (Layer 4/7)     │                                          │
│  └────────┬─────────┘                                          │
│           │                                                     │
│  ┌────────▼──────────────────────────────────────────────┐    │
│  │         Auth Controllers (N instances)                 │    │
│  │  - User/service authentication                         │    │
│  │  - JWT token issuance                                  │    │
│  │  - JWKS endpoint (/.well-known/jwks.json)              │    │
│  │  - Key rotation management                             │    │
│  └────────┬───────────────────────────────────────────────┘    │
│           │                                                     │
│  ┌────────▼──────────────────────────────────────────────┐    │
│  │           Global Controllers (N instances)             │    │
│  │  - HTTP/3 API endpoints                                │    │
│  │  - Meeting creation/management                         │    │
│  │  - Meeting controller discovery                        │    │
│  └────────┬───────────────────────────────────────────────┘    │
│           │                                                     │
│  ┌────────▼──────────────────────────────────────────────┐    │
│  │        Meeting Controllers (N instances)               │    │
│  │  - WebTransport signaling                              │    │
│  │  - Participant management                              │    │
│  │  - Meeting state coordination                          │    │
│  └────────┬───────────────────────────────────────────────┘    │
│           │                                                     │
│  ┌────────▼──────────────────────────────────────────────┐    │
│  │         Media Handlers (N instances)                   │    │
│  │  - Media stream routing (SFU)                          │    │
│  │  - Transcoding (optional)                              │    │
│  │  - Bandwidth adaptation                                │    │
│  └────────────────────────────────────────────────────────┘    │
│                                                                 │
│  ┌────────────────────────────────────────────────────────┐    │
│  │                Data Layer                               │    │
│  │  ┌──────────────┐          ┌────────────────┐         │    │
│  │  │  PostgreSQL  │          │  Redis Cluster │         │    │
│  │  │  (Primary +  │          │  (Sharded)     │         │    │
│  │  │   Replicas)  │          │                │         │    │
│  │  └──────────────┘          └────────────────┘         │    │
│  └────────────────────────────────────────────────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘


┌──────────────────────────────────────┐
│         Client (Browser)             │
│  - Svelte/TypeScript SPA             │
│  - WebTransport connections          │
│  - WebCodec media processing         │
│  - E2E encryption                    │
└──────────────────────────────────────┘
```

---

## Component Architecture

### 1. Auth Controller

**Responsibilities**:
- Centralized authentication and authorization service
- User authentication (username/password, OAuth future)
- Service-to-service authentication (OAuth 2.0 Client Credentials)
- JWT token issuance (user tokens, service tokens)
- JWKS endpoint for public key distribution
- Key rotation management (weekly with 1-week overlap)
- Federation support (multi-cluster token validation)

**Stateless Design**:
- Each request is independent - no session state
- Token validation via local JWKS cache (no database lookup)
- PostgreSQL only for credential storage and key persistence
- No cross-service dependencies for validation (zero-trust)

**Technology**:
- Rust + Axum web framework
- EdDSA (Ed25519) for JWT signing
- PostgreSQL for credentials and key storage (encrypted at rest)
- Actor-based architecture (see ADR-0001):
  - `JwksManagerActor` - JWKS cache and refresh
  - `TokenIssuerActor` - Rate-limited token generation
  - `KeyRotationActor` - Weekly key rotation

**OAuth 2.0 Scope System**:
- Format: `{principal}.{operation}.{component}`
- Principals: `user`, `service`
- Operations: `read`, `write`, `admin`
- Components: `gc`, `mc`, `mh`, `ac`
- Examples: `user.write.gc`, `service.read.mh`

**Federation Architecture**:
- One Auth Controller per cluster (region or customer)
- Each cluster has unique signing key
- Services fetch JWKS from all federated clusters
- Cross-cluster token validation via public key
- JWKS cached 1 hour, refreshed on unknown kid

**Key APIs**:
```
POST   /v1/auth/user/token         # Issue user token (1-hour lifetime)
POST   /v1/auth/service/token      # Issue service token (2-hour lifetime)
POST   /v1/admin/services/register # Register new service (deployment)
GET    /.well-known/jwks.json      # Public key distribution (JWKS)
```

**Scaling**:
- Horizontally scalable (truly stateless)
- Fast token issuance (<50ms p99)
- No database lookup for validation
- Rate limiting via TokenIssuerActor
- Auto-scaling based on request rate

**Security Features**:
- Short-lived tokens (1-2 hours)
- bcrypt password hashing (cost factor 12+)
- Private keys encrypted at rest
- Audit logging for all auth events
- Rate limiting on login attempts
- Weekly key rotation with overlap

**Reference**: See ADR-0003 for detailed authentication and federation design.

### 2. Global Controller

**Responsibilities**:
- HTTP/3 API gateway
- Meeting CRUD operations (metadata only)
- Meeting controller discovery and assignment
- Load balancing across meeting controllers
- Subdomain-based tenant routing
- Token validation (via JWKS from Auth Controller)

**Stateless Design**:
- **No live meeting state** stored in Global Controller
- Reads configuration from PostgreSQL (tenant settings, limits)
- Uses Redis for regional caching only (not cross-region)
- Each request is independent - no session affinity required
- Meeting state lives entirely in Meeting Controllers

**State Management**:
- **PostgreSQL**: Persistent data (orgs, users, meeting metadata)
  - Read-only for Global Controller during request processing
  - Writes for meeting creation/updates
- **Redis**: Regional cache for:
  - Tenant configuration (TTL: 5 minutes)
  - Meeting controller availability
  - Rate limiting counters
- **No cross-region coordination** required

**Technology**:
- Rust + Axum web framework
- PostgreSQL read replicas per region
- Regional Redis cluster (not shared across regions)
- JWT validation via JwksManagerActor (see ADR-0001)

**Scaling**:
- Horizontally scalable (truly stateless)
- No sticky sessions required
- Auto-scaling based on request rate
- Load balanced via Layer 7 LB with round-robin

**Key APIs**:
```
POST   /v1/meetings           # Create meeting metadata
GET    /v1/meetings/{id}      # Get meeting info + controller URL
DELETE /v1/meetings/{id}      # Mark meeting as ended
GET    /v1/health             # Health check
```

### 3. Meeting Controller

**Responsibilities**:
- WebTransport signaling server
- Manage individual meeting state
- Participant join/leave coordination
- Media handler assignment
- Subscription management
- E2E key exchange coordination

**Technology**:
- Rust + Quinn (QUIC/WebTransport)
- Protocol Buffers for signaling
- Redis for meeting state
- gRPC for internal communication

**Scaling**:
- Horizontally scalable
- Meetings distributed via consistent hashing
- Sticky sessions (participant → controller)
- Can handle 1000+ concurrent meetings per instance

**State Management**:
- Meeting state in Redis
- Participant sessions in Redis (with TTL)
- Coordination via Redis pub/sub

### 4. Media Handler

**Responsibilities**:
- Selective Forwarding Unit (SFU) with intelligent routing
- Receive media via QUIC datagrams from clients
- Route media to subscribers (client or other handlers)
- Cascading forwarding (handler-to-handler)
- Bandwidth monitoring and reporting
- Optional transcoding
- Optional audio mixing

**Multi-Handler Architecture**:
- Clients connect to **multiple Media Handlers simultaneously** (initially 2, scalable to N)
- Meeting Controller determines which handler(s) to use for each stream
- Enables:
  - Geographic distribution (handlers in different datacenters within region)
  - Redundancy and failover
  - Load balancing
  - Optimized routing based on network conditions

**Routing Modes**:
1. **Direct**: Client → Handler → Client (single hop)
2. **Cascading**: Client → Handler A → Handler B → Client (multi-hop for optimization)
3. **Hybrid**: Different streams use different paths dynamically

**Technology**:
- Rust for performance
- Quinn for WebTransport/QUIC datagrams
- Proprietary binary protocol for media (42-byte header)
- Lock-free data structures for low latency
- SFrame pass-through (cannot decrypt E2E encrypted media)

**Scaling**:
- Horizontally scalable
- Each instance handles 10,000+ datagram streams
- Vertical scaling for transcoding workloads
- GPU acceleration for transcoding (optional)
- Auto-scaling based on stream count and bandwidth

**Optimizations**:
- Zero-copy forwarding via datagram pass-through
- SIMD for media processing (when decryption not required)
- Direct memory access (DMA) for NICs
- Huge pages for reduced TLB misses
- Datagram batching for efficiency

### 5. Client (Web Application)

**Responsibilities**:
- User interface
- Media capture (camera, microphone, screen)
- Media encoding/decoding (WebCodec)
- WebTransport communication
- E2E encryption/decryption
- Bandwidth estimation

**Technology**:
- Svelte for UI framework
- TypeScript for type safety
- WebCodec API for media processing
- WebTransport API for communication
- Web Crypto API for E2E encryption

**Features**:
- Adaptive bitrate encoding
- Simulcast support
- Multiple camera support
- Multiple screen shares
- Picture-in-picture
- Virtual backgrounds (future)

---

## Deployment Architecture

### Kubernetes Deployment

Each component runs as a Kubernetes Deployment:

```yaml
# Example: Meeting Controller Deployment
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mc-service
spec:
  replicas: 10  # Auto-scaled
  selector:
    matchLabels:
      app: mc-service
  template:
    spec:
      containers:
      - name: mc-service
        image: darktower/mc-service:v0.1.0
        resources:
          requests:
            cpu: "2000m"
            memory: "4Gi"
          limits:
            cpu: "4000m"
            memory: "8Gi"
        ports:
        - containerPort: 4433
          protocol: UDP  # QUIC
```

### Service Mesh

Istio or Linkerd for service-to-service communication:
- mTLS between services
- Traffic management
- Circuit breaking
- Retry logic
- Observability

### Multi-Region Deployment

```
┌─────────────────────────────────────────────────────────┐
│                    Global Layer                         │
│                                                          │
│  ┌────────────────────────────────────────────────┐    │
│  │  Global DNS (Route53 / Cloud DNS)             │    │
│  │  - GeoDNS routing to nearest region            │    │
│  │  - Health check based failover                 │    │
│  └────────────────────────────────────────────────┘    │
│                                                          │
│  ┌────────────────────────────────────────────────┐    │
│  │  Global Database (PostgreSQL)                  │    │
│  │  - Primary in one region                       │    │
│  │  - Read replicas in all regions                │    │
│  │  - Cross-region replication                    │    │
│  └────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘

┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│  us-west-1   │   │  us-east-1   │   │  eu-west-1   │
│              │   │              │   │              │
│  All         │   │  All         │   │  All         │
│  Components  │   │  Components  │   │  Components  │
│              │   │              │   │              │
│  Local Redis │   │  Local Redis │   │  Local Redis │
│  Cluster     │   │  Cluster     │   │  Cluster     │
└──────────────┘   └──────────────┘   └──────────────┘
```

### Cross-Region Meetings (Future)

Participants from different regions can join the same meeting:

1. **Option A: Cascading SFUs**
   - Each region has a media handler
   - Media handlers exchange media between regions
   - Reduces latency for regional participants

2. **Option B: Single-Region Meeting**
   - All participants connect to one region
   - Simpler but higher latency for remote participants

---

## Media Routing Strategy

The Meeting Controller implements intelligent media routing across multiple Media Handlers.

### Routing Algorithm

When a client subscribes to media (via layout request):

1. **Analyze Subscriptions**:
   - Collect all layout requests from all participants
   - Determine which sources (user_id) need to send media
   - Calculate total subscriber count per source

2. **Optimize Routing**:
   - Co-locate participants in same meeting on same Media Handler when possible
   - Minimize handler-to-handler hops
   - Balance load across available handlers
   - Consider network topology and latency

3. **Generate Routing Plan**:
   - For each publisher: which handler(s) to send to
   - For each handler: which streams to forward where
   - For each subscriber: which handler to receive from

4. **Execute Routing**:
   - Send routing commands to Media Handlers
   - Send publish/subscribe commands to clients
   - Update routing state in Redis

### Example: 3 Participants, 2 Handlers

```
Participants:
- Alice: wants grid 2x2 (Bob camera, Bob screen, Charlie camera)
- Bob: wants grid 2x2 (Alice camera, Alice screen, Charlie camera)
- Charlie: wants speaker view (active speaker only)

Media Handlers:
- MH1: us-west-dc1
- MH2: us-west-dc2

Routing Decision:
1. Alice, Bob → MH1 (co-located for low latency)
2. Charlie → MH2 (load balance)

Routing Plan:
- Alice sends camera to MH1 (stream_id assigned by Alice subscriber)
- Alice sends screen to MH1
- Bob sends camera to MH1
- Bob sends screen to MH1
- Charlie sends camera to MH2

- MH1 forwards Bob camera/screen to Alice
- MH1 forwards Alice camera/screen to Bob
- MH1 forwards Alice camera → MH2 (cascade for Charlie)
- MH2 forwards to Charlie (active speaker = Alice)

Total: 6 client streams, 1 handler-to-handler stream
```

### Dynamic Rerouting

Routing updates occur when:
- Participant joins/leaves
- Layout changes
- Handler becomes unhealthy
- Network conditions change significantly
- Active speaker changes (for speaker-focused layouts)

**Seamless Updates**:
- New routes established before old routes torn down
- Brief period of dual streaming during transition
- No frame loss during rerouting

### Handler Selection Criteria

Priority order for assigning handlers to participants:

1. **Geographic proximity** (lowest latency)
2. **Current load** (avoid overloaded handlers)
3. **Existing connections** (reuse when possible)
4. **Network quality** (prefer stable paths)
5. **Handler capabilities** (transcoding, mixing)

### Scaling Considerations

- **Small meetings (<10 participants)**: Single handler sufficient
- **Medium meetings (10-50)**: 2 handlers for redundancy
- **Large meetings (50-100)**: 2-3 handlers for load distribution
- **Very large meetings (100+)**: 3-N handlers with cascading

---

## Layout System

Clients subscribe to layouts instead of individual streams. The Meeting Controller translates layout requests into specific stream subscriptions.

### Grid Layout

**Description**: NxM grid of equal-sized video tiles, up to 256 rows or columns.

**Configuration**:
```typescript
{
  layout_type: "grid",
  rows: 3,
  columns: 3,
  max_streams: 9,  // rows * columns
  stream_ids: [1, 2, 3, 4, 5, 6, 7, 8, 9]  // subscriber-chosen IDs
}
```

**Behavior**:
- Meeting Controller fills grid slots with participant streams
- Priority: pinned users first, then active speakers, then others
- Unfilled slots remain empty
- Grid automatically adjusts as participants join/leave

**Customization Options**:
```typescript
{
  pinned_users: [user_id_1, user_id_2],  // Must appear in grid
  excluded_users: [user_id_3],            // Must not appear
  prefer_video_over_audio: true,          // Prioritize video streams
  include_self: false                      // Exclude own streams
}
```

**Stream Assignment**:
1. Fill pinned user slots first
2. Fill with screen shares (high priority)
3. Fill with active speaker videos
4. Fill remaining with other participant videos
5. Assign each to a subscriber-provided stream_id

**Example: 3x3 Grid**:
```
Meeting: 15 participants
Client requests: 3x3 grid (9 slots)
Pinned: Alice, Bob
Excluded: Charlie

Slot Assignment:
1. Alice (camera)      - stream_id: 0x001 (pinned)
2. Bob (camera)        - stream_id: 0x002 (pinned)
3. David (screen)      - stream_id: 0x003 (screen share)
4. Eve (camera)        - stream_id: 0x004 (active speaker)
5. Frank (camera)      - stream_id: 0x005
6. Grace (camera)      - stream_id: 0x006
7. Heidi (camera)      - stream_id: 0x007
8. Ivan (camera)       - stream_id: 0x008
9. Judy (camera)       - stream_id: 0x009

Not shown: Charlie (excluded), 5 others (no space)
```

**Dynamic Updates**:
- Active speaker changes: Meeting Controller swaps streams in grid
- Participant joins: May displace lowest priority participant in grid
- Screen share starts: Automatically promoted to grid slot
- Pinned user leaves: Slot filled with next priority participant

---

## Data Flow

### Join Meeting Flow

```
1. User navigates to meeting URL
   └─> Client loads SPA

2. Client → Auth Controller (HTTPS)
   POST /v1/auth/user/token
   Body: { username, password }
   └─> Auth Controller validates credentials
   └─> Returns JWT user token (1-hour lifetime)
       - Includes scopes: user.read.gc, user.write.mc, etc.

3. Client → Global Controller (HTTPS)
   GET /v1/meetings/{meeting_code}
   Header: Authorization: Bearer <JWT>
   └─> Global Controller validates JWT (via JWKS)
   └─> Returns meeting info + meeting_controller_url

4. Client → Meeting Controller (WebTransport)
   JoinRequest with JWT token
   └─> Meeting Controller validates token (via JWKS)
   └─> Creates participant session in Redis
   └─> Assigns Media Handler
   └─> Issues connection token for Media Handler
   └─> Returns JoinResponse with:
       - participant_id
       - existing participants
       - media_handler_url + connection_token
       - E2E encryption keys

5. Client → Media Handler (WebTransport)
   Authenticate with connection_token
   └─> Media Handler validates token
   └─> Media Handler creates session
   └─> Ready to receive/send media

6. Client captures camera/microphone
   └─> Encodes with WebCodec
   └─> Encrypts (E2E)
   └─> Sends to Media Handler

7. Media Handler
   └─> Receives encrypted media
   └─> Routes to subscribers (cannot decrypt)

8. Other participants receive media
   └─> Decrypt (E2E)
   └─> Decode with WebCodec
   └─> Render to video elements

Total time: < 250ms from step 1 to step 8
```

### Publish Media Stream Flow

```
Client                 Meeting Controller         Media Handler
  │                            │                        │
  │ 1. PublishStream           │                        │
  │ (video_camera, VP9)        │                        │
  │───────────────────────────>│                        │
  │                            │                        │
  │                            │ 2. RouteMediaCommand   │
  │                            │ (setup routing)        │
  │                            │───────────────────────>│
  │                            │                        │
  │                            │<────── 3. ACK ─────────│
  │                            │                        │
  │<──── 4. StreamPublished ───│                        │
  │    (with stream_id)        │                        │
  │                            │                        │
  │ 5. Open UniStream to       │                        │
  │    Media Handler           │                        │
  │────────────────────────────┼───────────────────────>│
  │                            │                        │
  │ 6. Stream Header           │                        │
  │────────────────────────────┼───────────────────────>│
  │                            │                        │
  │ 7. MediaFrames             │                        │
  │────────────────────────────┼───────────────────────>│
  │   (continuous)             │                        │
  │                            │                        │
```

### Subscribe to Stream Flow

```
Client                 Meeting Controller         Media Handler
  │                            │                        │
  │ 1. SubscribeStream         │                        │
  │ (stream_id=abc123)         │                        │
  │───────────────────────────>│                        │
  │                            │                        │
  │                            │ 2. Update routing      │
  │                            │ (add subscriber)       │
  │                            │───────────────────────>│
  │                            │                        │
  │                            │<────── 3. ACK ─────────│
  │                            │                        │
  │<────── 4. ACK ─────────────│                        │
  │                            │                        │
  │                            │ 5. UniStream from      │
  │                            │    Media Handler       │
  │<───────────────────────────┼────────────────────────│
  │                            │                        │
  │ 6. Receive MediaFrames     │                        │
  │<───────────────────────────┼────────────────────────│
  │   (continuous)             │                        │
  │                            │                        │
```

---

## Scaling Strategy

### Horizontal Scaling

All components are designed to scale horizontally:

| Component | Scaling Trigger | Target Metric |
|-----------|----------------|---------------|
| Global Controller | Request rate > 1000 req/s | CPU > 70% |
| Meeting Controller | Meetings > 800 per instance | Memory > 75% |
| Media Handler | Streams > 8000 per instance | Network > 8 Gbps |

### Vertical Scaling

Media Handlers benefit from vertical scaling:
- More CPU cores for transcoding
- More memory for buffering
- Faster NICs for throughput

### Auto-Scaling Configuration

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: mc-service-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: mc-service
  minReplicas: 5
  maxReplicas: 50
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 75
```

### Database Scaling

**PostgreSQL**:
- Primary for writes
- Multiple read replicas per region
- Connection pooling (PgBouncer)
- Partitioning for large tables (audit_logs)

**Redis**:
- Clustered mode (sharded)
- 3-5 nodes per cluster
- Replication for high availability
- Separate clusters for different data types

---

## Security Architecture

### Defense in Depth

```
┌─────────────────────────────────────────────────────┐
│ Layer 1: Network Security                           │
│  - VPC isolation                                    │
│  - Security groups / firewall rules                 │
│  - DDoS protection (CloudFlare, etc.)               │
└─────────────────────────────────────────────────────┘
                        │
┌─────────────────────────────────────────────────────┐
│ Layer 2: Transport Security                         │
│  - TLS 1.3 for all connections                      │
│  - Certificate pinning (optional)                   │
│  - QUIC encryption                                  │
└─────────────────────────────────────────────────────┘
                        │
┌─────────────────────────────────────────────────────┐
│ Layer 3: Application Security                       │
│  - Auth Controller for centralized authentication   │
│  - JWT authentication (EdDSA signing)               │
│  - Short-lived tokens (1-2 hours)                   │
│  - OAuth 2.0 scope-based authorization              │
│  - mTLS for service-to-service communication        │
│  - Rate limiting                                    │
│  - Input validation                                 │
└─────────────────────────────────────────────────────┘
                        │
┌─────────────────────────────────────────────────────┐
│ Layer 4: Data Security                              │
│  - End-to-end encryption for media                  │
│  - Encryption at rest (database)                    │
│  - Secrets management (Vault)                       │
│  - Audit logging                                    │
└─────────────────────────────────────────────────────┘
```

### End-to-End Encryption with SFrame

**SFrame** (Secure Frame) provides frame-level encryption compatible with SFU architecture.

#### Key Components

1. **SFrame Standard**: [draft-ietf-sframe](https://datatracker.ietf.org/doc/draft-ietf-sframe/)
2. **Insertable Streams API**: WebRTC extension for client-side encryption
3. **Key Management**: MLS (Messaging Layer Security) for group key distribution

#### Encryption Flow

```
Client A                 Media Handler              Client B
   │                          │                         │
   │ 1. Join meeting          │                         │
   │ Receive group key (MLS)  │   Receive group key     │
   │                          │                         │
   │ 2. Encode frame (WebCodec)                        │
   │ 3. Encrypt with SFrame   │                         │
   │    - Key ID in header    │                         │
   │    - AES-GCM payload     │                         │
   │                          │                         │
   │ 4. Send via datagram     │                         │
   │ [encrypted payload]      │                         │
   │─────────────────────────>│                         │
   │                          │                         │
   │                          │ 5. Forward unchanged    │
   │                          │    (cannot decrypt)     │
   │                          │ [encrypted payload]     │
   │                          │────────────────────────>│
   │                          │                         │
   │                          │  6. Decrypt with SFrame │
   │                          │  7. Decode with WebCodec│
   │                          │  8. Render              │
   │                          │                         │
```

#### SFrame Header Format

```
┌──────────────────────────────────┐
│ X (1 bit): Extended Key ID       │
│ CTR (3-7 bits): Counter          │
│ Key ID (variable)                │
├──────────────────────────────────┤
│ Encrypted Media Frame            │
│ (AES-GCM-128 or AES-GCM-256)     │
├──────────────────────────────────┤
│ Authentication Tag               │
└──────────────────────────────────┘
```

#### Key Management

**Key Rotation Triggers**:
- Participant joins meeting → New key generated
- Participant leaves meeting → New key generated
- Manual rotation request
- Time-based rotation (every 1 hour)

**MLS Key Distribution**:
1. Meeting Controller acts as MLS Delivery Service
2. Clients generate MLS key packages on join
3. Group key derived via MLS protocol
4. Forward secrecy: old keys deleted after rotation

**Key Storage**:
- Keys stored only in client memory
- Never persisted to disk
- Never sent to server unencrypted

#### Implementation

**Client-Side (JavaScript)**:
```javascript
// Using Insertable Streams API
const senderTransform = new TransformStream({
  transform(encodedFrame, controller) {
    const encryptedFrame = sframe.encrypt(encodedFrame, currentKey);
    controller.enqueue(encryptedFrame);
  }
});

// Apply to WebCodec encoded stream
videoEncoder.readable
  .pipeThrough(senderTransform)
  .pipeTo(datagramWriter);
```

**Server-Side**:
- Media Handler forwards encrypted frames without decryption
- Zero knowledge of plaintext media
- Cannot read, modify, or inject frames

#### Security Properties

- **End-to-End**: Only participants can decrypt
- **Forward Secrecy**: Compromise of current key doesn't reveal past media
- **Post-Compromise Security**: New key rotation limits exposure
- **Authentication**: Each frame authenticated via AEAD
- **Replay Protection**: Counter prevents replay attacks

Server never has access to decryption keys or plaintext media.

---

## Observability

### Metrics Collection

**Global Controller Metrics**:
- Request rate, latency (p50, p95, p99)
- Error rate by endpoint
- Active connections
- Authentication failures

**Meeting Controller Metrics**:
- Active meetings
- Participants per meeting
- Join latency
- Message throughput
- Redis operation latency

**Media Handler Metrics**:
- Active streams
- Bandwidth (in/out)
- Packet loss rate
- Frame drop rate
- Transcoding queue depth

### Distributed Tracing

OpenTelemetry spans for:
1. HTTP request → Response
2. Join flow end-to-end
3. Publish stream flow
4. Subscribe stream flow

Example trace:
```
join_meeting (250ms)
  ├─ http_get_meeting_info (45ms)
  ├─ webtransport_connect (80ms)
  ├─ join_request_processing (50ms)
  │   ├─ validate_token (5ms)
  │   ├─ create_participant_session (10ms)
  │   ├─ assign_media_handler (20ms)
  │   └─ get_existing_participants (15ms)
  ├─ media_connection_establish (60ms)
  └─ first_frame_received (15ms)
```

### Logging

Structured JSON logs with fields:
- `timestamp`
- `level` (info, warn, error)
- `component` (gc-service, mc-service, etc.)
- `trace_id`
- `span_id`
- `meeting_id`
- `participant_id`
- `message`
- `metadata`

### Dashboards

Grafana dashboards for:
1. System overview (all regions)
2. Per-region health
3. Per-component metrics
4. Meeting quality metrics
5. Cost tracking

---

## Failure Scenarios

### 1. Global Controller Failure

**Impact**: New meetings cannot be created

**Mitigation**:
- Multiple instances behind load balancer
- Health checks with automatic removal
- Auto-scaling replaces failed instances
- Graceful degradation (read-only mode)

**Recovery Time**: < 30 seconds

### 2. Meeting Controller Failure

**Impact**: Active meetings on that controller are disrupted

**Mitigation**:
- Clients automatically reconnect to new controller
- Meeting state in Redis (survives controller failure)
- Participant sessions have 5-minute TTL for reconnection
- Load balancer redirects to healthy instances

**Recovery Time**: < 10 seconds for client reconnection

### 3. Media Handler Failure

**Impact**: Reduced redundancy, potential stream interruption

**Mitigation** (Multi-Handler Architecture):
- Client already connected to 2+ handlers
- Meeting Controller detects handler failure via health checks
- Meeting Controller reroutes streams to remaining healthy handlers
- If needed, Meeting Controller assigns additional handler
- Client establishes new connection in background
- Minimal interruption due to redundancy

**Recovery Time**:
- Stream failover: < 1 second (already connected to backup handler)
- New handler connection: < 3 seconds

**Degraded Mode**:
- System continues with 1 handler if necessary
- Meeting Controller prioritizes restoring redundancy

### 4. Database Failure

**PostgreSQL Primary Failure**:
- Automatic failover to replica (Patroni/Stolon)
- Recovery time: < 60 seconds
- No meeting state lost (replicated)

**Redis Cluster Failure**:
- Cluster mode provides automatic failover
- Active meetings may experience brief disruption
- Recovery time: < 10 seconds

### 5. Network Partition

**Between Regions**:
- Meetings remain regional
- Cross-region meetings degrade to single-region
- Automatic recovery when partition heals

**Within Region**:
- Components use health checks
- Failed components removed from rotation
- Kubernetes reschedules pods

### 6. Client Network Issues

**Temporary Disconnection**:
- QUIC handles brief outages automatically
- Client-side buffering prevents frame loss
- Seamless recovery

**Prolonged Disconnection (> 10s)**:
- Client triggers reconnection logic
- Meeting Controller maintains session (5 min TTL)
- Rejoin with same participant_id
- Other participants notified of reconnection

---

## Future Enhancements

1. **Recording Service**: Separate component for cloud recording
2. **RTMP Streaming**: Broadcast to YouTube, Twitch, etc.
3. **SIP Gateway**: Dial-in via phone
4. **Breakout Rooms**: Split meetings into sub-groups
5. **AI Features**: Noise suppression, virtual backgrounds, live transcription
6. **Analytics Service**: Meeting quality analytics, usage patterns
7. **CDN Integration**: Distribute recordings globally

---

## Conclusion

Dark Tower's architecture is designed for:
- **Performance**: Sub-250ms latency, optimized media paths
- **Scale**: Thousands of concurrent meetings per region
- **Reliability**: No single points of failure, automatic recovery
- **Security**: End-to-end encryption, defense in depth
- **Observability**: Comprehensive metrics, tracing, and logging

The modular design allows each component to evolve independently while maintaining system-wide consistency and reliability.
