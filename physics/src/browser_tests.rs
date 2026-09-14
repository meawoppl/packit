use super::*;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test(async)]
async fn gpu_matches_cpu_and_keeps_direct_edits() {
    let gpu = Physics::new(4, 4.0);
    let cpu = Physics::new(4, 4.0);
    for p in [&gpu, &cpu] {
        p.set_params(Params {
            attraction: true,
            edge_attraction: 25.0,
            ..p.params()
        });
        p.set_pose(0, 1.4, 1.0, 0.15);
        p.set_pose(1, 2.2, 1.0, -0.1);
        p.set_pose(2, 1.1, 2.8, 0.1);
        p.set_pose(3, 2.5, 2.8, -0.1);
        p.turn(0, 0.04);
    }
    gpu.init_gpu().await;
    assert_eq!(gpu.mode(), Backend::Gpu, "test requires a WebGPU adapter");
    gpu.step(1).await;
    cpu.step(1).await;
    for (a, b) in gpu.bodies().iter().zip(cpu.bodies()) {
        for (x, y) in [a.x, a.y, a.theta, a.vx, a.vy, a.omega]
            .into_iter()
            .zip([b.x, b.y, b.theta, b.vx, b.vy, b.omega])
        {
            assert!((x - y).abs() < 0.005, "GPU={x}, CPU={y}");
        }
    }
    for (a, b) in gpu.contact_forces().iter().zip(cpu.contact_forces()) {
        for (x, y) in a.iter().zip(b) {
            assert!(
                (x - y).abs() < 0.02 + 0.002 * x.abs().max(y.abs()),
                "force GPU={x}, CPU={y}"
            );
        }
    }
    // Poll once to submit, then edit while the mapping is pending.
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let mut pending = std::pin::pin!(gpu.step(2));
    assert!(matches!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    gpu.set_pose(0, 3.0, 3.0, 0.0);
    gpu.set_mouse(2.0, 2.0, Some(1), true);
    let revision = gpu.state.borrow().revision;
    gpu.turn(1, 0.04);
    assert_eq!(gpu.state.borrow().revision, revision);
    gpu.step(1).await; // A concurrent step is safely ignored.
    pending.await;
    assert_eq!(gpu.bodies()[0].x, 3.0);
    assert_eq!(gpu.bodies()[0].y, 3.0);
    assert!(gpu.contact_forces().iter().all(|f| *f == [0.0; 2]));
    gpu.dispose();
    assert_eq!(gpu.mode(), Backend::Cpu);
}
#[wasm_bindgen_test(async)]
async fn gpu_drag_and_band_are_physical() {
    let p = Physics::new(2, 4.0);
    p.set_pose(0, 1.0, 2.0, 0.0);
    p.set_pose(1, 2.1, 2.0, 0.0);
    p.init_gpu().await;
    assert_eq!(p.mode(), Backend::Gpu);
    let revision = p.state.borrow().revision;
    for _ in 0..100 {
        p.set_mouse(10.0, 2.0, Some(0), true);
        p.step(6).await;
    }
    let b = p.bodies();
    assert!(b[1].x > 3.3 && b[1].x < 3.6);
    assert!(b[1].x - b[0].x > 0.9);
    assert!(b[0].x < 2.7);
    assert_eq!(p.state.borrow().revision, revision);
    p.set_mouse(0.0, 0.0, None, false);
    p.set_params(Params {
        band_tension: 40.0,
        target_side: 3.0,
        ..p.params()
    });
    for _ in 0..100 {
        p.step(6).await;
    }
    assert!(p.side() < 3.2);
    p.set_params(Params {
        band_tension: 0.0,
        ..p.params()
    });
    let side = p.side();
    p.step(6).await;
    assert_eq!(p.side(), side);
    p.dispose();
}
#[wasm_bindgen_test(async)]
async fn device_loss_and_cancelled_readback_recover() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let p = Physics::new(1, 4.0);
    p.init_gpu().await;
    assert_eq!(p.mode(), Backend::Gpu);
    p.set_mouse(2.0, 0.0, Some(0), true);
    {
        let mut pending = std::pin::pin!(p.step(2));
        assert!(matches!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    } // Cancelling a readback must release its map and in-flight guard.
    p.step(2).await;
    assert_eq!(p.mode(), Backend::Gpu);
    p.state.borrow().gpu.as_ref().unwrap().destroy();
    assert_eq!(p.mode(), Backend::Cpu);
    let y = p.bodies()[0].y;
    p.step(6).await;
    assert!(p.bodies()[0].y < y);
    p.dispose();
}

#[wasm_bindgen_test(async)]
async fn missing_webgpu_uses_cpu() {
    let navigator = web_sys::window().unwrap().navigator();
    let descriptor = js_sys::Object::new();
    js_sys::Reflect::set(&descriptor, &"configurable".into(), &true.into()).unwrap();
    js_sys::Object::define_property(navigator.as_ref(), &"gpu".into(), &descriptor);
    let p = Physics::new(1, 4.0);
    p.init_gpu().await;
    // Restore browser capability before assertions so subsequent tests can use it.
    js_sys::Reflect::delete_property(navigator.as_ref(), &"gpu".into()).unwrap();
    assert_eq!(p.mode(), Backend::Cpu);
    p.set_mouse(2.0, 0.0, Some(0), true);
    let y = p.bodies()[0].y;
    p.step(6).await;
    assert!(p.bodies()[0].y < y);
    p.dispose();
}

#[wasm_bindgen_test(async)]
async fn gpu_torque_clamp_and_resting_grids() {
    let p = Physics::new(1, 4.0);
    p.init_gpu().await;
    assert_eq!(p.mode(), Backend::Gpu);
    for omega in [-100.0, 100.0] {
        p.set_pose(0, 2.0, 2.0, 0.2);
        p.state.borrow_mut().bodies[0].omega = omega;
        p.step(1).await;
        assert_eq!(p.bodies()[0].omega, omega.signum() * 8.0);
    }
    p.dispose();
    for edge_attraction in [0.0, 30.0] {
        let p = Physics::new(9, 3.5);
        p.init_gpu().await;
        assert_eq!(p.mode(), Backend::Gpu);
        p.set_params(Params {
            edge_attraction,
            band_tension: 30.0,
            target_side: 3.0,
            ..p.params()
        });
        for _ in 0..160 {
            p.step(6).await;
        }
        assert!(
            p.motion() < 0.002 * 9.0,
            "edge={edge_attraction}, motion={}, bodies={:?}",
            p.motion(),
            p.bodies()
        );
        p.dispose();
    }
}

#[wasm_bindgen_test(async)]
async fn gpu_turn_pushes_neighbors_and_resists_a_wedge() {
    let p = Physics::new(2, 4.0);
    p.set_pose(0, 1.5, 2.0, 0.0);
    p.set_pose(1, 2.51, 2.0, 0.0);
    p.init_gpu().await;
    assert_eq!(p.mode(), Backend::Gpu);
    let revision = p.state.borrow().revision;
    p.turn(0, 0.35);
    assert_eq!(p.bodies()[0].theta, 0.0);
    for _ in 0..10 {
        p.step(6).await;
    }
    assert!(p.bodies()[1].x > 2.53, "{:?}", p.bodies());
    assert_eq!(p.state.borrow().revision, revision);
    p.dispose();
    let wedged = Physics::new(4, 2.0);
    wedged.init_gpu().await;
    assert_eq!(wedged.mode(), Backend::Gpu);
    for _ in 0..20 {
        wedged.turn(0, 10.0);
        wedged.step(6).await;
    }
    assert!(
        wedged.bodies()[0].theta.abs() < 0.2,
        "{:?}",
        wedged.bodies()
    );
    assert_eq!(wedged.state.borrow().revision, 1);
    wedged.dispose();
}

#[wasm_bindgen_test(async)]
async fn glue_pairs_match_cpu_in_both_orders() {
    let gpu = Physics::new(2, 4.0);
    let cpu = Physics::new(2, 4.0);
    gpu.init_gpu().await;
    assert_eq!(gpu.mode(), Backend::Gpu);
    let features0 = [
        Feature::Edge { square: 0, edge: 0 },
        Feature::Corner {
            square: 0,
            corner: 3,
        },
        Feature::Midpoint { square: 0, edge: 0 },
    ];
    let features1 = [
        Feature::Edge { square: 1, edge: 2 },
        Feature::Corner {
            square: 1,
            corner: 0,
        },
        Feature::Midpoint { square: 1, edge: 2 },
        Feature::Wall(2),
    ];
    for a in features0 {
        for b in features1 {
            for reversed in [false, true] {
                let g = if reversed {
                    Glue { a: b, b: a }
                } else {
                    Glue { a, b }
                };
                for p in [&gpu, &cpu] {
                    p.reset();
                    p.set_pose(0, 1.1, 1.1, 0.13);
                    p.set_pose(1, 2.35, 1.3, -0.17);
                    p.set_glues(&[g]).unwrap();
                }
                gpu.step(1).await;
                cpu.step(1).await;
                assert_eq!(gpu.mode(), Backend::Gpu);
                for (a, b) in gpu.bodies().iter().zip(cpu.bodies()) {
                    for (x, y) in [a.x, a.y, a.theta, a.vx, a.vy, a.omega]
                        .into_iter()
                        .zip([b.x, b.y, b.theta, b.vx, b.vy, b.omega])
                    {
                        assert!((x - y).abs() < 0.002, "{g:?}: GPU={x}, CPU={y}");
                    }
                }
            }
        }
    }
    gpu.dispose();
}

#[wasm_bindgen_test(async)]
async fn redundant_corner_unions_settle_on_gpu() {
    let p = Physics::new(4, 5.0);
    p.init_gpu().await;
    assert_eq!(p.mode(), Backend::Gpu);
    tests::dense_corner_cluster(&p);
    for _ in 0..400 {
        p.step(6).await;
    }
    assert_eq!(p.mode(), Backend::Gpu);
    assert!(p.motion() < 0.008, "motion={}", p.motion());
    p.dispose();
}

#[wasm_bindgen_test(async)]
async fn centered_drag_growth_matches_cpu_on_every_wall() {
    let gpu = Physics::new(1, 2.0);
    let cpu = Physics::new(1, 2.0);
    gpu.init_gpu().await;
    assert_eq!(gpu.mode(), Backend::Gpu);
    for (x, y) in [(-2.0, 1.0), (1.0, -2.0), (4.0, 1.0), (1.0, 4.0)] {
        for p in [&cpu, &gpu] {
            p.load(&shared::Arrangement {
                container: shared::Shape::Square,
                shape: shared::Shape::Square,
                n: 1,
                side: 2.0,
                squares: vec![shared::Placement {
                    cx: 1.0,
                    cy: 1.0,
                    theta: 0.0,
                }],
            });
            p.set_drag_expansion(true);
        }
        let shifts = [cpu.frame_shift(), gpu.frame_shift()];
        for _ in 0..80 {
            for (p, start) in [&cpu, &gpu].into_iter().zip(shifts) {
                let shift = (p.frame_shift() - start) as f32;
                p.set_mouse(x + shift, y + shift, Some(0), true);
                let old_side = p.side();
                p.step(6).await;
                if p.side() > old_side {
                    assert!(p.contact_forces()[0].iter().any(|f| f.abs() > 1.0));
                }
            }
        }
        assert!(gpu.side() > 2.1, "wall {x},{y}: {}", gpu.side());
        assert!((cpu.side() - gpu.side()).abs() < 0.03);
        for (a, b) in cpu.bodies().iter().zip(gpu.bodies()) {
            assert!((a.x - b.x).abs() < 0.03 && (a.y - b.y).abs() < 0.03);
        }
        gpu.set_mouse(0.0, 0.0, None, false);
        let side = gpu.side();
        gpu.step(6).await;
        assert_eq!(gpu.side(), side);
    }
    gpu.dispose();
}

#[wasm_bindgen_test(async)]
async fn gpu_settle_resolves_contacts_and_reports_glued_jams() {
    let gpu = Physics::new(2, 1.8);
    gpu.set_pose(0, 0.5, 0.9, 0.0);
    gpu.set_pose(1, 1.3, 0.9, 0.0);
    gpu.init_gpu().await;
    assert_eq!(gpu.mode(), Backend::Gpu);
    let before = gpu.bodies();
    gpu.begin_settle();
    assert_eq!(gpu.bodies(), before);
    for _ in 0..260 {
        gpu.step(6).await;
    }
    assert_eq!(
        gpu.settle_status().phase,
        SettlePhase::Settled,
        "{:?}",
        gpu.settle_status()
    );
    assert!(gpu.violations().max_depth <= SETTLE_DEPTH);
    assert!(gpu.paused());
    let links = [0, 2].map(|edge| Glue {
        a: Feature::Midpoint { square: 0, edge },
        b: Feature::Midpoint { square: 1, edge },
    });
    gpu.set_glues(&links).unwrap();
    gpu.begin_settle();
    for _ in 0..260 {
        gpu.step(6).await;
    }
    assert_eq!(
        gpu.settle_status().phase,
        SettlePhase::Blocked,
        "{:?}",
        gpu.settle_status()
    );
    assert_eq!(gpu.glues(), links);
    assert!(gpu.paused());
    gpu.dispose();
}

#[wasm_bindgen_test(async)]
async fn gpu_settle_preserves_resolvable_wall_glue_chain() {
    let gpu = Physics::new(4, 2.0);
    gpu.init_gpu().await;
    assert_eq!(gpu.mode(), Backend::Gpu);
    for offset in [0.005, 0.01, 0.02, 0.05] {
        gpu.load(&shared::Arrangement {
            container: shared::Shape::Square,
            shape: shared::Shape::Square,
            n: 4,
            side: 2.0,
            squares: (0..4)
                .map(|i| shared::Placement {
                    cx: 0.5 + (i % 2) as f64,
                    cy: 0.5 + (i / 2) as f64 - if i >= 2 { offset } else { 0.0 },
                    theta: 0.0,
                })
                .collect(),
        });
        let point = |square, edge| Feature::Midpoint { square, edge };
        gpu.set_glues(&[
            Glue {
                a: Feature::Wall(0),
                b: point(0, 2),
            },
            Glue {
                a: point(0, 0),
                b: point(1, 2),
            },
            Glue {
                a: point(1, 0),
                b: Feature::Wall(2),
            },
        ])
        .unwrap();
        gpu.begin_settle();
        for _ in 0..260 {
            gpu.step(6).await;
            if gpu.settle_status().phase != SettlePhase::Running {
                break;
            }
        }
        assert_eq!(
            gpu.settle_status().phase,
            SettlePhase::Settled,
            "offset {offset}: {:?}",
            gpu.settle_status()
        );
        assert_eq!(gpu.side(), 2.0);
    }
    gpu.dispose();
}

#[wasm_bindgen_test(async)]
async fn touch_springs_use_live_cpu_state_then_resume_gpu() {
    let p = Physics::new(2, 6.0);
    p.set_pose(0, 1.5, 3.0, 0.0);
    p.set_pose(1, 4.5, 3.0, 0.0);
    p.init_gpu().await;
    assert_eq!(p.mode(), Backend::Gpu);
    p.set_grabs(&[(0, 1.5, 4.0), (1, 4.5, 2.0)]);
    assert_eq!(p.mode(), Backend::Cpu);
    for _ in 0..20 {
        p.step(6).await;
    }
    assert!(p.bodies()[0].y > 3.5 && p.bodies()[1].y < 2.5);
    p.set_grabs(&[]);
    let before = p.bodies();
    assert_eq!(p.mode(), Backend::Gpu);
    p.step(1).await;
    for (a, b) in before.iter().zip(p.bodies()) {
        assert!((a.y - b.y).abs() < 0.15);
    }
}

#[wasm_bindgen_test(async)]
async fn gpu_slack_settle_tightens_then_finishes_clear() {
    let gpu = Physics::new(2, 3.0);
    gpu.init_gpu().await;
    assert_eq!(gpu.mode(), Backend::Gpu);
    let before = gpu.arrangement();
    gpu.begin_settle_with_pressure();
    assert_eq!(gpu.arrangement(), before);
    assert!(gpu.params().band_tension > 0.0);
    for _ in 0..390 {
        gpu.step(6).await;
    }
    assert_eq!(
        gpu.settle_status().phase,
        SettlePhase::Settled,
        "{:?}",
        gpu.settle_status()
    );
    assert!(gpu.side() < 2.9);
    assert!(gpu.violations().max_depth <= crate::SETTLE_DEPTH);
    assert_eq!(gpu.params().band_tension, 0.0);
    gpu.dispose();
}
