//! In-browser tests of the mounted play screen: real DOM events through Yew's
//! handlers into the physics. Run with `cargo test -p frontend --target
//! wasm32-unknown-unknown` under wasm-bindgen-test-runner.

use super::*;
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement, PointerEventInit};

wasm_bindgen_test_configure!(run_in_browser);

/// `Game` renders router links, so tests mount it inside a router.
#[function_component(Host)]
fn host() -> Html {
    html! { <BrowserRouter><Game n={2} /></BrowserRouter> }
}

/// Remove `navigator.gpu` so the CPU path runs deterministically; returns a
/// guard that restores it.
struct NoWebGpu;
impl NoWebGpu {
    fn install() -> Self {
        let navigator = web_sys::window().unwrap().navigator();
        let descriptor = js_sys::Object::new();
        js_sys::Reflect::set(&descriptor, &"configurable".into(), &true.into()).unwrap();
        js_sys::Object::define_property(navigator.as_ref(), &"gpu".into(), &descriptor);
        Self
    }
}
impl Drop for NoWebGpu {
    fn drop(&mut self) {
        let navigator = web_sys::window().unwrap().navigator();
        let _ = js_sys::Reflect::delete_property(navigator.as_ref(), &"gpu".into());
    }
}

async fn mount() -> (yew::AppHandle<Host>, Element, Physics) {
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.create_element("div").unwrap();
    root.set_attribute("style", "width: 600px").unwrap();
    document.body().unwrap().append_child(&root).unwrap();
    let handle = yew::Renderer::<Host>::with_root(root.clone()).render();
    sleep(100).await;
    let physics = TEST_PHYSICS
        .with(|p| p.borrow().clone())
        .expect("Game registered its physics");
    (handle, root, physics)
}

async fn sleep(ms: u64) {
    gloo_timers::future::sleep(Duration::from_millis(ms)).await;
}

fn text(root: &Element, selector: &str) -> String {
    root.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("{selector} rendered"))
        .text_content()
        .unwrap_or_default()
}

/// Dispatch a bubbling pointer event at world position `p`.
fn pointer(canvas: &HtmlCanvasElement, kind: &str, p: (f64, f64), extent: f64) {
    let r = canvas.get_bounding_client_rect();
    let pad = r.width() * 0.045;
    let scale = (r.width() - 2.0 * pad) / extent;
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_pointer_id(1);
    init.set_is_primary(true);
    init.set_client_x((r.left() + pad + p.0 * scale) as i32);
    init.set_client_y((r.bottom() - pad - p.1 * scale) as i32);
    let event = PointerEvent::new_with_event_init_dict(kind, &init).unwrap();
    canvas.dispatch_event(&event).unwrap();
}

#[wasm_bindgen_test]
async fn cpu_readout_is_filled_on_first_render() {
    let _gpu = NoWebGpu::install();
    let (handle, root, _) = mount().await;
    assert!(text(&root, ".pg-stat").starts_with("2.500000"));
    assert!(text(&root, ".pg-mode").contains("CPU fallback"));
    assert!(text(&root, ".pg-row strong").ends_with('%'));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn drag_wakes_a_paused_scene_and_pushes_the_neighbor() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas: HtmlCanvasElement = root
        .query_selector("canvas")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();

    // Pause through the UI, then grab square 1 and drag it into square 2.
    let pause: HtmlElement = root
        .query_selector(".pg-force-actions button")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    pause.click();
    sleep(50).await;
    assert!(physics.paused());
    let start = physics.bodies();
    let extent = physics.side();
    let grab = (start[0].x as f64, start[0].y as f64);
    pointer(&canvas, "pointerdown", grab, extent);
    // Yew applies the resulting message on a later tick.
    sleep(20).await;
    assert!(!physics.paused(), "grabbing a square resumes physics");
    for k in 1..=10 {
        let x = grab.0 + (2.4 - grab.0) * k as f64 / 10.0;
        pointer(&canvas, "pointermove", (x, grab.1), extent);
        sleep(40).await;
    }
    sleep(1200).await;
    let end = physics.bodies();
    pointer(&canvas, "pointerup", (2.4, grab.1), extent);

    assert!(
        end[1].x > start[1].x + 0.15,
        "neighbor pushed right: {} -> {}",
        start[1].x,
        end[1].x
    );
    assert!(
        end[1].x - end[0].x > 0.9,
        "squares do not pass through: {end:?}"
    );
    handle.destroy();
    root.remove();
}

/// A force-panel button whose label starts with `label`.
fn force_button(root: &Element, label: &str) -> HtmlElement {
    let buttons = root.query_selector_all(".pg-force-actions button").unwrap();
    (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default().starts_with(label))
        .unwrap_or_else(|| panic!("{label} button rendered"))
}

