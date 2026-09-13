//! Canvas rendering, pointer mapping, and hit testing for the play screen.

use super::glue::{self, Anchor};
use physics::{Body, Feature, Glue, ViolationReport};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Margin around the playfield as a fraction of the canvas width.
const PAD_FRACTION: f64 = 0.045;
const PALETTE: [&str; 5] = ["#c7f36b", "#7dd5ce", "#b2a0ef", "#f0b578", "#8dabf2"];
const PULSE: &str = "#ff4d5e";
/// Violations shallower than this are springy contact, not worth flagging.
const PULSE_DEPTH: f64 = 2e-3;
/// Violations this deep pulse at full strength.
const PULSE_FULL: f64 = 0.05;
const PULSE_PERIOD_MS: f64 = 1600.0;

/// Everything the renderer needs for one frame.
pub struct Scene<'a> {
    pub bodies: &'a [Body],
    pub side: f64,
    /// Side length the viewport is scaled to; the band may contract inside it.
    pub view_side: f64,
    pub band_on: bool,
    pub band_tension: f32,
    pub target_side: f64,
    /// Net contact/edge forces during direct manipulation.
    pub forces: Option<&'a [[f32; 2]]>,
    pub mouse_force: Option<(usize, [f32; 2])>,
    pub selected: Option<usize>,
    /// Mouse-spring target while a square is being dragged.
    pub tether: Option<(f64, f64)>,
    /// Glued feature pairs, drawn as links.
    pub glues: &'a [Glue],
    /// While the glue tool is open: the first pick, if chosen yet.
    pub glue_tool: Option<Option<Feature>>,
    /// Overlaps and unmet glue, pulsed in red on squares and walls.
    pub violations: &'a ViolationReport,
    /// Animation clock for the pulse, in milliseconds.
    pub now_ms: f64,
    /// The best-known side and its credit, drawn over a settled packing.
    pub best: Option<(f64, &'a str)>,
}

/// Opacity of the red pulse over a violation `depth` deep at `now_ms`, or
/// `None` when it's too shallow to show. Deeper violations pulse stronger.
fn pulse_alpha(depth: f64, now_ms: f64) -> Option<f64> {
    if depth < PULSE_DEPTH {
        return None;
    }
    let strength = 0.4 + 0.6 * (depth / PULSE_FULL).min(1.0);
    let wave = 0.5 - 0.5 * (std::f64::consts::TAU * now_ms / PULSE_PERIOD_MS).cos();
    Some(strength * (0.25 + 0.35 * wave))
}

/// Map a client-space pointer position to world coordinates (origin
/// bottom-left) for a viewport `extent` wide around a box of `side`, which
/// stays centered on the canvas.
pub fn to_world(
    canvas: &HtmlCanvasElement,
    client: (f64, f64),
    extent: f64,
    side: f64,
) -> (f64, f64) {
    let r = canvas.get_bounding_client_rect();
    let scale = r.width() * (1.0 - 2.0 * PAD_FRACTION) / extent;
    let center = (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0);
    (
        side / 2.0 + (client.0 - center.0) / scale,
        side / 2.0 - (client.1 - center.1) / scale,
    )
}

/// World length of `px` client pixels for a viewport showing `[0, extent]^2`.
pub fn px_to_world(canvas: &HtmlCanvasElement, px: f64, extent: f64) -> f64 {
    let width = canvas.get_bounding_client_rect().width();
    px * extent / (width * (1.0 - 2.0 * PAD_FRACTION))
}

/// Topmost square containing `p`.
pub fn hit(bodies: &[Body], p: (f64, f64)) -> Option<usize> {
    bodies
        .iter()
        .enumerate()
        .rev()
        .find(|(_, b)| {
            let (dx, dy) = (p.0 - b.x as f64, p.1 - b.y as f64);
            let (s, c) = (b.theta as f64).sin_cos();
            (dx * c + dy * s).abs() <= 0.5 && (-dx * s + dy * c).abs() <= 0.5
        })
        .map(|(i, _)| i)
}

