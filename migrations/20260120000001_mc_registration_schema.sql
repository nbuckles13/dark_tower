-- MC Registration Schema Updates (ADR-0010)
-- Adds gRPC and WebTransport endpoints, expands health status states

-- Step 1: Add updated_at column if not exists (meeting_controllers was missing it)
ALTER TABLE meeting_controllers
ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Step 2: Add new endpoint columns (nullable initially for backfill)
ALTER TABLE meeting_controllers
ADD COLUMN IF NOT EXISTS grpc_endpoint VARCHAR(255),
ADD COLUMN IF NOT EXISTS webtransport_endpoint VARCHAR(255);

-- Step 3: Drop existing health_status constraint
ALTER TABLE meeting_controllers
DROP CONSTRAINT IF EXISTS valid_health_status;

-- Step 4: Add expanded health_status constraint with new states
-- 'pending' - newly registered, not yet verified
-- 'healthy' - fully operational
-- 'degraded' - operational but with issues
-- 'unhealthy' - not responding to heartbeats
-- 'draining' - accepting no new meetings, graceful shutdown
ALTER TABLE meeting_controllers
ADD CONSTRAINT valid_health_status
CHECK (health_status IN ('pending', 'healthy', 'degraded', 'unhealthy', 'draining'));

-- Step 5: Backfill grpc_endpoint from existing endpoint column
-- Existing 'endpoint' is the registration endpoint, same as grpc_endpoint
UPDATE meeting_controllers
SET grpc_endpoint = endpoint
WHERE grpc_endpoint IS NULL;

-- Step 6: Make grpc_endpoint NOT NULL now that backfill is complete
ALTER TABLE meeting_controllers
ALTER COLUMN grpc_endpoint SET NOT NULL;

-- Step 7: Create trigger for updated_at if not exists
-- Note: The function update_updated_at_column() already exists from initial schema
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgname = 'update_meeting_controllers_updated_at'
    ) THEN
        CREATE TRIGGER update_meeting_controllers_updated_at
        BEFORE UPDATE ON meeting_controllers
        FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
    END IF;
END$$;

-- Step 8: Add index for health status filtering with draining state
-- Useful for load balancer queries that exclude draining controllers
CREATE INDEX IF NOT EXISTS idx_meeting_controllers_available
ON meeting_controllers(region, health_status)
WHERE health_status IN ('healthy', 'degraded');

-- Add comment explaining column purposes
COMMENT ON COLUMN meeting_controllers.endpoint IS 'Legacy endpoint column - kept for backward compatibility, same as grpc_endpoint';
COMMENT ON COLUMN meeting_controllers.grpc_endpoint IS 'gRPC registration endpoint for service-to-service communication';
COMMENT ON COLUMN meeting_controllers.webtransport_endpoint IS 'WebTransport endpoint for client connections (nullable, may differ from gRPC endpoint)';
COMMENT ON COLUMN meeting_controllers.health_status IS 'Controller health: pending, healthy, degraded, unhealthy, draining';
