//! In-browser tests of the mounted play screen: real DOM events through Yew's
//! handlers into the physics. Run with `cargo test -p frontend --target
//! wasm32-unknown-unknown` under wasm-bindgen-test-runner.

use super::*;
use crate::account::browser_tests::{
    click, empty, find, login_started, not_signed_in, reply, text_of, wait_until, Api, Passkeys,
};
use crate::account::{AccountMenu, AccountProvider};
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement, PointerEventInit};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    /// Sign-in asks from the play screen, in order.
    static ASKS: std::cell::RefCell<Vec<SignInAsk>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// An account context that is signed in as `username`, or signed out, and
/// records sign-in asks for [`answer`].
fn account(username: Option<&str>) -> Account {
    ASKS.with(|a| a.borrow_mut().clear());
    Account {
        username: username.map(Into::into),
        ask: Callback::from(|ask| ASKS.with(|a| a.borrow_mut().push(ask))),
    }
}

/// The reasons of the asks so far.
fn asks() -> Vec<&'static str> {
    ASKS.with(|a| a.borrow().iter().map(|ask| ask.reason).collect())
}

/// Answer the latest ask, as the account would after a sign-in (`true`) or
/// a dismissed dialog.
fn answer(signed_in: bool) {
    let done = ASKS
        .with(|a| a.borrow().last().map(|ask| ask.done.clone()))
        .expect("the play screen asked to sign in");
    done.emit(signed_in);
}

#[derive(Properties, PartialEq)]
struct HostProps {
    account: Account,
    #[prop_or_default]
    shape: shared::Shape,
    #[prop_or_default]
    container: shared::Shape,
}

/// `Game` renders router links, so tests mount it inside a router, with an
/// account context.
#[function_component(Host)]
fn host(props: &HostProps) -> Html {
    html! {
        <BrowserRouter>
            <ContextProvider<Account> context={props.account.clone()}>
                <Game n={2} shape={props.shape} container={props.container} />
            </ContextProvider<Account>>
        </BrowserRouter>
    }
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

/// Mount signed in with `?{query}` as the page query (empty clears it).
/// Settling writes a share code into the URL, so every mount sets the query
/// it expects.
async fn mount_at(query: &str) -> (yew::AppHandle<Host>, Element, Physics) {
    mount_as(query, Some("tester")).await
}

/// Mount as `username`, or signed out.
async fn mount_as(query: &str, username: Option<&str>) -> (yew::AppHandle<Host>, Element, Physics) {
    set_query(query);
    let root = new_root();
    let props = HostProps {
        container: shared::Shape::Square,
        account: account(username),
        shape: shared::Shape::Square,
    };
    let handle = yew::Renderer::<Host>::with_root_and_props(root.clone(), props).render();
    sleep(100).await;
    let physics = TEST_PHYSICS
        .with(|p| p.borrow().clone())
        .expect("Game registered its physics");
    (handle, root, physics)
}

/// Set the page query for the next mount, and clear what the last one
/// reported and drew.
fn set_query(query: &str) {
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
    TEST_EXTENT.with(|e| e.set(0.0));
}

fn new_root() -> Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.create_element("div").unwrap();
    root.set_attribute("style", "width: 600px").unwrap();
    document.body().unwrap().append_child(&root).unwrap();
    root
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
    match glue::anchor(
        physics.shape(),
        physics.container(),
        &physics.bodies(),
        physics.side(),
        feature,
    ) {
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
                && glue::pick(
                    physics.shape(),
                    physics.container(),
                    &bodies,
                    side,
                    *p,
                    0.3,
                    |_| true,
                )
                .is_none()
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

    click_button(&root, ".pg-submit", "Settle");
    wait_for_report(3000).await;
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
            slide(&root, SQUEEZE, "2.2");
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
    let (a, b) = glue::link(
        physics.shape(),
        physics.container(),
        &physics.bodies(),
        physics.side(),
        top,
    )
    .unwrap();
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
    sleep(20_000).await;
    wait_paused(&physics).await;
    assert_eq!(physics.params().band_tension, 0.0, "band released");
    force_button(&root, "Anneal");
    assert!(!text(&root, ".pg-status").starts_with("Annealing"));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn squeeze_slider_animates_pressure_and_wakes_a_paused_scene() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    force_button(&root, "Pause").click();
    sleep(30).await;
    assert!(physics.paused());
    assert_eq!(physics.params().band_tension, 0.0);
    let start = physics.side();
    let right_square = physics.bodies()[1].x;
    let slider = squeeze_slider(&root);
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

/// Optional precision slider inside Advanced.
const SQUEEZE: &str = ".pg-advanced #pg-size";

fn squeeze_slider(root: &Element) -> HtmlInputElement {
    root.query_selector(SQUEEZE)
        .unwrap()
        .expect("precision slider in Advanced")
        .dyn_into()
        .unwrap()
}

fn key_event(kind: &str, key: &str, repeat: bool) -> KeyboardEvent {
    let init = web_sys::KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_key(key);
    init.set_repeat(repeat);
    KeyboardEvent::new_with_keyboard_event_init_dict(kind, &init).unwrap()
}

/// Fire `kind` at `slider` as the browser would: pointer and key events
/// bubble, `blur` doesn't.
fn fire(slider: &HtmlInputElement, kind: &str) {
    let event: Event = match kind {
        "pointerup" | "pointercancel" => {
            let init = PointerEventInit::new();
            init.set_bubbles(true);
            init.set_pointer_id(1);
            init.set_is_primary(true);
            PointerEvent::new_with_event_init_dict(kind, &init)
                .unwrap()
                .into()
        }
        "keyup" => key_event(kind, "ArrowDown", false).into(),
        "blur" => Event::new(kind).unwrap(),
        _ => {
            let init = web_sys::EventInit::new();
            init.set_bubbles(true);
            Event::new_with_event_init_dict(kind, &init).unwrap()
        }
    };
    slider.dispatch_event(&event).unwrap();
}

/// The live side every 5 ms for `ms` milliseconds.
async fn sample_sides(physics: &Physics, ms: u64) -> Vec<f64> {
    let mut sides = Vec::new();
    for _ in 0..ms / 5 {
        sleep(5).await;
        sides.push(physics.side());
    }
    sides
}

/// Largest change in the side between neighbouring samples. A teleport to
/// the band's target is at least its 0.3 reach; a slow frame's worth of
/// band motion stays well under this.
const NO_JUMP: f64 = 0.05;

fn assert_continuous(sides: &[f64], at: &str) {
    let worst = sides
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f64::max);
    assert!(
        worst < NO_JUMP,
        "the side jumped by {worst} {at}: {sides:?}"
    );
}

/// The Squeeze slider's thumb and readout sit on the live side, to the
/// slider's step.
fn assert_shows_side(root: &Element, slider: &HtmlInputElement, physics: &Physics, at: &str) {
    let side = physics.side();
    let thumb: f64 = slider.value().parse().unwrap();
    assert!(
        (thumb - side).abs() <= 0.0005 + 1e-9,
        "thumb {thumb} on side {side} {at}"
    );
    assert_eq!(
        text(root, "label[for=pg-size] output"),
        format!("{side:.3}"),
        "readout {at}"
    );
}

#[wasm_bindgen_test]
async fn corner_controls_keep_the_precision_slider_in_advanced() {
    let _gpu = NoWebGpu::install();
    let (handle, root, _) = mount().await;
    assert_eq!(root.query_selector_all("#pg-size").unwrap().length(), 1);
    squeeze_slider(&root);
    assert!(text(&root, "label[for=pg-size]").starts_with("Squeeze"));
    let ranges = root
        .query_selector_all(".pg-advanced input[type=range]")
        .unwrap();
    let ids: Vec<String> = (0..ranges.length())
        .filter_map(|i| ranges.item(i))
        .filter_map(|r| r.dyn_into::<Element>().ok())
        .map(|r| r.id())
        .collect();
    assert_eq!(
        ids,
        [
            "pg-size",
            "pg-band",
            "pg-edge-attraction",
            "pg-damping",
            "pg-stiffness"
        ],
        "Advanced keeps one precision slider"
    );
    handle.destroy();
    root.remove();
}

/// Held, the Squeeze slider presses the band in on the squares. Let go, the
/// band releases and Settle lets their pressure push the box back out until
/// nothing overlaps, with the slider riding the box's side.
#[wasm_bindgen_test]
async fn squeeze_slider_springs_back_when_let_go() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let mut steps = Vec::new();
    wait_ready(&root, &physics, &mut steps, "before squeezing").await;
    let slider = squeeze_slider(&root);
    let start = physics.side();
    // A drag down the track, one input per move.
    let mut sides = vec![start];
    for value in ["2.3", "2.1", "1.9", "1.7"] {
        slide(&root, SQUEEZE, value);
        sides.extend(sample_sides(&physics, 50).await);
    }
    assert_continuous(&sides, "as the squeeze engages");
    let params = physics.params();
    assert_eq!(params.band_tension, 30.0, "the band engages");
    assert!(params.target_side < start, "target {}", params.target_side);
    // Hold until the band stops gaining on the squares.
    let mut squeezed = start;
    for _ in 0..80 {
        sleep(100).await;
        let side = physics.side();
        let stalled = side > squeezed - 1e-4;
        squeezed = squeezed.min(side);
        if stalled && squeezed < start - 0.05 {
            break;
        }
    }
    assert!(
        squeezed < start - 0.05,
        "pressure shrinks the box: {start} -> {squeezed}"
    );
    assert_eq!(physics.params().band_tension, 30.0, "still held");
    assert_eq!(
        physics.settle_status().phase,
        SettlePhase::Idle,
        "a held squeeze doesn't settle: {}",
        screen_state(&root, &physics)
    );

    let mut sides = vec![physics.side()];
    fire(&slider, "change");
    sides.extend(sample_sides(&physics, 30).await);
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "letting go releases the band"
    );
    assert_eq!(
        physics.settle_status().phase,
        SettlePhase::Running,
        "and settles"
    );
    let released = physics.side();
    let (mut lowest, mut lag) = (released, 0.0_f64);
    for _ in 0..600 {
        if TEST_REPORT.with(|r| r.borrow().is_some()) {
            break;
        }
        let thumb: f64 = slider.value().parse().unwrap();
        let side = physics.side();
        lag = lag.max((thumb - side).abs());
        lowest = lowest.min(side);
        sides.push(side);
        sleep(20).await;
    }
    let report = TEST_REPORT
        .with(|r| r.borrow().clone())
        .unwrap_or_else(|| panic!("no certified report: {}", screen_state(&root, &physics)));
    assert_continuous(&sides, "as the box springs back");
    assert!(
        lowest >= released - 1e-9,
        "the box never closes in after letting go: {released} -> {lowest}"
    );
    assert!(lag < 0.01, "the slider rides the side, {lag} behind");
    assert!(
        report.side >= released - 1e-9 && report.side >= squeezed - 1e-9,
        "settled at {} from {released}, squeezed to {squeezed}",
        report.side
    );
    sleep(100).await;
    assert_shows_side(&root, &slider, &physics, "once certified");
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Ready"), "{status}");

    // The squeeze was released once; later let-go events change nothing.
    for kind in ["pointerup", "keyup", "change", "blur"] {
        fire(&slider, kind);
    }
    sleep(100).await;
    assert!(physics.paused(), "still at the certified packing");
    assert_eq!(physics.settle_status().phase, SettlePhase::Idle);
    assert_eq!(text(&root, ".pg-status"), status);
    handle.destroy();
    root.remove();
}

