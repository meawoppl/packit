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

diesel::table! {
    solution_shares (token) {
        token -> Text,
        payload_hash -> Bytea,
        n -> Int4,
        code -> Text,
        created_at -> Timestamptz,
    }
}
