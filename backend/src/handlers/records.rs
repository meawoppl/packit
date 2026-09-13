use axum::Json;
use serde::Deserialize;
use shared::KnownRecord;
use std::sync::LazyLock;

const BEST_KNOWN_JSON: &str = include_str!("../../../refs/best_known.json");
const CREDITS_JSON: &str = include_str!("../../../refs/credits.json");

/// The parts of a `refs/credits.json` entry the API serves. `prior_credits`
/// (earlier or base packings) are never served as the record's finders.
#[derive(Deserialize)]
struct Credit {
    n: u32,
    packing_by: Option<Vec<Person>>,
    /// Sources disagree on the finder; `packing_by` lists every candidate.
    #[serde(default)]
    disputed: bool,
    proof_by: Option<Proof>,
}

#[derive(Deserialize)]
struct Person {
    name: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Proof {
    People(Vec<Person>),
    Marker(Marker),
}

#[derive(Deserialize)]
enum Marker {
    /// A perfect square, optimal as a grid.
    #[serde(rename = "trivial")]
    Trivial,
}

/// Names in source order, each once.
fn names(people: Vec<Person>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for person in people {
        if !names.contains(&person.name) {
            names.push(person.name);
        }
    }
    names
}

/// Best known results from the literature, compiled into the binary, with
/// the people credited for them.
pub static BEST_KNOWN: LazyLock<Vec<KnownRecord>> = LazyLock::new(|| {
    let mut records: Vec<KnownRecord> =
        serde_json::from_str(BEST_KNOWN_JSON).expect("refs/best_known.json is valid");
    records.sort_by_key(|r| r.n);
    let credits: Vec<Credit> =
        serde_json::from_str(CREDITS_JSON).expect("refs/credits.json is valid");
    for credit in credits {
        let Some(record) = records.iter_mut().find(|r| r.n == credit.n) else {
            continue;
        };
        record.packing_by = names(credit.packing_by.unwrap_or_default());
        record.packing_disputed = credit.disputed;
        match credit.proof_by {
            Some(Proof::People(people)) => record.proof_by = names(people),
            Some(Proof::Marker(Marker::Trivial)) => record.proof_trivial = true,
            None => {}
        }
    }
    records
});

pub fn for_shape(shape: shared::Shape) -> &'static Vec<KnownRecord> {
    match shape {
        shared::Shape::Square => &BEST_KNOWN,
        shared::Shape::Triangle => &TRIANGLE_RECORDS,
        shared::Shape::Pentagon => &PENTAGON_RECORDS,
        shared::Shape::Hexagon => &HEXAGON_RECORDS,
    }
}
static TRIANGLE_RECORDS: LazyLock<Vec<KnownRecord>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../refs/triangle_known.json"))
        .expect("valid polygon records")
});
static PENTAGON_RECORDS: LazyLock<Vec<KnownRecord>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../refs/pentagon_known.json"))
        .expect("valid polygon records")
});
static HEXAGON_RECORDS: LazyLock<Vec<KnownRecord>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../refs/hexagon_known.json"))
        .expect("valid polygon records")
});
#[derive(Deserialize, Default)]
pub struct RecordsQuery {
    #[serde(default)]
    shape: shared::Shape,
}
pub async fn records(
    axum::extract::Query(query): axum::extract::Query<RecordsQuery>,
) -> Json<Vec<KnownRecord>> {
    Json(for_shape(query.shape).clone())
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

    #[test]
    fn credits_cover_every_record_and_agree_with_it() {
        let credits: Vec<serde_json::Value> = serde_json::from_str(CREDITS_JSON).unwrap();
        let ns: Vec<u64> = credits.iter().map(|c| c["n"].as_u64().unwrap()).collect();
        let records: Vec<u64> = BEST_KNOWN.iter().map(|r| r.n as u64).collect();
        assert_eq!(ns, records, "credits.json covers exactly best_known.json");
        for (credit, record) in credits.iter().zip(BEST_KNOWN.iter()) {
            let n = record.n;
            let side = credit["side"].as_f64().unwrap();
            assert!((side - record.side).abs() < 1e-9, "n={n} side");
            assert_eq!(
                credit["proven_optimal"].as_bool(),
                Some(record.proven_optimal),
                "n={n}"
            );
            let credited = !record.proof_by.is_empty() || record.proof_trivial;
            assert_eq!(credited, record.proven_optimal, "n={n} proof credit");
            if record.proof_trivial {
                let root = (n as f64).sqrt().round() as u32;
                assert_eq!(root * root, n, "only perfect squares are trivial");
                assert!(record.proof_by.is_empty(), "n={n}");
            }
            if record.packing_disputed {
                assert!(record.packing_by.len() >= 2, "n={n} lists every candidate");
            }
        }
    }

    #[test]
    fn records_carry_finders_and_provers_but_not_prior_credits() {
        let at = |n: u32| BEST_KNOWN.iter().find(|r| r.n == n).unwrap();
        assert_eq!(at(10).packing_by, ["Frits Göbel"]);
        assert_eq!(at(10).proof_by, ["Walter Stromquist"]);
        assert!(!at(10).packing_disputed && !at(10).proof_trivial);
        // Sources disagree on n=67: both candidates, flagged, none picked.
        assert!(at(67).packing_disputed);
        assert_eq!(at(67).packing_by, ["Evert Stenlund", "Frits Göbel"]);
        assert!(at(67).proof_by.is_empty());
        // A perfect square is trivially optimal; nobody is named for it.
        assert!(at(4).proven_optimal && at(4).proof_trivial);
        assert!(at(4).proof_by.is_empty() && at(4).packing_by.is_empty());
        // Repeated names in the source appear once.
        let ellsworth = at(88).packing_by.iter().filter(|p| *p == "David Ellsworth");
        assert_eq!(ellsworth.count(), 1);
    }
}
