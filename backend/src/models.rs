use chrono::NaiveDateTime;
use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::scores;

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = scores)]
pub struct Score {
    pub id: Uuid,
    pub player: String,
    pub n: i32,
    pub side: f64,
    pub arrangement: serde_json::Value,
    pub submitted_at: NaiveDateTime,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = scores)]
pub struct NewScore {
    pub player: String,
    pub user_id: Uuid,
    pub n: i32,
    pub side: f64,
    pub arrangement: serde_json::Value,
}
