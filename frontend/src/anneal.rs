//! Annealing schedule for the play screen: progressively smaller shakes while
//! the band tightens toward a floor, then a quiet cool-down and a measurement.
//!
//! Pure and time-driven: the play screen calls [`Anneal::advance`] once per
//! frame with the elapsed seconds and applies the returned [`Command`]s.

/// Tunables for one annealing run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Schedule {
    /// Total run time in seconds, including the quiet cool-down.
    pub duration: f64,
    /// Fraction of `duration` spent shaking and tightening; the rest is quiet.
    pub heat_fraction: f64,
    /// Seconds between shakes.
    pub shake_interval: f64,
    /// Shake strength at the start (1.0 matches the Shake button).
    pub initial_strength: f32,
    /// Shakes weaker than this are skipped.
    pub min_strength: f32,
    /// Band tension held for the whole run.
    pub band_tension: f32,
    /// Seconds per squeeze-then-relax cycle of the band; zero never relaxes.
    pub relax_period: f64,
    /// Least the band opens past its squeeze at the start of the run, in
    /// side-length units; it fades out over the heating phase.
    pub relax_depth: f64,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            duration: 20.0,
            heat_fraction: 0.8,
            shake_interval: 0.8,
            initial_strength: 1.0,
            min_strength: 0.02,
            band_tension: 30.0,
            relax_period: 0.0,
            relax_depth: 0.0,
        }
    }
}

impl Schedule {
    /// Gentle squeeze: no shakes, just a slow, soft tightening of the band
    /// toward the floor, then a short settle and a measurement.
    pub fn gentle() -> Self {
        Self {
            duration: 12.0,
            heat_fraction: 0.85,
            initial_strength: 0.0,
            band_tension: 15.0,
            ..Self::default()
        }
    }

