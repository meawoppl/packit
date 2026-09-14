//! Per-pointer board gestures. Positions are client pixels, so camera changes
//! never feed back into the gesture that caused them.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Finger {
    client: (f64, f64),
    start: (f64, f64),
    moved: bool,
    body: Option<usize>,
    offset: (f64, f64),
}
#[derive(Default)]
pub(super) struct Touch {
    fingers: BTreeMap<i32, Finger>,
    multiple: bool,
}
impl Touch {
    pub fn active(&self) -> bool {
        !self.fingers.is_empty()
    }
}
impl Game {
    pub(super) fn touch_down(&mut self, e: &PointerEvent) -> bool {
        if self.busy || self.corner_drag.is_some() {
            return false;
        }
        let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
            return false;
        };
        e.prevent_default();
        focus_without_scroll(&canvas);
        let p = self.world(&canvas, e);
        let bodies = self.physics.bodies();
        let body = canvas::hit_for(self.physics.shape(), &bodies, p);
        if self.touch.active() {
            self.touch.multiple = true;
            self.taps = tap::DoubleTap::default();
            self.glue_tool = None;
        }
        let offset = body.map_or((0.0, 0.0), |i| {
            (bodies[i].x as f64 - p.0, bodies[i].y as f64 - p.1)
        });
        self.touch.fingers.insert(
            e.pointer_id(),
            Finger {
                client: (e.client_x() as f64, e.client_y() as f64),
                start: (e.client_x() as f64, e.client_y() as f64),
                moved: false,
                body,
                offset,
            },
        );
        if body.is_some() && self.glue_tool.is_none() {
            self.selected = body;
            self.stop_anneal();
            self.set_pause(false);
        }
        self.dragging = true; // Also freezes auto-fit while navigating the view.
        self.physics.set_mouse(0.0, 0.0, None, false);
        let _ = canvas.set_pointer_capture(e.pointer_id());
        self.sync_touch(&canvas);
        true
    }
    pub(super) fn touch_move(&mut self, e: &PointerEvent) -> bool {
        let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
            return false;
        };
        let old = self.touch.fingers.clone();
        let Some(f) = self.touch.fingers.get_mut(&e.pointer_id()) else {
            return false;
        };
        e.prevent_default();
        f.client = (e.client_x() as f64, e.client_y() as f64);
        f.moved |= (f.client.0 - f.start.0).hypot(f.client.1 - f.start.1) > 16.0;
        if self.touch.fingers.len() == 2 {
            let pair: Vec<_> = self.touch.fingers.values().copied().collect();
            let before: Vec<_> = old.values().copied().collect();
            let vector =
                |a: &[Finger]| (a[1].client.0 - a[0].client.0, a[1].client.1 - a[0].client.1);
            let (v, w) = (vector(&before), vector(&pair));
            if pair[0].body.is_some() && pair[0].body == pair[1].body {
                if v.0.hypot(v.1) > 8.0 && w.0.hypot(w.1) > 8.0 {
                    let delta = v.1.atan2(v.0) - w.1.atan2(w.0);
                    self.physics
                        .turn(pair[0].body.unwrap(), delta.sin().atan2(delta.cos()) as f32);
                    self.invalidate();
                }
            } else if pair.iter().all(|f| f.body.is_none()) {
                let midpoint = |a: &[Finger]| {
                    (
                        (a[0].client.0 + a[1].client.0) / 2.0,
                        (a[0].client.1 + a[1].client.1) / 2.0,
                    )
                };
                let anchor = self.client_world(&canvas, midpoint(&before));
                let ratio = if w.0.hypot(w.1) > 8.0 && v.0.hypot(v.1) > 8.0 {
                    v.0.hypot(v.1) / w.0.hypot(w.1)
                } else {
                    1.0
                };
                let base = self.physics.side() * self.physics.container().extent();
                self.extent
                    .set((self.extent.get() * ratio).clamp(base / 12.0, base * 4.0));
                let after = self.client_world(&canvas, midpoint(&pair));
                let pan = self.pan.get();
                self.pan
                    .set((pan.0 + anchor.0 - after.0, pan.1 + anchor.1 - after.1));
                self.manual_view = true;
            }
        }
        self.sync_touch(&canvas);
        true // Corner handles follow the camera too.
    }
    pub(super) fn touch_up(&mut self, e: &PointerEvent) -> bool {
        let Some(f) = self.touch.fingers.remove(&e.pointer_id()) else {
            return false;
        };
        let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
            return false;
        };
        let cancelled = e.type_() != "pointerup";
        if !cancelled
            && !f.moved
            && !self.touch.multiple
            && (f.client.0 - f.start.0).hypot(f.client.1 - f.start.1) <= 16.0
        {
            let p = self.client_world(&canvas, f.client);
            let reach = self.reach(&canvas, e);
            if self.glue_tool.is_some() {
                self.glue_tap(p, reach);
            } else if self.taps.down(e.time_stamp(), f.client) {
                self.open_glue(p, reach);
            }
        } else {
            self.taps = tap::DoubleTap::default();
        }
        // Rebase only the other finger of a same-piece rotation: no stale spring
        // target can jump it when one finger lifts. Independent drags retain
        // their original target even while another pointer lifts or cancels.
        let bodies = self.physics.bodies();
        let offsets: Vec<_> = self
            .touch
            .fingers
            .iter()
            .filter(|(_, remaining)| f.body.is_some() && remaining.body == f.body)
            .map(|(&id, f)| {
                let p = self.client_world(&canvas, f.client);
                (
                    id,
                    f.body.map_or((0.0, 0.0), |i| {
                        (bodies[i].x as f64 - p.0, bodies[i].y as f64 - p.1)
                    }),
                )
            })
            .collect();
        for (id, offset) in offsets {
            self.touch.fingers.get_mut(&id).unwrap().offset = offset;
        }
        self.dragging = self.touch.active();
        if !self.touch.active() {
            self.touch.multiple = false;
        }
        self.sync_touch(&canvas);
        true
    }
    fn sync_touch(&mut self, canvas: &HtmlCanvasElement) {
        if self.glue_tool.is_some() {
            self.physics.set_grabs(&[]);
            return;
        }
        let mut targets = Vec::new();
        for f in self.touch.fingers.values() {
            if let Some(i) = f.body {
                // Two fingers on one piece turn it, without a translation spring.
                if self
                    .touch
                    .fingers
                    .values()
                    .filter(|g| g.body == Some(i))
                    .count()
                    > 1
                {
                    continue;
                }
                let p = self.client_world(canvas, f.client);
                targets.push((i, (p.0 + f.offset.0) as f32, (p.1 + f.offset.1) as f32));
            }
        }
        self.physics.set_grabs(&targets);
        if !targets.is_empty() {
            self.invalidate();
        }
    }
    pub(super) fn clear_touch(&mut self) {
        self.touch = Touch::default();
        self.physics.set_grabs(&[]);
    }
    pub(super) fn client_world(
        &self,
        canvas: &HtmlCanvasElement,
        client: (f64, f64),
    ) -> (f64, f64) {
        let p = canvas::to_world(canvas, client, self.extent.get(), self.physics.side());
        let pan = self.pan.get();
        (p.0 + pan.0, p.1 + pan.1)
    }
}
