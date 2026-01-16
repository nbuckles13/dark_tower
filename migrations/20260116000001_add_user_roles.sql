-- User Roles junction table
-- Phase 4: User provisioning and login support per ADR-0020

-- ============================================================================
-- UP MIGRATION
-- ============================================================================

-- User Roles table (junction table for user-role relationship)
CREATE TABLE user_roles (
    user_id UUID NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Composite primary key (user can have multiple roles, but each role once)
    PRIMARY KEY (user_id, role),

    -- Valid role values per ADR-0020
    CONSTRAINT valid_role CHECK (role IN ('user', 'admin', 'org_admin'))
);

-- Index on user_id for efficient role lookups
CREATE INDEX idx_user_roles_user_id ON user_roles(user_id);

-- ============================================================================
-- DOWN MIGRATION
-- ============================================================================

-- To rollback this migration, run the following in a separate transaction:
--
-- DROP INDEX IF EXISTS idx_user_roles_user_id;
-- DROP TABLE IF EXISTS user_roles;
