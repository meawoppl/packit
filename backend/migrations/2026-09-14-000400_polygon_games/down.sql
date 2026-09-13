LOCK TABLE board_states, scores IN ACCESS EXCLUSIVE MODE;
DO $$ BEGIN
IF EXISTS(SELECT 1 FROM scores WHERE shape <> 4) OR EXISTS(SELECT 1 FROM board_states WHERE left(code,2) <> '01') THEN
RAISE EXCEPTION 'Cannot remove polygon support while polygon scores or boards exist';
END IF;
END $$;
DROP INDEX scores_shape_n_side_idx;
ALTER TABLE scores DROP COLUMN shape;
