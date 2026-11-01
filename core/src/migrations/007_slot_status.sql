CREATE TRIGGER slot_decided_stays BEFORE UPDATE OF status ON slot
WHEN OLD.status IN ('declined', 'superseded') AND NEW.status <> OLD.status
BEGIN
    SELECT RAISE(ABORT, 'a decided slot does not reopen');
END;

CREATE TRIGGER slot_canonical_not_demoted BEFORE UPDATE OF status ON slot
WHEN OLD.status = 'canonical' AND NEW.status = 'proposed'
BEGIN
    SELECT RAISE(ABORT, 'a canonical slot is not set back to proposed');
END;
