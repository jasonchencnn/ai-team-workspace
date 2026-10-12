-- DEV-02-07: retain the first stable error's whitelisted details on replay.
-- Existing keys, codes, results and timestamps are untouched; NULL legacy
-- details remain NULL. This adds storage, not new command or error semantics.
ALTER TABLE command_idempotency ADD COLUMN first_error_details TEXT
    CHECK (first_error_details IS NULL OR json_valid(first_error_details));

CREATE TRIGGER command_failure_details_insert
BEFORE INSERT ON command_idempotency
WHEN NEW.first_error_details IS NOT NULL AND NEW.state != 'FAILED'
BEGIN SELECT RAISE(ABORT, 'failure details require a failed command'); END;

CREATE TRIGGER command_failure_details_update
BEFORE UPDATE ON command_idempotency
WHEN (NEW.first_error_details IS NOT NULL AND NEW.state != 'FAILED')
  OR (OLD.state = 'FAILED' AND NEW.first_error_details IS NOT OLD.first_error_details)
BEGIN SELECT RAISE(ABORT, 'first failure details are immutable'); END;
