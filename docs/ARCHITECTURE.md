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
│  │           Global Controllers (N instances)             │    │
│  │  - HTTP/3 API endpoints                                │    │
│  │  - Meeting creation/management                         │    │
│  │  - Authentication                                       │    │
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

### 1. Global Controller

**Responsibilities**:
- HTTP/3 API gateway
- Authentication and authorization
- Meeting CRUD operations
- Meeting controller discovery and assignment
- Load balancing across meeting controllers

**Technology**:
- Rust + Axum web framework
- PostgreSQL for persistent data
- Redis for caching
- JWT for authentication

**Scaling**:
- Horizontally scalable (stateless)
- Auto-scaling based on request rate
- Load balanced via Layer 7 LB

**Key APIs**:
```
POST   /api/v1/meetings
GET    /api/v1/meetings/{id}
DELETE /api/v1/meetings/{id}
POST   /api/v1/auth/token
GET    /api/v1/health
```

### 2. Meeting Controller

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

### 3. Media Handler

**Responsibilities**:
- Selective Forwarding Unit (SFU)
- Receive media from clients
- Route media to subscribers
- Bandwidth adaptation
- Optional transcoding
- Optional audio mixing

**Technology**:
- Rust for performance
- Quinn for WebTransport/QUIC
- Proprietary binary protocol for media
- Lock-free data structures for low latency

**Scaling**:
- Horizontally scalable
- Each instance handles 1000+ streams
- Vertical scaling for transcoding workloads
- GPU acceleration for transcoding (optional)

**Optimizations**:
- Zero-copy forwarding
- SIMD for media processing
- Direct memory access (DMA) for NICs
- Huge pages for reduced TLB misses

### 4. Client (Web Application)

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
  name: meeting-controller
spec:
  replicas: 10  # Auto-scaled
  selector:
    matchLabels:
      app: meeting-controller
  template:
    spec:
      containers:
      - name: meeting-controller
        image: darktower/meeting-controller:v0.1.0
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

## Data Flow

### Join Meeting Flow

```
1. User navigates to meeting URL
   └─> Client loads SPA

2. Client → Global Controller (HTTPS)
   GET /api/v1/meetings/{meeting_code}
   └─> Returns meeting info + meeting_controller_url

3. Client → Meeting Controller (WebTransport)
   JoinRequest with JWT token
   └─> Meeting Controller validates token
   └─> Creates participant session in Redis
   └─> Assigns Media Handler
   └─> Returns JoinResponse with:
       - participant_id
       - existing participants
       - media_handler_url + token
       - E2E encryption keys

4. Client → Media Handler (WebTransport)
   Authenticate with token
   └─> Media Handler creates session
   └─> Ready to receive/send media

5. Client captures camera/microphone
   └─> Encodes with WebCodec
   └─> Encrypts (E2E)
   └─> Sends to Media Handler

6. Media Handler
   └─> Receives encrypted media
   └─> Routes to subscribers (cannot decrypt)

7. Other participants receive media
   └─> Decrypt (E2E)
   └─> Decode with WebCodec
   └─> Render to video elements

Total time: < 250ms from step 1 to step 7
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
  name: meeting-controller-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: meeting-controller
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
│  - JWT authentication                               │
│  - Short-lived tokens (5 min)                       │
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

### End-to-End Encryption

```
Client A                 Media Handler              Client B
   │                          │                         │
   │ Generate key pair        │                         │
   │ (ECDH P-256)            │                         │
   │                          │                         │
   │ Publish public key       │    Publish public key  │
   │ (via Meeting Controller) │                         │
   │                          │                         │
   │ Derive shared secret     │    Derive shared secret│
   │ (using B's public key)   │    (using A's public key)
   │                          │                         │
   │ Encrypt media with AES-GCM                        │
   │─────────────────────────>│                         │
   │                          │  Forward encrypted      │
   │                          │  (cannot decrypt)       │
   │                          │────────────────────────>│
   │                          │                         │
   │                          │         Decrypt & render│
   │                          │                         │
```

Server never has access to decryption keys.

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
- `component` (global-controller, meeting-controller, etc.)
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

**Impact**: Media streams routed through that handler are lost

**Mitigation**:
- Clients detect connection loss
- Meeting Controller assigns new Media Handler
- Clients re-establish media streams
- Buffering on client minimizes visible disruption

**Recovery Time**: < 5 seconds for re-establishment

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
