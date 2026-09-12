//! The play screen: Rust physics on a canvas, plus the solver and leaderboard flow.

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests;
mod canvas;
mod files;

use crate::anneal::{Anneal, Command, Schedule};
use crate::benchmark::Benchmark;
use crate::{api, Route};
use canvas::Scene;
use gloo_events::{EventListener, EventListenerOptions};
use gloo_render::{request_animation_frame, AnimationFrame};
use physics::{Backend, Physics};
use shared::{Arrangement, KnownRecord, ScoreEntry, SubmitScore, MAX_N};
use solver::SolveReport;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::JsCast;
use web_sys::{
    Event, HtmlCanvasElement, HtmlInputElement, KeyboardEvent, PointerEvent, WheelEvent,
};
use yew::prelude::*;
use yew_router::prelude::*;

#[cfg(all(test, target_arch = "wasm32"))]
thread_local! {
    /// Physics of the most recently created `Game`, so browser tests can
    /// observe the simulation behind the mounted component.
    static TEST_PHYSICS: std::cell::RefCell<Option<Physics>> = const { std::cell::RefCell::new(None) };
}

const STEP_HZ: f64 = 120.0;
const MAX_SUBSTEPS: u32 = 6;
/// Frames of near-zero motion before "Settle & measure" runs by itself.
const SETTLE_FRAMES: u32 = 150;
const NUDGE: f32 = 0.025;
const ROTATE_STEP: f32 = 0.04;

#[derive(Properties, PartialEq)]
pub struct GameProps {
    pub n: u32,
}

pub enum Msg {
    Frame(f64),
    Stepped,
    GpuReady,
    Records(Result<Vec<KnownRecord>, String>),
    PointerDown(PointerEvent),
    PointerMove(PointerEvent),
    PointerUp,
    Wheel(usize, f32),
    Key(KeyboardEvent),
    SetCount(String),
    TargetSide(f64),
    BandTension(f32),
    Gravity(bool),
    Attraction(bool),
    EdgeAttraction(f32),
    Damping(f32),
    Stiffness(f32),
    TogglePause,
    Shake,
    Reset,
    Anneal,
    Measure,
    Refine,
    Player(String),
    Submit,
    Submitted(Result<ScoreEntry, String>),
    Export,
    ImportPick,
    ImportFile(Event),
    Imported(Result<String, String>),
}

/// Sidebar text derived from the simulation; the component only re-renders
/// when this changes.
#[derive(Default, PartialEq)]
struct Readout {
    mode: String,
    side: String,
    density: String,
    meter: String,
    record: String,
    size_value: f64,
    /// Annealing progress in percent while a run is active.
    anneal: Option<u32>,
}

pub struct Game {
    physics: Physics,
    canvas: NodeRef,
    file_input: NodeRef,
    frame: Option<AnimationFrame>,
    wheel: Option<EventListener>,
    /// Side the viewport is scaled to, so a contracting band stays in view.
    view_side: Rc<Cell<f64>>,
    last_time: Option<f64>,
    accumulator: f64,
    stepping: bool,
    busy: bool,
    selected: Option<usize>,
    rotating: bool,
    dragging: bool,
    last_pointer: Option<(f64, f64)>,
    grab: (f64, f64),
    mouse: (f64, f64),
    record: Option<KnownRecord>,
    record_note: String,
    last_report: Option<SolveReport>,
    bound: String,
    status: String,
    status_error: bool,
    auto_measured: bool,
    settled_frames: u32,
    player: String,
    submitting: bool,
    submit_after_measure: bool,
    pending_import: Option<Arrangement>,
    anneal: Option<Anneal>,
    readout: Readout,
}

fn initial_side(n: u32) -> f64 {
    (n as f64).sqrt().ceil() + 0.5
}

fn input_value(e: &Event) -> String {
    e.target_unchecked_into::<HtmlInputElement>().value()
}

/// `element.focus({preventScroll: true})`; focusing must not scroll the page,
/// or pointer-to-world coordinates shift mid-drag. The pinned web-sys has no
/// `FocusOptions`, so the options object is built by hand.
fn focus_without_scroll(canvas: &HtmlCanvasElement) {
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &"preventScroll".into(), &true.into());
    if let Ok(focus) = js_sys::Reflect::get(canvas, &"focus".into()) {
        if let Some(focus) = focus.dyn_ref::<js_sys::Function>() {
            let _ = focus.call1(canvas, &options);
        }
    }
}

