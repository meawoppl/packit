diesel::table! {
    scores (id) {
        id -> Uuid,
        player -> Text,
        n -> Int4,
        side -> Float8,
        arrangement -> Jsonb,
        submitted_at -> Timestamp,
    }
}
