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

fn canvas_of(root: &Element) -> HtmlCanvasElement {
    root.query_selector("canvas")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap()
}

/// A tap (down then up) at world `p`.
async fn tap_at(canvas: &HtmlCanvasElement, p: (f64, f64), extent: f64) {
    pointer(canvas, "pointerdown", p, extent);
    pointer(canvas, "pointerup", p, extent);
    sleep(30).await;
}

/// Two quick taps at world `p`, which open the glue tool.
async fn double_tap(canvas: &HtmlCanvasElement, p: (f64, f64), extent: f64) {
    pointer(canvas, "pointerdown", p, extent);
    pointer(canvas, "pointerup", p, extent);
    tap_at(canvas, p, extent).await;
}

fn midpoint(physics: &Physics, square: usize, edge: u8) -> (f64, f64) {
    let feature = Feature::Midpoint { square, edge };
    match glue::anchor(&physics.bodies(), physics.side(), feature) {
        Some(glue::Anchor::Point(p)) => p,
        other => panic!("{other:?}"),
    }
}

fn top_midpoint(physics: &Physics, square: usize) -> (f64, f64) {
    midpoint(physics, square, 1)
}

/// A spot on the board with no square and no glue target nearby.
fn empty_spot(physics: &Physics) -> (f64, f64) {
    let (bodies, side) = (physics.bodies(), physics.side());
    (1..10)
        .flat_map(|i| (1..10).map(move |j| (side * i as f64 / 10.0, side * j as f64 / 10.0)))
        .find(|p| {
            canvas::hit(&bodies, *p).is_none()
                && glue::pick(&bodies, side, *p, 0.3, |_| true).is_none()
        })
        .expect("an empty spot on the board")
}

#[wasm_bindgen_test]
async fn double_tap_glues_two_features_and_clear_removes_them() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let extent = physics.side();

    double_tap(&canvas, top_midpoint(&physics, 0), extent).await;
    assert!(physics.paused(), "the glue tool pauses the scene");
    let status = text(&root, ".pg-status");
    assert!(status.contains("now tap a target"), "{status}");

    tap_at(&canvas, top_midpoint(&physics, 1), extent).await;
    assert_eq!(
        physics.glues(),
        vec![Glue {
            a: Feature::Midpoint { square: 0, edge: 1 },
            b: Feature::Midpoint { square: 1, edge: 1 },
        }]
    );
    assert!(!physics.paused(), "closing the tool resumes the scene");

    let clear: HtmlElement = root
        .query_selector(".pg-clear-glue")
        .unwrap()
        .expect("Clear glue is shown while glue exists")
        .dyn_into()
        .unwrap();
    clear.click();
    sleep(30).await;
    assert!(physics.glues().is_empty());
    assert!(root.query_selector(".pg-clear-glue").unwrap().is_none());
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn tapping_empty_space_cancels_the_glue_tool() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let extent = physics.side();
    let was_paused = physics.paused();
    let empty = empty_spot(&physics);

    double_tap(&canvas, empty, extent).await;
    assert!(physics.paused());
    let status = text(&root, ".pg-status");
    assert!(status.contains("tap a corner"), "{status}");

    tap_at(&canvas, empty, extent).await;
    let status = text(&root, ".pg-status");
    assert!(status.contains("Glue cancelled"), "{status}");
    assert_eq!(physics.paused(), was_paused, "pause state restored");
    assert!(physics.glues().is_empty());
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn escape_closes_the_glue_tool() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let extent = physics.side();

    double_tap(&canvas, top_midpoint(&physics, 0), extent).await;
    assert!(physics.paused());
    let key = web_sys::KeyboardEventInit::new();
    key.set_bubbles(true);
    key.set_key("Escape");
    canvas
        .dispatch_event(&KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &key).unwrap())
        .unwrap();
    sleep(30).await;
    let status = text(&root, ".pg-status");
    assert!(status.contains("Glue cancelled"), "{status}");
    assert!(!physics.paused(), "the scene the tap woke runs again");

    // The next tap is an ordinary tap, not a second glue pick.
    tap_at(&canvas, top_midpoint(&physics, 1), extent).await;
    assert!(physics.glues().is_empty());
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn glue_survives_settle_and_measure() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let extent = physics.side();
    double_tap(&canvas, top_midpoint(&physics, 0), extent).await;
    tap_at(&canvas, top_midpoint(&physics, 1), extent).await;
    let glued = physics.glues();
    assert_eq!(glued.len(), 1);

    let buttons = root.query_selector_all(".pg-submit button").unwrap();
    let measure: HtmlElement = (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default() == "Settle & measure")
        .unwrap();
    measure.click();
    wait_for_share_code(3000).await;
    assert_eq!(
        physics.glues(),
        glued,
        "measuring reloads but keeps the glue"
    );
    handle.destroy();
    root.remove();
}

