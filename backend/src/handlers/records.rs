use axum::Json;
use shared::KnownRecord;
use std::sync::LazyLock;

const BEST_KNOWN_JSON: &str = include_str!("../../../refs/best_known.json");

/// Best known results from the literature, compiled into the binary.
pub static BEST_KNOWN: LazyLock<Vec<KnownRecord>> = LazyLock::new(|| {
    let mut records: Vec<KnownRecord> =
        serde_json::from_str(BEST_KNOWN_JSON).expect("refs/best_known.json is valid");
    records.sort_by_key(|r| r.n);
    records
});

pub async fn records() -> Json<Vec<KnownRecord>> {
    Json(BEST_KNOWN.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_known_parses_and_is_consistent() {
        assert!(!BEST_KNOWN.is_empty());
        for pair in BEST_KNOWN.windows(2) {
            assert!(pair[0].n < pair[1].n, "duplicate n={}", pair[1].n);
            assert!(
                pair[0].side <= pair[1].side + 1e-12,
                "side must not shrink as n grows (n={})",
                pair[1].n
            );
        }
        for r in BEST_KNOWN.iter() {
            assert!(r.n >= 1);
            assert!(
                r.side >= (r.n as f64).sqrt() - 1e-12,
                "n={} below area bound",
                r.n
            );
        }
    }
}
