-- Existing scores stay square records. New writes derive shape from their board.
ALTER TABLE scores ADD COLUMN shape INTEGER NOT NULL DEFAULT 4 CHECK (shape IN (3,4,5,6));
CREATE INDEX scores_shape_n_side_idx ON scores(shape,n,side,submitted_at,id);
