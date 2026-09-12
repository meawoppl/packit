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
    mount_at("").await
}

/// Mount with `?{query}` as the page query (empty clears it). Settling writes a
/// share code into the URL, so every mount sets the query it expects.
async fn mount_at(query: &str) -> (yew::AppHandle<Host>, Element, Physics) {
    let window = web_sys::window().unwrap();
    let path = window.location().pathname().unwrap();
    let url = if query.is_empty() {
        path
    } else {
        format!("{path}?{query}")
    };
    window
        .history()
        .unwrap()
        .replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url))
        .unwrap();
    TEST_REPORT.with(|r| r.take());
    let document = window.document().unwrap();
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
    // The request (1.8) is beyond the 0.3 reach at tension 30, so the band's
    // target leads the actual side by that reach and the readout shows it.
    let target = physics.params().target_side;
    assert!(
        target > 1.8 && (target - (physics.side() - 0.3)).abs() < 0.02,
        "target {target} leads side {} by the reach",
        physics.side()
    );
    let shown: f64 = text(&root, "label[for=pg-size] output").parse().unwrap();
    assert!((shown - target).abs() < 0.02, "readout {shown} vs {target}");
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

/// Set a range input's value and fire `input` like a user drag. Returns the
/// value the input actually holds, since range inputs clamp to min/max.
fn slide(root: &Element, selector: &str, value: &str) -> f64 {
    let slider: HtmlInputElement = root
        .query_selector(selector)
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    slider.set_value(value);
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    slider
        .dispatch_event(&Event::new_with_event_init_dict("input", &init).unwrap())
        .unwrap();
    slider.value().parse().unwrap()
}

