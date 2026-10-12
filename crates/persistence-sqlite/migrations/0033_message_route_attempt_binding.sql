-- DEV-02-08 HC-01: separate the override DECISION snapshot from the per-attempt
-- DELIVERY snapshot, so a reassigned Role that has since been rebound can still
-- receive the retry attempt Frozen routing requires.
--
-- Frozen basis: STATE_MACHINES.md section 4 ("ACTIVE -> DRAINING: queued
-- messages re-resolve at delivery time") and section 5 ("QUEUED -> ROUTED:
-- resolve the current ACTIVE Binding; create the attempt and its target Binding
-- snapshot"; "NEEDS_HUMAN -> QUEUED: new attempt, record route override");
-- DOMAIN_MODEL.md section 3/5 (a DeliveryAttempt is appended history and is
-- never rewritten) and DATA_CONVENTIONS sections 5/6 (attempt history is only
-- appended to; Binding generation increases monotonically for the same Role).
--
-- The defect this replaces: version 30's delivery_attempts_route_override_insert
-- required the new attempt to carry the override's own decision-time
-- target_binding_id/generation. After a legal rebind of the already-reassigned
-- Role, the current ACTIVE Binding is a different row with a higher generation,
-- so the only retry Frozen routing allows was rejected by the database
-- (SQLite code 1811, "Delivery attempt does not match the override it
-- adopted"). The rejection was correct under a decision-snapshot reading and
-- wrong under a delivery-snapshot reading; Frozen requires the latter.
--
-- What is NOT changed. The override row keeps its immutable decision snapshot
-- and stays byte-identical. No Message addressing, no old attempt, no Binding,
-- no Event and no migration 0001-0032 is altered. The route association is
-- still required on every attempt that adopts an override:
--   * route_override_id and route_override_number are both present or both NULL;
--   * the attempt carries a COMPLETE Binding snapshot (id and generation);
--   * the override belongs to this attempt's Team, this attempt's Message and
--     this attempt's override_number;
--   * the override's to_role_id is this attempt's target_role_id, so the
--     attempt still delivers to exactly the Role the override authorized.
-- The completeness clause is not a loosening: under version 30 an override
-- attempt could only match because both columns were compared for identity
-- against NOT NULL override columns, so a NULL binding id or generation was
-- already refused. The original-addressing path (route_override_id IS NULL)
-- keeps its existing rules untouched; per 0025 an attempt there may still have
-- no Binding at all.
-- 0017's delivery_attempts_target_binding_insert and 0025's
-- delivery_attempts_target_role_team_insert continue to require the attempt's
-- own Binding snapshot to name that Role inside that Team with a matching
-- generation, so a foreign, renamed or half-written Binding is still refused.
-- Which of the Role's bindings is ACTIVE at dispatch time, TTL, permissions and
-- subscriber visibility remain application execution-point duties and are
-- deliberately not invented here as database state.
--
-- Forward-only 0033; SQLx registers no_tx=false so the replacement trigger is
-- swapped atomically. Recovery is an audited forward repair or a consistent
-- backup, never a dropped guard or a deleted ledger row.

DROP TRIGGER delivery_attempts_route_override_insert;

CREATE TRIGGER delivery_attempts_route_override_insert
BEFORE INSERT ON delivery_attempts
FOR EACH ROW
WHEN (NEW.route_override_id IS NULL) != (NEW.route_override_number IS NULL)
  OR (NEW.route_override_id IS NOT NULL AND (
        NEW.target_binding_id IS NULL
     OR NEW.target_binding_generation IS NULL
     OR NOT EXISTS (
        SELECT 1 FROM message_route_overrides AS override
        WHERE override.id = NEW.route_override_id
          AND override.message_id = NEW.message_id
          AND override.team_id = NEW.team_id
          AND override.override_number = NEW.route_override_number
          AND override.to_role_id = NEW.target_role_id
      )
  ))
BEGIN
    SELECT RAISE(ABORT, 'Delivery attempt does not match the override it adopted');
END;
