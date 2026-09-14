use chrono::NaiveDateTime;
use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::scores;

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = scores)]
pub struct Score {
    pub shape: i32,
    pub container: i32,
    pub id: Uuid,
    pub player: String,
    pub n: i32,
    pub side: f64,
    pub arrangement: serde_json::Value,
    pub submitted_at: NaiveDateTime,
    pub user_id: Option<Uuid>,
    pub board_token: String,
    pub glue_recorded: bool,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = scores)]
pub struct NewScore {
    pub shape: i32,
    pub container: i32,
    pub player: String,
    pub user_id: Uuid,
    pub n: i32,
    pub side: f64,
    pub arrangement: serde_json::Value,
    pub board_token: String,
}