#[wasm_bindgen_test]
async fn size_scrub_reach_follows_band_pressure() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    // Two squares side by side hold the container near 2.0, so the smallest
    // request the slider allows (sqrt(2)) can't be reached at tension 30; the
    // target may only lead the side by the 0.3 reach.
    let desired = slide(&root, "#pg-size", "1.2");
    for _ in 0..40 {
        sleep(50).await;
        let (target, side) = (physics.params().target_side, physics.side());
        assert!(
            (target - desired.max(side - 0.3)).abs() < 0.03,
            "tension 30 reach: target {target}, side {side}, desired {desired}"
        );
    }
    assert!(physics.params().target_side > 1.5, "squares push back");

    // Full pressure lets the target run a whole unit ahead, down to the request.
    slide(&root, "#pg-band", "100");
    sleep(100).await;
    let (target, side) = (physics.params().target_side, physics.side());
    assert_eq!(physics.params().band_tension, 100.0);
    assert!(
        (target - desired.max(side - 1.0)).abs() < 0.03,
        "tension 100 reach: target {target}, side {side}, desired {desired}"
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

fn two_squares() -> Arrangement {
    let sq = |cx| shared::Placement {
        cx,
        cy: 0.75,
        theta: 0.125,
    };
    Arrangement {
        n: 2,
        side: 2.5,
        squares: vec![sq(0.75), sq(1.875)],
    }
}

#[wasm_bindgen_test]
async fn share_link_loads_paused_and_unvalidated() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at(&format!("s={}", share::encode(&two_squares()))).await;
    assert!(physics.paused(), "shared packings open paused");
    assert_eq!(physics.arrangement(), two_squares());
    assert!(text(&root, ".pg-status").starts_with("Shared packing loaded"));
    assert!(
        text(&root, ".pg-benchmark").contains("unchecked")
            || text(&root, ".pg-benchmark").contains("No reference")
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn bad_share_link_reports_an_error() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at("s=zz").await;
    assert!(text(&root, ".pg-status").starts_with("Share link:"));
    assert_eq!(physics.side(), 2.5, "nothing was loaded");
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn share_button_writes_a_decodable_link() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    force_button(&root, "Pause").click();
    sleep(30).await;
    let buttons = root.query_selector_all(".pg-submit button").unwrap();
    let share: HtmlElement = (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default() == "Share")
        .unwrap();
    share.click();
    sleep(100).await;
    let search = web_sys::window().unwrap().location().search().unwrap();
    let code = search
        .strip_prefix("?s=")
        .expect("share code in the address bar");
    assert_eq!(share::decode(code, 2).unwrap(), physics.arrangement());
    let status = text(&root, ".pg-status");
    assert!(
        status.starts_with("Share link copied") || status.contains("address bar"),
        "{status}"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn gentle_squeeze_tightens_and_hands_over_to_anneal() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let start_side = physics.side();
    force_button(&root, "Gentle squeeze").click();
    sleep(1000).await;
    let params = physics.params();
    assert_eq!(params.band_tension, 15.0, "soft band");
    assert!(params.target_side < start_side, "{}", params.target_side);
    // Stopping the squeeze directly releases the band and restores its label.
    force_button(&root, "Stop").click();
    sleep(50).await;
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "stopping releases the band"
    );
    assert!(text(&root, ".pg-status").starts_with("Gentle squeeze stopped"));
    force_button(&root, "Gentle squeeze").click();
    sleep(200).await;
    assert_eq!(physics.params().band_tension, 15.0);
    force_button(&root, "Stop");
    force_button(&root, "Anneal");

    // The other run's button switches runs rather than stacking them.
    force_button(&root, "Anneal").click();
    sleep(100).await;
    assert_eq!(physics.params().band_tension, 30.0);
    force_button(&root, "Gentle squeeze");

    force_button(&root, "Stop").click();
    sleep(50).await;
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "cancel releases the band"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn finished_gentle_squeeze_measures_and_releases_the_band() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    force_button(&root, "Gentle squeeze").click();
    sleep(13_500).await;
    assert_eq!(physics.params().band_tension, 0.0, "band released");
    assert!(physics.paused(), "measuring pauses the scene");
    force_button(&root, "Gentle squeeze");
    handle.destroy();
    root.remove();
}

fn history_length() -> u32 {
    web_sys::window()
        .unwrap()
        .history()
        .unwrap()
        .length()
        .unwrap()
}

/// Poll the address bar for a share code, for up to `ms` milliseconds.
async fn wait_for_share_code(ms: u64) -> String {
    for _ in 0..ms / 100 {
        let search = web_sys::window().unwrap().location().search().unwrap();
        if let Some(code) = search.strip_prefix("?s=") {
            return code.to_string();
        }
        sleep(100).await;
    }
    panic!("no share code in the URL after {ms} ms");
}

/// After a valid measure the URL carries the refined f64 arrangement, not the
/// f32 display scene: refine's final (1 + 2e-10) expansion leaves a side that
/// f32 cannot represent.
fn assert_validated_f64(code: &str, root: &Element) {
    let status = text(root, ".pg-status");
    assert!(status.starts_with("Ready"), "measure validated: {status}");
    let shared = share::decode(code, 2).unwrap();
    let report = TEST_REPORT
        .with(|r| r.borrow().clone())
        .expect("a validated report");
    assert_eq!(
        shared, report,
        "URL carries the validated report bit-for-bit"
    );
    // Supplemental: the refined side is not something f32 could represent.
    assert_ne!(
        shared.side as f32 as f64, shared.side,
        "URL keeps the refined f64 side"
    );
}

/// The URL's code decodes to the arrangement now in the physics (which holds
/// the refined packing in f32, hence the tolerance).
fn assert_url_matches_scene(code: &str, physics: &Physics) {
    let shared = share::decode(code, 2).unwrap();
    let scene = physics.arrangement();
    assert!(
        (shared.side - scene.side).abs() < 1e-6,
        "{} vs {}",
        shared.side,
        scene.side
    );
    for (a, b) in shared.squares.iter().zip(&scene.squares) {
        for (x, y) in [(a.cx, b.cx), (a.cy, b.cy), (a.theta, b.theta)] {
            assert!((x - y).abs() < 1e-6, "URL {a:?} vs scene {b:?}");
        }
    }
}

#[wasm_bindgen_test]
async fn auto_settle_writes_the_solution_into_the_url() {
    let _gpu = NoWebGpu::install();
    let entries = history_length();
    let (handle, root, physics) = mount().await;
    // A fresh grid is already calm, so auto-measure runs after the settle window.
    let code = wait_for_share_code(8000).await;
    assert!(physics.paused(), "settling measured the scene");
    assert_url_matches_scene(&code, &physics);
    assert_validated_f64(&code, &root);
    assert_eq!(
        history_length(),
        entries,
        "replaceState adds no history entry"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn manual_measure_writes_the_solution_into_the_url() {
    let _gpu = NoWebGpu::install();
    let entries = history_length();
    let (handle, root, physics) = mount().await;
    let buttons = root.query_selector_all(".pg-submit button").unwrap();
    let measure: HtmlElement = (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default() == "Settle & measure")
        .unwrap();
    measure.click();
    // Well inside the ~2.5 s auto-settle window, so this is the manual measure.
    let code = wait_for_share_code(1500).await;
    assert_url_matches_scene(&code, &physics);
    assert_validated_f64(&code, &root);
    assert_eq!(
        history_length(),
        entries,
        "replaceState adds no history entry"
    );
    handle.destroy();
    root.remove();
}
