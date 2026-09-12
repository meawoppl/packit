//! Share links: a packing encoded as hex in the `s` query parameter of
//! `/play/:n`. Little-endian layout: version `u8`, `n` as `u16`, `side` as
//! `f64`, then `cx`, `cy`, `theta` as `f64` for each square. Full `f64`
//! precision keeps a validated packing exactly as it was measured.

use super::files;
use shared::{Arrangement, Placement, MAX_N};

const VERSION: u8 = 1;
const HEADER_BYTES: usize = 1 + 2 + 8;
const SQUARE_BYTES: usize = 3 * 8;

/// Hex length of a share code for `n` squares.
fn hex_len(n: u32) -> usize {
    2 * (HEADER_BYTES + SQUARE_BYTES * n as usize)
}

pub fn encode(a: &Arrangement) -> String {
    let mut bytes = Vec::with_capacity(hex_len(a.n) / 2);
    bytes.push(VERSION);
    bytes.extend_from_slice(&(a.n as u16).to_le_bytes());
    bytes.extend_from_slice(&a.side.to_le_bytes());
    for p in &a.squares {
        for v in [p.cx, p.cy, p.theta] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a share code for the `n` in the page path. The length is checked
/// before anything is allocated, and the result passes the same limits as a
/// JSON import.
pub fn decode(hex: &str, n: u32) -> Result<Arrangement, String> {
    if !(1..=MAX_N).contains(&n) {
        return Err(format!("n must be between 1 and {MAX_N}"));
    }
    if hex.len() != hex_len(n) {
        return Err(format!("This share link is not for {n} squares"));
    }
    let bytes = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            // `from_str_radix` alone would accept a leading '+' ("+1").
            pair.iter()
                .all(u8::is_ascii_hexdigit)
                .then(|| std::str::from_utf8(pair).ok())
                .flatten()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| "This share link is not valid hex".to_string())
        })
        .collect::<Result<Vec<u8>, _>>()?;
    let f64_at = |i: usize| f64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
    if bytes[0] != VERSION {
        return Err(format!("Unsupported share link version {}", bytes[0]));
    }
    if u16::from_le_bytes([bytes[1], bytes[2]]) as u32 != n {
        return Err(format!("This share link is not for {n} squares"));
    }
    let squares = (0..n as usize)
        .map(|i| {
            let at = HEADER_BYTES + i * SQUARE_BYTES;
            Placement {
                cx: f64_at(at),
                cy: f64_at(at + 8),
                theta: f64_at(at + 16),
            }
        })
        .collect();
    let arrangement = Arrangement {
        n,
        side: f64_at(3),
        squares,
    };
    files::check_arrangement(&arrangement, n)?;
    Ok(arrangement)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn five() -> Arrangement {
        let s = 2.0 + std::f64::consts::FRAC_1_SQRT_2;
        let sq = |cx, cy, theta| Placement { cx, cy, theta };
        Arrangement {
            n: 5,
            side: s,
            squares: vec![
                sq(0.5, 0.5, 0.0),
                sq(s - 0.5, 0.5, 0.0),
                sq(0.5, s - 0.5, 0.0),
                sq(s - 0.5, s - 0.5, 0.0),
                sq(s / 2.0, s / 2.0, std::f64::consts::FRAC_PI_4),
            ],
        }
    }

    #[test]
    fn round_trips_bit_exact() {
        let a = five();
        let hex = encode(&a);
        assert_eq!(hex.len(), hex_len(5));
        assert!(hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        let back = decode(&hex, 5).unwrap();
        assert_eq!(back, a);
        assert_eq!(back.side.to_bits(), a.side.to_bits());
    }

    #[test]
    fn rejects_wrong_length_n_and_version() {
        let hex = encode(&five());
        assert!(decode(&hex, 4).is_err(), "path n must match");
        assert!(decode(&hex[..hex.len() - 2], 5).is_err());
        assert!(decode(&format!("{hex}00"), 5).is_err());
        assert!(decode(&format!("02{}", &hex[2..]), 5).is_err());
        // Right length for n=4 but encoded n=5 in the header.
        let mut short = hex[..hex_len(4)].to_string();
        short.replace_range(0..2, "01");
        assert!(decode(&short, 4).is_err());
    }

    #[test]
    fn rejects_bad_hex_and_out_of_range_values() {
        let hex = encode(&five());
        assert!(decode(&hex.replacen('0', "g", 1), 5).is_err());
        // Same length, but "+1" in place of the version byte "01".
        assert!(decode(&format!("+1{}", &hex[2..]), 5).is_err());
        assert!(decode(&hex.replacen('0', "é", 1), 5).is_err());
        assert!(decode("", 1).is_err());
        assert!(
            decode(&"00".repeat(1_000_000), 5).is_err(),
            "oversized input"
        );
        assert!(decode(&encode(&five()), 0).is_err());
        let mut far = five();
        far.squares[0].cx = 1e9;
        assert!(decode(&encode(&far), 5).is_err());
        let mut nan = five();
        nan.squares[4].theta = f64::NAN;
        assert!(decode(&encode(&nan), 5).is_err());
        let mut tiny = five();
        tiny.side = 0.5;
        assert!(decode(&encode(&tiny), 5).is_err());
    }
}