/// A held arrow key squeezes. Chrome fires `change` with every step a key
/// makes, so only the key's release lets go.
#[wasm_bindgen_test]
async fn squeeze_slider_works_from_the_keyboard() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let mut steps = Vec::new();
    wait_ready(&root, &physics, &mut steps, "before the keys").await;
    let slider = squeeze_slider(&root);
    slider.focus().unwrap();
    let start = physics.side();
    // Synthetic keys don't move a range input, so each repeat does what
    // the browser would: step the value, then fire input and change.
    for i in 0..300 {
        slider
            .dispatch_event(&key_event("keydown", "ArrowDown", i > 0))
            .unwrap();
        let stepped = slider.value().parse::<f64>().unwrap() - 0.001;
        slider.set_value(&format!("{stepped:.3}"));
        fire(&slider, "input");
        fire(&slider, "change");
        if i % 10 == 9 {
            sleep(10).await;
        }
    }
    for _ in 0..60 {
        if physics.side() < start - 0.05 {
            break;
        }
        sleep(50).await;
    }
    assert!(
        physics.side() < start - 0.05,
        "the held key squeezes: {start} -> {}",
        physics.side()
    );
    assert_eq!(
        physics.params().band_tension,
        30.0,
        "a step's change doesn't let go while the key is held"
    );
    assert_eq!(physics.settle_status().phase, SettlePhase::Idle);

    let mut sides = vec![physics.side()];
    fire(&slider, "keyup");
    sides.extend(sample_sides(&physics, 30).await);
    assert_continuous(&sides, "as the key lets go");
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "key up releases the band"
    );
    assert_eq!(physics.settle_status().phase, SettlePhase::Running);
    let released = physics.side();
    wait_for_report(12_000).await;
    let report = TEST_REPORT.with(|r| r.borrow().clone()).unwrap();
    assert!(
        report.side >= released - 1e-9,
        "{released} -> {}",
        report.side
    );
    handle.destroy();
    root.remove();
}

/// Pointer up, pointer cancel and blur let go too, so pressure can't stay
/// latched when a touch or focus leaves the slider.
#[wasm_bindgen_test]
async fn every_way_off_the_slider_lets_go() {
    let _gpu = NoWebGpu::install();
    for kind in ["pointerup", "pointercancel", "blur"] {
        let (handle, root, physics) = mount().await;
        let mut steps = Vec::new();
        wait_ready(&root, &physics, &mut steps, kind).await;
        slide(&root, SQUEEZE, "1.9");
        sleep(200).await;
        assert_eq!(physics.params().band_tension, 30.0, "{kind}: held");
        let mut sides = vec![physics.side()];
        fire(&squeeze_slider(&root), kind);
        sides.extend(sample_sides(&physics, 30).await);
        assert_continuous(&sides, kind);
        assert_eq!(physics.params().band_tension, 0.0, "{kind} releases");
        assert_eq!(
            physics.settle_status().phase,
            SettlePhase::Running,
            "{kind} settles"
        );
        handle.destroy();
        root.remove();
    }
}

/// Letting go of a slider that never moved, or an unrelated key, starts
/// nothing.
#[wasm_bindgen_test]
async fn letting_go_without_moving_does_nothing() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    force_button(&root, "Pause").click();
    sleep(30).await;
    assert!(physics.paused());
    let (status, side) = (text(&root, ".pg-status"), physics.side());
    let slider = squeeze_slider(&root);
    slider
        .dispatch_event(&key_event("keydown", "Shift", false))
        .unwrap();
    for kind in ["keyup", "pointerup", "pointercancel", "change", "blur"] {
        fire(&slider, kind);
    }
    sleep(100).await;
    assert!(physics.paused(), "nothing woke the scene");
    assert_eq!(physics.settle_status().phase, SettlePhase::Idle);
    assert_eq!(physics.params().band_tension, 0.0);
    assert_eq!(physics.side(), side);
    assert_eq!(text(&root, ".pg-status"), status);
    assert_eq!(settle_button(&root).text_content().unwrap(), "Settle");
    handle.destroy();
    root.remove();
}

/// When idle, the slider sits on the live side of whatever scene loads.
#[wasm_bindgen_test]
async fn idle_squeeze_slider_follows_loaded_scenes() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at(&format!("s={}", board::encode(&cramped(), &[]))).await;
    let slider = squeeze_slider(&root);
    assert_eq!(physics.side(), 1.8);
    assert_shows_side(&root, &slider, &physics, "for a shared scene");
    choose_import(&root, &serde_json::to_string(&two_squares()).unwrap());
    for _ in 0..60 {
        if text(&root, ".pg-status").starts_with("Imported") {
            break;
        }
        sleep(30).await;
    }
    sleep(100).await;
    assert_eq!(physics.side(), 2.5);
    assert_shows_side(&root, &slider, &physics, "for an imported scene");
    handle.destroy();
    root.remove();
}

/// A loaded scene can sit outside the slider's usual bounds: far above them,
/// or overlapped below sqrt(n). The idle slider widens to take in the live
/// side, and its bounds hold still while a squeeze is held.
#[wasm_bindgen_test]
async fn squeeze_bounds_take_in_out_of_range_scenes() {
    let _gpu = NoWebGpu::install();
    let sq = |cx, cy| shared::Placement { cx, cy, theta: 0.0 };
    let roomy = Arrangement {
        container: shared::Shape::Square,
        shape: shared::Shape::Square,
        n: 2,
        side: 10.0,
        squares: vec![sq(4.5, 5.0), sq(5.5, 5.0)],
    };
    let (handle, root, physics) = mount_at(&format!("s={}", board::encode(&roomy, &[]))).await;
    let slider = squeeze_slider(&root);
    assert_eq!(physics.side(), 10.0);
    assert_shows_side(&root, &slider, &physics, "for a side-10 share");
    assert_eq!(slider.max(), "10");
    let bounds = (slider.min(), slider.max());
    slide(&root, SQUEEZE, "9.5");
    for _ in 0..60 {
        if physics.side() < 9.9 {
            break;
        }
        sleep(50).await;
    }
    assert!(
        physics.side() < 9.9,
        "the squeeze moves the box: {}",
        physics.side()
    );
    assert_eq!(
        (slider.min(), slider.max()),
        bounds,
        "the bounds hold still while held"
    );
    fire(&slider, "change");
    handle.destroy();
    root.remove();

    let overlapped = Arrangement {
        container: shared::Shape::Square,
        shape: shared::Shape::Square,
        n: 2,
        side: 1.3,
        squares: vec![sq(0.5, 0.5), sq(0.8, 0.8)],
    };
    let (handle, root, physics) = mount_at(&format!("s={}", board::encode(&overlapped, &[]))).await;
    let slider = squeeze_slider(&root);
    assert_eq!(physics.side(), 1.3);
    assert_shows_side(&root, &slider, &physics, "below sqrt(n)");
    assert_eq!(slider.min(), "1.3");
    handle.destroy();
    root.remove();
}

