-- Media Handlers Registry Schema (ADR-0010 Section 4a)
-- Tracks MH registration, load reports, and health for load balancing.

-- Media handlers registry table
CREATE TABLE IF NOT EXISTS media_handlers (
    handler_id VARCHAR(255) PRIMARY KEY,
    region TEXT NOT NULL,
    webtransport_endpoint VARCHAR(512) NOT NULL,
    grpc_endpoint VARCHAR(512) NOT NULL,
    max_streams INTEGER NOT NULL DEFAULT 1000,
    current_streams INTEGER NOT NULL DEFAULT 0,
    health_status VARCHAR(20) NOT NULL DEFAULT 'pending'
        CHECK (health_status IN ('pending', 'healthy', 'degraded', 'unhealthy', 'draining')),
    cpu_usage_percent REAL,
    memory_usage_percent REAL,
    bandwidth_usage_percent REAL,
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for finding healthy MHs in a region (for load balancing)
CREATE INDEX IF NOT EXISTS idx_media_handlers_healthy
ON media_handlers(region, health_status)
WHERE health_status = 'healthy';

-- Index for finding available MHs with capacity
CREATE INDEX IF NOT EXISTS idx_media_handlers_available
ON media_handlers(region, current_streams, max_streams)
WHERE health_status IN ('healthy', 'degraded') AND current_streams < max_streams;

-- Create trigger for updated_at
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgname = 'update_media_handlers_updated_at'
    ) THEN
        CREATE TRIGGER update_media_handlers_updated_at
        BEFORE UPDATE ON media_handlers
        FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
    END IF;
END$$;

-- Add comments for documentation
COMMENT ON TABLE media_handlers IS 'Registry of Media Handlers for load balancing. Each MH sends heartbeats to GC.';
COMMENT ON COLUMN media_handlers.handler_id IS 'Unique identifier for the media handler';
COMMENT ON COLUMN media_handlers.region IS 'Deployment region (e.g., us-east-1)';
COMMENT ON COLUMN media_handlers.webtransport_endpoint IS 'WebTransport endpoint for client media connections';
COMMENT ON COLUMN media_handlers.grpc_endpoint IS 'gRPC endpoint for MC→MH communication';
COMMENT ON COLUMN media_handlers.max_streams IS 'Maximum concurrent streams this MH can handle';
COMMENT ON COLUMN media_handlers.current_streams IS 'Current number of active streams';
COMMENT ON COLUMN media_handlers.health_status IS 'Handler health: pending, healthy, degraded, unhealthy, draining';
COMMENT ON COLUMN media_handlers.cpu_usage_percent IS 'Latest reported CPU usage (0-100)';
COMMENT ON COLUMN media_handlers.memory_usage_percent IS 'Latest reported memory usage (0-100)';
COMMENT ON COLUMN media_handlers.bandwidth_usage_percent IS 'Latest reported bandwidth usage (0-100)';
COMMENT ON COLUMN media_handlers.last_heartbeat_at IS 'Timestamp of last heartbeat from MH';
COMMENT ON COLUMN media_handlers.registered_at IS 'When the MH first registered';
COMMENT ON COLUMN media_handlers.updated_at IS 'When the MH record was last updated';
