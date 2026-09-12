//! Canvas rendering, pointer mapping, and hit testing for the play screen.

use physics::Body;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Margin around the playfield as a fraction of the canvas width.
const PAD_FRACTION: f64 = 0.045;
const PALETTE: [&str; 5] = ["#c7f36b", "#7dd5ce", "#b2a0ef", "#f0b578", "#8dabf2"];

/// Everything the renderer needs for one frame.
pub struct Scene<'a> {
    pub bodies: &'a [Body],
    pub side: f64,
    /// Side length the viewport is scaled to; the band may contract inside it.
    pub view_side: f64,
    pub band_on: bool,
    pub selected: Option<usize>,
    /// Mouse-spring target while a square is being dragged.
    pub tether: Option<(f64, f64)>,
}

/// Map a client-space pointer position to world coordinates (origin
/// bottom-left) for a viewport showing `[0, extent]^2`.
pub fn to_world(canvas: &HtmlCanvasElement, client: (f64, f64), extent: f64) -> (f64, f64) {
    let r = canvas.get_bounding_client_rect();
    let pad = r.width() * PAD_FRACTION;
    let scale = (r.width() - 2.0 * pad) / extent;
    (
        (client.0 - r.left() - pad) / scale,
        (r.bottom() - client.1 - pad) / scale,
    )
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
    if canvas.width() != size {
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
    let pad = w * PAD_FRACTION;
    let scale = (w - 2.0 * pad) / scene.view_side.max(scene.side);
    let sx = |x: f64| pad + x * scale;
    let sy = |y: f64| w - pad - y * scale;
    let side = scene.side;

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
    ctx.stroke_rect(pad, sy(side), side * scale, side * scale);

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
        ctx.set_fill_style_str("#172431");
        ctx.set_font(&format!("600 {}px system-ui", (scale * 0.16).max(10.0)));
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(&(i + 1).to_string(), 0.0, 0.0);
        ctx.restore();
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

fn line(ctx: &CanvasRenderingContext2d, from: (f64, f64), to: (f64, f64)) {
    ctx.begin_path();
    ctx.move_to(from.0, from.1);
    ctx.line_to(to.0, to.1);
    ctx.stroke();
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
