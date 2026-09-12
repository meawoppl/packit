//! Double-tap (and double-click) detection on pointer-down, which opens the
//! glue tool. Mouse double-clicks arrive as two pointer-downs too, so one
//! detector covers both.

/// Longest gap between the two downs of a double tap.
const MAX_GAP_MS: f64 = 350.0;
/// Farthest the second down may land from the first, in client pixels;
/// generous enough for a finger.
const MAX_TRAVEL_PX: f64 = 16.0;

#[derive(Default)]
pub struct DoubleTap {
    last: Option<(f64, (f64, f64))>,
}

impl DoubleTap {
    /// Record a pointer-down at `time_ms` and client position `at`. Returns
    /// true when it completes a double tap; the next down then starts over.
    pub fn down(&mut self, time_ms: f64, at: (f64, f64)) -> bool {
        let double = self.last.is_some_and(|(t, p)| {
            (0.0..=MAX_GAP_MS).contains(&(time_ms - t))
                && (at.0 - p.0).hypot(at.1 - p.1) <= MAX_TRAVEL_PX
        });
        self.last = if double { None } else { Some((time_ms, at)) };
        double
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_nearby_second_down_is_a_double_tap() {
        let mut tap = DoubleTap::default();
        assert!(!tap.down(1000.0, (50.0, 50.0)));
        assert!(tap.down(1200.0, (58.0, 44.0)));
    }

    #[test]
    fn slow_or_distant_downs_are_single_taps() {
        let mut tap = DoubleTap::default();
        assert!(!tap.down(0.0, (0.0, 0.0)));
        assert!(!tap.down(400.0, (0.0, 0.0)), "too slow");
        assert!(!tap.down(500.0, (40.0, 0.0)), "too far");
        assert!(!tap.down(400.0, (40.0, 0.0)), "clock went backwards");
    }

    #[test]
    fn a_third_quick_down_starts_over() {
        let mut tap = DoubleTap::default();
        assert!(!tap.down(0.0, (0.0, 0.0)));
        assert!(tap.down(100.0, (0.0, 0.0)));
        assert!(!tap.down(200.0, (0.0, 0.0)));
        assert!(tap.down(300.0, (0.0, 0.0)));
    }
}
