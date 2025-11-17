-- Dark Tower Database Initialization
-- This script runs automatically when the PostgreSQL container is first created

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Set timezone
SET timezone = 'UTC';

-- Create initial tables (from DATABASE_SCHEMA.md)
-- Full schema will be managed by migrations in production

-- Users table
CREATE TABLE IF NOT EXISTS users (
    user_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT true,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Meetings table
CREATE TABLE IF NOT EXISTS meetings (
    meeting_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_by_user_id UUID REFERENCES users(user_id) ON DELETE SET NULL,
    display_name VARCHAR(255) NOT NULL,
    meeting_code VARCHAR(20) UNIQUE NOT NULL,
    max_participants INTEGER NOT NULL DEFAULT 100,
    enable_e2e_encryption BOOLEAN NOT NULL DEFAULT true,
    require_auth BOOLEAN NOT NULL DEFAULT false,
    allow_recording BOOLEAN NOT NULL DEFAULT false,
    waiting_room_enabled BOOLEAN NOT NULL DEFAULT false,
    status VARCHAR(20) NOT NULL DEFAULT 'scheduled',
    scheduled_start_time TIMESTAMPTZ,
    actual_start_time TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    assigned_controller_id VARCHAR(255),
    assigned_region VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Meeting participants table
CREATE TABLE IF NOT EXISTS meeting_participants (
    id BIGSERIAL PRIMARY KEY,
    meeting_id UUID NOT NULL REFERENCES meetings(meeting_id) ON DELETE CASCADE,
    participant_id UUID NOT NULL,
    user_id UUID REFERENCES users(user_id) ON DELETE SET NULL,
    display_name VARCHAR(255) NOT NULL,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    leave_reason VARCHAR(50),
    ip_address INET,
    user_agent TEXT,
    client_version VARCHAR(50),
    total_duration_seconds INTEGER DEFAULT 0,
    UNIQUE(meeting_id, participant_id)
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_meetings_meeting_code ON meetings(meeting_code);
CREATE INDEX IF NOT EXISTS idx_meetings_status ON meetings(status);
CREATE INDEX IF NOT EXISTS idx_participants_meeting_id ON meeting_participants(meeting_id);

-- Create a function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create triggers for updated_at
CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_meetings_updated_at BEFORE UPDATE ON meetings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Insert a test user for development
INSERT INTO users (email, display_name, is_active)
VALUES ('test@darktower.dev', 'Test User', true)
ON CONFLICT (email) DO NOTHING;

-- Insert a test meeting for development
INSERT INTO meetings (display_name, meeting_code, status)
VALUES ('Development Test Meeting', 'dev-test-123', 'active')
ON CONFLICT (meeting_code) DO NOTHING;

-- Grant permissions
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO darktower;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO darktower;