/// Only keys that step the slider take part in a squeeze: pressing and
/// releasing Shift mid-drag leaves the squeeze held, and letting go of the
/// pointer still settles it.
#[wasm_bindgen_test]
async fn unrelated_keys_leave_a_held_squeeze_alone() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let mut steps = Vec::new();
    wait_ready(&root, &physics, &mut steps, "before squeezing").await;
    let slider = squeeze_slider(&root);
    slide(&root, SQUEEZE, "1.9");
    sleep(100).await;
    assert_eq!(physics.params().band_tension, 30.0, "held");
    slider
        .dispatch_event(&key_event("keydown", "Shift", false))
        .unwrap();
    slider
        .dispatch_event(&key_event("keyup", "Shift", false))
        .unwrap();
    sleep(200).await;
    assert_eq!(physics.params().band_tension, 30.0, "Shift doesn't let go");
    assert_eq!(physics.settle_status().phase, SettlePhase::Idle);
    assert!(!text(&root, ".pg-status").starts_with("Settling"));
    fire(&slider, "pointerup");
    sleep(30).await;
    assert_eq!(physics.params().band_tension, 0.0, "the pointer lets go");
    assert_eq!(physics.settle_status().phase, SettlePhase::Running);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn squeeze_reach_follows_band_pressure() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    // Two squares side by side hold the container near 2.0, so the smallest
    // request the slider allows (sqrt(2)) can't be reached at tension 30; the
    // target may only lead the side by the 0.3 reach.
    let desired = slide(&root, SQUEEZE, "1.2");
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
    // Select the square, then pause; keyboard turns must wake it again. A
    // calm grid settles and measures itself, and input is ignored while it
    // measures, so each step waits until the controls take input.
    let mut steps = Vec::new();
    wait_ready(&root, &physics, &mut steps, "before selecting").await;
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
    pause_when_ready(&root, &physics, &mut steps, "after selecting").await;
    let start = physics.bodies()[0].theta;
    let key = web_sys::KeyboardEventInit::new();
    key.set_bubbles(true);
    key.set_key("e");
    canvas
        .dispatch_event(&KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &key).unwrap())
        .unwrap();
    // Yew handles the key on a later tick, which a busy machine delays.
    for _ in 0..50 {
        if !physics.paused() {
            break;
        }
        sleep(10).await;
    }
    if physics.paused() {
        // The turn button reports when nothing is selected.
        let state = screen_state(&root, &physics);
        turn_button(&root, "Turn right").click();
        sleep(50).await;
        panic!(
            "a key turn wakes the scene: {state}; a turn button then says {:?}; steps {steps:?}",
            text(&root, ".pg-status")
        );
    }
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
    pause_when_ready(&root, &physics, &mut steps, "before the wheel").await;
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
    for _ in 0..50 {
        if !physics.paused() {
            break;
        }
        sleep(10).await;
    }
    assert!(
        !physics.paused(),
        "a wheel turn wakes the scene: {}; steps {steps:?}",
        screen_state(&root, &physics)
    );
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
        container: shared::Shape::Square,
        shape: shared::Shape::Square,
        n: 2,
        side: 2.5,
        squares: vec![sq(0.75), sq(1.875)],
    }
}

#[wasm_bindgen_test]
async fn share_link_loads_paused_and_unvalidated() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) =
        mount_at(&format!("s={}", board::encode(&two_squares(), &[]))).await;
    assert!(physics.paused(), "shared packings open paused");
    assert_eq!(physics.arrangement(), two_squares());
    assert!(physics.glues().is_empty());
    assert!(text(&root, ".pg-status").starts_with("Board loaded"));
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
    assert!(text(&root, ".pg-status").starts_with("Board link:"));
    assert_eq!(physics.side(), 2.5, "nothing was loaded");
    handle.destroy();
    root.remove();
}

/// One scripted `/api/boards` response; status 0 never answers.
#[derive(Clone, Copy)]
struct Reply {
    status: u16,
    retry_after: Option<&'static str>,
    /// Send the headers but a body that never finishes arriving.
    stall_body: bool,
}
const HANG: Reply = Reply {
    status: 0,
    retry_after: None,
    stall_body: false,
};
const OK: Reply = Reply {
    status: 200,
    retry_after: None,
    stall_body: false,
};
const UNAVAILABLE: Reply = Reply {
    status: 503,
    retry_after: None,
    stall_body: false,
};

