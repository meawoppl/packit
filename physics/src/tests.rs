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