    /// Squeeze down: no shakes. The band squeezes, then relaxes so squares
    /// can slide and turn into better spots, then squeezes lower again.
    /// The relaxing fades out, so the run ends fully squeezed and measures.
    pub fn squeeze_down() -> Self {
        Self {
            duration: 18.0,
            heat_fraction: 0.85,
            initial_strength: 0.0,
            band_tension: 40.0,
            relax_period: 2.4,
            relax_depth: 0.3,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// Kick every square with random velocity scaled by `strength` (`Physics::shake_scaled`).
    Shake { seed: u64, strength: f32 },
    /// Set the band target side and tension.
    Band { target_side: f64, tension: f32 },
    /// The run finished; settle and measure.
    Measure,
}

#[derive(Debug, Clone)]
pub struct Anneal {
    schedule: Schedule,
    start_side: f64,
    floor_side: f64,
    elapsed: f64,
    next_shake: f64,
    seed: u64,
    finished: bool,
}

impl Anneal {
    /// Anneal from `start_side` toward `floor_side` (e.g. `sqrt(n)`); the band
    /// target never drops below the floor.
    pub fn new(schedule: Schedule, start_side: f64, floor_side: f64, seed: u64) -> Self {
        Self {
            schedule,
            start_side: start_side.max(floor_side),
            floor_side,
            elapsed: 0.0,
            next_shake: 0.0,
            seed,
            finished: false,
        }
    }

    /// Fraction of the run completed, in `[0, 1]`.
    pub fn progress(&self) -> f64 {
        (self.elapsed / self.schedule.duration).clamp(0.0, 1.0)
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Progress through the heating phase, in `[0, 1]`.
    fn heat(&self) -> f64 {
        let heat_end = self.schedule.duration * self.schedule.heat_fraction;
        (self.elapsed / heat_end).clamp(0.0, 1.0)
    }

    /// Temperature in `[0, 1]`: quadratic cooling over the heating phase.
    fn temperature(&self) -> f64 {
        let t = self.heat();
        (1.0 - t) * (1.0 - t)
    }

    /// Band target: eases from the start side to the floor over the heating
    /// phase, then holds. With relaxing on, each cycle squeezes and then
    /// opens the band again, by less each time, so it ends at the floor.
    fn target_side(&self) -> f64 {
        let t = self.heat();
        let eased = t * t * (3.0 - 2.0 * t);
        let squeeze = self.start_side + (self.floor_side - self.start_side) * eased;
        let period = self.schedule.relax_period;
        if period <= 0.0 {
            return squeeze;
        }
        // Zero at the start of each cycle (squeezed), one halfway (relaxed).
        let wave = 0.5 * (1.0 - (std::f64::consts::TAU * self.elapsed / period).cos());
        // Open by at least 0.4 of the squeeze span, so relaxes stay visible
        // even where the eased squeeze falls fastest, mid-run. The span
        // tests are the evidence, from small spans to large.
        let span = self.start_side - self.floor_side;
        let depth = self.schedule.relax_depth.max(0.4 * span);
        // Relaxing never opens the band past where the run started.
        (squeeze + depth * (1.0 - t) * wave).min(self.start_side)
    }

    /// Advance by `dt` seconds and return what to apply this frame. A long
    /// frame stall yields at most one shake, never a burst.
    pub fn advance(&mut self, dt: f64) -> Vec<Command> {
        if self.finished {
            return Vec::new();
        }
        self.elapsed += dt.max(0.0);
        let mut commands = vec![Command::Band {
            target_side: self.target_side(),
            tension: self.schedule.band_tension,
        }];
        if self.elapsed >= self.schedule.duration {
            self.finished = true;
            commands.push(Command::Measure);
            return commands;
        }
        if self.elapsed >= self.next_shake {
            self.next_shake = self.elapsed + self.schedule.shake_interval;
            let strength = self.schedule.initial_strength * self.temperature() as f32;
            if strength >= self.schedule.min_strength {
                self.seed = splitmix(self.seed);
                commands.push(Command::Shake {
                    seed: self.seed,
                    strength,
                });
            }
        }
        commands
    }
}

fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f64 = 1.0 / 60.0;

    /// Run a full anneal at 60 fps and collect (time, command) pairs.
    fn run(start: f64, floor: f64) -> Vec<(f64, Command)> {
        let mut a = Anneal::new(Schedule::default(), start, floor, 7);
        let mut out = Vec::new();
        let mut t = 0.0;
        while !a.is_finished() {
            t += FRAME;
            out.extend(a.advance(FRAME).into_iter().map(|c| (t, c)));
            assert!(t < 30.0, "anneal must terminate");
        }
        out
    }

    fn shakes(cmds: &[(f64, Command)]) -> Vec<(f64, f32, u64)> {
        cmds.iter()
            .filter_map(|(t, c)| match c {
                Command::Shake { seed, strength } => Some((*t, *strength, *seed)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn shakes_weaken_and_stop_before_cool_down() {
        let cmds = run(4.5, 3.0);
        let shakes = shakes(&cmds);
        assert!(shakes.len() >= 10, "{} shakes", shakes.len());
        // The first shake lands on the first frame, at nearly full strength.
        assert!(shakes[0].1 > 0.99, "first strength {}", shakes[0].1);
        assert!(shakes.windows(2).all(|w| w[1].1 < w[0].1));
        assert!(
            shakes.windows(2).all(|w| w[1].2 != w[0].2),
            "fresh seed per shake"
        );
        let heat_end = 20.0 * 0.8;
        assert!(shakes.iter().all(|(t, _, _)| *t <= heat_end));
    }

    #[test]
    fn band_eases_to_floor_and_holds() {
        let cmds = run(4.5, 3.0);
        let targets: Vec<f64> = cmds
            .iter()
            .filter_map(|(_, c)| match c {
                Command::Band {
                    target_side,
                    tension,
                } => {
                    assert_eq!(*tension, 30.0);
                    Some(*target_side)
                }
                _ => None,
            })
            .collect();
        assert!(targets.windows(2).all(|w| w[1] <= w[0] + 1e-12));
        assert!(targets.iter().all(|s| *s >= 3.0 - 1e-12));
        assert!((targets.last().unwrap() - 3.0).abs() < 1e-12);
        assert!(targets[0] > 4.49);
    }

    #[test]
    fn measures_exactly_once_at_the_end() {
        let cmds = run(4.5, 3.0);
        let measures: Vec<f64> = cmds
            .iter()
            .filter(|(_, c)| *c == Command::Measure)
            .map(|(t, _)| *t)
            .collect();
        assert_eq!(measures.len(), 1);
        assert!(measures[0] >= 20.0 && measures[0] < 20.0 + 2.0 * FRAME);
        assert_eq!(cmds.last().unwrap().1, Command::Measure);
    }

    #[test]
    fn gentle_squeeze_never_shakes_and_measures_once() {
        let mut a = Anneal::new(Schedule::gentle(), 4.5, 3.0, 7);
        let mut commands = Vec::new();
        let mut t = 0.0;
        while !a.is_finished() {
            t += FRAME;
            commands.extend(a.advance(FRAME));
            assert!(t < 20.0, "gentle squeeze must terminate");
        }
        assert!(!commands.iter().any(|c| matches!(c, Command::Shake { .. })));
        let targets: Vec<(f64, f32)> = commands
            .iter()
            .filter_map(|c| match c {
                Command::Band {
                    target_side,
                    tension,
                } => Some((*target_side, *tension)),
                _ => None,
            })
            .collect();
        assert!(targets.iter().all(|(_, tension)| *tension == 15.0));
        assert!(targets.windows(2).all(|w| w[1].0 <= w[0].0 + 1e-12));
        assert!((targets.last().unwrap().0 - 3.0).abs() < 1e-12);
        let measures = commands.iter().filter(|c| **c == Command::Measure).count();
        assert_eq!(measures, 1);
        assert!((t - 12.0).abs() < 2.0 * FRAME);
    }

    #[test]
    fn squeeze_down_relaxes_between_squeezes_and_ends_squeezed() {
        let mut a = Anneal::new(Schedule::squeeze_down(), 4.5, 3.0, 7);
        let mut commands = Vec::new();
        let mut t = 0.0;
        while !a.is_finished() {
            t += FRAME;
            commands.extend(a.advance(FRAME));
            assert!(t < 25.0, "squeeze down must terminate");
        }
        assert!(!commands.iter().any(|c| matches!(c, Command::Shake { .. })));
        let targets: Vec<f64> = commands
            .iter()
            .filter_map(|c| match c {
                Command::Band {
                    target_side,
                    tension,
                } => {
                    assert_eq!(*tension, 40.0);
                    Some(*target_side)
                }
                _ => None,
            })
            .collect();
        // Relaxed peaks: the band opens again several times, each time
        // from a lower squeeze. Early relaxes can reach the start side,
        // where the clamp flattens them; count only the free peaks.
        let peaks: Vec<f64> = targets
            .windows(3)
            .filter(|w| w[1] > w[0] && w[1] >= w[2] && w[1] < 4.5)
            .map(|w| w[1])
            .collect();
        assert!(peaks.len() >= 4, "{} relax cycles", peaks.len());
        assert!(peaks.windows(2).all(|w| w[1] < w[0]), "{peaks:?}");
        // A real relax: the band opens by a visible amount, then squeezes.
        let rise = targets.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max);
        assert!(rise > 1e-3, "largest per-frame opening {rise}");
        assert!(targets.iter().all(|s| (3.0 - 1e-12..=4.5).contains(s)));
        assert!((targets[0] - 4.5).abs() < 0.01, "starts at the start side");
        assert!((targets.last().unwrap() - 3.0).abs() < 1e-12);
        let measures = commands.iter().filter(|c| **c == Command::Measure).count();
        assert_eq!(measures, 1);
    }

    #[test]
    fn squeeze_down_relaxes_visibly_at_every_span() {
        // Small, typical (n = 2), mid, and large squeeze spans.
        for (start, floor) in [(1.6, 1.414), (2.5, 1.414), (4.5, 3.0), (15.0, 10.0)] {
            let mut a = Anneal::new(Schedule::squeeze_down(), start, floor, 3);
            let mut targets = Vec::new();
            let mut measures = 0;
            while !a.is_finished() {
                for c in a.advance(FRAME) {
                    match c {
                        Command::Band { target_side, .. } => targets.push(target_side),
                        Command::Measure => measures += 1,
                        Command::Shake { .. } => panic!("squeeze down never shakes"),
                    }
                }
            }
            // Openings: rising stretches of the band target that gain over 0.01.
            let mut openings = 0;
            let mut gain = 0.0;
            for w in targets.windows(2) {
                if w[1] > w[0] {
                    gain += w[1] - w[0];
                } else {
                    openings += usize::from(gain > 0.01);
                    gain = 0.0;
                }
            }
            openings += usize::from(gain > 0.01);
            let span = format!("{start} -> {floor}");
            assert!(openings >= 4, "{span}: {openings} openings");
            assert!(
                targets.iter().all(|s| (floor - 1e-12..=start).contains(s)),
                "{span}: target left [floor, start]"
            );
            assert!((targets.last().unwrap() - floor).abs() < 1e-12, "{span}");
            assert_eq!(measures, 1, "{span}");
        }
    }

    #[test]
    fn squeeze_down_at_the_floor_holds_still() {
        for start in [3.0, 2.0] {
            let mut a = Anneal::new(Schedule::squeeze_down(), start, 3.0, 1);
            while !a.is_finished() {
                for c in a.advance(FRAME) {
                    if let Command::Band { target_side, .. } = c {
                        assert_eq!(target_side, 3.0, "start {start}");
                    }
                }
            }
        }
    }

    #[test]
    fn frame_stall_does_not_burst_shakes() {
        let mut a = Anneal::new(Schedule::default(), 4.5, 3.0, 1);
        let burst = a.advance(5.0);
        let count = burst
            .iter()
            .filter(|c| matches!(c, Command::Shake { .. }))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn start_below_floor_is_clamped_and_progress_is_bounded() {
        let mut a = Anneal::new(Schedule::default(), 2.0, 3.0, 1);
        for c in a.advance(FRAME) {
            if let Command::Band { target_side, .. } = c {
                assert_eq!(target_side, 3.0);
            }
        }
        assert!(a.progress() > 0.0 && a.progress() < 0.01);
        a.advance(1000.0);
        assert_eq!(a.progress(), 1.0);
    }
}