#[wasm_bindgen_test]
async fn play_screen_shows_benchmark_bubble() {
    let _gpu = NoWebGpu::install();
    let (handle, root, _) = mount().await;
    assert!(root
        .query_selector(".pg-top .pg-benchmark")
        .unwrap()
        .is_some());
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn anneal_tightens_the_band_and_a_drag_cancels_it() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas: HtmlCanvasElement = root
        .query_selector("canvas")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let start_side = physics.side();

    force_button(&root, "Anneal").click();
    sleep(1500).await;
    let params = physics.params();
    assert_eq!(params.band_tension, 30.0);
    assert!(params.target_side < start_side, "{}", params.target_side);
    force_button(&root, "Stop");

    let b = physics.bodies()[0];
    pointer(
        &canvas,
        "pointerdown",
        (b.x as f64, b.y as f64),
        physics.side(),
    );
    sleep(50).await;
    pointer(
        &canvas,
        "pointerup",
        (b.x as f64, b.y as f64),
        physics.side(),
    );
    force_button(&root, "Anneal");
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "cancel releases the band"
    );
    let frozen = physics.params().target_side;
    sleep(500).await;
    assert_eq!(
        physics.params().target_side,
        frozen,
        "no more schedule updates"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn pause_cancels_anneal() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    force_button(&root, "Anneal").click();
    sleep(300).await;
    force_button(&root, "Pause").click();
    sleep(50).await;
    assert!(physics.paused());
    force_button(&root, "Anneal");
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "cancel releases the band"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn finished_anneal_measures_and_releases_the_band() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    force_button(&root, "Anneal").click();
    sleep(21_500).await;
    assert_eq!(physics.params().band_tension, 0.0, "band released");
    assert!(physics.paused(), "measuring pauses the scene");
    force_button(&root, "Anneal");
    assert!(!text(&root, ".pg-status").starts_with("Annealing"));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn size_slider_animates_pressure_and_wakes_a_paused_scene() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    force_button(&root, "Pause").click();
    sleep(30).await;
    assert!(physics.paused());
    assert_eq!(physics.params().band_tension, 0.0);
    let start = physics.side();
    let right_square = physics.bodies()[1].x;
    let slider: HtmlInputElement = root
        .query_selector("#pg-size")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    slider.set_value("1.8");
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    slider
        .dispatch_event(&Event::new_with_event_init_dict("input", &init).unwrap())
        .unwrap();
    sleep(50).await;
    assert!(!physics.paused());
    assert_eq!(physics.params().target_side, 1.8);
    assert_eq!(text(&root, "label[for=pg-size] output"), "1.800");
    assert_eq!(physics.params().band_tension, 30.0);
    assert!(physics.side() > start - 0.2, "no instant resize");
    // Allow slow/headless animation scheduling while requiring real movement.
    for _ in 0..60 {
        if physics.side() < start - 0.05 && physics.bodies()[1].x < right_square - 0.02 {
            break;
        }
        sleep(50).await;
    }
    assert!(
        physics.side() < start - 0.05,
        "pressure shrinks the band: {}",
        physics.side()
    );
    assert!(
        physics.bodies()[1].x < right_square - 0.02,
        "the band pushes squares"
    );
    assert!(
        physics.side() > 1.8,
        "contacts and spring resist the target"
    );
    let mut params = physics.params();
    params.band_tension = 55.0;
    physics.set_params(params);
    let before = physics.side();
    slider.set_value("3.2");
    slider
        .dispatch_event(&Event::new_with_event_init_dict("input", &init).unwrap())
        .unwrap();
    sleep(50).await;
    assert_eq!(
        physics.params().band_tension,
        55.0,
        "preserve chosen pressure"
    );
    assert!(physics.side() < before + 0.2, "expansion also animates");
    for _ in 0..60 {
        if physics.side() > before + 0.05 {
            break;
        }
        sleep(50).await;
    }
    assert!(
        physics.side() > before + 0.05,
        "pressure expands the band: {}",
        physics.side()
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn wheel_and_keys_wake_physics_and_turn_without_teleporting() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas: HtmlCanvasElement = root
        .query_selector("canvas")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    // Select the square, then pause; keyboard turns must wake it again.
    let b = physics.bodies()[0];
    pointer(
        &canvas,
        "pointerdown",
        (b.x as f64, b.y as f64),
        physics.side(),
    );
    pointer(
        &canvas,
        "pointerup",
        (b.x as f64, b.y as f64),
        physics.side(),
    );
    sleep(30).await;
    force_button(&root, "Pause").click();
    sleep(30).await;
    let start = physics.bodies()[0].theta;
    let key = web_sys::KeyboardEventInit::new();
    key.set_bubbles(true);
    key.set_key("e");
    canvas
        .dispatch_event(&KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &key).unwrap())
        .unwrap();
    sleep(30).await;
    assert!(!physics.paused());
    assert!(
        (physics.bodies()[0].theta - start).abs() < 0.02,
        "key does not teleport"
    );
    for _ in 0..60 {
        if physics.bodies()[0].theta > start + 0.015 {
            break;
        }
        sleep(30).await;
    }
    assert!(physics.bodies()[0].theta > start + 0.015);
    force_button(&root, "Pause").click();
    sleep(30).await;
    let b = physics.bodies()[0];
    let r = canvas.get_bounding_client_rect();
    let pad = r.width() * 0.045;
    let scale = (r.width() - 2.0 * pad) / physics.side();
    let wheel = web_sys::WheelEventInit::new();
    wheel.set_bubbles(true);
    wheel.set_cancelable(true);
    wheel.set_client_x((r.left() + pad + b.x as f64 * scale) as i32);
    wheel.set_client_y((r.bottom() - pad - b.y as f64 * scale) as i32);
    wheel.set_delta_y(-1.0);
    canvas
        .dispatch_event(&WheelEvent::new_with_event_init_dict("wheel", &wheel).unwrap())
        .unwrap();
    sleep(30).await;
    assert!(!physics.paused());
    assert!(
        (physics.bodies()[0].theta - b.theta).abs() < 0.02,
        "wheel does not teleport"
    );
    for _ in 0..60 {
        if physics.bodies()[0].theta < b.theta - 0.01 {
            break;
        }
        sleep(30).await;
    }
    assert!(physics.bodies()[0].theta < b.theta - 0.01);
    handle.destroy();
    root.remove();
}