/// Controls that move the scene close the glue tool, so the next tap is an
/// ordinary tap rather than a second pick.
#[wasm_bindgen_test]
async fn scene_controls_close_the_glue_tool() {
    let _gpu = NoWebGpu::install();
    for control in ["Shake", "Anneal", "Resume", "size"] {
        let (handle, root, physics) = mount().await;
        let canvas = canvas_of(&root);
        let extent = physics.side();
        double_tap(&canvas, top_midpoint(&physics, 0), extent).await;
        let status = text(&root, ".pg-status");
        assert!(status.contains("now tap a target"), "{control}: {status}");
        if control == "size" {
            slide(&root, "#pg-size", "2.2");
        } else {
            force_button(&root, control).click();
        }
        sleep(30).await;
        tap_at(&canvas, top_midpoint(&physics, 1), extent).await;
        assert!(physics.glues().is_empty(), "{control} closes the glue tool");
        handle.destroy();
        root.remove();
    }
}

/// Choose a file holding `text` in the import picker, as a user would.
fn choose_import(root: &Element, text: &str) {
    let input: HtmlInputElement = root
        .query_selector("input[type=file]")
        .unwrap()
        .expect("import input rendered")
        .dyn_into()
        .unwrap();
    let window = web_sys::window().unwrap();
    let construct = |name: &str, args: &js_sys::Array| {
        let class: js_sys::Function = js_sys::Reflect::get(&window, &name.into())
            .unwrap()
            .dyn_into()
            .unwrap();
        js_sys::Reflect::construct(&class, args).unwrap()
    };
    let file = construct(
        "File",
        &js_sys::Array::of2(&js_sys::Array::of1(&text.into()), &"packing.json".into()),
    );
    let transfer = construct("DataTransfer", &js_sys::Array::new());
    let items = js_sys::Reflect::get(&transfer, &"items".into()).unwrap();
    let add: js_sys::Function = js_sys::Reflect::get(&items, &"add".into())
        .unwrap()
        .dyn_into()
        .unwrap();
    add.call1(&items, &file).unwrap();
    let files = js_sys::Reflect::get(&transfer, &"files".into()).unwrap();
    js_sys::Reflect::set(&input, &"files".into(), &files).unwrap();
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
}

#[wasm_bindgen_test]
async fn import_closes_the_glue_tool() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    double_tap(&canvas, top_midpoint(&physics, 0), physics.side()).await;
    choose_import(&root, &serde_json::to_string(&two_squares()).unwrap());
    for _ in 0..60 {
        if text(&root, ".pg-status").starts_with("Imported") {
            break;
        }
        sleep(30).await;
    }
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Imported"), "{status}");

    // The imported scene is framed to its own side.
    tap_at(&canvas, top_midpoint(&physics, 1), physics.side()).await;
    assert!(physics.glues().is_empty(), "no pick survives the import");
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn tapping_a_link_removes_just_that_link() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let extent = physics.side();
    double_tap(&canvas, top_midpoint(&physics, 0), extent).await;
    tap_at(&canvas, top_midpoint(&physics, 1), extent).await;
    double_tap(&canvas, midpoint(&physics, 0, 3), extent).await;
    tap_at(&canvas, midpoint(&physics, 1, 3), extent).await;
    let [top, bottom] = physics.glues()[..] else {
        panic!("two links: {:?}", physics.glues());
    };
    assert_eq!(bottom.a, Feature::Midpoint { square: 0, edge: 3 });

    force_button(&root, "Pause").click();
    sleep(30).await;
    let (a, b) = glue::link(&physics.bodies(), physics.side(), top).unwrap();
    let on_link = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    double_tap(&canvas, on_link, extent).await;
    let status = text(&root, ".pg-status");
    assert!(status.contains("Tap a link to remove it"), "{status}");
    tap_at(&canvas, on_link, extent).await;
    assert_eq!(physics.glues(), vec![bottom]);
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

