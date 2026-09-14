-- Preserve existing square-container scores and their boards verbatim.
LOCK TABLE board_states, scores IN ACCESS EXCLUSIVE MODE;
ALTER TABLE scores ADD COLUMN container INTEGER NOT NULL DEFAULT 4 CHECK (container IN (3,4,5,6));
CREATE INDEX scores_container_shape_n_side_idx ON scores(container,shape,n,side,submitted_at,id);
