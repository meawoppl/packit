//! Board codes: a board state (a packing and its glue) encoded as hex. They
//! are what `board_states` stores, what Share and Submit send, and the `s`
//! query parameter of `/play/:n`. Little-endian layout: version `u8`, `n` as
//! `u16`, `side` as `f64`, then `cx`, `cy`, `theta` as `f64` for each square.
//! Full `f64` precision keeps a validated packing exactly as it was measured.
//!
//! A glue trailer follows only when there is at least one glue, so glue-free
//! codes keep the layout above: tag `u8` (`b'G'`), trailer version `u8` (2),
//! glue count `u16` (1 to `MAX_GLUES`), then each glue's two features as a
//! `u16` each: bits 0-1 kind (0 edge, 1 corner, 2 midpoint, 3 wall), bits 2-3
//! the edge, corner, or wall index, bits 4-10 the square (0 for a wall), and
//! bits 11-15 zero.

use crate::glue::{self, Feature, Glue, MAX_GLUES};
use crate::{Arrangement, Placement, MAX_N};

const VERSION: u8 = 1;
const HEADER_BYTES: usize = 1 + 2 + 8;
const SQUARE_BYTES: usize = 3 * 8;
const GLUE_TAG: u8 = b'G';
const GLUE_VERSION: u8 = 2;
const TRAILER_BYTES: usize = 1 + 1 + 2;
const FEATURE_BYTES: usize = 2;
const _: () = assert!(MAX_N <= 128, "square indices must fit in 7 bits");
/// Coordinate bound for loaded arrangements.
const MAX_COORD: f64 = 1000.0;

/// A board state: the packing and the glue between its features.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardState {
    pub arrangement: Arrangement,
    pub glues: Vec<Glue>,
}

/// Longest valid board code, for request size limits.
pub const MAX_LEN: usize = hex_len(MAX_N, MAX_GLUES);

/// Hex length of a board code for `n` squares and `glues` glues.
const fn hex_len(n: u32, glues: usize) -> usize {
    let trailer = if glues == 0 {
        0
    } else {
        TRAILER_BYTES + 2 * FEATURE_BYTES * glues
    };
    2 * (HEADER_BYTES + SQUARE_BYTES * n as usize + trailer)
}