/// Stub only /api/shares; other mounted-screen requests still use fetch.
struct ShareApi {
    original: wasm_bindgen::JsValue,
    _fetch: wasm_bindgen::closure::Closure<
        dyn FnMut(wasm_bindgen::JsValue, wasm_bindgen::JsValue) -> js_sys::Promise,
    >,
    requests: Rc<std::cell::RefCell<Vec<shared::CreateShare>>>,
}
impl ShareApi {
    fn install(fail: bool) -> Self {
        let window = web_sys::window().unwrap();
        let original = js_sys::Reflect::get(&window, &"fetch".into()).unwrap();
        let fetch = original.clone().dyn_into::<js_sys::Function>().unwrap();
        let requests = Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = requests.clone();
        let replacement = wasm_bindgen::closure::Closure::wrap(Box::new(
            move |request: wasm_bindgen::JsValue, init: wasm_bindgen::JsValue| {
                let req = request.clone().dyn_into::<web_sys::Request>().unwrap();
                if !req.url().ends_with("/api/shares") {
                    return fetch
                        .call2(&web_sys::window().unwrap(), &request, &init)
                        .unwrap()
                        .unchecked_into();
                }
                let captured = captured.clone();
                wasm_bindgen_futures::future_to_promise(async move {
                    let body = wasm_bindgen_futures::JsFuture::from(req.text().unwrap())
                        .await?
                        .as_string()
                        .unwrap();
                    captured
                        .borrow_mut()
                        .push(serde_json::from_str(&body).unwrap());
                    sleep(150).await;
                    let options = web_sys::ResponseInit::new();
                    options.set_status(if fail { 503 } else { 200 });
                    let text = if fail {
                        r#"{"error":"storage unavailable"}"#
                    } else {
                        r#"{"url":"https://packit.test/s/0123456789abcdef01234567"}"#
                    };
                    Ok(web_sys::Response::new_with_opt_str_and_init(Some(text), &options)?.into())
                })
            },
        )
            as Box<dyn FnMut(wasm_bindgen::JsValue, wasm_bindgen::JsValue) -> js_sys::Promise>);
        js_sys::Reflect::set(&window, &"fetch".into(), replacement.as_ref()).unwrap();
        Self {
            original,
            _fetch: replacement,
            requests,
        }
    }
}
impl Drop for ShareApi {
    fn drop(&mut self) {
        js_sys::Reflect::set(&web_sys::window().unwrap(), &"fetch".into(), &self.original).unwrap();
    }
}

struct Clipboard {
    _write: wasm_bindgen::closure::Closure<dyn FnMut(String) -> js_sys::Promise>,
    values: Rc<std::cell::RefCell<Vec<String>>>,
    fail: Rc<Cell<bool>>,
}
impl Clipboard {
    fn install(fail: bool) -> Self {
        let values = Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = values.clone();
        let fail = Rc::new(Cell::new(fail));
        let failing = fail.clone();
        let write = wasm_bindgen::closure::Closure::wrap(Box::new(move |text: String| {
            if failing.get() {
                js_sys::Promise::reject(&wasm_bindgen::JsValue::UNDEFINED)
            } else {
                captured.borrow_mut().push(text);
                js_sys::Promise::resolve(&wasm_bindgen::JsValue::UNDEFINED)
            }
        })
            as Box<dyn FnMut(String) -> js_sys::Promise>);
        let clipboard = js_sys::Object::new();
        js_sys::Reflect::set(&clipboard, &"writeText".into(), write.as_ref()).unwrap();
        let descriptor = js_sys::Object::new();
        js_sys::Reflect::set(&descriptor, &"configurable".into(), &true.into()).unwrap();
        js_sys::Reflect::set(&descriptor, &"value".into(), &clipboard).unwrap();
        js_sys::Object::define_property(
            web_sys::window().unwrap().navigator().as_ref(),
            &"clipboard".into(),
            &descriptor,
        );
        Self {
            _write: write,
            values,
            fail,
        }
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        let _ = js_sys::Reflect::delete_property(
            web_sys::window().unwrap().navigator().as_ref(),
            &"clipboard".into(),
        );
    }
}

fn submit_button(root: &Element, label: &str) -> HtmlElement {
    let buttons = root.query_selector_all(".pg-submit button").unwrap();
    (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default() == label)
        .unwrap()
}

async fn wait_share(root: &Element) {
    for _ in 0..100 {
        if !text(root, ".pg-share-status").contains("Creating") {
            return;
        }
        sleep(20).await;
    }
    panic!("share request did not complete");
}

