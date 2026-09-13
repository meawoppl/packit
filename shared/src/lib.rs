use serde::{Deserialize, Serialize};
use uuid::Uuid;
use ws_bridge::WsEndpoint;

pub mod geometry;
pub mod share;

// ---------------------------------------------------------------------------
// Packing model — the coordinate conventions every crate shares
// ---------------------------------------------------------------------------

/// Largest square count accepted for play and leaderboard submissions.
pub const MAX_N: u32 = 100;

/// Overlap / containment slack the server allows when validating a submission.
pub const VALIDATION_TOL: f64 = 1e-9;

/// One unit square: center `(cx, cy)` and counter-clockwise rotation `theta`
/// in radians.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub cx: f64,
    pub cy: f64,
    pub theta: f64,
}

/// `n` unit squares packed into the container `[0, side] x [0, side]`,
/// origin at bottom-left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arrangement {
    pub n: u32,
    pub side: f64,
    pub squares: Vec<Placement>,
}

/// A best known (or proven optimal) result from the literature, loaded from
/// `refs/best_known.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownRecord {
    pub n: u32,
    pub side: f64,
    /// Closed form of `side` when one is known, e.g. `"2 + 1/sqrt(2)"`.
    pub side_expr: Option<String>,
    pub proven_optimal: bool,
    pub source: String,
    /// Who found the current best-known packing, from `refs/credits.json`.
    /// Empty for the plain grid packings that no source credits.
    #[serde(default)]
    pub packing_by: Vec<String>,
    /// The sources disagree on who found it; `packing_by` lists every
    /// candidate rather than picking one.
    #[serde(default)]
    pub packing_disputed: bool,
    /// Who proved `side` optimal; empty unless proven by someone named.
    #[serde(default)]
    pub proof_by: Vec<String>,
    /// Optimal trivially: a perfect square, packed as a grid.
    #[serde(default)]
    pub proof_trivial: bool,
}

// ---------------------------------------------------------------------------
// WebSocket endpoint definition — single source of truth for server + client
// ---------------------------------------------------------------------------

/// The main application WebSocket endpoint.
pub struct AppSocket;

impl WsEndpoint for AppSocket {
    const PATH: &'static str = "/ws";
    type ServerMsg = ServerMsg;
    type ClientMsg = ClientMsg;
}

/// Messages sent from the server to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    /// Heartbeat to keep connection alive
    Heartbeat,

    /// Error from server
    Error { message: String },

    /// Server is shutting down
    ServerShutdown {
        reason: String,
        reconnect_delay_ms: u64,
    },
}

/// Messages sent from the client to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    /// Ping — server should respond with Heartbeat
    Ping,
}

// ---------------------------------------------------------------------------
// HTTP API types
// ---------------------------------------------------------------------------

/// Health check response from `/api/health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

/// Body of `POST /api/scores`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubmitScore {
    pub player: String,
    pub arrangement: Arrangement,
}

/// One leaderboard row, returned by `POST /api/scores` and `GET /api/scores`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreEntry {
    pub id: Uuid,
    pub player: String,
    pub n: u32,
    pub side: f64,
    pub submitted_at: chrono::NaiveDateTime,
    /// 1-based position among all submissions for this `n` (smaller side wins).
    pub rank: u32,
}

/// Query string for `GET /api/scores`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoresQuery {
    pub n: Option<u32>,
    pub limit: Option<u32>,
}

/// Response of `GET /api/scores/:id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreDetail {
    pub entry: ScoreEntry,
    pub arrangement: Arrangement,
}

/// JSON error body returned by API endpoints on failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

/// A snapshot to shorten: the existing bit-preserving share code and its n.
/// This accepts unfinished packings and does not create a leaderboard score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateShare {
    pub n: u32,
    pub code: String,
}

/// Permanent public URL of an immutable solution snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortShare {
    pub url: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(v: T) {
        let json = serde_json::to_string(&v).unwrap();
        let parsed: T = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, v);
    }

    fn sample_arrangement() -> Arrangement {
        Arrangement {
            n: 2,
            side: 2.0,
            squares: vec![
                Placement {
                    cx: 0.5,
                    cy: 0.5,
                    theta: 0.0,
                },
                Placement {
                    cx: 1.5,
                    cy: 0.5,
                    theta: 0.0,
                },
            ],
        }
    }

    fn sample_entry() -> ScoreEntry {
        ScoreEntry {
            id: Uuid::new_v4(),
            player: "ada".to_string(),
            n: 2,
            side: 2.0,
            submitted_at: chrono::DateTime::from_timestamp(1_700_000_000, 0)
                .unwrap()
                .naive_utc(),
            rank: 1,
        }
    }

    #[test]
    fn server_msg_heartbeat_roundtrip() {
        let msg = ServerMsg::Heartbeat;
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMsg = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ServerMsg::Heartbeat));
    }

    #[test]
    fn server_msg_error_roundtrip() {
        let msg = ServerMsg::Error {
            message: "something broke".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMsg = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMsg::Error { message } => assert_eq!(message, "something broke"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn server_msg_shutdown_roundtrip() {
        let msg = ServerMsg::ServerShutdown {
            reason: "restarting".to_string(),
            reconnect_delay_ms: 1000,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ServerMsg = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMsg::ServerShutdown {
                reason,
                reconnect_delay_ms,
            } => {
                assert_eq!(reason, "restarting");
                assert_eq!(reconnect_delay_ms, 1000);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn client_msg_ping_roundtrip() {
        let msg = ClientMsg::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClientMsg = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ClientMsg::Ping));
    }

    #[test]
    fn arrangement_roundtrip() {
        roundtrip(sample_arrangement());
    }

    #[test]
    fn known_record_roundtrip() {
        roundtrip(KnownRecord {
            n: 5,
            side: 2.0 + std::f64::consts::FRAC_1_SQRT_2,
            side_expr: Some("2 + 1/sqrt(2)".to_string()),
            proven_optimal: true,
            source: "Göbel 1979".to_string(),
            packing_by: vec!["Frits Göbel".to_string()],
            packing_disputed: false,
            proof_by: vec!["Frits Göbel".to_string()],
            proof_trivial: false,
        });
    }

    #[test]
    fn submit_score_roundtrip() {
        roundtrip(SubmitScore {
            player: "ada".to_string(),
            arrangement: sample_arrangement(),
        });
    }

    #[test]
    fn score_entry_roundtrip() {
        roundtrip(sample_entry());
    }

    #[test]
    fn score_detail_roundtrip() {
        roundtrip(ScoreDetail {
            entry: sample_entry(),
            arrangement: sample_arrangement(),
        });
    }

    #[test]
    fn scores_query_roundtrip() {
        roundtrip(ScoresQuery {
            n: Some(5),
            limit: None,
        });
    }

    #[test]
    fn api_error_roundtrip() {
        roundtrip(ApiError {
            error: "bad".to_string(),
        });
    }
}
