//! Link previews for share links: `/play/:n?s=` pages carry Open Graph and
//! Twitter metadata, and `/api/preview.png` draws the shared packing.

use crate::handlers::records::BEST_KNOWN;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use shared::{geometry, share, Arrangement, MAX_N, VALIDATION_TOL};
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

/// Trunk's built page, the same file memory-serve embeds, so the hashed
/// script and SRI tags are served unchanged.
const INDEX_HTML: &str = include_str!("../../../frontend/dist/index.html");
pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 630;
const PAD: f32 = 40.0;
/// The play screen's square colors.
const PALETTE: [[u8; 3]; 5] = [
    [0xc7, 0xf3, 0x6b],
    [0x7d, 0xd5, 0xce],
    [0xb2, 0xa0, 0xef],
    [0xf0, 0xb5, 0x78],
    [0x8d, 0xab, 0xf2],
];

#[derive(Deserialize)]
pub struct PlayQuery {
    s: Option<String>,
}

#[derive(Deserialize)]
pub struct PreviewQuery {
    n: u32,
    s: String,
}

/// The SPA page for `/play/:n`, with metadata describing the shared packing
/// when `s` decodes, and generic metadata otherwise.
pub async fn play(
    State(state): State<Arc<AppState>>,
    Path(n): Path<String>,
    query: Option<Query<PlayQuery>>,
) -> impl IntoResponse {
    let n = n.parse().ok().filter(|n| (1..=MAX_N).contains(n));
    let code = query.and_then(|Query(q)| q.s);
    let shared = n
        .zip(code)
        .and_then(|(n, s)| share::decode(&s, n).ok().map(|snap| (snap.arrangement, s)));
    let tags = meta_tags(
        &state.public_url,
        n,
        shared.as_ref().map(|(a, s)| (a, s.as_str())),
    );
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(INDEX_HTML.replacen("</head>", &format!("{tags}</head>"), 1)),
    )
}

/// PNG of a shared packing. The URL carries the whole validated payload, so
/// the image never changes and may be cached forever.
pub async fn preview_png(query: Option<Query<PreviewQuery>>) -> Response {
    let png = query
        .ok_or_else(|| "n and s are required".to_string())
        .and_then(|Query(q)| share::decode(&q.s, q.n))
        .and_then(|snap| render(&snap.arrangement));
    match png {
        Ok(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            png,
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            [(header::CACHE_CONTROL, "no-store")],
            e,
        )
            .into_response(),
    }
}

fn meta_tags(public_url: &str, n: Option<u32>, shared: Option<(&Arrangement, &str)>) -> String {
    let (title, description, url, image) = match (n, shared) {
        (Some(n), Some((a, code))) => (
            format!("{n} squares in a {:.4} box", a.side),
            describe(a),
            format!("{public_url}/play/{n}?s={code}"),
            Some(format!("{public_url}/api/preview.png?n={n}&s={code}")),
        ),
        (Some(n), None) => (
            format!("Pack {n} unit squares"),
            format!("Pack {n} unit squares into the smallest square you can."),
            format!("{public_url}/play/{n}"),
            None,
        ),
        (None, _) => (
            "packit".to_string(),
            "Pack unit squares into the smallest square you can.".to_string(),
            format!("{public_url}/"),
            None,
        ),
    };
    let card = if image.is_some() {
        "summary_large_image"
    } else {
        "summary"
    };
    let mut tags = vec![
        ("name", "description", description.clone()),
        ("property", "og:site_name", "packit".to_string()),
        ("property", "og:type", "website".to_string()),
        ("property", "og:title", title.clone()),
        ("property", "og:description", description.clone()),
        ("property", "og:url", url),
        ("name", "twitter:card", card.to_string()),
        ("name", "twitter:title", title),
        ("name", "twitter:description", description),
    ];
    if let Some(image) = image {
        tags.extend([
            ("property", "og:image", image.clone()),
            ("property", "og:image:type", "image/png".to_string()),
            ("property", "og:image:width", WIDTH.to_string()),
            ("property", "og:image:height", HEIGHT.to_string()),
            ("name", "twitter:image", image),
        ]);
    }
    tags.iter()
        .map(|(attr, key, value)| {
            format!("<meta {attr}=\"{key}\" content=\"{}\">\n", escape(value))
        })
        .collect()
}

