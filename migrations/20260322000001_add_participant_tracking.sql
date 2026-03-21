-- Add participant tracking columns and constraints for meeting join user story (R-9)
-- Extends existing participants table with participant_type, role, and active uniqueness

-- Add participant_type column (member = same-org, external = cross-org, guest = anonymous)
ALTER TABLE participants ADD COLUMN IF NOT EXISTS participant_type VARCHAR(20) NOT NULL DEFAULT 'member';
ALTER TABLE participants ADD CONSTRAINT valid_participant_type CHECK (participant_type IN ('member', 'external', 'guest'));

-- Add role column (host = creator/moderator, participant = regular attendee, guest = anonymous guest)
ALTER TABLE participants ADD COLUMN IF NOT EXISTS role VARCHAR(20) NOT NULL DEFAULT 'participant';
ALTER TABLE participants ADD CONSTRAINT valid_participant_role CHECK (role IN ('host', 'participant', 'guest'));

-- Unique constraint: only one active participant per (meeting_id, user_id)
-- Partial unique index enforces uniqueness only for rows where left_at IS NULL (active participants).
-- Rows with left_at set (historical) are excluded, allowing rejoin after leaving.
-- NULLs in user_id are treated as distinct, so multiple guest participants are allowed.
CREATE UNIQUE INDEX IF NOT EXISTS idx_participants_unique_active
ON participants(meeting_id, user_id) WHERE left_at IS NULL;

-- Partial index on meeting_id for efficient active participant counting (capacity checks)
CREATE INDEX IF NOT EXISTS idx_participants_meeting_active
ON participants(meeting_id) WHERE left_at IS NULL;

-- Comments for documentation
COMMENT ON COLUMN participants.participant_type IS 'Participant type: member (same-org), external (cross-org), or guest (anonymous)';
COMMENT ON COLUMN participants.role IS 'Participant role: host (creator/moderator), participant (regular attendee), or guest (anonymous guest)';

-- DOWN migration (manual rollback):
-- DROP INDEX IF EXISTS idx_participants_meeting_active;
-- DROP INDEX IF EXISTS idx_participants_unique_active;
-- ALTER TABLE participants DROP CONSTRAINT IF EXISTS valid_participant_role;
-- ALTER TABLE participants DROP CONSTRAINT IF EXISTS valid_participant_type;
-- ALTER TABLE participants DROP COLUMN IF EXISTS role;
-- ALTER TABLE participants DROP COLUMN IF EXISTS participant_type;