#[wasm_bindgen_test]
async fn share_button_copies_a_short_link_for_the_captured_precise_snapshot() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(false);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Settle & measure").click();
    for _ in 0..100 {
        if TEST_REPORT.with(|r| r.borrow().is_some()) {
            break;
        }
        sleep(20).await;
    }
    let report = TEST_REPORT.with(|r| r.borrow().clone()).unwrap();
    let url = web_sys::window().unwrap().location().href().unwrap();
    submit_button(&root, "Share").click();
    sleep(30).await;
    force_button(&root, "Shake").click();
    wait_share(&root).await;
    let body = api.requests.borrow()[0].clone();
    assert_eq!(share::decode(&body.code, body.n).unwrap(), report);
    assert_eq!(
        clipboard.values.borrow().as_slice(),
        ["https://packit.test/s/0123456789abcdef01234567"]
    );
    assert!(text(&root, ".pg-share-status").contains("Snapshot link copied"));
    assert_eq!(
        web_sys::window().unwrap().location().href().unwrap(),
        url,
        "an in-flight share never overwrites the live URL"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn share_copy_failure_offers_selectable_url_and_fresh_gesture_retry() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(false);
    let clipboard = Clipboard::install(true);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    assert!(text(&root, ".pg-share-status").contains("Tap Copy link"));
    assert!(clipboard.values.borrow().is_empty());
    let input: HtmlInputElement = root
        .query_selector("#pg-share-link")
        .unwrap()
        .unwrap()
        .unchecked_into();
    assert_eq!(
        input.value(),
        "https://packit.test/s/0123456789abcdef01234567"
    );
    clipboard.fail.set(false);
    submit_button(&root, "Copy link").click();
    sleep(50).await;
    assert!(text(&root, ".pg-share-status").contains("Snapshot link copied"));
    assert_eq!(
        api.requests.borrow().len(),
        1,
        "copy retry does not create another snapshot"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn share_storage_failure_is_reported_without_claiming_a_copy() {
    let _gpu = NoWebGpu::install();
    let _api = ShareApi::install(true);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    assert!(text(&root, ".pg-share-status").contains("storage unavailable"));
    assert!(root.query_selector("#pg-share-link").unwrap().is_none());
    assert!(clipboard.values.borrow().is_empty());
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn unmount_discards_pending_share_without_writing_the_clipboard() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(false);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    handle.destroy();
    root.remove();
    sleep(300).await;
    assert_eq!(api.requests.borrow().len(), 1);
    assert!(clipboard.values.borrow().is_empty());
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

#[wasm_bindgen_test]
async fn squeeze_down_relaxes_between_squeezes_and_stop_keeps_glue() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let extent = physics.side();
    double_tap(&canvas, top_midpoint(&physics, 0), extent).await;
    tap_at(&canvas, top_midpoint(&physics, 1), extent).await;
    let glued = physics.glues();
    assert_eq!(glued.len(), 1);

    force_button(&root, "Squeeze down").click();
    // About a cycle and a half: the band opens up and squeezes back down.
    let mut targets = Vec::new();
    for _ in 0..40 {
        sleep(100).await;
        targets.push(physics.params().target_side);
    }
    assert_eq!(physics.params().band_tension, 40.0);
    assert!(
        targets.windows(2).any(|w| w[1] > w[0] + 1e-3),
        "the band relaxes: {targets:?}"
    );
    assert!(
        targets.windows(2).any(|w| w[1] < w[0] - 1e-3),
        "and squeezes again: {targets:?}"
    );

    force_button(&root, "Stop").click();
    sleep(50).await;
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "stopping releases the band"
    );
    assert!(text(&root, ".pg-status").starts_with("Squeeze down stopped"));
    assert_eq!(physics.glues(), glued, "the run leaves glue alone");
    force_button(&root, "Squeeze down");
    handle.destroy();
    root.remove();
}

/// Settling a cramped scene relaxes the box first, so the measured packing
/// is where the squares already are instead of a pop to the solver's result.
#[wasm_bindgen_test]
async fn settling_relaxes_the_box_instead_of_popping() {
    let _gpu = NoWebGpu::install();
    // Two squares overlapping by 0.2 in a box they need 2.0 to fit.
    let sq = |cx| shared::Placement {
        cx,
        cy: 0.5,
        theta: 0.0,
    };
    let cramped = Arrangement {
        n: 2,
        side: 1.8,
        squares: vec![sq(0.5), sq(1.3)],
    };
    let (handle, root, physics) = mount_at(&format!("s={}", share::encode(&cramped))).await;
    let buttons = root.query_selector_all(".pg-submit button").unwrap();
    let measure: HtmlElement = (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default() == "Settle & measure")
        .unwrap();
    measure.click();
    sleep(30).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Relaxing the box"), "{status}");

    // The last live frame before the measurement lands. Refine loads its
    // result and records it in the same message, so a frame sampled with
    // no report yet is always the relaxed scene.
    let mut relaxed = physics.arrangement();
    let mut band_relaxed = false;
    for _ in 0..600 {
        if TEST_REPORT.with(|r| r.borrow().is_some()) {
            break;
        }
        band_relaxed |= physics.params().band_tension > 0.0;
        relaxed = physics.arrangement();
        sleep(20).await;
    }
    let report = TEST_REPORT
        .with(|r| r.borrow().clone())
        .expect("the relaxed scene measured valid");
    assert!(band_relaxed, "the band relaxed before measuring");
    let left = shared::geometry::worst_violation(&relaxed);
    assert!(left <= 2e-3, "relaxed until clear, {left} left");
    let pop = relaxed
        .squares
        .iter()
        .zip(&report.squares)
        .map(|(a, b)| (a.cx - b.cx).hypot(a.cy - b.cy))
        .fold(0.0, f64::max);
    assert!(pop < 0.05, "measuring moved a square by {pop}");
    assert!(
        (report.side - relaxed.side).abs() < 0.05,
        "box {} -> {}",
        relaxed.side,
        report.side
    );
    assert!(report.side >= 2.0 - 1e-9);
    assert_eq!(physics.params().band_tension, 0.0, "band released");
    handle.destroy();
    root.remove();
}

/// A jam the band can't relax is reported rather than measured, since
/// measuring it would pop the squares apart.
#[wasm_bindgen_test]
async fn a_glued_jam_that_cannot_relax_is_reported_not_popped() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    // Both side midpoints of square 1 glued to square 2's: only a full
    // overlap satisfies the glue, and contacts resist it, so an overlap
    // stays however far the box relaxes.
    let glues = vec![
        Glue {
            a: Feature::Midpoint { square: 0, edge: 0 },
            b: Feature::Midpoint { square: 1, edge: 0 },
        },
        Glue {
            a: Feature::Midpoint { square: 0, edge: 2 },
            b: Feature::Midpoint { square: 1, edge: 2 },
        },
    ];
    physics.set_glues(&glues).unwrap();
    for _ in 0..100 {
        if shared::geometry::worst_violation(&physics.arrangement()) > 0.02 {
            break;
        }
        sleep(50).await;
    }
    let jammed = shared::geometry::worst_violation(&physics.arrangement());
    assert!(jammed > 2e-3, "the glue holds an overlap: {jammed}");

    let buttons = root.query_selector_all(".pg-submit button").unwrap();
    let measure: HtmlElement = (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default() == "Settle & measure")
        .unwrap();
    measure.click();
    for _ in 0..120 {
        if text(&root, ".pg-status").starts_with("Couldn't relax") {
            break;
        }
        sleep(100).await;
    }
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Couldn't relax"), "{status}");
    assert!(
        TEST_REPORT.with(|r| r.borrow().is_none()),
        "no measurement loaded over the jam"
    );
    assert_eq!(physics.params().band_tension, 0.0, "band released");
    assert_eq!(physics.glues(), glues, "glue kept");
    handle.destroy();
    root.remove();
}

