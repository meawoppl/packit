//! Username normalization and the reserved-name list.

/// Returned for taken and reserved names alike.
pub const UNAVAILABLE: &str = "That username isn't available";

pub const INVALID: &str =
    "Usernames are 3 to 24 letters, digits, _ or -, starting with a letter or digit";

const RESERVED: &[&str] = &["admin", "root", "system", "packit", "api"];

/// Trim and ASCII-lowercase `raw`, then require
/// `^[a-z0-9][a-z0-9_-]{2,23}$`. Anything non-ASCII is rejected rather than
/// folded.
pub fn normalize(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if !name.is_ascii() {
        return Err(INVALID);
    }
    let name = name.to_ascii_lowercase();
    let bytes = name.as_bytes();
    let valid = (3..=24).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid {
        Ok(name)
    } else {
        Err(INVALID)
    }
}

/// Names no player may take. Credited profiles are reserved separately, by
/// the rows the passkey migration seeds into `users`.
pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_lowercases() {
        for (raw, want) in [
            ("ada", "ada"),
            ("  Ada_99 ", "ada_99"),
            ("\tGRACE-H\n", "grace-h"),
            ("0xdead", "0xdead"),
            ("abc", "abc"),
            (&"a".repeat(24), &"a".repeat(24)),
            ("a__", "a__"),
            ("9-_", "9-_"),
        ] {
            assert_eq!(normalize(raw).as_deref(), Ok(want), "{raw:?}");
        }
    }

    #[test]
    fn rejects_bad_shapes() {
        for raw in [
            "",
            "   ",
            "ab",
            " ab ",
            &"a".repeat(25),
            "_ada",
            "-ada",
            "ada lovelace",
            "ada.l",
            "ada@x",
            "ada/../x",
            "ada\u{0}",
        ] {
            assert_eq!(normalize(raw), Err(INVALID), "{raw:?}");
        }
    }

    #[test]
    fn rejects_non_ascii() {
        for raw in [
            "adé",
            "\u{0430}da",       // Cyrillic a
            "\u{ff41}da",       // fullwidth a
            "ada\u{200b}",      // zero-width space
            "\u{212a}elvin",    // Kelvin sign, lowercases to k in Unicode
            "\u{a0}ada\u{a0}x", // no-break spaces
        ] {
            assert_eq!(normalize(raw), Err(INVALID), "{raw:?}");
        }
    }

    #[test]
    fn reserved_names_match_after_normalization() {
        for raw in ["admin", "ROOT", " System ", "Packit", "API"] {
            assert!(is_reserved(&normalize(raw).unwrap()), "{raw}");
        }
        assert!(!is_reserved("ada"));
        assert!(!is_reserved("admin2"));
    }

    /// Everyone named in refs/credits.json has a seeded credited profile,
    /// and every seeded profile is someone named there.
    #[test]
    fn credited_profiles_match_the_migration_seed() {
        use serde_json::Value;
        use std::collections::{BTreeMap, BTreeSet};
        use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

        let credits: Vec<Value> =
            serde_json::from_str(include_str!("../../../refs/credits.json")).unwrap();
        let mut people = BTreeSet::new();
        for entry in &credits {
            for field in ["packing_by", "proof_by", "other_proofs", "prior_credits"] {
                match &entry[field] {
                    Value::Null => {}
                    Value::String(s) => assert_eq!(s, "trivial", "n={} {field}", entry["n"]),
                    Value::Array(list) => {
                        for person in list {
                            people.insert(person["name"].as_str().unwrap().to_string());
                        }
                    }
                    other => panic!("n={} {field}: {other}", entry["n"]),
                }
            }
        }

        // ö, ü and ä become oe, ue and ae; other diacritics are dropped.
        let fold = |s: &str| {
            let mut out = String::new();
            for c in s.chars() {
                match c {
                    'ö' | 'Ö' => out.push_str("oe"),
                    'ü' | 'Ü' => out.push_str("ue"),
                    'ä' | 'Ä' => out.push_str("ae"),
                    c => out.push(c),
                }
            }
            out.nfkd()
                .filter(|c| !is_combining_mark(*c))
                .collect::<String>()
                .to_lowercase()
        };
        let surname = |name: &str| fold(name.split_whitespace().last().unwrap());
        let mut derived = BTreeMap::new();
        for name in &people {
            let base = surname(name);
            let shared = people.iter().filter(|p| surname(p) == base).count() > 1;
            let username = if shared {
                format!("{base}-{}", fold(name).chars().next().unwrap())
            } else {
                base
            };
            assert_eq!(
                normalize(&username).as_deref(),
                Ok(username.as_str()),
                "{name}"
            );
            assert!(!is_reserved(&username), "{name}");
            assert!(
                derived.insert(username, name.clone()).is_none(),
                "{name} collides"
            );
        }

        let up = include_str!("../../migrations/2026-09-14-000100_passkey_auth/up.sql");
        let seeded: BTreeMap<String, String> = up
            .lines()
            .filter_map(|line| line.trim().strip_prefix("(gen_random_uuid(), "))
            .map(|row| {
                // 'username', 'credited', 'Display Name'),
                let quoted: Vec<&str> = row.split('\'').collect();
                assert_eq!(quoted.len(), 7, "{row}");
                assert_eq!(quoted[3], "credited", "{row}");
                (quoted[1].to_string(), quoted[5].to_string())
            })
            .collect();
        assert!(!seeded.is_empty());
        assert_eq!(seeded, derived);
    }
}