impl Component for Game {
    type Message = Msg;
    type Properties = GameProps;

    fn create(ctx: &Context<Self>) -> Self {
        let n = ctx.props().n;
        let side = initial_side(n);
        let physics = Physics::new(n, side);
        #[cfg(all(test, target_arch = "wasm32"))]
        TEST_PHYSICS.with(|p| p.replace(Some(physics.clone())));
        let gpu = physics.clone();
        ctx.link().send_future(async move {
            gpu.init_gpu().await;
            Msg::GpuReady
        });
        ctx.link()
            .send_future(async { Msg::Records(api::known_records().await) });
        let mut game = Self {
            physics,
            canvas: NodeRef::default(),
            file_input: NodeRef::default(),
            frame: None,
            wheel: None,
            view_side: Rc::new(Cell::new(side)),
            last_time: None,
            accumulator: 0.0,
            stepping: false,
            busy: false,
            selected: None,
            rotating: false,
            dragging: false,
            last_pointer: None,
            grab: (0.0, 0.0),
            mouse: (0.0, 0.0),
            record: None,
            record_note: "Loading reference record…".into(),
            last_report: None,
            bound: String::new(),
            status: "Find your rhythm. Then squeeze a little.".into(),
            status_error: false,
            auto_measured: false,
            settled_frames: 0,
            player: String::new(),
            submitting: false,
            submit_after_measure: false,
            pending_import: None,
            anneal: None,
            readout: Readout::default(),
        };
        // Fill the sidebar before the first view; later frames only re-render on change.
        game.refresh_readout();
        game
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }
        if let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() {
            // Registered by hand: wheel needs a non-passive listener to stop page scroll.
            let (physics, view_side, link) = (
                self.physics.clone(),
                self.view_side.clone(),
                ctx.link().clone(),
            );
            let target = canvas.clone();
            self.wheel = Some(EventListener::new_with_options(
                &canvas,
                "wheel",
                EventListenerOptions::enable_prevent_default(),
                move |e| {
                    let Some(e) = e.dyn_ref::<WheelEvent>() else {
                        return;
                    };
                    let extent = view_side.get().max(physics.side());
                    let p = canvas::to_world(
                        &target,
                        (e.client_x() as f64, e.client_y() as f64),
                        extent,
                    );
                    if let Some(i) = canvas::hit(&physics.bodies(), p) {
                        e.prevent_default();
                        link.send_message(Msg::Wheel(i, e.delta_y().signum() as f32));
                    }
                },
            ));
        }
        self.schedule_frame(ctx);
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let n = ctx.props().n;
        match msg {
            Msg::Frame(time) => {
                self.frame = None;
                let elapsed = self
                    .last_time
                    .map_or(0.0, |last| ((time - last) / 1000.0).clamp(0.0, 0.05));
                self.last_time = Some(time);
                self.tick_anneal(ctx, elapsed);
                if self.physics.paused() {
                    self.accumulator = 0.0;
                } else {
                    self.accumulator += elapsed;
                    let steps = ((self.accumulator * STEP_HZ) as u32).min(MAX_SUBSTEPS);
                    if steps > 0 {
                        self.accumulator -= steps as f64 / STEP_HZ;
                        self.stepping = true;
                        let physics = self.physics.clone();
                        ctx.link().send_future(async move {
                            physics.step(steps).await;
                            Msg::Stepped
                        });
                        return false;
                    }
                }
                self.after_frame(ctx)
            }
            Msg::Stepped => {
                self.stepping = false;
                if let Some(a) = self.pending_import.take() {
                    self.apply_import(&a);
                }
                self.after_frame(ctx)
            }
            Msg::GpuReady => self.refresh_readout(),
            Msg::Records(rows) => {
                match rows {
                    Ok(rows) => {
                        self.record = rows.into_iter().find(|r| r.n == n);
                        if self.record.is_none() {
                            self.record_note = "No reference loaded for this square count.".into();
                        }
                    }
                    Err(_) => self.record_note = "Reference records unavailable.".into(),
                }
                self.refresh_readout()
            }
            Msg::PointerDown(e) => {
                if self.busy {
                    return false;
                }
                e.prevent_default();
                let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
                    return false;
                };
                focus_without_scroll(&canvas);
                let p = self.world(&canvas, &e);
                let bodies = self.physics.bodies();
                self.selected = canvas::hit(&bodies, p);
                self.rotating = e.shift_key();
                self.last_pointer = Some(p);
                self.grab = self.selected.map_or((0.0, 0.0), |i| {
                    (bodies[i].x as f64 - p.0, bodies[i].y as f64 - p.1)
                });
                if self.selected.is_some() {
                    self.stop_anneal();
                    self.set_pause(false);
                }
                self.dragging = self.selected.is_some() && !self.rotating;
                self.mouse = (p.0 + self.grab.0, p.1 + self.grab.1);
                self.push_mouse();
                let _ = canvas.set_pointer_capture(e.pointer_id());
                self.invalidate();
                true
            }
            Msg::PointerMove(e) => {
                let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
                    return false;
                };
                let p = self.world(&canvas, &e);
                if let (Some(last), Some(i)) = (self.last_pointer, self.selected) {
                    if canvas.has_pointer_capture(e.pointer_id()) {
                        if self.rotating {
                            self.stop_anneal();
                            self.set_pause(false);
                            self.physics.turn(i, ((p.0 - last.0) * 2.0) as f32);
                        }
                        self.last_pointer = Some(p);
                        self.invalidate();
                    }
                }
                self.mouse = (p.0 + self.grab.0, p.1 + self.grab.1);
                self.push_mouse();
                false
            }
            Msg::PointerUp => {
                self.dragging = false;
                self.rotating = false;
                self.last_pointer = None;
                self.push_mouse();
                false
            }
            Msg::Wheel(i, sign) => {
                if self.busy {
                    return false;
                }
                self.stop_anneal();
                self.selected = Some(i);
                self.physics.turn(i, sign * ROTATE_STEP);
                self.set_pause(false);
                false
            }
            Msg::Key(e) => {
                if self.busy {
                    return false;
                }
                if e.code() == "Space" {
                    e.prevent_default();
                    self.stop_anneal();
                    self.set_pause(!self.physics.paused());
                    return self.refresh_readout();
                }
                let Some(i) = self.selected else {
                    return false;
                };
                match e.key().as_str() {
                    "ArrowLeft" => self.physics.nudge(i, -NUDGE, 0.0),
                    "ArrowRight" => self.physics.nudge(i, NUDGE, 0.0),
                    "ArrowUp" => self.physics.nudge(i, 0.0, NUDGE),
                    "ArrowDown" => self.physics.nudge(i, 0.0, -NUDGE),
                    "q" => self.physics.turn(i, -ROTATE_STEP),
                    "e" => self.physics.turn(i, ROTATE_STEP),
                    _ => return false,
                }
                e.prevent_default();
                self.stop_anneal();
                self.set_pause(false);
                false
            }
            Msg::SetCount(value) => {
                if let (Ok(count), Some(nav)) = (value.parse::<u32>(), ctx.link().navigator()) {
                    if (1..=MAX_N).contains(&count) {
                        nav.push(&Route::Play { n: count });
                    }
                }
                false
            }
            Msg::TargetSide(side) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.target_side = side;
                // A size edit is a spring target, never a teleport. Enable
                // pressure if it was off, retaining any chosen nonzero strength.
                if params.band_tension == 0.0 {
                    params.band_tension = 30.0;
                }
                self.physics.set_params(params);
                self.set_pause(false);
                self.refresh_readout();
                true
            }
            Msg::BandTension(tension) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.band_tension = tension;
                self.physics.set_params(params);
                self.invalidate();
                self.set_pause(false);
                true
            }
            Msg::Gravity(on) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.gravity = on;
                self.physics.set_params(params);
                self.invalidate();
                true
            }
            Msg::Attraction(on) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.attraction = on;
                self.physics.set_params(params);
                self.invalidate();
                true
            }
            Msg::EdgeAttraction(strength) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.edge_attraction = strength;
                self.physics.set_params(params);
                self.invalidate();
                self.set_pause(false);
                true
            }
            Msg::Damping(damping) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.damping = damping;
                self.physics.set_params(params);
                true
            }
            Msg::Stiffness(stiffness) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.stiffness = stiffness;
                self.physics.set_params(params);
                true
            }
            Msg::TogglePause => {
                self.stop_anneal();
                self.set_pause(!self.physics.paused());
                self.refresh_readout();
                true
            }
            Msg::Shake => {
                self.stop_anneal();
                self.physics
                    .shake((js_sys::Math::random() * u64::MAX as f64) as u64);
                self.invalidate();
                self.set_pause(false);
                true
            }
            Msg::Reset => {
                self.stop_anneal();
                let side = initial_side(n);
                self.physics.set_side(side);
                let mut params = self.physics.params();
                params.target_side = side;
                self.physics.set_params(params);
                self.view_side.set(side);
                self.physics.reset();
                self.invalidate();
                self.set_status("Fresh grid. Make it yours.", false);
                true
            }
            Msg::Anneal => {
                if self.anneal.is_some() {
                    self.stop_anneal();
                    self.refresh_readout();
                    return true;
                }
                if self.busy {
                    return false;
                }
                let seed = (js_sys::Math::random() * u64::MAX as f64) as u64;
                let floor = (n as f64).sqrt();
                self.anneal = Some(Anneal::new(
                    Schedule::default(),
                    self.physics.side(),
                    floor,
                    seed,
                ));
                self.set_pause(false);
                self.set_status("Annealing: gentler shakes, tighter band…", false);
                self.refresh_readout();
                true
            }
            Msg::Measure => {
                self.stop_anneal();
                self.begin_measure(ctx)
            }
            Msg::Refine => {
                if self.stepping {
                    ctx.link().send_future(async {
                        gloo_timers::future::sleep(Duration::from_millis(5)).await;
                        Msg::Refine
                    });
                    return false;
                }
                self.refine(ctx);
                true
            }
            Msg::Player(name) => {
                self.player = name;
                false
            }
            Msg::Submit => {
                let player = self.player.trim().to_string();
                if player.is_empty() {
                    self.set_status("Choose a leaderboard name first.", true);
                    return true;
                }
                match &self.last_report {
                    Some(r) if r.valid => {
                        let arrangement = r.arrangement.clone();
                        self.send_submit(ctx, player, arrangement);
                    }
                    Some(_) => self.set_status(
                        "This packing still has overlaps. Give it a little more room.",
                        true,
                    ),
                    None => {
                        self.submit_after_measure = true;
                        ctx.link().send_message(Msg::Measure);
                    }
                }
                true
            }
            Msg::Submitted(result) => {
                self.submitting = false;
                match result {
                    Ok(entry) => self.set_status(
                        &format!("Saved! Rank #{} for {n} squares.", entry.rank),
                        false,
                    ),
                    Err(e) => self.set_status(&e, true),
                }
                true
            }
            Msg::Export => {
                let value = match &self.last_report {
                    Some(report) => serde_json::to_value(report),
                    None => serde_json::to_value(serde_json::json!({
                        "arrangement": self.physics.arrangement()
                    })),
                };
                let result = value
                    .map_err(|e| e.to_string())
                    .and_then(|v| files::download_json(&format!("packit-{n}.json"), &v));
                if let Err(e) = result {
                    self.set_status(&e, true);
                    return true;
                }
                false
            }
            Msg::ImportPick => {
                if let Some(input) = self.file_input.cast::<HtmlInputElement>() {
                    input.click();
                }
                false
            }
            Msg::ImportFile(e) => {
                let input: HtmlInputElement = e.target_unchecked_into();
                let file = input.files().and_then(|files| files.get(0));
                input.set_value("");
                let Some(file) = file else {
                    return false;
                };
                if file.size() > files::MAX_IMPORT_BYTES {
                    self.set_status("File must be smaller than 1 MB", true);
                    return true;
                }
                ctx.link().send_future(async move {
                    let text = wasm_bindgen_futures::JsFuture::from(file.text())
                        .await
                        .ok()
                        .and_then(|v| v.as_string())
                        .ok_or_else(|| "Could not read the file".to_string());
                    Msg::Imported(text)
                });
                false
            }
            Msg::Imported(text) => {
                match text.and_then(|t| files::parse_arrangement(&t, n)) {
                    Ok(a) => {
                        self.stop_anneal();
                        self.set_pause(true);
                        if self.stepping {
                            self.pending_import = Some(a);
                        } else {
                            self.apply_import(&a);
                        }
                    }
                    Err(e) => self.set_status(&e, true),
                }
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let n = ctx.props().n;
        let params = self.physics.params();
        let r = &self.readout;
        let size_min = ((n as f64).sqrt() * 1000.0).ceil() / 1000.0;
        let size_max = (n as f64).sqrt().ceil() + 3.0;
        let status_class = classes!("pg-status", self.status_error.then_some("pg-invalid"));
        // Compare the refined f64 side once validated; otherwise the live side.
        let (bench_side, validated) = match &self.last_report {
            Some(report) if report.valid => (report.arrangement.side, true),
            _ => (self.physics.side(), false),
        };
        html! {
            <div class="packing-game">
                <div class="pg-top">
                    <div>
                        <div class="pg-eyebrow">{ "A little chaos. A tighter fit." }</div>
                        <h1>{ "Make room." }</h1>
                        <div class="pg-sub">{ "Pack unit squares. Chase the smallest container." }</div>
                    </div>
                    <Benchmark
                        side={bench_side}
                        reference_side={self.record.as_ref().map(|r| r.side)}
                        proven={self.record.as_ref().is_some_and(|r| r.proven_optimal)}
                        {validated} />
                    <label class="pg-sub">
                        { "Squares " }
                        <input class="pg-count" type="number" min="1" max={MAX_N.to_string()} value={n.to_string()}
                            aria-label="Number of squares"
                            onchange={link.callback(|e: Event| Msg::SetCount(input_value(&e)))} />
                    </label>
                </div>
                <div class="pg-layout">
                    <div>
                        <div class="pg-board">
                            <canvas ref={self.canvas.clone()} tabindex="0"
                                aria-label="Square packing playfield. Drag to move. Select a square, then use arrow keys to move and Q or E to rotate."
                                onpointerdown={link.callback(Msg::PointerDown)}
                                onpointermove={link.callback(Msg::PointerMove)}
                                onpointerup={link.callback(|_| Msg::PointerUp)}
                                onpointercancel={link.callback(|_| Msg::PointerUp)}
                                onlostpointercapture={link.callback(|_| Msg::PointerUp)}
                                onkeydown={link.callback(Msg::Key)} />
                            <div class="pg-board-footer">
                                <span class="pg-mode">{ &r.mode }</span>
                                <span>{ "drag · wheel to rotate · shift-drag to spin" }</span>
                            </div>
                        </div>
                        <p class="pg-help">{ "Force arrows: blue = net contact and edge pull · gold = mouse spring. Dashed band = target size." }</p>
                        <details class="pg-details">
                            <summary>{ "How to play & what the score means" }</summary>
                            <p>{ "Each square has side length 1. Make the container smaller while keeping every square inside and avoiding overlap. Dragging and rotating resume physics, push neighbors, and resist blocked motion. Lower the container target with outer band tension enabled to squeeze the packing. Turn on forces, or use Q/E to rotate a selected square. Arrow keys nudge it. Space pauses." }</p>
                            <p>{ "The simulation has springy contacts. “Settle & measure” pauses the scene and refines its contacts with a numerical polynomial solver. Only an independently validated arrangement can be submitted. A best-known packing is an upper bound, not necessarily a proven optimum. A numerical match is not an exact proof." }</p>
                            <p>
                                <a href="https://kingbird.myphotos.cc/packing/squares_in_squares.html" target="_blank" rel="noopener">
                                    { "Explore the research records ↗" }
                                </a>
                            </p>
                        </details>
                    </div>
                    <aside class="pg-sidebar">
                        <section class="pg-panel">
                            <h2>{ "Your packing" }</h2>
                            <div class="pg-stat">{ &r.side }<small>{ " side length" }</small></div>
                            <div class="pg-meter"><span style={format!("width: {}", r.meter)}></span></div>
                            <div class="pg-row"><span>{ "Area filled" }</span><strong>{ &r.density }</strong></div>
                            <div class="pg-help">{ &r.record }</div>
                            <label class="pg-row" for="pg-size">
                                { "Container target side " }<output>{ format!("{:.3}", r.size_value) }</output>
                            </label>
                            <input id="pg-size" type="range" min={size_min.to_string()} max={size_max.to_string()} step="0.001"
                                value={r.size_value.to_string()}
                                oninput={link.callback(|e: InputEvent| Msg::TargetSide(input_value(&e).parse().unwrap_or(1.0)))} />
                        </section>
                        <section class="pg-panel">
                            <h2>{ "Play with forces" }</h2>
                            <label class="pg-row" for="pg-band">
                                { "Outer band tension " }
                                <output>{ if params.band_tension > 0.0 { format!("{}", params.band_tension) } else { "Off".into() } }</output>
                            </label>
                            <input id="pg-band" type="range" min="0" max="100" step="1" value={params.band_tension.to_string()}
                                oninput={link.callback(|e: InputEvent| Msg::BandTension(input_value(&e).parse().unwrap_or(0.0)))} />
                            <p class="pg-help">{ "Changing container size animates the band with live pressure. Squares push back. Zero holds the current size; moving the size slider re-engages pressure at 30." }</p>
                            <label class="pg-row">
                                { "Gravity " }
                                <input type="checkbox" checked={params.gravity}
                                    onchange={link.callback(|e: Event| Msg::Gravity(e.target_unchecked_into::<HtmlInputElement>().checked()))} />
                            </label>
                            <label class="pg-row">
                                { "Square attraction " }
                                <input type="checkbox" checked={params.attraction}
                                    onchange={link.callback(|e: Event| Msg::Attraction(e.target_unchecked_into::<HtmlInputElement>().checked()))} />
                            </label>
                            <label class="pg-row" for="pg-edge-attraction">
                                { "Edge attraction " }
                                <output>{ if params.edge_attraction > 0.0 { format!("{}",params.edge_attraction) } else { "Off".into() } }</output>
                            </label>
                            <input id="pg-edge-attraction" type="range" min="0" max="40" step="1" value={params.edge_attraction.to_string()}
                                oninput={link.callback(|e: InputEvent| Msg::EdgeAttraction(input_value(&e).parse().unwrap_or(0.0)))} />
                            <p class="pg-help">{ "Pull nearby facing edges together and turn them toward a flush fit, including the outer band. Zero turns it off." }</p>
                            <label class="pg-row" for="pg-damping">
                                { "Damping " }<output>{ format!("{:.1}", params.damping) }</output>
                            </label>
                            <input id="pg-damping" type="range" min="0.3" max="12" step="0.1" value={params.damping.to_string()}
                                oninput={link.callback(|e: InputEvent| Msg::Damping(input_value(&e).parse().unwrap_or(3.0)))} />
                            <label class="pg-row" for="pg-stiffness">{ "Contact stiffness" }</label>
                            <input id="pg-stiffness" type="range" min="300" max="1600" step="50" value={params.stiffness.to_string()}
                                oninput={link.callback(|e: InputEvent| Msg::Stiffness(input_value(&e).parse().unwrap_or(900.0)))} />
                            <div class="pg-actions pg-force-actions">
                                <button onclick={link.callback(|_| Msg::TogglePause)}>
                                    { if self.physics.paused() { "Resume" } else { "Pause" } }
                                </button>
                                <button onclick={link.callback(|_| Msg::Shake)}>{ "Shake" }</button>
                                <button onclick={link.callback(|_| Msg::Reset)}>{ "Reset" }</button>
                                <button class={classes!(r.anneal.is_some().then_some("pg-primary"))}
                                    onclick={link.callback(|_| Msg::Anneal)}>
                                    { match r.anneal { Some(p) => format!("Stop · {p}%"), None => "Anneal".into() } }
                                </button>
                            </div>
                        </section>
                        <section class="pg-panel pg-submit">
                            <h2>{ "Chase the record" }</h2>
                            <div class="pg-actions">
                                <button class="pg-primary" disabled={self.busy} onclick={link.callback(|_| Msg::Measure)}>
                                    { "Settle & measure" }
                                </button>
                                <button onclick={link.callback(|_| Msg::Export)}>{ "Export" }</button>
                                <button onclick={link.callback(|_| Msg::ImportPick)}>{ "Import" }</button>
                                <input ref={self.file_input.clone()} type="file" accept="application/json,.json" hidden=true
                                    onchange={link.callback(Msg::ImportFile)} />
                            </div>
                            <p class="pg-help">{ &self.bound }</p>
                            <p class={status_class} role="status" aria-live="polite">{ &self.status }</p>
                            <label class="pg-help" for="pg-player">{ "Leaderboard name" }</label>
                            <input id="pg-player" class="pg-field" type="text" maxlength="32" placeholder="Your name"
                                autocomplete="nickname" value={self.player.clone()}
                                oninput={link.callback(|e: InputEvent| Msg::Player(input_value(&e)))} />
                            <button class="pg-primary pg-wide" disabled={self.submitting} onclick={link.callback(|_| Msg::Submit)}>
                                { "Submit packing" }
                            </button>
                            <p class="pg-help">
                                <Link<Route> to={Route::LeaderboardN { n }}>{ "See the leaderboard ↗" }</Link<Route>>
                            </p>
                        </section>
                    </aside>
                </div>
            </div>
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.physics.dispose();
    }
}