/// Two squares overlapping by 0.2 in a box they need 2.0 to fit.
fn cramped() -> Arrangement {
    let sq = |cx| shared::Placement {
        cx,
        cy: 0.5,
        theta: 0.0,
    };
    Arrangement {
        n: 2,
        side: 1.8,
        squares: vec![sq(0.5), sq(1.3)],
    }
}

/// Click the button labelled `label` inside `scope`.
fn click_button(root: &Element, scope: &str, label: &str) {
    let buttons = root.query_selector_all(&format!("{scope} button")).unwrap();
    (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| b.text_content().unwrap_or_default().trim() == label)
        .unwrap_or_else(|| panic!("{label} button rendered"))
        .click();
}

#[wasm_bindgen_test]
async fn starting_a_run_takes_the_band_from_a_relax() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at(&format!("s={}", share::encode(&cramped()))).await;
    click_button(&root, ".pg-submit", "Settle & measure");
    sleep(100).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Relaxing the box"), "{status}");

    force_button(&root, "Anneal").click();
    sleep(50).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Annealing"), "{status}");
    for _ in 0..40 {
        sleep(50).await;
        assert_eq!(physics.params().band_tension, 30.0, "the run owns the band");
        assert!(!physics.paused(), "no measurement mid-run");
    }
    assert!(TEST_REPORT.with(|r| r.borrow().is_none()));
    force_button(&root, "Stop").click();
    sleep(50).await;
    assert_eq!(physics.params().band_tension, 0.0);
    handle.destroy();
    root.remove();
}

