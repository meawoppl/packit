use super::*;
fn advance(p: &Physics, n: usize) {
    for _ in 0..n {
        p.state.borrow_mut().cpu_step();
    }
}
#[test]
fn gravity_and_stiff_contacts_stay_bounded() {
    let p = Physics::new(16, 4.5);
    p.set_params(Params {
        gravity: true,
        attraction: true,
        stiffness: 1600.0,
        damping: 0.3,
        ..p.params()
    });
    advance(&p, 1200);
    for b in p.bodies() {
        assert!(b.x.is_finite() && b.y.is_finite() && b.theta.is_finite());
        assert!((-0.1..4.6).contains(&b.x));
        assert!((-0.1..4.6).contains(&b.y));
    }
}
#[test]
fn mouse_pushes_neighbor_and_resists_wall() {
    let p = Physics::new(2, 4.0);
    p.set_pose(0, 1.0, 2.0, 0.0);
    p.set_pose(1, 2.1, 2.0, 0.0);
    let revision = p.state.borrow().revision;
    p.set_mouse(10.0, 2.0, Some(0), true);
    advance(&p, 600);
    let b = p.bodies();
    assert!(b[1].x > 3.3 && b[1].x < 3.6);
    assert!(b[1].x - b[0].x > 0.9);
    assert!(b[0].x < 2.7);
    assert_eq!(p.state.borrow().revision, revision);
}
#[test]
fn band_tightens_and_contacts_support_it() {
    let p = Physics::new(2, 2.0);
    p.set_pose(0, 0.5, 0.5, 0.0);
    p.set_pose(1, 1.5, 0.5, 0.0);
    p.set_params(Params {
        band_tension: 40.0,
        target_side: 2.0_f64.sqrt(),
        ..p.params()
    });
    advance(&p, 1200);
    assert!(p.side() > 1.8 && p.side() < 2.01);
    p.set_params(Params {
        band_tension: 0.0,
        ..p.params()
    });
    let side = p.side();
    advance(&p, 120);
    assert_eq!(p.side(), side);
}
#[test]
fn clones_share_edits_and_load_clears_momentum() {
    let p = Physics::new(4, 3.0);
    let clone = p.clone();
    let initial = p.arrangement();
    clone.shake(123);
    assert!(p.motion() > 0.0);
    clone.load(&initial);
    assert_eq!(p.motion(), 0.0);
    assert_eq!(p.arrangement().squares[0].cx, initial.squares[0].cx);
    p.set_pose(0, f32::NAN, 1.0, 0.0);
    assert!(p.bodies()[0].x.is_finite());
}

#[test]
fn stale_readback_keeps_edits_and_merges_other_motion() {
    let p = Physics::new(2, 4.0);
    let initial = p.bodies();
    let revision = p.state.borrow().revision;
    let mut computed = initial.clone();
    computed[0].x += 0.1;
    computed[0].y += 0.2;
    computed[1].x += 0.3;
    computed[0].vx = 1.0;
    p.nudge(0, 1.0, 0.0);
    p.state
        .borrow_mut()
        .merge_readback(&initial, &computed, revision);
    let b = p.bodies();
    assert_eq!(b[0].x, initial[0].x + 1.0);
    assert_eq!(b[0].y, computed[0].y);
    assert_eq!(b[0].vx, 1.0);
    assert_eq!(b[1].x, computed[1].x);
}
#[test]
fn pause_dispose_and_step_budget() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let p = Physics::new(4, 3.0);
    p.set_params(Params {
        gravity: true,
        ..p.params()
    });
    p.set_paused(true);
    let initial = p.bodies();
    let run = |steps| {
        let mut future = std::pin::pin!(p.step(steps));
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(())
        ));
    };
    run(6);
    assert_eq!(p.bodies(), initial);
    p.set_paused(false);
    run(0);
    assert_eq!(p.bodies(), initial);
    run(600);
    let moved = p.bodies();
    let expected = Physics::new(4, 3.0);
    expected.set_params(p.params());
    advance(&expected, 6);
    assert_eq!(moved, expected.bodies());
    p.dispose();
    run(6);
    assert_eq!(p.bodies(), moved);
}
#[test]
fn rotated_overlap_and_attraction() {
    let p = Physics::new(2, 5.0);
    p.set_pose(0, 2.0, 2.0, 0.15);
    p.set_pose(1, 2.7, 2.0, -0.15);
    advance(&p, 240);
    let b = p.bodies();
    assert!((b[0].x - b[1].x).hypot(b[0].y - b[1].y) > 0.99);
    let p = Physics::new(2, 8.0);
    p.set_pose(0, 2.0, 4.0, 0.0);
    p.set_pose(1, 6.0, 4.0, 0.0);
    p.set_params(Params {
        attraction: true,
        ..p.params()
    });
    advance(&p, 120);
    let b = p.bodies();
    assert!(b[1].x - b[0].x < 4.0);
}