pub fn encode(a: &Arrangement, glues: &[Glue]) -> String {
    let mut bytes = Vec::with_capacity(hex_len(a.n, glues.len()) / 2);
    bytes.push(if a.shape.is_square() {
        VERSION
    } else {
        a.shape.sides() as u8
    });
    bytes.extend_from_slice(&(a.n as u16).to_le_bytes());
    bytes.extend_from_slice(&a.side.to_le_bytes());
    for p in &a.squares {
        for v in [p.cx, p.cy, p.theta] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    if !glues.is_empty() {
        bytes.extend_from_slice(&[GLUE_TAG, if a.shape.is_square() { GLUE_VERSION } else { 3 }]);
        bytes.extend_from_slice(&(glues.len() as u16).to_le_bytes());
        for f in glues.iter().flat_map(|g| [g.a, g.b]) {
            let (kind, square, index) = match f {
                Feature::Edge { square, edge } => (0, square, edge),
                Feature::Corner { square, corner } => (1, square, corner),
                Feature::Midpoint { square, edge } => (2, square, edge),
                Feature::Wall(w) => (3, 0, w),
            };
            let bits = kind
                | ((index as u16) << 2)
                | ((square as u16) << if a.shape.is_square() { 4 } else { 5 });
            bytes.extend_from_slice(&bits.to_le_bytes());
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(hex: &[u8]) -> Result<Vec<u8>, String> {
    hex.as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            // `from_str_radix` alone would accept a leading '+' ("+1").
            pair.iter()
                .all(u8::is_ascii_hexdigit)
                .then(|| std::str::from_utf8(pair).ok())
                .flatten()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| "This board code is not valid hex".to_string())
        })
        .collect()
}

fn feature(bits: u16, polygon: bool) -> Result<Feature, String> {
    let unknown = || Err("This board code has an unknown glue feature".into());
    if bits >> if polygon { 12 } else { 11 } != 0 {
        return unknown();
    }
    let index = ((bits >> 2) & if polygon { 7 } else { 3 }) as u8;
    let square = (bits >> if polygon { 5 } else { 4 }) as usize;
    Ok(match bits & 3 {
        0 => Feature::Edge {
            square,
            edge: index,
        },
        1 => Feature::Corner {
            square,
            corner: index,
        },
        2 => Feature::Midpoint {
            square,
            edge: index,
        },
        _ if square == 0 => Feature::Wall(index),
        _ => return unknown(),
    })
}

/// Decode a board code for `n` squares. The length is checked
/// before anything is allocated, and the result passes the same limits as a
/// JSON import and as the play engine's glue.
pub fn decode(hex: &str, n: u32) -> Result<BoardState, String> {
    if !(1..=MAX_N).contains(&n) {
        return Err(format!("n must be between 1 and {MAX_N}"));
    }
    let hex = hex.as_bytes();
    let version = from_hex(hex.get(..2).ok_or("Missing board version")?)?[0];
    let shape = if version == VERSION {
        crate::Shape::Square
    } else {
        crate::Shape::from_sides(version)
            .filter(|s| !s.is_square())
            .ok_or("Unsupported board code version")?
    };
    let base = hex_len(n, 0);
    // The trailer's own header gives the glue count, so the full length is
    // known before the rest of the code is decoded.
    let count = if hex.len() == base {
        0
    } else {
        let Some(trailer) = hex.get(base..base + 2 * TRAILER_BYTES) else {
            return Err(format!("This board code is not for {n} squares"));
        };
        let trailer = from_hex(trailer)?;
        if trailer[..2] != [GLUE_TAG, if shape.is_square() { GLUE_VERSION } else { 3 }] {
            return Err("This board code has unsupported glue data".into());
        }
        let count = u16::from_le_bytes([trailer[2], trailer[3]]) as usize;
        if !(1..=MAX_GLUES).contains(&count) {
            return Err("This board code has an invalid glue count".into());
        }
        if hex.len() != hex_len(n, count) {
            return Err("This board code's glue data has the wrong length".into());
        }
        count
    };
    let bytes = from_hex(hex)?;
    let f64_at = |i: usize| f64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
    if bytes[0] != version {
        return Err(format!("Unsupported board code version {}", bytes[0]));
    }
    if u16::from_le_bytes([bytes[1], bytes[2]]) as u32 != n {
        return Err(format!("This board code is not for {n} squares"));
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
        shape,
        n,
        side: f64_at(3),
        squares,
    };
    check_arrangement(&arrangement, n)?;
    let features = base / 2 + TRAILER_BYTES;
    let feature_at = |i: usize| {
        let at = features + i * FEATURE_BYTES;
        feature(
            u16::from_le_bytes([bytes[at], bytes[at + 1]]),
            !shape.is_square(),
        )
    };
    let glues = (0..count)
        .map(|i| {
            Ok(Glue {
                a: feature_at(2 * i)?,
                b: feature_at(2 * i + 1)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    glue::check_for(&glues, n as usize, shape)?;
    Ok(BoardState { arrangement, glues })
}

/// Limits every loaded arrangement must meet (JSON import and board codes):
/// `n` squares, a finite side in `[1, 1000]`, and finite bounded coordinates.
pub fn check_arrangement(arr: &Arrangement, n: u32) -> Result<(), String> {
    let in_range = |v: f64| v.is_finite() && v.abs() <= MAX_COORD;
    let ok = (1..=MAX_N).contains(&n)
        && arr.n == n
        && arr.squares.len() == n as usize
        && arr.side.is_finite()
        && (arr.shape.min_side()..=MAX_COORD).contains(&arr.side)
        && arr
            .squares
            .iter()
            .all(|p| in_range(p.cx) && in_range(p.cy) && p.theta.is_finite());
    if ok {
        Ok(())
    } else {
        Err(format!("Expected {n} pieces with finite coordinates"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn five() -> Arrangement {
        let s = 2.0 + std::f64::consts::FRAC_1_SQRT_2;
        let sq = |cx, cy, theta| Placement { cx, cy, theta };
        Arrangement {
            shape: crate::Shape::Square,
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

    /// Every pairing of feature kinds, walls included.
    fn glues() -> Vec<Glue> {
        let kinds = |square: usize, k: u8| {
            [
                Feature::Edge { square, edge: k },
                Feature::Corner { square, corner: k },
                Feature::Midpoint { square, edge: k },
                Feature::Wall(k),
            ]
        };
        let mut glues = Vec::new();
        for (i, a) in kinds(1, 1).into_iter().enumerate() {
            for (j, b) in kinds(4, 3).into_iter().enumerate() {
                if i != 3 || j != 3 {
                    glues.push(Glue { a, b });
                }
            }
        }
        glues
    }

    /// `hex` with the byte at `at` replaced.
    fn with_byte(hex: &str, at: usize, byte: u8) -> String {
        let mut hex = hex.to_string();
        hex.replace_range(2 * at..2 * at + 2, &format!("{byte:02x}"));
        hex
    }

    /// `hex`, a code for `five()`, with its `i`th glue feature set to `bits`.
    fn with_feature(hex: &str, i: usize, bits: u16) -> String {
        let at = hex_len(5, 0) / 2 + TRAILER_BYTES + i * FEATURE_BYTES;
        let [lo, hi] = bits.to_le_bytes();
        with_byte(&with_byte(hex, at, lo), at + 1, hi)
    }

    #[test]
    fn round_trips_bit_exact() {
        let a = five();
        let hex = encode(&a, &[]);
        assert_eq!(hex.len(), hex_len(5, 0));
        assert!(hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        let back = decode(&hex, 5).unwrap();
        assert_eq!(back.arrangement, a);
        assert!(back.glues.is_empty());
        assert_eq!(back.arrangement.side.to_bits(), a.side.to_bits());
    }

    #[test]
    fn round_trips_every_feature_kind() {
        let glues = glues();
        assert_eq!(glues.len(), 15);
        let hex = encode(&five(), &glues);
        assert_eq!(hex.len(), hex_len(5, 15));
        assert!(hex.starts_with(&encode(&five(), &[])));
        assert_eq!(
            decode(&hex, 5).unwrap(),
            BoardState {
                arrangement: five(),
                glues
            }
        );
    }

    #[test]
    fn glue_free_codes_keep_the_original_layout() {
        // A code written before glue existed: one unit square at (1, 1) in a
        // box of side 2.
        let old = concat!(
            "01",
            "0100",
            "0000000000000040",
            "000000000000f03f",
            "000000000000f03f",
            "0000000000000000"
        );
        let snapshot = decode(old, 1).unwrap();
        assert!(snapshot.glues.is_empty());
        assert_eq!(snapshot.arrangement.side, 2.0);
        assert_eq!(
            snapshot.arrangement.squares,
            [Placement {
                cx: 1.0,
                cy: 1.0,
                theta: 0.0
            }]
        );
        assert_eq!(encode(&snapshot.arrangement, &[]), old);
    }

    #[test]
    fn packs_each_feature_into_two_bytes() {
        let hex = encode(&five(), &glues()[..1]);
        // Tag, version 2, one glue, then Edge { 1, 1 } and Edge { 4, 3 }.
        assert_eq!(&hex[hex_len(5, 0)..], concat!("47020100", "1400", "4c00"));
        assert_eq!(hex.len(), hex_len(5, 1));
        assert_eq!(MAX_LEN, 37_598);
    }

    #[test]
    fn rejects_malformed_glue_trailers() {
        let base = encode(&five(), &[]);
        let one = &glues()[..1];
        let hex = encode(&five(), one);
        let trailer = base.len() / 2;
        assert!(decode(&hex, 5).is_ok());
        // Edge { square: 1, edge: 3 } against the first feature's square 1.
        let self_glue = with_feature(&hex, 1, (3 << 2) | (1 << 4));
        let duplicate = encode(&five(), &[one[0], one[0]]);
        let reversed = encode(
            &five(),
            &[
                one[0],
                Glue {
                    a: one[0].b,
                    b: one[0].a,
                },
            ],
        );
        let mut cases = vec![
            (format!("{base}4702"), "truncated trailer header"),
            (format!("{base}47020000"), "zero glue count"),
            (hex[..hex.len() - 2].to_string(), "truncated glue"),
            (format!("{hex}00"), "trailing byte"),
            (format!("{hex}00000000"), "an extra glue's bytes"),
            (with_byte(&hex, trailer, b'H'), "unknown tag"),
            (with_byte(&hex, trailer + 1, 1), "the unshipped version 1"),
            (with_byte(&hex, trailer + 1, 3), "unknown trailer version"),
            (
                with_byte(&hex, trailer + 2, 2),
                "count larger than the data",
            ),
            (with_feature(&hex, 0, 5 << 4), "square equal to n"),
            (with_feature(&hex, 0, 127 << 4), "largest square beyond n"),
            (with_feature(&hex, 0, 3 | (1 << 4)), "wall with a square"),
            (
                with_feature(&hex, 0, 3 | (127 << 4)),
                "wall with square bits",
            ),
            (self_glue, "self glue"),
            (duplicate, "duplicate"),
            (reversed, "reversed duplicate"),
            (hex.replacen("47", "4g", 1), "bad hex"),
        ];
        for bit in 11..16 {
            // Edge { square: 1, edge: 1 } plus one reserved bit.
            cases.push((with_feature(&hex, 0, 0x14 | (1 << bit)), "a reserved bit"));
        }
        for (bad, why) in cases {
            assert!(decode(&bad, 5).is_err(), "{why}: {bad}");
        }
    }

    #[test]
    fn rejects_oversized_glue_counts_before_decoding() {
        let base = encode(&five(), &[]);
        let too_many = (MAX_GLUES as u16 + 1).to_le_bytes();
        // The count is rejected from the trailer header alone, even though
        // the rest of the code is far too short to hold that many glues.
        let over = format!("{base}4702{:02x}{:02x}", too_many[0], too_many[1]);
        assert_eq!(
            decode(&over, 5),
            Err("This board code has an invalid glue count".into())
        );
        assert!(decode(&format!("{base}4702ffff"), 5).is_err());
        let max = (MAX_GLUES as u16).to_le_bytes();
        assert_eq!(
            decode(&format!("{base}4702{:02x}{:02x}", max[0], max[1]), 5),
            Err("This board code's glue data has the wrong length".into())
        );
    }

    #[test]
    fn rejects_wrong_length_n_and_version() {
        let hex = encode(&five(), &[]);
        assert!(decode(&hex, 4).is_err(), "path n must match");
        assert!(decode(&hex[..hex.len() - 2], 5).is_err());
        assert!(decode(&format!("{hex}00"), 5).is_err());
        assert!(decode(&format!("02{}", &hex[2..]), 5).is_err());
        // Right length for n=4 but encoded n=5 in the header.
        let mut short = hex[..hex_len(4, 0)].to_string();
        short.replace_range(0..2, "01");
        assert!(decode(&short, 4).is_err());
    }

    #[test]
    fn rejects_bad_hex_and_out_of_range_values() {
        let hex = encode(&five(), &[]);
        assert!(decode(&hex.replacen('0', "g", 1), 5).is_err());
        // Same length, but "+1" in place of the version byte "01".
        assert!(decode(&format!("+1{}", &hex[2..]), 5).is_err());
        assert!(decode(&hex.replacen('0', "é", 1), 5).is_err());
        assert!(decode(&format!("{hex}é0000000"), 5).is_err());
        assert!(decode("", 1).is_err());
        assert!(
            decode(&"00".repeat(1_000_000), 5).is_err(),
            "oversized input"
        );
        assert!(decode(&encode(&five(), &[]), 0).is_err());
        let mut far = five();
        far.squares[0].cx = 1e9;
        assert!(decode(&encode(&far, &[]), 5).is_err());
        let mut nan = five();
        nan.squares[4].theta = f64::NAN;
        assert!(decode(&encode(&nan, &[]), 5).is_err());
        let mut tiny = five();
        tiny.side = 0.5;
        assert!(decode(&encode(&tiny, &[]), 5).is_err());
    }
}

#[cfg(test)]
mod polygon_tests {
    use super::*;
    #[test]
    fn every_shape_and_last_feature_round_trips() {
        for shape in crate::Shape::ALL {
            let a = Arrangement {
                shape,
                n: 1,
                side: 3.0,
                squares: vec![crate::Placement {
                    cx: 1.5,
                    cy: 1.5,
                    theta: 0.2,
                }],
            };
            let glue = Glue {
                a: Feature::Corner {
                    square: 0,
                    corner: (shape.sides() - 1) as u8,
                },
                b: Feature::Wall(2),
            };
            let code = encode(&a, &[glue]);
            let decoded = decode(&code, 1).unwrap();
            assert_eq!(decoded.arrangement, a);
            assert_eq!(decoded.glues, vec![glue]);
            if !shape.is_square() {
                let mut bytes = from_hex(code.as_bytes()).unwrap();
                let index = 11 + 24 + 4;
                for bit in 12..16 {
                    let mut bad = bytes.clone();
                    let word = u16::from_le_bytes([bad[index], bad[index + 1]]) | (1 << bit);
                    bad[index..index + 2].copy_from_slice(&word.to_le_bytes());
                    let hex: String = bad.iter().map(|x| format!("{x:02x}")).collect();
                    assert!(decode(&hex, 1).is_err());
                }
                bytes[0] = 2;
                assert!(decode(
                    &bytes.iter().map(|x| format!("{x:02x}")).collect::<String>(),
                    1
                )
                .is_err());
            }
        }
    }
}
