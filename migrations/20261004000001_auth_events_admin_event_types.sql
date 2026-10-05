-- Extend auth_events for admin credential audit and failed-registration throttling.
--
-- New event types (AuthEventType in crates/ac-service/src/models/mod.rs is the
-- vocabulary SSoT; tests/auth_events_check_drift.rs fails if the enum and this
-- CHECK disagree in either direction):
--   service_scopes_updated    admin PUT /api/v1/admin/clients/{id}
--   service_deactivated       admin DELETE (soft delete; active -> inactive only)
--   service_secret_rotated    admin POST .../{id}/rotate-secret
--   user_registered           user self-registration (was misrecorded as user_login)
--   user_registration_failed  refused registration attempt; feeds the per-IP limiter
--
-- user_registration_failed rows are subject-less (no user exists yet), so
-- event_has_subject gains an exemption for it beside the key_* types.
--
-- Both constraints are swapped NOT VALID: the new rule is enforced on every new
-- INSERT/UPDATE immediately, but the full-table validating scan is deferred to
-- 20261004000002 (VALIDATE CONSTRAINT runs under SHARE UPDATE EXCLUSIVE, so
-- audit inserts continue). Postgres cannot tell a relaxation from a tightening,
-- so a plain ADD CONSTRAINT would scan under the ACCESS EXCLUSIVE lock.
-- lock_timeout makes a long-held conflicting lock fail the migration loudly
-- instead of queueing every other session behind it; rerun the db-migrate Job.

SET LOCAL lock_timeout = '5s';

ALTER TABLE auth_events
    DROP CONSTRAINT valid_event_type,
    ADD CONSTRAINT valid_event_type CHECK (event_type IN (
        'user_login',
        'user_login_failed',
        'service_token_issued',
        'service_token_failed',
        'service_registered',
        'key_generated',
        'key_rotated',
        'key_expired',
        'token_validation_failed',
        'rate_limit_exceeded',
        'service_scopes_updated',
        'service_deactivated',
        'service_secret_rotated',
        'user_registered',
        'user_registration_failed'
    )) NOT VALID,
    DROP CONSTRAINT event_has_subject,
    ADD CONSTRAINT event_has_subject CHECK (
        user_id IS NOT NULL
        OR credential_id IS NOT NULL
        OR event_type IN ('key_generated', 'key_rotated', 'key_expired', 'user_registration_failed')
    ) NOT VALID;

-- DOWN migration (manual rollback):
-- SET LOCAL lock_timeout = '5s';
-- ALTER TABLE auth_events
--     DROP CONSTRAINT valid_event_type,
--     ADD CONSTRAINT valid_event_type CHECK (event_type IN (
--         'user_login', 'user_login_failed', 'service_token_issued',
--         'service_token_failed', 'service_registered', 'key_generated',
--         'key_rotated', 'key_expired', 'token_validation_failed',
--         'rate_limit_exceeded'
--     )),
--     DROP CONSTRAINT event_has_subject,
--     ADD CONSTRAINT event_has_subject CHECK (
--         user_id IS NOT NULL OR credential_id IS NOT NULL
--         OR event_type IN ('key_generated', 'key_rotated', 'key_expired')
--     );
--
-- This DOWN FAILS while any row with a new event_type exists (old
-- valid_event_type), and specifically on subject-less user_registration_failed
-- rows (old event_has_subject). Deleting audit rows to make it pass is not
-- acceptable, so schema rollback is a FORWARD FIX.
--
-- App-only rollback is safe WITHOUT touching the schema: the previous binary
-- writes only the original ten event types, which pass this superset CHECK.
-- A `git revert` of the commit that added this file must restore this file
-- (and 20261004000002) in the same revert commit: the db-migrate Job's plain
-- `sqlx migrate run` fails on an applied-but-missing version.
