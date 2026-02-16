# Database Specialist Checkpoint: MC Registration Schema

**Date**: 2026-01-20
**Specialist**: database
**Agent ID**: a5d22c6

---

## Migration Created

**File**: `migrations/20260120000001_mc_registration_schema.sql`

---

## Schema Changes

### 1. Added `updated_at` Column

The `meeting_controllers` table was missing the standard audit column. Added:
```sql
ALTER TABLE meeting_controllers
ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
```

Also created trigger for auto-update:
```sql
CREATE TRIGGER update_meeting_controllers_updated_at
BEFORE UPDATE ON meeting_controllers
FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

### 2. Added `grpc_endpoint` Column

For gRPC service-to-service registration endpoint:
```sql
ALTER TABLE meeting_controllers
ADD COLUMN IF NOT EXISTS grpc_endpoint VARCHAR(255);
```

Backfilled from existing `endpoint` column, then made NOT NULL:
```sql
UPDATE meeting_controllers SET grpc_endpoint = endpoint WHERE grpc_endpoint IS NULL;
ALTER TABLE meeting_controllers ALTER COLUMN grpc_endpoint SET NOT NULL;
```

### 3. Added `webtransport_endpoint` Column

For client WebTransport connections (nullable initially):
```sql
ALTER TABLE meeting_controllers
ADD COLUMN IF NOT EXISTS webtransport_endpoint VARCHAR(255);
```

### 4. Expanded `health_status` Constraint

Dropped old constraint and added expanded one:
```sql
ALTER TABLE meeting_controllers DROP CONSTRAINT IF EXISTS valid_health_status;

ALTER TABLE meeting_controllers ADD CONSTRAINT valid_health_status
CHECK (health_status IN ('pending', 'healthy', 'degraded', 'unhealthy', 'draining'));
```

**New states**:
- `pending` - newly registered, not yet verified
- `healthy` - fully operational
- `degraded` - operational but with issues
- `unhealthy` - not responding to heartbeats
- `draining` - accepting no new meetings, graceful shutdown

### 5. Added Partial Index

For efficient load balancer queries that exclude draining/unhealthy controllers:
```sql
CREATE INDEX IF NOT EXISTS idx_meeting_controllers_available
ON meeting_controllers(region, health_status)
WHERE health_status IN ('healthy', 'degraded');
```

### 6. Added Column Comments

Documentation for new columns and updated semantics.

---

## Migration Safety

### Order of Operations

1. Add nullable columns first (allows rollback)
2. Drop old constraint (temporarily allows any value)
3. Add new constraint (validates existing + new values)
4. Backfill data
5. Add NOT NULL constraint after backfill complete

### Backward Compatibility

- Kept `endpoint` column for existing code references
- New `grpc_endpoint` backfilled from `endpoint` (same value initially)
- `webtransport_endpoint` nullable - existing MCs don't have this yet

### Idempotency

All statements use `IF NOT EXISTS` or `IF EXISTS` where appropriate, making the migration safe to re-run.

---

## Gotchas / Notes

1. **Legacy `endpoint` column**: Kept for backward compatibility. New code should use `grpc_endpoint`. Eventually deprecate and migrate.

2. **`pending` is new default for fresh registrations**: MCs should be registered with `pending` status, then transition to `healthy` after first successful heartbeat.

3. **`draining` state**: MCs entering graceful shutdown should set this. Load balancer index excludes draining controllers from new meeting assignments.

4. **Index covers healthy + degraded only**: Queries for available controllers should use this partial index. Queries for all controllers (admin views) will do full table scan.

---

## Verification

Migration applies cleanly to fresh database and to database with existing data (backfill handles both cases).
