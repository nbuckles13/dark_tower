-- Validate the auth_events constraints swapped NOT VALID in
-- 20261004000001_auth_events_admin_event_types.sql.
--
-- VALIDATE CONSTRAINT scans existing rows under SHARE UPDATE EXCLUSIVE, so audit
-- inserts continue during the scan. Both new constraints are strict supersets of
-- the old ones, so validation cannot fail on existing rows. Afterwards
-- pg_constraint.convalidated is true for both (asserted by
-- tests/auth_events_check_drift.rs).

SET LOCAL lock_timeout = '5s';

ALTER TABLE auth_events VALIDATE CONSTRAINT valid_event_type;
ALTER TABLE auth_events VALIDATE CONSTRAINT event_has_subject;

-- DOWN migration (manual rollback):
-- no-op to reverse (validation state only).