#[test]
fn finite_large_import_angles_keep_their_orientation() {
    let p = Physics::new(1, 4.0);
    let mut arrangement = p.arrangement();
    arrangement.squares[0].theta = 1e100;
    arrangement.squares[0].cx = 1.0;
    p.load(&arrangement);
    let b = p.bodies()[0];
    assert_eq!(b.x, 1.0);
    assert!(b.theta.is_finite());
    assert!((b.theta.sin() as f64 - 1e100_f64.sin()).abs() < 1e-6);
}

#[test]
fn edge_attraction_closes_gaps_and_aligns_faces() {
    let p = Physics::new(2, 8.0);
    p.set_pose(0, 3.0, 4.0, 0.2);
    p.set_pose(1, 4.4, 4.0, 0.0);
    p.set_params(Params {
        edge_attraction: 30.0,
        ..p.params()
    });
    advance(&p, 1);
    let b = p.bodies();
    assert!(b[0].vx > 0.0 && b[1].vx < 0.0);
    assert!((b[0].vx + b[1].vx).abs() < 1e-5);
    assert!(
        b[0].omega - b[1].omega < 0.0,
        "facing edges turn toward alignment: {b:?}"
    );
    advance(&p, 600);
    let b = p.bodies();
    assert!(b[1].x - b[0].x < 1.3);
    let far = Physics::new(2, 10.0);
    far.set_pose(0, 3.0, 5.0, 0.0);
    far.set_pose(1, 6.0, 5.0, 0.0);
    far.set_params(p.params());
    advance(&far, 1);
    assert_eq!(far.motion(), 0.0);
}
#[test]
fn edge_attraction_includes_each_wall_and_reacts_on_band() {
    for (x, y, component, direction) in [
        (0.8, 3.0, 0, -1.0),
        (5.2, 3.0, 0, 1.0),
        (3.0, 0.8, 1, -1.0),
        (3.0, 5.2, 1, 1.0),
    ] {
        let p = Physics::new(1, 6.0);
        p.set_pose(0, x, y, 0.0);
        p.set_params(Params {
            edge_attraction: 30.0,
            ..p.params()
        });
        advance(&p, 1);
        let b = p.bodies()[0];
        assert!([b.vx, b.vy][component] * direction > 0.0);
        assert!(b.omega.abs() < 1e-6);
    }
    let p = Physics::new(1, 6.0);
    p.set_pose(0, 5.2, 3.0, 0.0);
    p.set_params(Params {
        edge_attraction: 30.0,
        band_tension: 20.0,
        ..p.params()
    });
    advance(&p, 1);
    assert!(
        p.band_velocity() < 0.0,
        "square pulls movable right wall inward"
    );
}
#[test]
fn off_center_bumps_and_wall_corners_impart_torque() {
    let p = Physics::new(2, 8.0);
    p.set_pose(0, 3.0, 4.0, 0.2);
    p.set_pose(1, 4.0, 4.0, 0.0);
    advance(&p, 1);
    let b = p.bodies();
    assert!(
        b[0].omega < -0.1 && b[1].omega > 0.1,
        "corner hits turn both bodies: {b:?}"
    );
    let p = Physics::new(2, 8.0);
    p.set_pose(0, 3.0, 4.0, 0.0);
    p.set_pose(1, 3.9, 4.0, 0.0);
    advance(&p, 1);
    assert!(
        p.bodies().iter().all(|b| b.omega.abs() < 1e-5),
        "centered face hits don't add spin"
    );
    let p = Physics::new(1, 4.0);
    p.set_pose(0, 3.55, 2.0, 0.2);
    advance(&p, 1);
    assert!(
        p.bodies()[0].omega < -0.1,
        "wall applies torque at support corner"
    );
    let p = Physics::new(1, 4.0);
    for omega in [-100.0, 100.0] {
        p.set_pose(0, 2.0, 2.0, 0.2);
        p.state.borrow_mut().bodies[0].omega = omega;
        advance(&p, 1);
        assert_eq!(p.bodies()[0].omega, omega.signum() * 8.0);
    }
}
#[test]
fn gravity_grids_settle_with_and_without_edge_attraction() {
    for edge_attraction in [0.0, 30.0] {
        let p = Physics::new(9, 3.5);
        p.set_params(Params {
            gravity: true,
            edge_attraction,
            ..p.params()
        });
        advance(&p, 960);
        assert!(
            p.motion() < 0.002 * 9.0,
            "edge={edge_attraction}, motion={}",
            p.motion()
        );
    }
}

#[test]
fn anneal_kicks_scale_deterministically() {
    let full = Physics::new(4, 3.0);
    let cool = Physics::new(4, 3.0);
    full.shake(42);
    cool.shake_scaled(42, 0.25);
    for (a, b) in full.bodies().iter().zip(cool.bodies()) {
        assert_eq!(a.vx * 0.25, b.vx);
        assert_eq!(a.vy * 0.25, b.vy);
        assert_eq!(a.omega * 0.25, b.omega);
        assert_eq!((a.x, a.y, a.theta), (b.x, b.y, b.theta));
    }
    cool.shake_scaled(42, 0.0);
    assert_eq!(cool.motion(), 0.0);
    cool.shake_scaled(42, f32::NAN);
    assert_eq!(cool.motion(), 0.0);
}