/// A submission waits on its measurement; cancelling the relax before it
/// drops the submission, so a later settle can't submit another scene.
#[wasm_bindgen_test]
async fn cancelling_a_relax_drops_its_pending_submit() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at(&format!("s={}", share::encode(&cramped()))).await;
    let name: HtmlInputElement = root
        .query_selector("#pg-player")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    name.set_value("relax-test");
    name.dispatch_event(&Event::new("input").unwrap()).unwrap();
    sleep(30).await;
    click_button(&root, ".pg-submit", "Submit packing");
    sleep(100).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Relaxing the box"), "{status}");

    force_button(&root, "Pause").click();
    sleep(50).await;
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "cancel releases the band"
    );

    click_button(&root, ".pg-submit", "Settle & measure");
    for _ in 0..600 {
        if TEST_REPORT.with(|r| r.borrow().is_some()) {
            break;
        }
        sleep(20).await;
    }
    assert!(TEST_REPORT.with(|r| r.borrow().is_some()), "measured");
    // A submission would have come back (saved or failed) by now.
    sleep(1500).await;
    let status = text(&root, ".pg-status");
    assert!(
        status.starts_with("Ready"),
        "nothing was submitted: {status}"
    );
    handle.destroy();
    root.remove();
}

/// Submitting while a relax is already running rides along with it: the
/// relax measures once clear and then submits.
#[wasm_bindgen_test]
async fn submitting_during_a_relax_submits_when_it_measures() {
    let _gpu = NoWebGpu::install();
    let (handle, root, _physics) = mount_at(&format!("s={}", share::encode(&cramped()))).await;
    let name: HtmlInputElement = root
        .query_selector("#pg-player")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    name.set_value("relax-test");
    name.dispatch_event(&Event::new("input").unwrap()).unwrap();
    click_button(&root, ".pg-submit", "Settle & measure");
    sleep(100).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Relaxing the box"), "{status}");

    click_button(&root, ".pg-submit", "Submit packing");
    sleep(50).await;
    let status = text(&root, ".pg-status");
    assert!(
        status.starts_with("Relaxing the box"),
        "the relax continues: {status}"
    );
    for _ in 0..600 {
        if TEST_REPORT.with(|r| r.borrow().is_some()) {
            break;
        }
        sleep(20).await;
    }
    assert!(TEST_REPORT.with(|r| r.borrow().is_some()), "measured");
    // There's no score server under test, so the submission comes back as
    // an error; either way it replaces the "Ready" measurement status.
    sleep(1500).await;
    let status = text(&root, ".pg-status");
    assert!(
        !status.starts_with("Ready"),
        "the submission went out: {status}"
    );
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

fn turn_button(root: &Element, label: &str) -> HtmlElement {
    root.query_selector(&format!(".pg-turn button[aria-label='{label}']"))
        .unwrap()
        .unwrap_or_else(|| panic!("{label} button rendered"))
        .dyn_into()
        .unwrap()
}

#[wasm_bindgen_test]
async fn turn_buttons_turn_the_selected_square() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    // Nothing selected yet: a turn tap asks for a square first.
    turn_button(&root, "Turn left").click();
    sleep(30).await;
    assert!(text(&root, ".pg-status").starts_with("Tap a square first"));

    let canvas: HtmlCanvasElement = root
        .query_selector("canvas")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let b = physics.bodies()[0];
    let at = (b.x as f64, b.y as f64);
    pointer(&canvas, "pointerdown", at, physics.side());
    pointer(&canvas, "pointerup", at, physics.side());
    sleep(30).await;
    force_button(&root, "Pause").click();
    sleep(30).await;
    assert!(physics.paused());
    let start = physics.bodies()[0].theta;
    turn_button(&root, "Turn left").click();
    sleep(30).await;
    assert!(!physics.paused(), "turning wakes the scene");
    for _ in 0..60 {
        if physics.bodies()[0].theta > start + 0.03 {
            break;
        }
        sleep(30).await;
    }
    assert!(
        physics.bodies()[0].theta > start + 0.03,
        "turn left is counterclockwise: {} -> {}",
        start,
        physics.bodies()[0].theta
    );
    handle.destroy();
    root.remove();
}