impl Game {
    /// Advance an annealing run by `dt` seconds and apply its commands.
    fn tick_anneal(&mut self, ctx: &Context<Self>, dt: f64) {
        let Some(anneal) = self.anneal.as_mut() else {
            return;
        };
        let commands = anneal.advance(dt);
        if anneal.is_finished() {
            self.anneal = None;
        }
        for command in commands {
            match command {
                Command::Shake { seed, strength } => self.physics.shake_scaled(seed, strength),
                Command::Band {
                    target_side,
                    tension,
                } => {
                    let mut params = self.physics.params();
                    params.target_side = target_side;
                    params.band_tension = tension;
                    self.physics.set_params(params);
                }
                Command::Measure => {
                    // Release the band so it stops compressing once the run ends.
                    let mut params = self.physics.params();
                    params.band_tension = 0.0;
                    self.physics.set_params(params);
                    self.begin_measure(ctx);
                }
            }
        }
    }

    /// Manual input takes over from an annealing run.
    fn stop_anneal(&mut self) {
        if self.anneal.take().is_some() {
            // Release the band the run was tightening; a slider handler that
            // called this may set its own value right after.
            let mut params = self.physics.params();
            params.band_tension = 0.0;
            self.physics.set_params(params);
            self.set_status("Anneal stopped.", false);
        }
    }

