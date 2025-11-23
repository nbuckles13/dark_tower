-- Auth Controller tables for service authentication and key management
-- Phase 1: OAuth 2.0 client credentials, EdDSA signing keys, audit events

-- ============================================================================
-- UP MIGRATION
-- ============================================================================

-- Service Credentials table (OAuth 2.0 Client Credentials)
CREATE TABLE service_credentials (
    credential_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id VARCHAR(255) UNIQUE NOT NULL,
    client_secret_hash VARCHAR(255) NOT NULL,  -- bcrypt cost 12+
    service_type VARCHAR(50) NOT NULL,
    region VARCHAR(50),
    scopes TEXT[] NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_service_type CHECK (service_type IN ('global-controller', 'meeting-controller', 'media-handler'))
);

-- Indexes for service_credentials
CREATE INDEX idx_service_credentials_client_id ON service_credentials(client_id);
CREATE INDEX idx_service_credentials_active ON service_credentials(is_active) WHERE is_active = true;
CREATE INDEX idx_service_credentials_service_type ON service_credentials(service_type, region) WHERE is_active = true;

-- Signing Keys table (EdDSA key pairs with AES-256-GCM encryption)
CREATE TABLE signing_keys (
    key_id VARCHAR(50) PRIMARY KEY,  -- Format: 'auth-{cluster}-{YYYY}-{NN}' (e.g., 'auth-us-2025-01')
    public_key TEXT NOT NULL,  -- PEM format
    private_key_encrypted BYTEA NOT NULL,  -- AES-256-GCM ciphertext
    encryption_nonce BYTEA NOT NULL,  -- 96-bit nonce (12 bytes)
    encryption_tag BYTEA NOT NULL,  -- 128-bit authentication tag (16 bytes)
    encryption_algorithm VARCHAR(50) NOT NULL DEFAULT 'AES-256-GCM',
    master_key_version INTEGER NOT NULL DEFAULT 1,  -- For rotation tracking
    algorithm VARCHAR(50) NOT NULL DEFAULT 'EdDSA',
    is_active BOOLEAN NOT NULL DEFAULT true,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_encryption_algorithm CHECK (encryption_algorithm = 'AES-256-GCM'),
    CONSTRAINT valid_signing_algorithm CHECK (algorithm IN ('EdDSA', 'RS256')),
    CONSTRAINT valid_nonce_size CHECK (octet_length(encryption_nonce) = 12),
    CONSTRAINT valid_tag_size CHECK (octet_length(encryption_tag) = 16),
    CONSTRAINT valid_date_range CHECK (valid_until > valid_from)
);

-- Indexes for signing_keys
CREATE INDEX idx_signing_keys_active ON signing_keys(is_active, valid_from, valid_until) WHERE is_active = true;
CREATE INDEX idx_signing_keys_valid_range ON signing_keys(valid_from, valid_until) WHERE is_active = true;
CREATE INDEX idx_signing_keys_master_key_version ON signing_keys(master_key_version);

-- Auth Events table (Audit log)
CREATE TABLE auth_events (
    event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(50) NOT NULL,  -- 'user_login', 'service_token_issued', 'key_rotated', etc.
    user_id UUID REFERENCES users(user_id),
    credential_id UUID REFERENCES service_credentials(credential_id),
    success BOOLEAN NOT NULL,
    failure_reason VARCHAR(255),
    ip_address INET,
    user_agent TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_event_type CHECK (event_type IN (
        'user_login',
        'user_login_failed',
        'service_token_issued',
        'service_token_failed',
        'service_registered',
        'key_generated',
        'key_rotated',
        'key_expired',
        'token_validation_failed',
        'rate_limit_exceeded'
    )),
    CONSTRAINT event_has_subject CHECK (
        user_id IS NOT NULL OR credential_id IS NOT NULL OR event_type IN ('key_generated', 'key_rotated', 'key_expired')
    )
);

-- Indexes for auth_events
CREATE INDEX idx_auth_events_created_at ON auth_events(created_at DESC);
CREATE INDEX idx_auth_events_user_id ON auth_events(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX idx_auth_events_credential_id ON auth_events(credential_id) WHERE credential_id IS NOT NULL;
CREATE INDEX idx_auth_events_type_success ON auth_events(event_type, success);
CREATE INDEX idx_auth_events_type_time ON auth_events(event_type, created_at DESC);
CREATE INDEX idx_auth_events_ip_address ON auth_events(ip_address) WHERE success = false;

-- Trigger for service_credentials updated_at
CREATE TRIGGER update_service_credentials_updated_at BEFORE UPDATE ON service_credentials
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- DOWN MIGRATION
-- ============================================================================

-- To rollback this migration, run the following in a separate transaction:
--
-- DROP TRIGGER IF EXISTS update_service_credentials_updated_at ON service_credentials;
-- DROP INDEX IF EXISTS idx_auth_events_ip_address;
-- DROP INDEX IF EXISTS idx_auth_events_type_time;
-- DROP INDEX IF EXISTS idx_auth_events_type_success;
-- DROP INDEX IF EXISTS idx_auth_events_credential_id;
-- DROP INDEX IF EXISTS idx_auth_events_user_id;
-- DROP INDEX IF EXISTS idx_auth_events_created_at;
-- DROP TABLE IF EXISTS auth_events;
-- DROP INDEX IF EXISTS idx_signing_keys_master_key_version;
-- DROP INDEX IF EXISTS idx_signing_keys_valid_range;
-- DROP INDEX IF EXISTS idx_signing_keys_active;
-- DROP TABLE IF EXISTS signing_keys;
-- DROP INDEX IF EXISTS idx_service_credentials_service_type;
-- DROP INDEX IF EXISTS idx_service_credentials_active;
-- DROP INDEX IF EXISTS idx_service_credentials_client_id;
-- DROP TABLE IF EXISTS service_credentials;