/// Stub only /api/boards; other mounted-screen requests still use fetch.
/// Replies follow the script, repeating its last entry.
struct ShareApi {
    original: wasm_bindgen::JsValue,
    _fetch: wasm_bindgen::closure::Closure<
        dyn FnMut(wasm_bindgen::JsValue, wasm_bindgen::JsValue) -> js_sys::Promise,
    >,
    requests: Rc<std::cell::RefCell<Vec<shared::BoardCode>>>,
    /// When each request arrived, in milliseconds.
    arrivals: Rc<std::cell::RefCell<Vec<f64>>>,
    /// Each request's abort signal.
    signals: Rc<std::cell::RefCell<Vec<web_sys::AbortSignal>>>,
}
impl ShareApi {
    fn install(script: &[Reply]) -> Self {
        let script = script.to_vec();
        let window = web_sys::window().unwrap();
        let original = js_sys::Reflect::get(&window, &"fetch".into()).unwrap();
        let fetch = original.clone().dyn_into::<js_sys::Function>().unwrap();
        let requests = Rc::new(std::cell::RefCell::new(Vec::new()));
        let arrivals = Rc::new(std::cell::RefCell::new(Vec::new()));
        let signals = Rc::new(std::cell::RefCell::new(Vec::new()));
        let (captured, arrived, signalled) = (requests.clone(), arrivals.clone(), signals.clone());
        let replacement = wasm_bindgen::closure::Closure::wrap(Box::new(
            move |request: wasm_bindgen::JsValue, init: wasm_bindgen::JsValue| {
                let req = request.clone().dyn_into::<web_sys::Request>().unwrap();
                if !req.url().ends_with("/api/boards") {
                    return fetch
                        .call2(&web_sys::window().unwrap(), &request, &init)
                        .unwrap()
                        .unchecked_into();
                }
                let (captured, arrived) = (captured.clone(), arrived.clone());
                signalled.borrow_mut().push(req.signal());
                let script = script.clone();
                wasm_bindgen_futures::future_to_promise(async move {
                    arrived.borrow_mut().push(js_sys::Date::now());
                    let body = wasm_bindgen_futures::JsFuture::from(req.text().unwrap())
                        .await?
                        .as_string()
                        .unwrap();
                    let reply = {
                        let mut captured = captured.borrow_mut();
                        captured.push(serde_json::from_str(&body).unwrap());
                        script[(captured.len() - 1).min(script.len() - 1)]
                    };
                    if reply.status == 0 {
                        // Never answer; only an abort ends this request.
                        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |_, _| {}))
                            .await?;
                    }
                    sleep(150).await;
                    let options = web_sys::ResponseInit::new();
                    options.set_status(reply.status);
                    let text = if reply.status == 200 {
                        r#"{"url":"https://packit.test/s/0123456789abcdef01234567"}"#
                    } else {
                        r#"{"error":"storage unavailable"}"#
                    };
                    let response: web_sys::Response = if reply.stall_body {
                        // Headers now, then a body stream that never closes.
                        let window = web_sys::window().unwrap();
                        let class =
                            |name: &str| -> Result<js_sys::Function, wasm_bindgen::JsValue> {
                                js_sys::Reflect::get(&window, &name.into())?.dyn_into()
                            };
                        let open = js_sys::Array::of1(&js_sys::Object::new());
                        let stream = js_sys::Reflect::construct(&class("ReadableStream")?, &open)?;
                        let args = js_sys::Array::of2(&stream, options.as_ref());
                        js_sys::Reflect::construct(&class("Response")?, &args)?.unchecked_into()
                    } else {
                        web_sys::Response::new_with_opt_str_and_init(Some(text), &options)?
                    };
                    if let Some(after) = reply.retry_after {
                        response.headers().set("retry-after", after)?;
                    }
                    Ok(response.into())
                })
            },
        )
            as Box<dyn FnMut(wasm_bindgen::JsValue, wasm_bindgen::JsValue) -> js_sys::Promise>);
        js_sys::Reflect::set(&window, &"fetch".into(), replacement.as_ref()).unwrap();
        Self {
            original,
            _fetch: replacement,
            requests,
            arrivals,
            signals,
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

/// Wait for a Share to finish, retries included.
async fn wait_share(root: &Element) {
    for _ in 0..400 {
        if !text(root, ".pg-share-status").contains("Creating") {
            return;
        }
        sleep(20).await;
    }
    panic!("share request did not complete");
}

fn share_alert(root: &Element) -> Option<String> {
    root.query_selector(".pg-share-error[role='alert']")
        .unwrap()
        .map(|e| e.text_content().unwrap_or_default())
}

#[wasm_bindgen_test]
async fn share_button_copies_a_short_link_for_the_captured_precise_snapshot() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Settle").click();
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
    assert_eq!(
        board::decode(&body.code, body.n).unwrap().arrangement,
        report
    );
    assert_eq!(
        clipboard.values.borrow().as_slice(),
        ["https://packit.test/s/0123456789abcdef01234567"]
    );
    assert!(text(&root, ".pg-share-status").contains("Board link copied"));
    assert_eq!(
        web_sys::window().unwrap().location().href().unwrap(),
        url,
        "an in-flight share never overwrites the live URL"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn sharing_during_a_settle_keeps_it_and_its_snapshot() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, physics) = mount_at(&format!("s={}", board::encode(&cramped(), &[]))).await;
    submit_button(&root, "Settle").click();
    sleep(30).await;
    assert_eq!(physics.settle_status().phase, SettlePhase::Running);
    let snapshot = physics.arrangement();
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    let body = api.requests.borrow()[0].clone();
    assert_eq!(
        board::decode(&body.code, body.n).unwrap().arrangement,
        snapshot
    );
    assert_eq!(clipboard.values.borrow().len(), 1);
    assert_eq!(
        physics.settle_status().phase,
        SettlePhase::Running,
        "sharing leaves the settle in control"
    );
    assert!(!physics.paused());
    assert!(text(&root, ".pg-status").starts_with("Settling"));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn share_copy_failure_offers_selectable_url_and_fresh_gesture_retry() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[OK]);
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
    assert!(text(&root, ".pg-share-status").contains("Board link copied"));
    assert_eq!(
        api.requests.borrow().len(),
        1,
        "copy retry does not create another snapshot"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn a_transient_failure_then_success_copies_the_short_link() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[UNAVAILABLE, OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    let requests = api.requests.borrow().clone();
    assert_eq!(requests.len(), 2, "one retry");
    assert_eq!(
        requests[0], requests[1],
        "the retry sends the same snapshot"
    );
    assert_eq!(
        clipboard.values.borrow().as_slice(),
        ["https://packit.test/s/0123456789abcdef01234567"]
    );
    assert_eq!(share_alert(&root), None);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn a_hung_request_is_aborted_and_retried() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[HANG, OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    assert_eq!(api.requests.borrow().len(), 2);
    let signals = api.signals.borrow();
    assert!(signals[0].aborted(), "the hung request was aborted");
    assert!(!signals[1].aborted(), "the retry completed");
    assert_eq!(
        clipboard.values.borrow().as_slice(),
        ["https://packit.test/s/0123456789abcdef01234567"]
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn retry_after_delays_the_next_attempt() {
    let _gpu = NoWebGpu::install();
    let limited = Reply {
        status: 429,
        retry_after: Some("1"),
        stall_body: false,
    };
    let api = ShareApi::install(&[limited, OK]);
    let _clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    let arrivals = api.arrivals.borrow().clone();
    assert_eq!(arrivals.len(), 2);
    // The 429 arrives 150 ms after the first request; the retry waits 1 s.
    assert!(
        arrivals[1] - arrivals[0] >= 1150.0,
        "retried after {} ms",
        arrivals[1] - arrivals[0]
    );
    assert!(text(&root, ".pg-share-status").contains("Board link copied"));
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn a_rejected_share_is_not_retried_and_alerts() {
    let _gpu = NoWebGpu::install();
    let rejected = Reply {
        status: 400,
        retry_after: None,
        stall_body: false,
    };
    let api = ShareApi::install(&[rejected]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    assert_eq!(api.requests.borrow().len(), 1, "a 400 is not retried");
    let alert = share_alert(&root).expect("the failure is announced");
    assert!(alert.starts_with("Couldn't create a short link"), "{alert}");
    assert!(clipboard.values.borrow().is_empty());
    handle.destroy();
    root.remove();
}

/// A 429's wait comes from its headers, even when its body never arrives.
#[wasm_bindgen_test]
async fn a_stalled_429_body_still_honors_retry_after() {
    let _gpu = NoWebGpu::install();
    let limited = Reply {
        status: 429,
        retry_after: Some("2"),
        stall_body: true,
    };
    let api = ShareApi::install(&[limited, OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    let arrivals = api.arrivals.borrow().clone();
    assert_eq!(arrivals.len(), 2);
    // Reading the stalled body would time out and retry after about 1.15 s.
    assert!(
        arrivals[1] - arrivals[0] >= 2150.0,
        "retried after {} ms",
        arrivals[1] - arrivals[0]
    );
    assert_eq!(clipboard.values.borrow().len(), 1);
    handle.destroy();
    root.remove();
}

/// A 400 fails on its status alone; a stalled body doesn't turn it into a
/// retried timeout.
#[wasm_bindgen_test]
async fn a_stalled_400_body_fails_at_once_without_retrying() {
    let _gpu = NoWebGpu::install();
    let rejected = Reply {
        status: 400,
        retry_after: None,
        stall_body: true,
    };
    let api = ShareApi::install(&[rejected]);
    let _clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    wait_share(&root).await;
    assert_eq!(api.requests.borrow().len(), 1, "not retried");
    assert!(share_alert(&root).is_some(), "the failure is announced");
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn share_gives_up_after_bounded_retries_and_alerts_once() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[UNAVAILABLE]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    sleep(300).await;
    assert_eq!(share_alert(&root), None, "no alert while retrying");
    wait_share(&root).await;
    assert_eq!(api.requests.borrow().len(), 4, "bounded attempts");
    let alert = share_alert(&root).expect("the final failure is announced");
    assert!(alert.contains("press Share to try again"), "{alert}");
    assert!(
        !alert.contains("storage unavailable"),
        "no server internals"
    );
    assert!(root.query_selector("#pg-share-link").unwrap().is_none());
    assert!(clipboard.values.borrow().is_empty());
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn unmounting_during_a_backoff_sends_no_more_requests() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[UNAVAILABLE, OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    // The first request fails after 150 ms; the retry waits 400 ms more.
    sleep(300).await;
    assert_eq!(api.requests.borrow().len(), 1);
    handle.destroy();
    root.remove();
    sleep(1500).await;
    assert_eq!(api.requests.borrow().len(), 1, "no retry after unmounting");
    assert!(clipboard.values.borrow().is_empty());
}

#[wasm_bindgen_test]
async fn unmount_discards_pending_share_without_writing_the_clipboard() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[OK]);
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

/// Every feature kind, walls included, between the two squares.
fn two_square_glues() -> Vec<Glue> {
    vec![
        Glue {
            a: Feature::Edge { square: 0, edge: 0 },
            b: Feature::Edge { square: 1, edge: 2 },
        },
        Glue {
            a: Feature::Corner {
                square: 0,
                corner: 3,
            },
            b: Feature::Wall(1),
        },
        Glue {
            a: Feature::Wall(3),
            b: Feature::Midpoint { square: 1, edge: 1 },
        },
    ]
}

#[wasm_bindgen_test]
async fn share_captures_the_glue_and_reopening_the_link_restores_it() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[UNAVAILABLE, OK]);
    let _clipboard = Clipboard::install(false);
    let (handle, root, physics) = mount().await;
    physics.set_glues(&two_square_glues()).unwrap();
    submit_button(&root, "Share").click();
    sleep(30).await;
    // Glue edited while the request is in flight stays out of the link,
    // including the retry.
    physics.set_glues(&[]).unwrap();
    wait_share(&root).await;
    let requests = api.requests.borrow().clone();
    assert_eq!(requests.len(), 2, "one retry");
    assert_eq!(
        requests[0], requests[1],
        "the retry sends the same snapshot"
    );
    let body = requests[1].clone();
    assert_eq!(
        board::decode(&body.code, 2).unwrap().glues,
        two_square_glues()
    );
    handle.destroy();
    root.remove();

    let (handle, root, physics) = mount_at(&format!("s={}", body.code)).await;
    assert_eq!(physics.glues(), two_square_glues());
    assert!(physics.paused(), "shared packings open paused");
    assert!(text(&root, ".pg-status").starts_with("Board loaded"));
    assert!(TEST_REPORT.with(|r| r.borrow().is_none()), "not validated");
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
    sleep(12_000).await;
    wait_paused(&physics).await;
    assert_eq!(physics.params().band_tension, 0.0, "band released");
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

/// Settling a cramped scene opens the box until the squares clear, so the
/// measured packing is where the squares already are instead of a pop to
/// the solver's result.
#[wasm_bindgen_test]
async fn settling_opens_the_box_instead_of_popping() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at(&format!("s={}", board::encode(&cramped(), &[]))).await;
    click_button(&root, ".pg-submit", "Settle");
    sleep(30).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Settling"), "{status}");
    assert_eq!(physics.params().band_tension, 0.0, "no band while settling");

    // The last live frame before the measurement lands. Refine loads its
    // result and records it in the same message, so a frame sampled with
    // no report yet is always the settled scene.
    let mut relaxed = physics.arrangement();
    for _ in 0..600 {
        if TEST_REPORT.with(|r| r.borrow().is_some()) {
            break;
        }
        relaxed = physics.arrangement();
        sleep(20).await;
    }
    let report = TEST_REPORT.with(|r| r.borrow().clone()).unwrap_or_else(|| {
        panic!(
            "no valid measurement: status {:?}, settle {:?}",
            text(&root, ".pg-status"),
            physics.settle_status()
        )
    });
    assert!(physics.frame_shift() > 0.0, "the box opened while settling");
    let left = shared::geometry::worst_violation(&relaxed);
    assert!(left <= 2e-3, "settled until clear, {left} left");
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

/// A jam that can't settle is reported rather than measured, since
/// measuring it would pop the squares apart.
#[wasm_bindgen_test]
async fn a_glued_jam_that_cannot_settle_is_reported_not_popped() {
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

    click_button(&root, ".pg-submit", "Settle");
    // The settle gives up after SETTLE_LIMIT seconds of simulated time.
    let mut seen = Vec::new();
    for _ in 0..300 {
        let now = (text(&root, ".pg-status"), physics.settle_status().phase);
        if seen.last() != Some(&now) {
            seen.push(now.clone());
        }
        if now.0.starts_with("Couldn't settle") {
            break;
        }
        sleep(100).await;
    }
    let status = text(&root, ".pg-status");
    assert!(
        status.starts_with("Couldn't settle"),
        "{status}: {:?}; transitions {seen:?}",
        physics.settle_status()
    );
    assert_eq!(physics.settle_status().phase, SettlePhase::Blocked);
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
        container: shared::Shape::Square,
        shape: shared::Shape::Square,
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
async fn starting_a_run_cancels_a_settle() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at(&format!("s={}", board::encode(&cramped(), &[]))).await;
    click_button(&root, ".pg-submit", "Settle");
    sleep(100).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Settling"), "{status}");

    force_button(&root, "Anneal").click();
    sleep(50).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Annealing"), "{status}");
    assert_eq!(physics.settle_status().phase, SettlePhase::Idle);
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

/// A submission waits on its measurement; cancelling the settle before it
/// drops the submission, so a later settle can't submit another scene.
#[wasm_bindgen_test]
async fn cancelling_a_settle_drops_its_pending_submit() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount_at(&format!("s={}", board::encode(&cramped(), &[]))).await;
    click_button(&root, ".pg-submit", "Submit packing");
    sleep(100).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Settling"), "{status}");

    force_button(&root, "Pause").click();
    sleep(50).await;
    assert_eq!(
        physics.settle_status().phase,
        SettlePhase::Idle,
        "pausing cancels the settle"
    );

    click_button(&root, ".pg-submit", "Settle");
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

/// Submitting while a settle is already running rides along with it: the
/// settle measures once clear and then submits.
#[wasm_bindgen_test]
async fn submitting_during_a_settle_submits_when_it_measures() {
    let _gpu = NoWebGpu::install();
    let (handle, root, _physics) = mount_at(&format!("s={}", board::encode(&cramped(), &[]))).await;
    click_button(&root, ".pg-submit", "Settle");
    sleep(100).await;
    let status = text(&root, ".pg-status");
    assert!(status.starts_with("Settling"), "{status}");

    click_button(&root, ".pg-submit", "Submit packing");
    sleep(50).await;
    let status = text(&root, ".pg-status");
    assert!(
        status.starts_with("Settling"),
        "the settle continues: {status}"
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

/// Wait for a validated measurement, for up to `ms` milliseconds.
/// Settle scores a loose packing where it stands; when the solver finds a
/// smaller box, Tighten jumps to it only on request, and certifies it.
#[wasm_bindgen_test]
async fn tighten_loads_the_solvers_smaller_box_on_request() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    click_button(&root, ".pg-submit", "Settle");
    wait_for_report(8000).await;
    let loose = TEST_REPORT.with(|r| r.take()).unwrap();
    assert!(
        (loose.side - physics.side()).abs() < 1e-6,
        "scored where it settled"
    );
    let status = text(&root, ".pg-status");
    assert!(status.contains("Press Tighten"), "{status}");
    tighten_button(&root).click();
    wait_for_report(3000).await;
    let tight = TEST_REPORT.with(|r| r.borrow().clone()).unwrap();
    assert!(
        tight.side < loose.side - 1e-3,
        "{} -> {}",
        loose.side,
        tight.side
    );
    assert!(text(&root, ".pg-status").starts_with("Ready"));
    assert!(physics.paused());
    handle.destroy();
    root.remove();
}

fn settle_button(root: &Element) -> HtmlElement {
    root.query_selector(".pg-submit .pg-actions .pg-primary")
        .unwrap()
        .expect("Settle button rendered")
        .dyn_into()
        .unwrap()
}

/// What the screen is doing, for timing-sensitive tests to report.
fn screen_state(root: &Element, physics: &Physics) -> String {
    let settle = settle_button(root);
    format!(
        "status {:?}, Settle button {:?} (disabled {}), paused {}, settle {:?}, measured {}, drawn extent {} for side {}",
        text(root, ".pg-status"),
        settle.text_content().unwrap_or_default(),
        settle.has_attribute("disabled"),
        physics.paused(),
        physics.settle_status().phase,
        TEST_REPORT.with(|r| r.borrow().is_some()),
        TEST_EXTENT.with(Cell::get),
        physics.side()
    )
}

/// Wait until the screen takes input: a frame is drawn, the canvas has
/// stopped moving (a pointer's world position is read from its rect when
/// the event is handled), and it's not settling or measuring, which the
/// Settle button shows. Records the moment and state in `steps`.
async fn wait_ready(root: &Element, physics: &Physics, steps: &mut Vec<String>, at: &str) {
    let now = || web_sys::window().unwrap().performance().unwrap().now();
    let rect = || {
        let r = canvas_of(root).get_bounding_client_rect();
        (r.left(), r.top(), r.width(), r.height())
    };
    let mut last = rect();
    for _ in 0..250 {
        sleep(20).await;
        let (settle, placed) = (settle_button(root), rect());
        let still = std::mem::replace(&mut last, placed) == placed;
        if still
            && TEST_EXTENT.with(Cell::get) > 0.0
            && !settle.has_attribute("disabled")
            && settle.text_content().unwrap_or_default() == "Settle"
        {
            steps.push(format!(
                "{:.0} ms {at}: {}",
                now(),
                screen_state(root, physics)
            ));
            return;
        }
    }
    panic!(
        "controls never ready {at}: {}; steps {steps:?}",
        screen_state(root, physics)
    );
}

/// Once the screen takes input, pause the scene unless a measurement
/// already has.
async fn pause_when_ready(root: &Element, physics: &Physics, steps: &mut Vec<String>, at: &str) {
    wait_ready(root, physics, steps, at).await;
    if !physics.paused() {
        force_button(root, "Pause").click();
    }
    for _ in 0..50 {
        if physics.paused() {
            break;
        }
        sleep(10).await;
    }
    assert!(
        physics.paused(),
        "paused {at}: {}; steps {steps:?}",
        screen_state(root, physics)
    );
}

/// A settle the user didn't start, like the automatic one on a calm scene,
/// keeps the selected square, so a key still turns it.
#[wasm_bindgen_test]
async fn an_automatic_settle_keeps_the_selection() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let b = physics.bodies()[0];
    tap_at(&canvas, (b.x as f64, b.y as f64), physics.side()).await;
    // The fresh grid is calm, so it settles and measures itself.
    wait_for_report(8000).await;
    assert!(physics.paused(), "measuring pauses the scene");
    let start = physics.bodies()[0].theta;
    let key = web_sys::KeyboardEventInit::new();
    key.set_bubbles(true);
    key.set_key("e");
    canvas
        .dispatch_event(&KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &key).unwrap())
        .unwrap();
    for _ in 0..50 {
        if !physics.paused() {
            break;
        }
        sleep(10).await;
    }
    assert!(
        !physics.paused(),
        "the key still acts on the selected square"
    );
    for _ in 0..60 {
        if physics.bodies()[0].theta > start + 0.015 {
            break;
        }
        sleep(30).await;
    }
    assert!(physics.bodies()[0].theta > start + 0.015, "and turns it");
    handle.destroy();
    root.remove();
}

fn tighten_button(root: &Element) -> HtmlElement {
    let buttons = root.query_selector_all(".pg-submit button").unwrap();
    (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<HtmlElement>().ok())
        .find(|b| {
            b.text_content()
                .unwrap_or_default()
                .starts_with("Tighten to")
        })
        .expect("Tighten offered")
}

/// A Tighten that would break glue is refused before the live scene is
/// touched: its pose, velocities, parameters, glue and pause stay as they
/// were.
#[wasm_bindgen_test]
async fn a_tighten_that_breaks_glue_leaves_the_scene_untouched() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    click_button(&root, ".pg-submit", "Settle");
    wait_for_report(8000).await;
    // Square 1's top edge to the top wall: no tighter packing of this grid
    // satisfies it.
    physics
        .set_glues(&[Glue {
            a: Feature::Edge { square: 0, edge: 1 },
            b: Feature::Wall(3),
        }])
        .unwrap();
    let state = |p: &Physics| {
        (
            p.arrangement(),
            p.bodies(),
            format!("{:?}", p.params()),
            p.glues(),
            p.paused(),
        )
    };
    let before = state(&physics);
    tighten_button(&root).click();
    sleep(100).await;
    let status = text(&root, ".pg-status");
    assert!(
        status.starts_with("Tightening would break a glue link"),
        "{status}"
    );
    assert_eq!(state(&physics), before, "a refused Tighten changes nothing");
    handle.destroy();
    root.remove();
}

/// Pushing a square hard into a wall grows the box under a pointer held
/// still. The view's scale holds for the whole drag, so the growth can't
/// chase the pointer, and the view refits to the grown box after release.
#[wasm_bindgen_test]
async fn drag_growth_holds_the_view_until_release() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let canvas = canvas_of(&root);
    let extent = TEST_EXTENT.with(Cell::get);
    let start = physics.side();
    assert!(
        (extent - start).abs() < 1e-9,
        "the view starts fitted: {extent} vs {start}"
    );
    let b = physics.bodies()[0];
    pointer(&canvas, "pointerdown", (b.x as f64, b.y as f64), extent);
    // Every move is to the same client point, well past the left wall.
    let held = (-0.8, b.y as f64);
    let mut drawn = Vec::new();
    for _ in 0..100 {
        pointer(&canvas, "pointermove", held, extent);
        sleep(50).await;
        drawn.push(TEST_EXTENT.with(Cell::get));
        if physics.side() > start + 0.1 {
            break;
        }
    }
    assert!(
        physics.side() > start + 0.05,
        "pushing into the wall grew the box: {start} -> {}",
        physics.side()
    );
    assert!(
        drawn.iter().all(|e| *e == extent),
        "the view held its scale: {drawn:?}"
    );
    pointer(&canvas, "pointerup", held, extent);
    for _ in 0..100 {
        if TEST_EXTENT.with(Cell::get) >= physics.side() {
            break;
        }
        sleep(20).await;
    }
    assert!(
        TEST_EXTENT.with(Cell::get) >= physics.side(),
        "the view refits after release"
    );
    handle.destroy();
    root.remove();
}

/// Wait for a finished run's settle and measurement, which pause the scene.
/// Settling runs on simulated time, so a busy machine can take a while.
async fn wait_paused(physics: &Physics) {
    for _ in 0..300 {
        if physics.paused() {
            return;
        }
        sleep(50).await;
    }
    panic!("measuring pauses the scene");
}

async fn wait_for_report(ms: u64) {
    for _ in 0..ms / 20 {
        if TEST_REPORT.with(|r| r.borrow().is_some()) {
            return;
        }
        sleep(20).await;
    }
    panic!("no validated measurement after {ms} ms");
}

/// Settling and sharing never touch the address bar, whether the page was
/// opened fresh or from a solution link: Share copies the link instead.
#[wasm_bindgen_test]
async fn settling_and_sharing_leave_the_address_bar_alone() {
    let _gpu = NoWebGpu::install();
    let _api = ShareApi::install(&[OK]);
    let _clipboard = Clipboard::install(false);
    let solution = format!("s={}", board::encode(&two_squares(), &[]));
    for query in ["", solution.as_str()] {
        let (handle, root, _) = mount_at(query).await;
        let href = web_sys::window().unwrap().location().href().unwrap();
        let entries = history_length();
        if query.is_empty() {
            // A fresh grid is already calm, so it measures itself after the
            // settle window.
            wait_for_report(8000).await;
        }
        // The manual measure must produce its own report.
        TEST_REPORT.with(|r| r.take());
        submit_button(&root, "Settle").click();
        sleep(300).await;
        wait_for_report(3000).await;
        submit_button(&root, "Share").click();
        sleep(30).await;
        wait_share(&root).await;
        let status = text(&root, ".pg-share-status");
        assert!(status.contains("Board link copied"), "{query}: {status}");
        assert_eq!(
            web_sys::window().unwrap().location().href().unwrap(),
            href,
            "the address bar is unchanged ({query})"
        );
        assert_eq!(history_length(), entries, "no history entries ({query})");
        handle.destroy();
        root.remove();
    }
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

fn sign_in_note(root: &Element) -> Option<String> {
    root.query_selector(".pg-sign-in[role='alert']")
        .unwrap()
        .map(|e| e.text_content().unwrap_or_default())
}

/// A saved score, as `POST /api/scores` returns it.
fn saved() -> crate::account::browser_tests::Reply {
    reply(
        200,
        serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "player": "tester",
            "n": 2,
            "side": 2.0,
            "submitted_at": "2026-09-13T00:00:00",
            "rank": 1,
            "account": true,
            "board": "0123456789abcdef01234567",
            "glue_recorded": true,
        }),
    )
}

/// Glue the top midpoints of the two squares with the glue tool, and
/// return that glue.
async fn glue_tops(root: &Element, physics: &Physics) -> Vec<Glue> {
    let canvas = canvas_of(root);
    let extent = physics.side();
    double_tap(&canvas, top_midpoint(physics, 0), extent).await;
    tap_at(&canvas, top_midpoint(physics, 1), extent).await;
    let glues = physics.glues();
    assert_eq!(glues.len(), 1);
    glues
}

/// The body Submit sends for `arrangement` and `glues`.
fn submitted(arrangement: &Arrangement, glues: &[Glue]) -> SubmitScore {
    SubmitScore {
        board: BoardCode {
            n: arrangement.n,
            code: board::encode(arrangement, glues),
        },
    }
}

/// The shared two-square packing with every kind of glue.
fn glued_code() -> String {
    board::encode(&two_squares(), &two_square_glues())
}

#[wasm_bindgen_test]
async fn signed_out_share_and_submit_ask_to_sign_in_and_send_nothing() {
    let _gpu = NoWebGpu::install();
    let shares = ShareApi::install(&[OK]);
    let scores = Api::install(&[("/api/scores", vec![saved()])]);
    let (handle, root, _) = mount_as("", None).await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    assert_eq!(sign_in_note(&root).as_deref(), Some(SHARE_NOTE));
    submit_button(&root, "Submit packing").click();
    sleep(30).await;
    assert_eq!(
        sign_in_note(&root).as_deref(),
        Some(SUBMIT_UNCERTIFIED_NOTE)
    );
    submit_button(&root, "Settle").click();
    wait_for_report(8000).await;
    submit_button(&root, "Submit packing").click();
    sleep(30).await;
    assert_eq!(sign_in_note(&root).as_deref(), Some(SUBMIT_NOTE));
    assert_eq!(asks(), [SHARE_NOTE, SUBMIT_UNCERTIFIED_NOTE, SUBMIT_NOTE]);
    sleep(300).await;
    assert!(shares.requests.borrow().is_empty(), "nothing was shared");
    assert!(
        scores.sent("/api/scores").is_empty(),
        "nothing was submitted"
    );
    assert_eq!(text(&root, ".pg-share-status"), "");
    handle.destroy();
    root.remove();
}

/// A Share pressed while signed out is frozen then, and sent once after
/// signing in: edits made while the sign-in is open stay out of it.
#[wasm_bindgen_test]
async fn a_frozen_share_goes_out_once_after_signing_in() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, physics) = mount_as(&format!("s={}", glued_code()), None).await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    physics.set_glues(&[]).unwrap();
    force_button(&root, "Shake").click();
    sleep(100).await;
    assert!(api.requests.borrow().is_empty());
    answer(true);
    sleep(30).await;
    wait_share(&root).await;
    let requests = api.requests.borrow().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        board::decode(&requests[0].code, 2).unwrap(),
        board::BoardState {
            arrangement: two_squares(),
            glues: two_square_glues(),
        }
    );
    answer(true);
    sleep(300).await;
    assert_eq!(api.requests.borrow().len(), 1, "sent at most once");
    assert_eq!(sign_in_note(&root), None);
    assert_eq!(clipboard.values.borrow().len(), 1);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn dismissing_sign_in_drops_the_frozen_requests() {
    let _gpu = NoWebGpu::install();
    let shares = ShareApi::install(&[OK]);
    let scores = Api::install(&[("/api/scores", vec![saved()])]);
    let (handle, root, _) = mount_as("", None).await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    answer(false);
    sleep(30).await;
    assert_eq!(sign_in_note(&root).as_deref(), Some(UNSHARED_NOTE));
    // A late yes to the same ask finds nothing left to send.
    answer(true);
    submit_button(&root, "Settle").click();
    wait_for_report(8000).await;
    submit_button(&root, "Submit packing").click();
    sleep(30).await;
    answer(false);
    sleep(30).await;
    assert_eq!(sign_in_note(&root).as_deref(), Some(UNSUBMITTED_NOTE));
    sleep(300).await;
    assert!(shares.requests.borrow().is_empty());
    assert!(scores.sent("/api/scores").is_empty());
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn unmounting_drops_a_request_waiting_for_sign_in() {
    let _gpu = NoWebGpu::install();
    let api = ShareApi::install(&[OK]);
    let (handle, root, _) = mount_as("", None).await;
    submit_button(&root, "Share").click();
    sleep(30).await;
    handle.destroy();
    root.remove();
    answer(true);
    sleep(300).await;
    assert!(api.requests.borrow().is_empty());
}

/// Submit sends one board code: the certified packing with the scene's
/// glue. Reopening that board, where its `/s/` link redirects, restores the
/// same squares and glue.
#[wasm_bindgen_test]
async fn signed_in_submit_sends_the_certified_board_with_its_glue() {
    let _gpu = NoWebGpu::install();
    let scores = Api::install(&[("/api/scores", vec![saved()])]);
    let (handle, root, physics) = mount().await;
    let glues = glue_tops(&root, &physics).await;
    submit_button(&root, "Settle").click();
    wait_for_report(8000).await;
    let report = TEST_REPORT.with(|r| r.borrow().clone()).unwrap();
    submit_button(&root, "Submit packing").click();
    wait_until("the saved score", || {
        text(&root, ".pg-status").starts_with("Saved! Rank #1")
    })
    .await;
    let sent = scores.sent("/api/scores");
    assert_eq!(sent.len(), 1);
    let body: SubmitScore = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(body, submitted(&report, &glues));
    assert!(asks().is_empty());
    handle.destroy();
    root.remove();

    let (handle, root, physics) = mount_at(&format!("s={}", body.board.code)).await;
    assert!(text(&root, ".pg-status").starts_with("Board loaded"));
    assert_eq!(physics.glues(), glues);
    // The physics holds positions as f32.
    let squares = report
        .squares
        .iter()
        .map(|p| shared::Placement {
            cx: p.cx as f32 as f64,
            cy: p.cy as f32 as f64,
            theta: p.theta as f32 as f64,
        })
        .collect();
    assert_eq!(
        physics.arrangement(),
        Arrangement {
            container: shared::Shape::Square,
            shape: shared::Shape::Square,
            squares,
            ..report
        }
    );
    handle.destroy();
    root.remove();
}

/// A Submit pressed while signed out freezes its board: glue removed while
/// the sign-in is open still goes out with it.
#[wasm_bindgen_test]
async fn a_frozen_submit_goes_out_with_the_glue_it_had() {
    let _gpu = NoWebGpu::install();
    let scores = Api::install(&[("/api/scores", vec![saved()])]);
    let (handle, root, physics) = mount_as("", None).await;
    let glues = glue_tops(&root, &physics).await;
    submit_button(&root, "Settle").click();
    wait_for_report(8000).await;
    let report = TEST_REPORT.with(|r| r.borrow().clone()).unwrap();
    submit_button(&root, "Submit packing").click();
    sleep(30).await;
    assert_eq!(asks(), [SUBMIT_NOTE]);
    physics.set_glues(&[]).unwrap();
    answer(true);
    wait_until("the saved score", || {
        text(&root, ".pg-status").starts_with("Saved!")
    })
    .await;
    let sent = scores.sent("/api/scores");
    assert_eq!(sent.len(), 1);
    let body: SubmitScore = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(body, submitted(&report, &glues));
    handle.destroy();
    root.remove();
}

/// An expired session's 401 isn't retried: it asks to sign in, and then
/// the same code goes out again.
#[wasm_bindgen_test]
async fn a_401_on_share_asks_to_sign_in_and_resends_the_same_code() {
    let _gpu = NoWebGpu::install();
    let expired = Reply {
        status: 401,
        retry_after: None,
        stall_body: false,
    };
    let api = ShareApi::install(&[expired, OK]);
    let clipboard = Clipboard::install(false);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Share").click();
    wait_until("the sign-in prompt", || sign_in_note(&root).is_some()).await;
    assert_eq!(sign_in_note(&root).as_deref(), Some(EXPIRED_SHARE_NOTE));
    assert_eq!(asks(), [EXPIRED_SHARE_NOTE]);
    assert_eq!(api.requests.borrow().len(), 1, "a 401 is not retried");
    assert_eq!(share_alert(&root), None);
    answer(true);
    sleep(30).await;
    wait_share(&root).await;
    let requests = api.requests.borrow().clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1], "the same board again");
    assert_eq!(clipboard.values.borrow().len(), 1);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn a_401_on_submit_asks_to_sign_in_and_resubmits() {
    let _gpu = NoWebGpu::install();
    let expired = reply(
        401,
        serde_json::json!({ "error": "Sign in to submit a score" }),
    );
    let scores = Api::install(&[("/api/scores", vec![expired, saved()])]);
    let (handle, root, _) = mount().await;
    submit_button(&root, "Settle").click();
    wait_for_report(8000).await;
    submit_button(&root, "Submit packing").click();
    wait_until("the sign-in prompt", || sign_in_note(&root).is_some()).await;
    assert_eq!(sign_in_note(&root).as_deref(), Some(EXPIRED_SUBMIT_NOTE));
    assert_eq!(scores.sent("/api/scores").len(), 1);
    answer(true);
    wait_until("the saved score", || {
        text(&root, ".pg-status").starts_with("Saved!")
    })
    .await;
    let sent = scores.sent("/api/scores");
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], sent[1], "the same board again");
    handle.destroy();
    root.remove();
}