pub fn draw(canvas: &HtmlCanvasElement, scene: &Scene) {
    let dpr = web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0)
        .min(2.0);
    let size = (canvas.client_width() as f64 * dpr).round() as u32;
    if size == 0 {
        return;
    }
    if canvas.width() != size || canvas.height() != size {
        canvas.set_width(size);
        canvas.set_height(size);
    }
    let Some(ctx) = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok())
    else {
        return;
    };

    let w = size as f64;
    let side = scene.side;
    let scale = w * (1.0 - 2.0 * PAD_FRACTION) / scene.view_side.max(side);
    // The box stays centered, so growing it on every side leaves the squares
    // where they were on screen.
    let sx = |x: f64| w / 2.0 + (x - side / 2.0) * scale;
    let sy = |y: f64| w / 2.0 - (y - side / 2.0) * scale;

    ctx.clear_rect(0.0, 0.0, w, w);
    ctx.set_fill_style_str("#101722");
    ctx.fill_rect(0.0, 0.0, w, w);

    ctx.set_stroke_style_str("#263247");
    ctx.set_line_width(1.0);
    for x in (0..).map(|k| k as f64 * 0.5).take_while(|x| *x <= side) {
        line(&ctx, (sx(x), sy(side)), (sx(x), sy(0.0)));
        line(&ctx, (sx(0.0), sy(x)), (sx(side), sy(x)));
    }

    ctx.set_stroke_style_str(if scene.band_on { "#c7f36b" } else { "#819376" });
    ctx.set_line_width(if scene.band_on { 3.0 } else { 2.0 });
    ctx.stroke_rect(sx(0.0), sy(side), side * scale, side * scale);

    // Draw the spring's rest boundary and inward pressure marks. The solid
    // boundary always remains the actual collision square.
    if scene.band_on {
        let stress = band_stress(side, scene.target_side, scene.band_tension);
        ctx.save();
        ctx.set_stroke_style_str("#c7f36b");
        ctx.set_global_alpha(0.15 + 0.55 * stress);
        ctx.set_line_width(4.0 + 10.0 * stress);
        ctx.stroke_rect(sx(0.0), sy(side), side * scale, side * scale);
        ctx.set_line_width(1.0);
        let target = scene.target_side.min(scene.view_side.max(side));
        let _ = ctx.set_line_dash(&js_sys::Array::of2(&4.into(), &6.into()));
        ctx.stroke_rect(sx(0.0), sy(target), target * scale, target * scale);
        let _ = ctx.set_line_dash(&JsValue::from(js_sys::Array::new()));
        if stress > 0.01 {
            let inward = if side > scene.target_side { 1.0 } else { -1.0 };
            ctx.set_global_alpha(0.4 + 0.6 * stress);
            ctx.set_line_width(2.0);
            for k in 1..=5 {
                let at = side * k as f64 / 6.0;
                let length = (8.0 + 16.0 * stress) * dpr;
                arrow(&ctx, (sx(at), sy(side)), (0.0, inward * length));
                arrow(&ctx, (sx(side), sy(at)), (-inward * length, 0.0));
            }
        }
        ctx.restore();
    }

    let half = scale / 2.0;
    for (i, b) in scene.bodies.iter().enumerate() {
        ctx.save();
        let _ = ctx.translate(sx(b.x as f64), sy(b.y as f64));
        let _ = ctx.rotate(-(b.theta as f64));
        ctx.set_fill_style_str(PALETTE[i % PALETTE.len()]);
        ctx.set_global_alpha(0.86);
        ctx.fill_rect(-half + 1.0, -half + 1.0, scale - 2.0, scale - 2.0);
        ctx.set_global_alpha(1.0);
        let selected = scene.selected == Some(i);
        ctx.set_stroke_style_str(if selected { "#fff" } else { "#ffffff45" });
        ctx.set_line_width(if selected { 3.0 } else { 1.0 });
        ctx.stroke_rect(-half + 1.0, -half + 1.0, scale - 2.0, scale - 2.0);
        ctx.restore();
        // Labels stay upright however the square is turned.
        ctx.set_fill_style_str("#172431");
        ctx.set_font(&format!("600 {}px system-ui", (scale * 0.16).max(10.0)));
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(&(i + 1).to_string(), sx(b.x as f64), sy(b.y as f64));
    }

    ctx.save();
    ctx.set_fill_style_str(PULSE);
    ctx.set_stroke_style_str(PULSE);
    for (b, depth) in scene.bodies.iter().zip(&scene.violations.bodies) {
        let Some(alpha) = pulse_alpha(*depth, scene.now_ms) else {
            continue;
        };
        ctx.save();
        let _ = ctx.translate(sx(b.x as f64), sy(b.y as f64));
        let _ = ctx.rotate(-(b.theta as f64));
        ctx.set_global_alpha(alpha);
        ctx.fill_rect(-half + 1.0, -half + 1.0, scale - 2.0, scale - 2.0);
        ctx.set_global_alpha((alpha * 2.0).min(1.0));
        ctx.set_line_width(2.0 * dpr);
        ctx.stroke_rect(-half + 1.0, -half + 1.0, scale - 2.0, scale - 2.0);
        ctx.restore();
    }
    // Walls in glue order: left, bottom, right, top.
    let walls = [
        ((0.0, 0.0), (0.0, side)),
        ((0.0, 0.0), (side, 0.0)),
        ((side, 0.0), (side, side)),
        ((0.0, side), (side, side)),
    ];
    ctx.set_line_width(5.0 * dpr);
    for ((a, b), depth) in walls.into_iter().zip(scene.violations.walls) {
        if let Some(alpha) = pulse_alpha(depth, scene.now_ms) {
            ctx.set_global_alpha((alpha * 2.0).min(1.0));
            line(&ctx, (sx(a.0), sy(a.1)), (sx(b.0), sy(b.1)));
        }
    }
    ctx.restore();

    ctx.save();
    ctx.set_stroke_style_str("#ffcf4d");
    ctx.set_fill_style_str("#ffcf4d");
    ctx.set_line_width(3.0 * dpr);
    for g in scene.glues {
        if let Some((a, b)) = glue::link(scene.bodies, side, *g) {
            line(&ctx, (sx(a.0), sy(a.1)), (sx(b.0), sy(b.1)));
            dot(
                &ctx,
                (sx((a.0 + b.0) / 2.0), sy((a.1 + b.1) / 2.0)),
                5.0 * dpr,
            );
        }
    }
    ctx.restore();

    // The best-known container, centered on this one for comparison.
    if let Some((best, label)) = scene.best {
        let (x, y, size) = (
            sx(side / 2.0 - best / 2.0),
            sy(side / 2.0 + best / 2.0),
            best * scale,
        );
        ctx.save();
        ctx.set_fill_style_str(PULSE);
        ctx.set_global_alpha(0.1);
        ctx.fill_rect(x, y, size, size);
        ctx.set_global_alpha(0.85);
        ctx.set_stroke_style_str("#ff9aa4");
        ctx.set_line_width(2.0 * dpr);
        ctx.stroke_rect(x, y, size, size);
        ctx.set_fill_style_str("#ffc4ca");
        ctx.set_font(&format!("600 {}px system-ui", 12.0 * dpr));
        ctx.set_text_align("left");
        ctx.set_text_baseline("bottom");
        let _ = ctx.fill_text_with_max_width(label, x + 4.0 * dpr, y - 3.0 * dpr, size - 8.0 * dpr);
        ctx.restore();
    }

    // With the glue tool open, show every target the next tap can take,
    // and the first pick in gold.
    if let Some(first) = scene.glue_tool {
        ctx.save();
        ctx.set_stroke_style_str("#ffffff");
        ctx.set_fill_style_str("#ffffff");
        ctx.set_global_alpha(0.6);
        ctx.set_line_width(2.0 * dpr);
        let targets = glue::features(scene.bodies.len())
            .filter(|f| first.is_none_or(|a| glue::compatible(a, *f)));
        for f in targets {
            match glue::anchor(scene.bodies, side, f) {
                Some(Anchor::Point(p)) => dot(&ctx, (sx(p.0), sy(p.1)), 3.5 * dpr),
                Some(Anchor::Segment(p, q)) => line(&ctx, (sx(p.0), sy(p.1)), (sx(q.0), sy(q.1))),
                None => {}
            }
        }
        if let Some(anchor) = first.and_then(|a| glue::anchor(scene.bodies, side, a)) {
            ctx.set_global_alpha(1.0);
            ctx.set_stroke_style_str("#ffcf4d");
            ctx.set_fill_style_str("#ffcf4d");
            ctx.set_line_width(5.0 * dpr);
            match anchor {
                Anchor::Point(p) => dot(&ctx, (sx(p.0), sy(p.1)), 7.0 * dpr),
                Anchor::Segment(p, q) => line(&ctx, (sx(p.0), sy(p.1)), (sx(q.0), sy(q.1))),
            }
        }
        ctx.restore();
    }

    if let Some(forces) = scene.forces {
        ctx.save();
        ctx.set_stroke_style_str("#7dcfff");
        ctx.set_line_width(2.5 * dpr);
        for (b, force) in scene.bodies.iter().zip(forces) {
            if let Some(vector) = force_vector(*force, scale) {
                arrow(&ctx, (sx(b.x as f64), sy(b.y as f64)), vector);
            }
        }
        ctx.restore();
    }
    if let Some((i, force)) = scene.mouse_force {
        if let (Some(b), Some(vector)) = (scene.bodies.get(i), force_vector(force, scale)) {
            ctx.save();
            ctx.set_stroke_style_str("#f9d878");
            ctx.set_line_width(3.0 * dpr);
            arrow(&ctx, (sx(b.x as f64), sy(b.y as f64)), vector);
            ctx.restore();
        }
    }

    if let (Some(i), Some(target)) = (scene.selected, scene.tether) {
        if let Some(b) = scene.bodies.get(i) {
            ctx.set_stroke_style_str("#ffffffaa");
            ctx.set_line_width(2.0);
            let _ = ctx.set_line_dash(&js_sys::Array::of2(&5.into(), &4.into()));
            line(
                &ctx,
                (sx(b.x as f64), sy(b.y as f64)),
                (sx(target.0), sy(target.1)),
            );
            let _ = ctx.set_line_dash(&JsValue::from(js_sys::Array::new()));
        }
    }
}

