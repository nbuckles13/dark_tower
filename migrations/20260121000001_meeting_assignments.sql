-- Meeting Assignments Schema (ADR-0010)
-- Maps meetings to meeting controllers in each region for load balancing.

-- Meeting-to-MC assignments (atomic via UNIQUE constraint)
CREATE TABLE IF NOT EXISTS meeting_assignments (
    meeting_id TEXT NOT NULL,
    meeting_controller_id VARCHAR(255) NOT NULL REFERENCES meeting_controllers(controller_id),
    region TEXT NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    assigned_by_gc_id TEXT NOT NULL,
    ended_at TIMESTAMPTZ,  -- NULL = active, set when meeting ends
    PRIMARY KEY (meeting_id, region)
);

-- Index for finding active assignments by MC (for capacity tracking)
CREATE INDEX IF NOT EXISTS idx_assignments_by_mc
ON meeting_assignments(meeting_controller_id)
WHERE ended_at IS NULL;

-- Index for region-based assignment lookups
CREATE INDEX IF NOT EXISTS idx_assignments_by_region
ON meeting_assignments(meeting_id, region)
WHERE ended_at IS NULL;

-- Index for cleanup of old ended assignments
CREATE INDEX IF NOT EXISTS idx_assignments_ended_at
ON meeting_assignments(ended_at)
WHERE ended_at IS NOT NULL;

-- Add comments for documentation
COMMENT ON TABLE meeting_assignments IS 'Maps meetings to meeting controllers for load balancing. Each meeting can have one active assignment per region.';
COMMENT ON COLUMN meeting_assignments.meeting_id IS 'The meeting being assigned (references meetings.meeting_id as TEXT for flexibility)';
COMMENT ON COLUMN meeting_assignments.meeting_controller_id IS 'The assigned meeting controller';
COMMENT ON COLUMN meeting_assignments.region IS 'Deployment region of the assignment (e.g., us-east-1)';
COMMENT ON COLUMN meeting_assignments.assigned_at IS 'When the assignment was created';
COMMENT ON COLUMN meeting_assignments.assigned_by_gc_id IS 'ID of the GC instance that made this assignment';
COMMENT ON COLUMN meeting_assignments.ended_at IS 'When the assignment ended (NULL = active)';