/// The whole site: the real account provider and header control.
#[function_component(Site)]
fn site() -> Html {
    html! {
        <BrowserRouter>
            <AccountProvider>
                <AccountMenu />
                <Game n={2} />
            </AccountProvider>
        </BrowserRouter>
    }
}

async fn mount_site(query: &str) -> (yew::AppHandle<Site>, Element, Physics) {
    set_query(query);
    let root = new_root();
    let handle = yew::Renderer::<Site>::with_root(root.clone()).render();
    sleep(100).await;
    let physics = TEST_PHYSICS
        .with(|p| p.borrow().clone())
        .expect("Game registered its physics");
    (handle, root, physics)
}

fn sign_in_api() -> Api {
    let ada = reply(200, serde_json::json!({ "username": "ada" }));
    Api::install(&[
        ("/api/auth/me", vec![not_signed_in(), ada]),
        ("/api/auth/login/start", vec![login_started()]),
        (
            "/api/auth/login/finish",
            vec![reply(200, serde_json::json!({ "username": "ada" }))],
        ),
    ])
}

#[wasm_bindgen_test]
async fn a_share_opens_the_sign_in_dialog_and_signing_in_sends_it() {
    let _gpu = NoWebGpu::install();
    let shares = ShareApi::install(&[OK]);
    let _clipboard = Clipboard::install(false);
    let _auth = sign_in_api();
    let _keys = Passkeys::install();
    let (handle, root, physics) = mount_site(&format!("s={}", glued_code())).await;
    submit_button(&root, "Share").click();
    sleep(50).await;
    assert_eq!(
        text_of(&root, ".account-reason").as_deref(),
        Some(SHARE_NOTE)
    );
    physics.set_glues(&[]).unwrap();
    sleep(30).await;
    click(&root, ".account-sign-in");
    wait_until("the share", || shares.requests.borrow().len() == 1).await;
    wait_share(&root).await;
    assert_eq!(shares.requests.borrow()[0].code, glued_code());
    assert_eq!(text_of(&root, ".account-name").as_deref(), Some("ada"));
    assert!(find(&root, ".account-dialog").is_none());
    assert_eq!(sign_in_note(&root), None);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn cancelling_the_sign_in_dialog_drops_the_share() {
    let _gpu = NoWebGpu::install();
    let shares = ShareApi::install(&[OK]);
    let _auth = sign_in_api();
    let _keys = Passkeys::install();
    let (handle, root, _) = mount_site(&format!("s={}", glued_code())).await;
    submit_button(&root, "Share").click();
    sleep(50).await;
    click(&root, ".account-cancel");
    sleep(30).await;
    assert!(find(&root, ".account-dialog").is_none());
    assert_eq!(sign_in_note(&root).as_deref(), Some(UNSHARED_NOTE));
    // Signing in afterwards sends nothing.
    click(&root, ".account-open");
    sleep(30).await;
    sleep(30).await;
    click(&root, ".account-sign-in");
    wait_until("the sign-in", || {
        text_of(&root, ".account-name").as_deref() == Some("ada")
    })
    .await;
    sleep(300).await;
    assert!(shares.requests.borrow().is_empty());
    handle.destroy();
    root.remove();
}

/// A sign-in that finishes after its dialog was cancelled, and after a
/// newer Share asked again, shares nothing. Only a sign-in for the newer
/// ask sends its snapshot.
#[wasm_bindgen_test]
async fn a_stale_sign_in_does_not_share_for_a_newer_ask() {
    let _gpu = NoWebGpu::install();
    let shares = ShareApi::install(&[OK]);
    let _clipboard = Clipboard::install(false);
    let ada = || reply(200, serde_json::json!({ "username": "ada" }));
    let auth = Api::install(&[
        (
            "/api/auth/me",
            vec![not_signed_in(), not_signed_in(), ada()],
        ),
        ("/api/auth/login/start", vec![login_started()]),
        ("/api/auth/login/finish", vec![ada().after(800), ada()]),
        ("/api/auth/logout", vec![empty(204)]),
    ]);
    let _keys = Passkeys::install();
    let (handle, root, _) = mount_site(&format!("s={}", glued_code())).await;
    submit_button(&root, "Share").click();
    sleep(50).await;
    sleep(30).await;
    click(&root, ".account-sign-in");
    wait_until("the first finish", || {
        auth.sent("/api/auth/login/finish").len() == 1
    })
    .await;
    click(&root, ".account-cancel");
    sleep(30).await;
    submit_button(&root, "Share").click();
    sleep(1100).await;
    assert!(
        shares.requests.borrow().is_empty(),
        "the stale sign-in shared nothing"
    );
    assert!(find(&root, ".account-name").is_none());
    assert_eq!(
        auth.sent("/api/auth/logout").len(),
        1,
        "that session was signed out"
    );
    assert_eq!(sign_in_note(&root).as_deref(), Some(SHARE_NOTE));
    click(&root, ".account-sign-in");
    wait_until("the share", || shares.requests.borrow().len() == 1).await;
    wait_share(&root).await;
    assert_eq!(shares.requests.borrow().len(), 1);
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn corner_tap_and_keyboard_nudge_really_shrink_the_box() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let mut steps = Vec::new();
    wait_ready(&root, &physics, &mut steps, "before corner").await;
    let corner = root
        .query_selector(".pg-corner")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    let before = physics.side();
    // Native keyboard activation follows the button's zero-detail click path.
    corner.click();
    for _ in 0..100 {
        if physics.side() < before - 0.001 {
            break;
        }
        sleep(20).await;
    }
    assert!(
        physics.side() < before - 0.001,
        "the nudge must act before releasing"
    );
    for _ in 0..100 {
        if physics.params().band_tension == 0.0 {
            break;
        }
        sleep(20).await;
    }
    assert_eq!(physics.params().band_tension, 0.0, "nudge releases itself");
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn corner_drag_squeezes_without_teleport_and_cancel_releases() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    let mut steps = Vec::new();
    wait_ready(&root, &physics, &mut steps, "before corner drag").await;
    let corner = root.query_selector(".pg-corner").unwrap().unwrap();
    let r = corner.get_bounding_client_rect();
    let x = r.x() + r.width() / 2.0;
    let y = r.y() + r.height() / 2.0;
    let send = |kind: &str, dx: i32| {
        let init = PointerEventInit::new();
        init.set_bubbles(true);
        init.set_is_primary(true);
        init.set_pointer_id(71);
        init.set_client_x(x as i32 + dx);
        init.set_client_y(y as i32 + dx);
        corner
            .dispatch_event(&PointerEvent::new_with_event_init_dict(kind, &init).unwrap())
            .unwrap();
    };
    // A new hold must cancel the previous tap's timed nudge.
    corner.clone().dyn_into::<HtmlElement>().unwrap().click();
    sleep(20).await;
    send("pointerdown", 0);
    sleep(3000).await;
    assert!(
        !text(&root, ".pg-status").starts_with("Ready"),
        "holding a corner excludes auto-measure"
    );
    let before = physics.side();
    send("pointermove", 20);
    sleep(20).await;
    assert!(
        physics.side() > before - 0.1,
        "spring input must not teleport"
    );
    assert!(physics.params().band_tension > 0.0);
    sleep(200).await;
    assert!(physics.side() < before, "inward movement squeezes");
    send("pointercancel", 20);
    sleep(30).await;
    assert_eq!(physics.params().band_tension, 0.0);
    send("pointermove", 40);
    sleep(20).await;
    assert_eq!(
        physics.params().band_tension,
        0.0,
        "late moves cannot revive capture"
    );
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn polygon_games_load_settle_and_offer_all_shapes() {
    for shape in [
        shared::Shape::Triangle,
        shared::Shape::Pentagon,
        shared::Shape::Hexagon,
    ] {
        let a = Arrangement {
            container: shared::Shape::Square,
            shape,
            n: 2,
            side: 5.0,
            squares: vec![
                shared::Placement {
                    cx: 1.5,
                    cy: 2.0,
                    theta: 0.1,
                },
                shared::Placement {
                    cx: 3.5,
                    cy: 2.0,
                    theta: 0.3,
                },
            ],
        };
        set_query(&format!("s={}", board::encode(&a, &[])));
        let root = new_root();
        let handle = yew::Renderer::<Host>::with_root_and_props(
            root.clone(),
            HostProps {
                container: shared::Shape::Square,
                account: account(Some("tester")),
                shape,
            },
        )
        .render();
        sleep(150).await;
        let physics = TEST_PHYSICS.with(|p| p.borrow().clone()).unwrap();
        assert_eq!(physics.shape(), shape);
        assert!(physics.paused());
        assert_eq!(
            root.query_selector_all(".pg-game-picker").unwrap().length(),
            1
        );
        assert!(root
            .query_selector(".pg-game-picker")
            .unwrap()
            .unwrap()
            .get_attribute("aria-label")
            .unwrap()
            .contains(shape.plural()));
        click_button(&root, ".pg-submit", "Settle");
        wait_until("polygon certified", || {
            TEST_REPORT.with(|r| r.borrow().is_some())
        })
        .await;
        TEST_REPORT.with(|r| {
            let r = r.borrow();
            assert_eq!(r.as_ref().unwrap().shape, shape);
        });
        handle.destroy();
        root.remove();
        sleep(40).await;
    }
}

#[wasm_bindgen_test]
async fn picker_is_a_modal_and_cancel_preserves_the_board() {
    let _gpu = NoWebGpu::install();
    let (handle, root, physics) = mount().await;
    physics.set_paused(true);
    let before = physics.arrangement();
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let body = document.body().unwrap();
    body.style().set_property("overflow", "auto").unwrap();
    let button = root
        .query_selector(".pg-game-picker")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    button.click();
    sleep(40).await;
    let dialog = root
        .query_selector("dialog")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::HtmlDialogElement>()
        .unwrap();
    assert!(dialog.open());
    assert_eq!(
        body.style().get_property_value("overflow").unwrap(),
        "hidden"
    );
    assert_eq!(document.active_element().unwrap().id(), "game-count");
    assert_eq!(
        dialog
            .query_selector_all("fieldset button")
            .unwrap()
            .length(),
        8
    );
    click_button(&root, "dialog", "Cancel");
    sleep(40).await;
    assert!(!dialog.open());
    assert_eq!(body.style().get_property_value("overflow").unwrap(), "auto");
    assert_eq!(
        document.active_element().unwrap(),
        button.clone().unchecked_into::<Element>()
    );
    assert_eq!(physics.arrangement(), before);
    button.click();
    sleep(30).await;
    let options = web_sys::EventInit::new();
    options.set_cancelable(true);
    options.set_bubbles(true);
    dialog
        .dispatch_event(&Event::new_with_event_init_dict("cancel", &options).unwrap())
        .unwrap();
    sleep(30).await;
    assert!(!dialog.open());
    assert_eq!(physics.arrangement(), before);
    body.style().remove_property("overflow").unwrap();
    handle.destroy();
    root.remove();
}

#[wasm_bindgen_test]
async fn all_container_games_restore_and_certify_without_changing_shape() {
    let _gpu = NoWebGpu::install();
    for container in shared::Shape::ALL {
        for shape in shared::Shape::ALL {
            let a = shared::Arrangement {
                shape,
                container,
                n: 2,
                side: 8.0,
                squares: vec![
                    shared::Placement {
                        cx: 3.0,
                        cy: 4.0,
                        theta: 0.0,
                    },
                    shared::Placement {
                        cx: 5.0,
                        cy: 4.0,
                        theta: 0.0,
                    },
                ],
            };
            set_query(&format!("s={}", board::encode(&a, &[])));
            let root = new_root();
            let handle = yew::Renderer::<Host>::with_root_and_props(
                root.clone(),
                HostProps {
                    account: account(Some("tester")),
                    shape,
                    container,
                },
            )
            .render();
            sleep(100).await;
            let physics = TEST_PHYSICS.with(|p| p.borrow().clone()).unwrap();
            assert_eq!(physics.shape(), shape);
            assert_eq!(physics.container(), container);
            assert!(physics.paused());
            assert_eq!(
                root.query_selector_all(".pg-corner").unwrap().length(),
                container.sides() as u32
            );
            click_button(&root, ".pg-submit", "Settle");
            wait_until("container certified", || {
                TEST_REPORT.with(|r| r.borrow().is_some())
            })
            .await;
            TEST_REPORT.with(|r| assert_eq!(r.borrow().as_ref().unwrap(), &a));
            handle.destroy();
            root.remove();
            sleep(25).await;
        }
    }
}