/// Log length keeps stiff penalty spikes readable without changing direction.
fn force_vector(force: [f32; 2], scale: f64) -> Option<(f64, f64)> {
    let magnitude = (force[0] as f64).hypot(force[1] as f64);
    if !magnitude.is_finite() || magnitude < 0.08 {
        return None;
    }
    let length = (magnitude.ln_1p() * 0.18).clamp(0.06, 0.85) * scale;
    Some((
        force[0] as f64 / magnitude * length,
        -force[1] as f64 / magnitude * length,
    ))
}
fn band_stress(side: f64, target: f64, tension: f32) -> f64 {
    (((side - target).abs() * tension as f64).ln_1p() / 5.0).clamp(0.0, 1.0)
}
fn arrow(ctx: &CanvasRenderingContext2d, from: (f64, f64), vector: (f64, f64)) {
    let length = vector.0.hypot(vector.1);
    if length < 1.0 {
        return;
    }
    let to = (from.0 + vector.0, from.1 + vector.1);
    let head = (length * 0.3).clamp(3.0, 9.0);
    let (ux, uy) = (vector.0 / length, vector.1 / length);
    line(ctx, from, to);
    ctx.begin_path();
    ctx.move_to(
        to.0 - head * ux - head * 0.5 * uy,
        to.1 - head * uy + head * 0.5 * ux,
    );
    ctx.line_to(to.0, to.1);
    ctx.line_to(
        to.0 - head * ux + head * 0.5 * uy,
        to.1 - head * uy - head * 0.5 * ux,
    );
    ctx.stroke();
}

