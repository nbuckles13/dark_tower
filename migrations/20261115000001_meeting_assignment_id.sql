-- Meeting assignment incarnation fence (ADR-0010 §3; docs/devloop-outputs/2026-11-15-meeting-end-safety/).
--
-- A meeting id has INCARNATIONS: an ended row is revived by the next join
-- (`atomic_assign`), and a join fails a row over to another MC when its MC
-- stops heartbeating. `NotifyMeetingEnded` used to name only
-- (meeting_id, region), so a late or duplicate notify could end a LATER
-- incarnation's live row (a split meeting). GC now mints a random
-- `assignment_id` for every MC assignment attempt, sends it to MC in
-- `AssignMeetingWithMhRequest`, and writes it here on insert, revive and
-- re-point. MC echoes it (with its controller id) in `NotifyMeetingEnded`,
-- and GC ends ONLY the row carrying both.
--
-- Existing rows are backfilled with fresh random ids. No MC holds those
-- values, so a notify can no longer end them. That is acceptable because
-- this migration ships together with an MC rollout: every MC process
-- restarts with a fresh controller id, the old ids stop heartbeating, and the
-- next join to any such meeting fails its row over (`atomic_assign`'s stale
-- arm); a row nobody re-joins is ended by the backstop
-- (`end_stale_assignments`).
--
-- The DEFAULT exists only for the backfill and is dropped in the same
-- migration: every writer must supply the id it sent to MC, and a writer that
-- forgets fails loudly on NOT NULL instead of minting an id no MC knows.
--
-- ROLLBACK (roll forward only; no down migration): a GC image older than this
-- change cannot INSERT assignments (NOT NULL, no default -> SQLSTATE 23502).
-- Before rolling GC (and, in lockstep, MC) back across it, apply a follow-up
-- migration FIRST: `ALTER COLUMN assignment_id SET DEFAULT gen_random_uuid()`
-- (or `DROP COLUMN assignment_id`). Order: that migration, then the image
-- rollback. Procedure: docs/runbooks/gc-deployment.md §Database Rollback
-- Considerations.

ALTER TABLE meeting_assignments
    ADD COLUMN assignment_id UUID NOT NULL DEFAULT gen_random_uuid();

ALTER TABLE meeting_assignments
    ALTER COLUMN assignment_id DROP DEFAULT;

COMMENT ON COLUMN meeting_assignments.assignment_id IS
    'Incarnation fence: GC-minted per MC assignment attempt, sent to MC in AssignMeetingWithMh; NotifyMeetingEnded ends the row only when controller id and this id both match';
