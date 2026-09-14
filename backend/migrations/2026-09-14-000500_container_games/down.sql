LOCK TABLE board_states, scores IN ACCESS EXCLUSIVE MODE;
DO $$ BEGIN
IF EXISTS (SELECT 1 FROM scores WHERE container <> 4) OR EXISTS (SELECT 1 FROM board_states WHERE left(code,1) = '8') THEN
RAISE EXCEPTION 'Cannot remove container support while polygon-container boards or scores exist';
END IF;
END $$;
DROP INDEX scores_container_shape_n_side_idx;
ALTER TABLE scores DROP COLUMN container;
