//! Smoothed directional load cue, not a material stress measurement.
//! Filter an unoriented tensor so force reversals never flip the ellipse.
#[derive(Default)]
pub struct Compression {
    tensors: Vec<[f64; 3]>,
    last_ms: Option<f64>,
}
#[derive(Clone, Copy, Debug)]
pub struct Ellipse {
    pub angle: f64,
    pub squeeze: f64,
    pub opacity: f64,
}
impl Compression {
    pub fn update(
        &mut self,
        now_ms: f64,
        forces: &[[f32; 2]],
        mouse: Option<(usize, [f32; 2])>,
    ) -> Vec<Option<Ellipse>> {
        if self.tensors.len() != forces.len() {
            self.tensors = vec![[0.0; 3]; forces.len()];
            self.last_ms = None;
        }
        let dt = self.last_ms.map_or(0.0, |last| (now_ms - last).max(0.0));
        self.last_ms = Some(now_ms);
        let alpha = 1.0 - (-dt / 240.0).exp();
        self.tensors
            .iter_mut()
            .zip(forces)
            .enumerate()
            .map(|(i, (state, force))| {
                let mut target = tensor(*force);
                if let Some((_, force)) = mouse.filter(|(index, _)| *index == i) {
                    let extra = tensor(force);
                    for k in 0..3 {
                        target[k] += extra[k];
                    }
                }
                let trace = target[0] + target[2];
                if trace > 1.0 {
                    for v in &mut target {
                        *v /= trace;
                    }
                }
                for k in 0..3 {
                    state[k] += alpha * (target[k] - state[k]);
                }
                let strength = (state[0] + state[2]).clamp(0.0, 1.0);
                if strength < 0.015 {
                    return None;
                }
                let anisotropy = (state[0] - state[2]).hypot(2.0 * state[1]).min(strength);
                Some(Ellipse {
                    angle: 0.5 * (2.0 * state[1]).atan2(state[0] - state[2]),
                    squeeze: 0.55 * anisotropy,
                    opacity: 0.45 * strength,
                })
            })
            .collect()
    }
}
fn tensor(force: [f32; 2]) -> [f64; 3] {
    let (x, y) = (f64::from(force[0]), f64::from(force[1]));
    let magnitude = x.hypot(y);
    if !magnitude.is_finite() || magnitude < 0.08 {
        return [0.0; 3];
    }
    let strength = (magnitude.ln_1p() / 5.0).min(1.0);
    let (x, y) = (x / magnitude, y / magnitude);
    [strength * x * x, strength * x * y, strength * y * y]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoothing_is_time_based_and_force_reversal_has_no_flip() {
        let run = |fps: usize, reverse: bool| {
            let mut c = Compression::default();
            c.update(0.0, &[[20.0, 0.0]], None);
            let mut result = None;
            for i in 1..=fps {
                result = c.update(
                    i as f64 * 1000.0 / fps as f64,
                    &[[if reverse && i % 2 == 0 { -20.0 } else { 20.0 }, 0.0]],
                    None,
                )[0];
            }
            result.unwrap()
        };
        let a = run(30, false);
        let b = run(120, true);
        assert!((a.squeeze - b.squeeze).abs() < 1e-12);
        assert!((a.opacity - b.opacity).abs() < 1e-12);
        assert_eq!(a.angle, b.angle);
    }
    #[test]
    fn spikes_are_bounded_and_release_fades_instead_of_disappearing() {
        let mut c = Compression::default();
        c.update(0.0, &[[0.0; 2]], None);
        let spike = c.update(16.0, &[[f32::MAX, 0.0]], None)[0].unwrap();
        assert!(spike.squeeze < 0.04);
        let fade = c.update(32.0, &[[0.0; 2]], None)[0].unwrap();
        assert!(fade.opacity < spike.opacity);
        assert!(c.update(2000.0, &[[0.0; 2]], None)[0].is_none());
        assert_eq!(tensor([f32::NAN, 0.0]), [0.0; 3]);
    }
    #[test]
    fn opposing_loads_do_not_cancel() {
        let mut c = Compression::default();
        c.update(0.0, &[[10.0, 0.0]], Some((0, [-10.0, 0.0])));
        let e = c.update(1000.0, &[[10.0, 0.0]], Some((0, [-10.0, 0.0])))[0].unwrap();
        assert!(e.squeeze > 0.4);
        assert_eq!(e.angle, 0.0);
    }
}
