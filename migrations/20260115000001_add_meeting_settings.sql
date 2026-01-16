-- Add meeting settings columns for Phase 2 Meeting API
-- Per ADR-0020: Guest access, external participants, and waiting room settings

-- Add guest access setting (default: false - guests not allowed)
ALTER TABLE meetings ADD COLUMN IF NOT EXISTS allow_guests BOOLEAN NOT NULL DEFAULT false;

-- Add external participant setting (default: false - only same-org participants)
ALTER TABLE meetings ADD COLUMN IF NOT EXISTS allow_external_participants BOOLEAN NOT NULL DEFAULT false;

-- Add waiting room setting (default: true - waiting room enabled for security)
ALTER TABLE meetings ADD COLUMN IF NOT EXISTS waiting_room_enabled BOOLEAN NOT NULL DEFAULT true;

-- Add index for guest-enabled meetings (commonly queried for public join)
CREATE INDEX IF NOT EXISTS idx_meetings_allow_guests ON meetings(meeting_code) WHERE allow_guests = true;

-- Comment on new columns for documentation
COMMENT ON COLUMN meetings.allow_guests IS 'Whether anonymous guests can join the meeting without authentication';
COMMENT ON COLUMN meetings.allow_external_participants IS 'Whether authenticated users from other organizations can join';
COMMENT ON COLUMN meetings.waiting_room_enabled IS 'Whether participants must wait for host approval before joining';
