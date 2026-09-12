//! Typed wrappers around the backend HTTP API.

use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;
use shared::{ApiError, KnownRecord, ScoreDetail, ScoreEntry, SubmitScore};
use uuid::Uuid;

async fn decode<T: DeserializeOwned>(resp: Response) -> Result<T, String> {
    if resp.ok() {
        resp.json::<T>().await.map_err(|e| e.to_string())
    } else {
        match resp.json::<ApiError>().await {
            Ok(err) => Err(err.error),
            Err(_) => Err(format!("HTTP {}", resp.status())),
        }
    }
}

pub async fn submit_score(body: SubmitScore) -> Result<ScoreEntry, String> {
    let resp = Request::post("/api/scores")
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn list_scores(n: Option<u32>, limit: Option<u32>) -> Result<Vec<ScoreEntry>, String> {
    let mut query = Vec::new();
    if let Some(n) = n {
        query.push(format!("n={n}"));
    }
    if let Some(limit) = limit {
        query.push(format!("limit={limit}"));
    }
    let url = format!("/api/scores?{}", query.join("&"));
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn get_score(id: Uuid) -> Result<ScoreDetail, String> {
    let resp = Request::get(&format!("/api/scores/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn known_records() -> Result<Vec<KnownRecord>, String> {
    let resp = Request::get("/api/records")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}