    /// Pause and refine the current scene, once the status has painted.
    fn begin_measure(&mut self, ctx: &Context<Self>) -> bool {
        if self.busy {
            return false;
        }
        self.busy = true;
        self.set_pause(true);
        self.set_status("Refining contacts from the walls inward…", false);
        // Let the status paint before the solver blocks the main thread.
        ctx.link().send_future(async {
            gloo_timers::future::sleep(Duration::from_millis(25)).await;
            Msg::Refine
        });
        true
    }

    fn schedule_frame(&mut self, ctx: &Context<Self>) {
        let link = ctx.link().clone();
        self.frame = Some(request_animation_frame(move |t| {
            link.send_message(Msg::Frame(t))
        }));
    }

    /// Draw, check for a settled scene, and queue the next frame. Returns
    /// whether the sidebar needs a re-render.
    fn after_frame(&mut self, ctx: &Context<Self>) -> bool {
        self.draw();
        self.check_settled(ctx);
        self.schedule_frame(ctx);
        self.refresh_readout()
    }

    fn draw(&self) {
        let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
            return;
        };
        let bodies = self.physics.bodies();
        let params = self.physics.params();
        // Make room smoothly for an expanding target; retain the larger view
        // when squeezing so boundary motion remains visible.
        let extent = if params.band_tension > 0.0 {
            params.target_side.max(self.physics.side())
        } else {
            self.physics.side()
        };
        if extent > self.view_side.get() {
            self.view_side
                .set(self.view_side.get() + (extent - self.view_side.get()) * 0.12);
        }
        let forces = self.physics.contact_forces();
        let show_forces = self.dragging
            || self.rotating
            || (self.selected.is_some() && self.physics.motion() > 0.002 * bodies.len() as f32);
        canvas::draw(
            &canvas,
            &Scene {
                bodies: &bodies,
                side: self.physics.side(),
                view_side: self.view_side.get(),
                band_on: params.band_tension > 0.0,
                band_tension: params.band_tension,
                target_side: params.target_side,
                forces: show_forces.then_some(&forces),
                mouse_force: self.physics.mouse_force(),
                selected: self.selected,
                tether: self.dragging.then_some(self.mouse),
            },
        );
    }

    fn check_settled(&mut self, ctx: &Context<Self>) {
        // An annealing run measures on its own schedule.
        if self.physics.paused()
            || self.dragging
            || self.busy
            || self.auto_measured
            || self.anneal.is_some()
        {
            return;
        }
        let n = ctx.props().n as f32;
        let calm = self.physics.motion() < 0.002 * n && self.physics.band_velocity().abs() < 1e-4;
        self.settled_frames = if calm { self.settled_frames + 1 } else { 0 };
        if self.settled_frames > SETTLE_FRAMES {
            self.auto_measured = true;
            ctx.link().send_message(Msg::Measure);
        }
    }

    fn refresh_readout(&mut self) -> bool {
        let side = self.physics.side();
        let n = self.physics.bodies().len() as f64;
        let density = n / (side * side) * 100.0;
        let params = self.physics.params();
        let mode = match self.physics.mode() {
            Backend::Gpu => "WebGPU compute",
            Backend::Cpu => "CPU fallback",
        };
        let record = match &self.record {
            Some(r) => {
                let gap = (side / r.side - 1.0) * 100.0;
                format!(
                    "{}: {:.6} · {gap:+.3}% side gap{}",
                    if r.proven_optimal {
                        "Proven optimum"
                    } else {
                        "Best known"
                    },
                    r.side,
                    if gap < 0.0 { " (check overlaps)" } else { "" }
                )
            }
            None => self.record_note.clone(),
        };
        let next = Readout {
            mode: format!(
                "{mode}{}",
                if self.physics.paused() {
                    " · paused"
                } else {
                    ""
                }
            ),
            side: format!("{side:.6}"),
            density: format!("{density:.1}%"),
            meter: format!("{:.1}%", density.min(100.0)),
            record,
            size_value: if params.band_tension > 0.0 {
                params.target_side
            } else {
                side
            },
            anneal: self.anneal.as_ref().map(|a| (a.progress() * 100.0) as u32),
        };
        let changed = next != self.readout;
        self.readout = next;
        changed
    }

    fn world(&self, canvas: &HtmlCanvasElement, e: &PointerEvent) -> (f64, f64) {
        let extent = self.view_side.get().max(self.physics.side());
        canvas::to_world(canvas, (e.client_x() as f64, e.client_y() as f64), extent)
    }

    fn push_mouse(&self) {
        self.physics.set_mouse(
            self.mouse.0 as f32,
            self.mouse.1 as f32,
            self.selected,
            self.dragging,
        );
    }

    /// Any change to the scene makes the last measurement stale.
    fn invalidate(&mut self) {
        self.last_report = None;
        self.auto_measured = false;
        self.settled_frames = 0;
    }

    fn set_pause(&mut self, paused: bool) {
        if !paused {
            self.invalidate();
        }
        self.physics.set_paused(paused);
    }

    fn set_status(&mut self, text: &str, error: bool) {
        self.status = text.to_string();
        self.status_error = error;
    }

    fn apply_import(&mut self, a: &Arrangement) {
        self.physics.load(a);
        self.view_side.set(self.physics.side());
        self.invalidate();
        self.set_status("Imported. Settle & measure to validate.", false);
    }

    fn refine(&mut self, ctx: &Context<Self>) {
        let submit = std::mem::take(&mut self.submit_after_measure);
        match solver::refine(&self.physics.arrangement()) {
            Ok(report) => {
                let side = report.arrangement.side;
                self.bound = format!(
                    "Lower bound {:.6} · gap {:.3}%",
                    report.lower_bound,
                    100.0 * (side / report.lower_bound - 1.0)
                );
                if report.valid {
                    self.physics.load(&report.arrangement);
                    self.view_side.set(self.physics.side());
                    let exact = report
                        .algebraic
                        .as_ref()
                        .map(|a| {
                            format!(
                                "Branch-exact side {} (assumed rotations).",
                                a.side_expression
                            )
                        })
                        .or_else(|| report.candidate_expression.clone())
                        .unwrap_or_else(|| "Numerical local solution.".into());
                    self.set_status(
                        &format!(
                            "Ready · {side:.9} side · contact residual {:.1e}. {exact}",
                            report.contacts.max_residual
                        ),
                        false,
                    );
                    if submit {
                        let player = self.player.trim().to_string();
                        self.send_submit(ctx, player, report.arrangement.clone());
                    }
                } else if submit {
                    self.set_status(
                        "This packing still has overlaps. Give it a little more room.",
                        true,
                    );
                } else {
                    self.set_status(&report.status, true);
                }
                self.last_report = Some(report);
            }
            Err(e) => self.set_status(&e, true),
        }
        self.busy = false;
    }

    fn send_submit(&mut self, ctx: &Context<Self>, player: String, arrangement: Arrangement) {
        self.submitting = true;
        ctx.link().send_future(async move {
            Msg::Submitted(
                api::submit_score(SubmitScore {
                    player,
                    arrangement,
                })
                .await,
            )
        });
    }
}
