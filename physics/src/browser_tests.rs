use super::*;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test(async)]
async fn gpu_matches_cpu_and_keeps_direct_edits() {
    let gpu = Physics::new(4, 4.0);
    let cpu = Physics::new(4, 4.0);
    for p in [&gpu, &cpu] {
        p.set_params(Params {
            gravity: true,
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
    p.set_params(Params {
        gravity: true,
        ..p.params()
    });
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
    p.set_params(Params {
        gravity: true,
        ..p.params()
    });
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
            gravity: true,
            edge_attraction,
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
