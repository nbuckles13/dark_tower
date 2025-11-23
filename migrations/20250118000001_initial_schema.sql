-- Initial Dark Tower database schema
-- Multi-tenant video conferencing platform

-- Organizations table (multi-tenancy)
CREATE TABLE IF NOT EXISTS organizations (
    org_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subdomain VARCHAR(63) UNIQUE NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    plan_tier VARCHAR(50) NOT NULL DEFAULT 'free',
    max_concurrent_meetings INTEGER NOT NULL DEFAULT 10,
    max_participants_per_meeting INTEGER NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT true,

    CONSTRAINT subdomain_format CHECK (subdomain ~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'),
    CONSTRAINT valid_plan_tier CHECK (plan_tier IN ('free', 'pro', 'enterprise'))
);

CREATE INDEX idx_organizations_subdomain ON organizations(subdomain) WHERE is_active = true;
CREATE INDEX idx_organizations_created_at ON organizations(created_at);

-- Users table
CREATE TABLE IF NOT EXISTS users (
    user_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL REFERENCES organizations(org_id) ON DELETE CASCADE,
    email VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,

    CONSTRAINT users_org_email_unique UNIQUE (org_id, email)
);

CREATE INDEX idx_users_org_id ON users(org_id);
CREATE INDEX idx_users_email ON users(org_id, email) WHERE is_active = true;
CREATE INDEX idx_users_created_at ON users(created_at);

-- Meetings table
CREATE TABLE IF NOT EXISTS meetings (
    meeting_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL REFERENCES organizations(org_id) ON DELETE CASCADE,
    created_by_user_id UUID NOT NULL REFERENCES users(user_id),
    display_name VARCHAR(255) NOT NULL,
    meeting_code VARCHAR(20) NOT NULL,
    join_token_secret VARCHAR(255) NOT NULL,
    max_participants INTEGER NOT NULL DEFAULT 100,
    enable_e2e_encryption BOOLEAN NOT NULL DEFAULT true,
    require_auth BOOLEAN NOT NULL DEFAULT false,
    recording_enabled BOOLEAN NOT NULL DEFAULT false,
    meeting_controller_id VARCHAR(255),
    meeting_controller_region VARCHAR(50),
    status VARCHAR(50) NOT NULL DEFAULT 'scheduled',
    scheduled_start_time TIMESTAMPTZ,
    actual_start_time TIMESTAMPTZ,
    actual_end_time TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT meetings_org_code_unique UNIQUE (org_id, meeting_code),
    CONSTRAINT valid_status CHECK (status IN ('scheduled', 'active', 'ended', 'cancelled'))
);

CREATE INDEX idx_meetings_org_id ON meetings(org_id);
CREATE INDEX idx_meetings_code ON meetings(org_id, meeting_code);
CREATE INDEX idx_meetings_created_by ON meetings(created_by_user_id);
CREATE INDEX idx_meetings_status ON meetings(status) WHERE status IN ('scheduled', 'active');
CREATE INDEX idx_meetings_scheduled_start ON meetings(scheduled_start_time) WHERE status = 'scheduled';
CREATE INDEX idx_meetings_created_at ON meetings(created_at);

-- Participants table
CREATE TABLE IF NOT EXISTS participants (
    participant_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meeting_id UUID NOT NULL REFERENCES meetings(meeting_id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(user_id),
    display_name VARCHAR(255) NOT NULL,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    leave_reason VARCHAR(50),
    connection_quality_avg FLOAT,

    CONSTRAINT valid_leave_reason CHECK (leave_reason IN ('voluntary', 'kicked', 'connection_lost', 'meeting_ended'))
);

CREATE INDEX idx_participants_meeting_id ON participants(meeting_id);
CREATE INDEX idx_participants_user_id ON participants(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX idx_participants_joined_at ON participants(joined_at);

-- Audit logs table
CREATE TABLE IF NOT EXISTS audit_logs (
    log_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL REFERENCES organizations(org_id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(user_id),
    action VARCHAR(100) NOT NULL,
    resource_type VARCHAR(50) NOT NULL,
    resource_id UUID,
    details JSONB,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_resource_type CHECK (resource_type IN ('organization', 'user', 'meeting', 'participant'))
);

CREATE INDEX idx_audit_logs_org_id ON audit_logs(org_id);
CREATE INDEX idx_audit_logs_user_id ON audit_logs(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);

-- Meeting Controller registry (not in schema doc, but needed for GC)
CREATE TABLE IF NOT EXISTS meeting_controllers (
    controller_id VARCHAR(255) PRIMARY KEY,
    region VARCHAR(50) NOT NULL,
    endpoint VARCHAR(255) NOT NULL,
    max_meetings INTEGER NOT NULL,
    current_meetings INTEGER NOT NULL DEFAULT 0,
    max_participants INTEGER NOT NULL,
    current_participants INTEGER NOT NULL DEFAULT 0,
    health_status VARCHAR(20) NOT NULL DEFAULT 'healthy',
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_health_status CHECK (health_status IN ('healthy', 'degraded', 'unhealthy'))
);

CREATE INDEX idx_meeting_controllers_region ON meeting_controllers(region) WHERE health_status = 'healthy';
CREATE INDEX idx_meeting_controllers_health ON meeting_controllers(health_status, last_heartbeat_at);

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Triggers for updated_at
CREATE TRIGGER update_organizations_updated_at BEFORE UPDATE ON organizations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_meetings_updated_at BEFORE UPDATE ON meetings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
