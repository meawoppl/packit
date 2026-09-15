//! Link previews for board links: `/play/:n?s=` pages carry Open Graph and
//! Twitter metadata, and `/api/preview.png` draws the board's packing.

use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use shared::{board, geometry, Arrangement, MAX_N, VALIDATION_TOL};
use std::sync::{Arc, LazyLock};
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

/// The SPA page for `/play/:n`, with metadata describing the board's packing
/// when `s` decodes, and generic metadata otherwise.
pub async fn play(
    State(state): State<Arc<AppState>>,
    Path(n): Path<String>,
    query: Option<Query<PlayQuery>>,
) -> impl IntoResponse {
    play_page(
        state,
        n,
        shared::Shape::Square,
        shared::Shape::Square,
        query,
    )
}

pub async fn polygon_play(
    State(state): State<Arc<AppState>>,
    Path((shape, n)): Path<(String, String)>,
    query: Option<Query<PlayQuery>>,
) -> Response {
    match shape.parse() {
        Ok(shape) => play_page(state, n, shape, shared::Shape::Square, query).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn container_play(
    State(state): State<Arc<AppState>>,
    Path((shape, container, n)): Path<(String, String, String)>,
    query: Option<Query<PlayQuery>>,
) -> Response {
    match (shape.parse(), container.parse()) {
        (Ok(shape), Ok(container)) => play_page(state, n, shape, container, query).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

fn play_page(
    state: Arc<AppState>,
    n: String,
    shape: shared::Shape,
    container: shared::Shape,
    query: Option<Query<PlayQuery>>,
) -> impl IntoResponse {
    let n = n.parse().ok().filter(|n| (1..=MAX_N).contains(n));
    let code = query.and_then(|Query(q)| q.s);
    let packing = n.zip(code).and_then(|(n, s)| {
        board::decode(&s, n)
            .ok()
            .filter(|b| b.arrangement.shape == shape && b.arrangement.container == container)
            .map(|b| (b.arrangement, s))
    });
    let tags = meta_tags_for(
        &state.public_url,
        shape,
        container,
        n,
        packing.as_ref().map(|(a, s)| (a, s.as_str())),
    );
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(INDEX_HTML.replacen("</head>", &format!("{tags}</head>"), 1)),
    )
}

/// PNG of a board's packing. Metadata URLs include both the rendering version
/// and the reference side bits, so updated references get a fresh cached image.
pub async fn preview_png(query: Option<Query<PreviewQuery>>) -> Response {
    let png = query
        .ok_or_else(|| "n and s are required".to_string())
        .and_then(|Query(q)| board::decode(&q.s, q.n))
        .and_then(|b| render(&b.arrangement));
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

fn meta_tags_for(
    public_url: &str,
    shape: shared::Shape,
    container: shared::Shape,
    n: Option<u32>,
    packing: Option<(&Arrangement, &str)>,
) -> String {
    let path = n
        .map(|n| shape.play_path(container, n))
        .unwrap_or_else(|| "/".into());
    let pieces = shape.plural();
    let (title, description, url, image) = match (n, packing) {
        (Some(n), Some((a, code))) => (
            format!(
                "{n} {pieces} in a {:.4} box{}",
                a.side,
                comparison(a)
                    .map(|c| format!(
                        " · {} of best known{}",
                        c.percent,
                        if c.record { " · NEW RECORD" } else { "" }
                    ))
                    .unwrap_or_default()
            ),
            describe(a),
            format!("{public_url}{path}?s={code}"),
            Some(format!(
                "{public_url}/api/preview.png?v=2-{:016x}&n={n}&s={code}",
                reference(a).unwrap_or(0.0).to_bits()
            )),
        ),
        (Some(n), None) => (
            format!("Pack {n} unit {pieces}"),
            format!("Pack {n} unit {pieces} into the smallest {container} you can."),
            format!("{public_url}{path}"),
            None,
        ),
        (None, _) => (
            "Potatos".to_string(),
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
        ("property", "og:site_name", "Potatos".to_string()),
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
    let best = comparison(a)
        .map(|c| {
            format!(
                " {} of best-known side.{}",
                c.percent,
                if c.record { " NEW RECORD." } else { "" }
            )
        })
        .unwrap_or_default();
    format!(
        "{kind} packing of {} unit {} in a {} of side {:.6}.{best} Open it to keep packing.",
        a.n,
        a.shape.plural(),
        a.container,
        a.side
    )
}

struct Comparison {
    percent: String,
    record: bool,
}

fn compare(side: f64, best: f64, valid: bool) -> Comparison {
    let ratio = side / best * 100.0;
    let mut percent = format!("{ratio:.2}%");
    // Do not round an actual improvement (or regression) into an apparent tie.
    if percent == "100.00%" && side != best {
        percent = format!("{}100.00%", if side < best { "<" } else { ">" });
    }
    Comparison {
        percent,
        record: valid && side < best,
    }
}

fn reference(a: &Arrangement) -> Option<f64> {
    if !a.container.is_square() {
        return None;
    }
    crate::handlers::records::for_shape(a.shape)
        .iter()
        .find(|r| r.n == a.n)
        .map(|r| r.side)
}

fn comparison(a: &Arrangement) -> Option<Comparison> {
    Some(compare(
        a.side,
        reference(a)?,
        geometry::validate(a, VALIDATION_TOL).is_ok(),
    ))
}

static FONT: LazyLock<fontdue::Font> = LazyLock::new(|| {
    fontdue::Font::from_bytes(
        include_bytes!("../../assets/Lato-Bold.ttf") as &[u8],
        fontdue::FontSettings::default(),
    )
    .expect("bundled Lato font")
});

fn text(pixmap: &mut Pixmap, value: &str, x: f32, baseline: i32, size: f32, color: [u8; 3]) {
    let mut pen = x;
    for ch in value.chars() {
        let (metrics, bitmap) = FONT.rasterize(ch, size);
        let left = pen.round() as i32 + metrics.xmin;
        let top = baseline - metrics.height as i32 - metrics.ymin;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let (px, py) = (left + col as i32, top + row as i32);
                if px < 0 || py < 0 || px >= WIDTH as i32 || py >= HEIGHT as i32 {
                    continue;
                }
                let alpha = bitmap[row * metrics.width + col] as u32;
                let pixel = &mut pixmap.pixels_mut()[py as usize * WIDTH as usize + px as usize];
                let blend = |fg: u8, bg: u8| {
                    ((u32::from(fg) * alpha + u32::from(bg) * (255 - alpha) + 127) / 255) as u8
                };
                *pixel = tiny_skia::PremultipliedColorU8::from_rgba(
                    blend(color[0], pixel.red()),
                    blend(color[1], pixel.green()),
                    blend(color[2], pixel.blue()),
                    255,
                )
                .unwrap();
            }
        }
        pen += metrics.advance_width;
    }
}

fn draw_caption(pixmap: &mut Pixmap, a: &Arrangement) {
    // Cover any out-of-box pieces from unverified imports behind the caption.
    let mut paint = Paint::default();
    paint.set_color_rgba8(0x10, 0x17, 0x22, 255);
    pixmap.fill_rect(
        Rect::from_xywh(0.0, 0.0, 590.0, HEIGHT as f32).unwrap(),
        &paint,
        Transform::identity(),
        None,
    );
    let white = [0xee, 0xf2, 0xf7];
    let muted = [0xa0, 0xad, 0xbe];
    let green = [0xc7, 0xf3, 0x6b];
    text(pixmap, "POTATOS", 48.0, 80, 30.0, green);
    text(
        pixmap,
        &format!("{} {}", a.n, a.shape.plural()),
        48.0,
        151,
        32.0,
        white,
    );
    text(
        pixmap,
        &format!("in a {}", a.container),
        48.0,
        192,
        26.0,
        muted,
    );
    if let Some(c) = comparison(a) {
        text(pixmap, &c.percent, 48.0, 313, 64.0, white);
        text(pixmap, "OF BEST KNOWN SIDE", 48.0, 354, 22.0, muted);
        if c.record {
            text(pixmap, "NEW RECORD", 48.0, 420, 34.0, green);
        }
    } else {
        text(pixmap, "NO KNOWN REFERENCE", 48.0, 300, 27.0, muted);
    }
    text(
        pixmap,
        &format!("Side {:.6}", a.side),
        48.0,
        500,
        25.0,
        white,
    );
    let verified = geometry::validate(a, VALIDATION_TOL).is_ok();
    text(
        pixmap,
        if verified {
            "Verified packing"
        } else {
            "Unverified board"
        },
        48.0,
        543,
        22.0,
        muted,
    );
    text(pixmap, "potatos.txcl.io", 48.0, 590, 21.0, muted);
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
    let scale = (HEIGHT as f32 - 2.0 * PAD) / (side * a.container.extent() as f32);
    let view = Transform::from_row(
        scale,
        0.0,
        0.0,
        -scale,
        885.0 - side * scale / 2.0,
        (HEIGHT as f32 + side * scale) / 2.0,
    );
    let mut paint = Paint::default();
    let container = if a.container.is_square() {
        PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, side, side).ok_or_else(fail)?)
    } else {
        let mut path = PathBuilder::new();
        for (i, (x, y)) in a
            .container
            .container_vertices(a.side)
            .into_iter()
            .enumerate()
        {
            if i == 0 {
                path.move_to(x as f32, y as f32);
            } else {
                path.line_to(x as f32, y as f32);
            }
        }
        path.close();
        path.finish().ok_or_else(fail)?
    };
    paint.set_color_rgba8(0x12, 0x18, 0x23, 0xff);
    pixmap.fill_path(&container, &paint, FillRule::Winding, view, None);

    let unit = if a.shape.is_square() {
        PathBuilder::from_rect(Rect::from_ltrb(-0.5, -0.5, 0.5, 0.5).ok_or_else(fail)?)
    } else {
        let vertices = a.shape.vertices(&shared::Placement {
            cx: 0.0,
            cy: 0.0,
            theta: 0.0,
        });
        let mut path = PathBuilder::new();
        path.move_to(vertices[0].0 as f32, vertices[0].1 as f32);
        for (x, y) in &vertices[1..] {
            path.line_to(*x as f32, *y as f32);
        }
        path.close();
        path.finish().ok_or_else(fail)?
    };
    let edge = Stroke {
        width: 2.0 / scale,
        ..Stroke::default()
    };
    for (i, p) in a.squares.iter().enumerate() {
        // Board codes accept any finite angle; reduce it in f64 so huge
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
    draw_caption(&mut pixmap, a);
    pixmap.encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_labels_improvements_ties_and_unverified_boards() {
        assert_eq!(compare(2.2, 2.0, true).percent, "110.00%");
        assert!(!compare(2.0, 2.0, true).record);
        assert!(compare(1.9, 2.0, true).record);
        assert!(!compare(1.9, 2.0, false).record);
        assert_eq!(compare(1.999999, 2.0, true).percent, "<100.00%");
        assert_eq!(compare(2.000001, 2.0, true).percent, ">100.00%");
    }

    #[test]
    fn reference_requires_matching_category_and_validity_for_badge() {
        let mut a = Arrangement {
            n: 1,
            side: 1.0,
            squares: vec![shared::Placement {
                cx: 0.5,
                cy: 0.5,
                theta: 0.0,
            }],
            shape: shared::Shape::Square,
            container: shared::Shape::Square,
        };
        let c = comparison(&a).unwrap();
        assert_eq!(c.percent, "100.00%");
        assert!(!c.record);
        a.side = 0.9;
        assert!(!comparison(&a).unwrap().record);
        let tags = meta_tags_for(
            "https://potatos.txcl.io",
            a.shape,
            a.container,
            Some(1),
            Some((&a, "test")),
        );
        assert!(tags.contains("90.00%"));
        assert!(!tags.contains("NEW RECORD"));
        a.container = shared::Shape::Triangle;
        assert!(comparison(&a).is_none());
    }

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
            container: shared::Shape::Square,
            shape: shared::Shape::Square,
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
            board::decode(&board::encode(&huge, &[]), 1)
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
