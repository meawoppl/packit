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
/// their rows in `users`.
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
}
