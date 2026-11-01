CREATE TRIGGER memory_owner_no_edit_more BEFORE UPDATE OF anchor, triggers, content_hash, valid_from, corpus_id, created_at ON memory_record
WHEN OLD.authority = 'owner'
BEGIN
    SELECT RAISE(ABORT, 'an owner record changes only by a new record that supersedes it');
END;

CREATE TRIGGER memory_owner_supersede_needs_successor BEFORE UPDATE OF status, superseded_by ON memory_record
WHEN OLD.authority = 'owner' AND NEW.status = 'superseded' AND NEW.superseded_by IS NULL
BEGIN
    SELECT RAISE(ABORT, 'an owner record is superseded only by a named successor');
END;

CREATE TRIGGER memory_owner_status_known BEFORE UPDATE OF status ON memory_record
WHEN OLD.authority = 'owner' AND NEW.status NOT IN ('active', 'stale', 'superseded')
BEGIN
    SELECT RAISE(ABORT, 'an owner record is only active, stale or superseded');
END;