fn describe(a: &Arrangement) -> String {
    let kind = if geometry::validate(a, VALIDATION_TOL).is_ok() {
        "A valid"
    } else {
        "An unverified"
    };
    let best = BEST_KNOWN
        .iter()
        .find(|r| r.n == a.n)
        .map(|r| format!(" Best known: {:.6}.", r.side))
        .unwrap_or_default();
    format!(
        "{kind} packing of {} unit squares in a square of side {:.6}.{best} Open it to keep packing.",
        a.n, a.side
    )
}

fn escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// Draw `a` like the play screen: the container centered, squares in the
/// palette, world y pointing up.
pub fn render(a: &Arrangement) -> Result<Vec<u8>, String> {
    let fail = || "could not draw preview".to_string();
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).ok_or_else(fail)?;
    pixmap.fill(Color::from_rgba8(0x10, 0x17, 0x22, 0xff));
    let side = a.side as f32;
    let scale = (HEIGHT as f32 - 2.0 * PAD) / side;
    let view = Transform::from_row(
        scale,
        0.0,
        0.0,
        -scale,
        (WIDTH as f32 - side * scale) / 2.0,
        HEIGHT as f32 - PAD,
    );
    let mut paint = Paint::default();
    let container = PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, side, side).ok_or_else(fail)?);
    paint.set_color_rgba8(0x12, 0x18, 0x23, 0xff);
    pixmap.fill_path(&container, &paint, FillRule::Winding, view, None);

    let unit = PathBuilder::from_rect(Rect::from_ltrb(-0.5, -0.5, 0.5, 0.5).ok_or_else(fail)?);
    let edge = Stroke {
        width: 2.0 / scale,
        ..Stroke::default()
    };
    for (i, p) in a.squares.iter().enumerate() {
        // Share codes accept any finite angle; reduce it in f64 so huge
        // values don't overflow f32 degrees.
        let turn = p.theta.sin().atan2(p.theta.cos());
        let at = view
            .pre_translate(p.cx as f32, p.cy as f32)
            .pre_rotate(turn.to_degrees() as f32);
        let [r, g, b] = PALETTE[i % PALETTE.len()];
        paint.set_color_rgba8(r, g, b, 0xdc);
        pixmap.fill_path(&unit, &paint, FillRule::Winding, at, None);
        paint.set_color_rgba8(0x10, 0x17, 0x22, 0xff);
        pixmap.stroke_path(&unit, &paint, &edge, at, None);
    }

    paint.set_color_rgba8(0xc7, 0xf3, 0x6b, 0xff);
    let wall = Stroke {
        width: 4.0 / scale,
        ..Stroke::default()
    };
    pixmap.stroke_path(&container, &paint, &wall, view, None);
    pixmap.encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_covers_html_metacharacters() {
        assert_eq!(
            escape(r#"<a href="x">'&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn huge_finite_angles_render_like_their_normalized_turn() {
        let square = |theta| Arrangement {
            n: 1,
            side: 2.0,
            squares: vec![shared::Placement {
                cx: 1.0,
                cy: 1.0,
                theta,
            }],
        };
        let huge = square(1e300);
        assert_eq!(
            share::decode(&share::encode(&huge, &[]), 1)
                .unwrap()
                .arrangement,
            huge
        );
        let normalized = square(1e300f64.sin().atan2(1e300f64.cos()));
        assert_eq!(render(&huge).unwrap(), render(&normalized).unwrap());
        assert_ne!(
            render(&huge).unwrap(),
            render(&square(normalized.squares[0].theta + 0.3)).unwrap()
        );
    }

    #[test]
    fn page_template_has_a_head_to_inject_into() {
        assert_eq!(INDEX_HTML.matches("</head>").count(), 1);
    }
}
