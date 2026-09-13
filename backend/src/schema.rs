diesel::table! {
    passkeys (credential_id) {
        credential_id -> Bytea,
        user_id -> Uuid,
        passkey -> Jsonb,
        created_at -> Timestamptz,
        last_used_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    scores (id) {
        id -> Uuid,
        player -> Text,
        n -> Int4,
        side -> Float8,
        arrangement -> Jsonb,
        submitted_at -> Timestamp,
        user_id -> Nullable<Uuid>,
    }
}

diesel::table! {
    sessions (token_hash) {
        token_hash -> Bytea,
        user_id -> Uuid,
        created_at -> Timestamptz,
        expires_at -> Timestamptz,
    }
}

diesel::table! {
    solution_shares (token) {
        token -> Text,
        payload_hash -> Bytea,
        n -> Int4,
        code -> Text,
        created_at -> Timestamptz,
        created_by -> Nullable<Uuid>,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        username -> Text,
        kind -> Text,
        display_name -> Nullable<Text>,
        created_at -> Timestamptz,
    }
}

diesel::joinable!(passkeys -> users (user_id));
diesel::joinable!(scores -> users (user_id));
diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(solution_shares -> users (created_by));

diesel::allow_tables_to_appear_in_same_query!(passkeys, scores, sessions, solution_shares, users,);