fn line(ctx: &CanvasRenderingContext2d, from: (f64, f64), to: (f64, f64)) {
    ctx.begin_path();
    ctx.move_to(from.0, from.1);
    ctx.line_to(to.0, to.1);
    ctx.stroke();
}

fn dot(ctx: &CanvasRenderingContext2d, at: (f64, f64), radius: f64) {
    ctx.begin_path();
    let _ = ctx.arc(at.0, at.1, radius, 0.0, std::f64::consts::TAU);
    ctx.fill();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(x: f32, y: f32, theta: f32) -> Body {
        Body {
            x,
            y,
            theta,
            ..Body::default()
        }
    }

    #[test]
    fn pulse_skips_springy_contact_and_grows_with_depth() {
        assert_eq!(pulse_alpha(1e-3, 0.0), None);
        let (dim, bright) = (0.0, PULSE_PERIOD_MS / 2.0);
        let shallow = (
            pulse_alpha(PULSE_DEPTH, dim),
            pulse_alpha(PULSE_DEPTH, bright),
        );
        let deep = (pulse_alpha(0.2, dim), pulse_alpha(0.2, bright));
        assert!(shallow.0 < shallow.1, "it pulses");
        assert!(
            shallow.0 < deep.0 && shallow.1 < deep.1,
            "deeper is stronger"
        );
        assert!(deep.1.unwrap() <= 0.6, "gentle even at full strength");
        assert!(shallow.0.unwrap() > 0.0, "never fades out entirely");
    }

    #[test]
    fn force_arrows_preserve_direction_and_bound_spikes() {
        assert_eq!(force_vector([0.0; 2], 100.0), None);
        assert_eq!(force_vector([f32::NAN, 1.0], 100.0), None);
        let v = force_vector([3.0, 4.0], 100.0).unwrap();
        assert!((v.0 / v.1 + 0.75).abs() < 1e-6);
        let spike = force_vector([1e8, 0.0], 100.0).unwrap();
        assert_eq!(spike, (85.0, 0.0));
        assert_eq!(band_stress(4.0, 3.0, 0.0), 0.0);
        assert_eq!(band_stress(3.0, 3.0, 30.0), 0.0);
        assert!(band_stress(4.0, 3.0, 30.0) > band_stress(4.0, 3.0, 10.0));
    }

    #[test]
    fn hit_finds_axis_aligned_square() {
        let bodies = [body(0.5, 0.5, 0.0), body(1.5, 0.5, 0.0)];
        assert_eq!(hit(&bodies, (0.9, 0.9)), Some(0));
        assert_eq!(hit(&bodies, (1.1, 0.1)), Some(1));
        assert_eq!(hit(&bodies, (0.5, 1.2)), None);
    }

    #[test]
    fn hit_respects_rotation_and_prefers_topmost() {
        let diamond = body(1.0, 1.0, std::f32::consts::FRAC_PI_4);
        // Inside the unrotated square's corner but outside the diamond.
        assert_eq!(hit(&[diamond], (1.45, 1.45)), None);
        assert_eq!(hit(&[diamond], (1.6, 1.0)), Some(0));
        let stacked = [body(1.0, 1.0, 0.0), body(1.2, 1.0, 0.0)];
        assert_eq!(hit(&stacked, (1.1, 1.0)), Some(1));
    }
}
