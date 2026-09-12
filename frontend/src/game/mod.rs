//! The play screen: Rust physics on a canvas, plus the solver and leaderboard flow.

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests;
mod canvas;
mod files;
mod glue;
mod tap;

use crate::anneal::{Anneal, Command, Schedule};
use crate::benchmark::Benchmark;
use crate::{api, Route};
use canvas::Scene;
use gloo_events::{EventListener, EventListenerOptions};
use gloo_render::{request_animation_frame, AnimationFrame};
use physics::{Backend, Feature, Glue, Physics};
use shared::{geometry, share, Arrangement, KnownRecord, ScoreEntry, SubmitScore, MAX_N};
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
    /// The last validated arrangement, for exact comparisons in browser tests.
    static TEST_REPORT: std::cell::RefCell<Option<Arrangement>> = const { std::cell::RefCell::new(None) };
}

const STEP_HZ: f64 = 120.0;
const MAX_SUBSTEPS: u32 = 6;
/// Frames of near-zero motion before "Settle & measure" runs by itself.
const SETTLE_FRAMES: u32 = 150;
const NUDGE: f32 = 0.025;
const ROTATE_STEP: f32 = 0.04;
/// Turn per tap of the on-screen turn buttons (touch has no wheel or keys).
const TOUCH_TURN: f32 = 0.12;
/// How far the effective size target may lead the actual container at full
/// band tension (100); lower tension shortens the reach proportionally.
const MAX_SCRUB: f64 = 1.0;
/// Before measuring an overlapping scene, the band's target grows at this
/// rate (side units per second) until nothing overlaps, so the measurement
/// doesn't pop the squares apart.
const RELAX_RATE: f64 = 0.1;
const RELAX_TENSION: f32 = 20.0;
/// Overlap or wall protrusion small enough to measure: well under a pixel.
const RELAX_TOL: f64 = 2e-3;
/// Frames the scene must stay clear before measuring.
const RELAX_CLEAR_FRAMES: u32 = 8;
/// Give up relaxing after this long. Glue or a jam can hold an overlap the
/// band can't clear, and measuring it then would pop the squares apart.
const RELAX_MAX_SECS: f64 = 8.0;

/// The band's size target for a requested `desired` side: it can only run
/// ahead of the actual `side` as far as the band pressure reaches.
fn scrub_target(desired: f64, side: f64, tension: f32) -> f64 {
    let reach = MAX_SCRUB * tension as f64 / 100.0;
    desired.clamp(side - reach, side + reach)
}

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
    /// On-screen turn buttons: turn the selected square by this many radians.
    Turn(f32),
    Key(KeyboardEvent),
    SetCount(String),
    TargetSide(f64),
    BandTension(f32),
    Attraction(bool),
    EdgeAttraction(f32),
    Damping(f32),
    Stiffness(f32),
    TogglePause,
    Shake,
    Reset,
    Anneal,
    Squeeze,
    SqueezeDown,
    Measure,
    Refine,
    Player(String),
    Submit,
    Submitted(Result<ScoreEntry, String>),
    Export,
    Share,
    /// Clipboard result for a share link; `Err` if it could not be copied.
    Shared(Result<(), ()>),
    ShareCreated(Result<shared::ShortShare, String>),
    CopyShare,
    ImportPick,
    ImportFile(Event),
    Imported(Result<String, String>),
    /// Remove every glue link.
    ClearGlue,
}

/// The scheduled runs that share one slot: shaking anneal or gentle squeeze.
/// The band relaxing outward before a measurement.
struct Relax {
    start_side: f64,
    elapsed: f64,
    /// Consecutive frames with nothing overlapping.
    clear_frames: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum RunKind {
    Anneal,
    Squeeze,
    SqueezeDown,
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
    sharing: bool,
    short_share: Option<String>,
    share_status: String,
    submit_after_measure: bool,
    pending_import: Option<Arrangement>,
    anneal: Option<Anneal>,
    /// Which button started the active scheduled run.
    run_kind: RunKind,
    /// Container side requested with the size slider; the band's target
    /// follows it only as far as the band pressure reaches.
    desired_side: Option<f64>,
    readout: Readout,
    /// Detects the double tap that opens the glue tool.
    taps: tap::DoubleTap,
    /// The glue tool: `None` when closed, `Some(None)` waiting for a first
    /// target, `Some(Some(a))` holding it.
    glue_tool: Option<Option<Feature>>,
    /// Whether closing the glue tool resumes the simulation it paused.
    glue_resume: bool,
    /// Set while the band relaxes out of overlap before a measurement.
    relax: Option<Relax>,
}

fn initial_side(n: u32) -> f64 {
    (n as f64).sqrt().ceil() + 0.5
}

/// Query string of `/play/:n`; `s` carries a share code.
#[derive(serde::Deserialize)]
struct PlayQuery {
    s: Option<String>,
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

/// `navigator.clipboard.writeText(text)` via `Reflect`: `cargo add` rejects
/// web-sys's `Clipboard` feature in this workspace, and the lookup also
/// covers the API being absent at runtime (e.g. an insecure context), in
/// which case the returned promise is rejected.
fn write_clipboard(window: &web_sys::Window, text: &str) -> js_sys::Promise {
    let write = || -> Option<js_sys::Promise> {
        let navigator = js_sys::Reflect::get(window, &"navigator".into()).ok()?;
        let clipboard = js_sys::Reflect::get(&navigator, &"clipboard".into()).ok()?;
        let write_text = js_sys::Reflect::get(&clipboard, &"writeText".into()).ok()?;
        let write_text = write_text.dyn_ref::<js_sys::Function>()?;
        write_text
            .call1(&clipboard, &text.into())
            .ok()?
            .dyn_into()
            .ok()
    };
    write().unwrap_or_else(|| js_sys::Promise::reject(&wasm_bindgen::JsValue::UNDEFINED))
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
            sharing: false,
            short_share: None,
            share_status: String::new(),
            submit_after_measure: false,
            pending_import: None,
            anneal: None,
            run_kind: RunKind::Anneal,
            desired_side: None,
            readout: Readout::default(),
            taps: tap::DoubleTap::default(),
            glue_tool: None,
            glue_resume: false,
            relax: None,
        };
        // A shared packing opens paused and unvalidated, even if it was
        // validated when shared; it has to be measured again here.
        let code = ctx
            .link()
            .location()
            .and_then(|l| l.query::<PlayQuery>().ok())
            .and_then(|q| q.s);
        if let Some(code) = code {
            match share::decode(&code, n) {
                Ok(a) => {
                    game.physics.load(&a);
                    game.physics.set_paused(true);
                    game.view_side.set(game.physics.side());
                    game.set_status(
                        "Shared packing loaded, not yet validated. Settle & measure to check it.",
                        false,
                    );
                }
                Err(e) => game.set_status(&format!("Share link: {e}"), true),
            }
        }
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
        // Anything that moves or reloads the scene closes the glue tool,
        // leaving the pause state to that control, so a pick never outlives
        // the targets it named. Keys decide below.
        if !matches!(
            msg,
            Msg::Frame(_)
                | Msg::Stepped
                | Msg::GpuReady
                | Msg::Records(_)
                | Msg::PointerDown(_)
                | Msg::PointerMove(_)
                | Msg::PointerUp
                | Msg::Key(_)
                | Msg::ClearGlue
                | Msg::Player(_)
                | Msg::Submitted(_)
                | Msg::Export
                | Msg::Share
                | Msg::Shared(_)
                | Msg::ShareCreated(_)
                | Msg::CopyShare
                | Msg::ImportPick
        ) {
            self.drop_glue_tool();
        }
        match msg {
            Msg::Frame(time) => {
                self.frame = None;
                let elapsed = self
                    .last_time
                    .map_or(0.0, |last| ((time - last) / 1000.0).clamp(0.0, 0.05));
                self.last_time = Some(time);
                self.tick_anneal(ctx, elapsed);
                self.tick_relax(ctx, elapsed);
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
                let reach = self.reach(&canvas, &e);
                if self.glue_tool.is_some() {
                    self.glue_tap(p, reach);
                    return true;
                }
                if self
                    .taps
                    .down(e.time_stamp(), (e.client_x() as f64, e.client_y() as f64))
                {
                    self.open_glue(p, reach);
                    return true;
                }
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
                self.taps.moved((e.client_x() as f64, e.client_y() as f64));
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
            Msg::Turn(delta) => {
                let Some(i) = self.selected else {
                    self.set_status("Tap a square first, then turn it.", false);
                    return true;
                };
                if self.busy {
                    return false;
                }
                self.stop_anneal();
                self.physics.turn(i, delta);
                self.set_pause(false);
                true
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
                if self.glue_tool.is_some() && e.key() == "Escape" {
                    e.prevent_default();
                    self.set_status("Glue cancelled.", false);
                    self.close_glue();
                    return true;
                }
                if e.code() == "Space" {
                    e.prevent_default();
                    self.drop_glue_tool();
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
                self.drop_glue_tool();
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
                // A size edit is a spring target, never a teleport. Enable
                // pressure if it was off, retaining any chosen nonzero strength.
                if params.band_tension == 0.0 {
                    params.band_tension = 30.0;
                }
                self.desired_side = Some(side);
                params.target_side = scrub_target(side, self.physics.side(), params.band_tension);
                self.physics.set_params(params);
                self.set_pause(false);
                self.refresh_readout();
                true
            }
            Msg::BandTension(tension) => {
                self.stop_anneal();
                let mut params = self.physics.params();
                params.band_tension = tension;
                if let Some(desired) = self.desired_side {
                    params.target_side = scrub_target(desired, self.physics.side(), tension);
                }
                self.physics.set_params(params);
                self.invalidate();
                self.set_pause(false);
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
            Msg::ClearGlue => {
                self.apply_glues(&[], "Glue cleared.");
                true
            }
            Msg::Reset => {
                self.stop_anneal();
                self.desired_side = None;
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
            Msg::Anneal => self.toggle_run(n, RunKind::Anneal),
            Msg::Squeeze => self.toggle_run(n, RunKind::Squeeze),
            Msg::SqueezeDown => self.toggle_run(n, RunKind::SqueezeDown),
            Msg::Measure => {
                self.stop_anneal();
                self.settle(ctx)
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
                        // A relax in progress measures (and so submits) once
                        // it clears; restarting it would drop this request.
                        if self.relax.is_none() {
                            ctx.link().send_message(Msg::Measure);
                        }
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
            Msg::Share => {
                if self.sharing {
                    return false;
                }
                // Freeze the requested snapshot, including the solver's f64
                // precision. Later edits never change what this link contains.
                let arrangement = self.share_arrangement();
                let body = shared::CreateShare {
                    n: arrangement.n,
                    code: share::encode(&arrangement),
                };
                self.sharing = true;
                self.short_share = None;
                self.share_status = "Creating a short link…".into();
                ctx.link()
                    .send_future(async move { Msg::ShareCreated(api::create_share(body).await) });
                true
            }
            Msg::ShareCreated(result) => {
                match result {
                    Ok(link) => {
                        self.short_share = Some(link.url);
                        // Process the response in the mounted component before
                        // starting a clipboard write, so unmounts discard it.
                        self.copy_share(ctx);
                    }
                    Err(error) => {
                        self.sharing = false;
                        self.share_status =
                            format!("Couldn't create a short link: {error}. Try Share again.");
                    }
                }
                true
            }
            Msg::CopyShare => {
                if !self.sharing {
                    self.sharing = true;
                    self.copy_share(ctx);
                }
                true
            }
            Msg::Shared(copied) => {
                self.sharing = false;
                self.share_status = match copied {
                    Ok(()) => "Snapshot link copied to the clipboard.".into(),
                    Err(()) => {
                        "Short link ready. Tap Copy link, or select and copy it below.".into()
                    }
                };
                true
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
                                aria-label="Square packing playfield. Drag to move. Select a square, then use arrow keys to move and Q or E to rotate. Double-click to glue two features; Escape cancels."
                                onpointerdown={link.callback(Msg::PointerDown)}
                                onpointermove={link.callback(Msg::PointerMove)}
                                onpointerup={link.callback(|_| Msg::PointerUp)}
                                onpointercancel={link.callback(|_| Msg::PointerUp)}
                                onlostpointercapture={link.callback(|_| Msg::PointerUp)}
                                onkeydown={link.callback(Msg::Key)} />
                            <div class="pg-board-footer">
                                <span class="pg-mode">{ &r.mode }</span>
                                <span class="pg-turn">
                                    <button aria-label="Turn left" onclick={link.callback(|_| Msg::Turn(TOUCH_TURN))}>{ "⟲" }</button>
                                    <button aria-label="Turn right" onclick={link.callback(|_| Msg::Turn(-TOUCH_TURN))}>{ "⟳" }</button>
                                </span>
                                { (!self.physics.glues().is_empty()).then(|| html! {
                                    <button class="pg-clear-glue" onclick={link.callback(|_| Msg::ClearGlue)}>{ "Clear glue" }</button>
                                }) }
                                <span class="pg-hint-mouse">{ "drag · wheel to rotate · shift-drag to spin · double-click to glue" }</span>
                                <span class="pg-hint-touch">{ "drag to move · tap a square, then ⟲ ⟳ to turn · double-tap to glue" }</span>
                            </div>
                        </div>
                        <p class="pg-help">{ "Force arrows: blue = net contact and edge pull · gold = mouse spring. Dashed band = target size." }</p>
                        <details class="pg-details">
                            <summary>{ "How to play & what the score means" }</summary>
                            <p>{ "Each square has side length 1. Make the container smaller while keeping every square inside and avoiding overlap. Dragging and rotating resume physics, push neighbors, and resist blocked motion. Lower the container target with outer band tension enabled to squeeze the packing. Turn on forces, or use Q/E to rotate a selected square. Arrow keys nudge it. Space pauses." }</p>
                            <p>{ "The simulation has springy contacts. “Settle & measure” first relaxes the box until nothing overlaps, then pauses the scene and refines its contacts with a numerical polynomial solver. Only an independently validated arrangement can be submitted. A best-known packing is an upper bound, not necessarily a proven optimum. A numerical match is not an exact proof." }</p>
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
                            <p class="pg-help">{ "Changing container size animates the band with live pressure. Squares push back, and higher pressure lets the size target run further ahead of the container. Zero holds the current size; moving the size slider re-engages pressure at 30." }</p>
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
                                { for [(RunKind::Anneal, "Anneal"), (RunKind::Squeeze, "Gentle squeeze"), (RunKind::SqueezeDown, "Squeeze down")].map(|(kind, label)| {
                                    let running = r.anneal.filter(|_| self.run_kind == kind);
                                    html! {
                                        <button class={classes!(running.is_some().then_some("pg-primary"))}
                                            onclick={link.callback(move |_| match kind {
                                                RunKind::Anneal => Msg::Anneal,
                                                RunKind::Squeeze => Msg::Squeeze,
                                                RunKind::SqueezeDown => Msg::SqueezeDown,
                                            })}>
                                            { match running { Some(p) => format!("Stop · {p}%"), None => label.into() } }
                                        </button>
                                    }
                                }) }
                            </div>
                        </section>
                        <section class="pg-panel pg-submit">
                            <h2>{ "Chase the record" }</h2>
                            <div class="pg-actions">
                                <button class="pg-primary" disabled={self.busy} onclick={link.callback(|_| Msg::Measure)}>
                                    { "Settle & measure" }
                                </button>
                                <button onclick={link.callback(|_| Msg::Export)}>{ "Export" }</button>
                                <button disabled={self.sharing} onclick={link.callback(|_| Msg::Share)}>{ if self.sharing { "Sharing…" } else { "Share" } }</button>
                                <button onclick={link.callback(|_| Msg::ImportPick)}>{ "Import" }</button>
                                <input ref={self.file_input.clone()} type="file" accept="application/json,.json" hidden=true
                                    onchange={link.callback(Msg::ImportFile)} />
                            </div>
                            <p class="pg-share-status pg-help" role="status" aria-live="polite">{ &self.share_status }</p>
                            { if let Some(url) = &self.short_share {
                                html! {
                                    <div class="pg-share-result">
                                        <label class="pg-help" for="pg-share-link">{ "Shared snapshot" }</label>
                                        <input id="pg-share-link" class="pg-field" type="url" readonly=true value={url.clone()}
                                            onclick={Callback::from(|e: MouseEvent| e.target_unchecked_into::<HtmlInputElement>().select())} />
                                        <button disabled={self.sharing} onclick={link.callback(|_| Msg::CopyShare)}>{ "Copy link" }</button>
                                    </div>
                                }
                            } else { Html::default() } }
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
                    // Release the band so it stops compressing once the run
                    // ends, then relax out of any overlap and measure.
                    let mut params = self.physics.params();
                    params.band_tension = 0.0;
                    self.physics.set_params(params);
                    self.settle(ctx);
                }
            }
        }
    }

    /// Start a scheduled run of `kind`, or stop it if it is the active one.
    /// Clicking the other run's button switches runs.
    fn toggle_run(&mut self, n: u32, kind: RunKind) -> bool {
        // A run takes the band over from a relax in progress.
        self.stop_relax();
        if self.anneal.is_some() {
            let same = self.run_kind == kind;
            self.stop_anneal();
            if same {
                self.refresh_readout();
                return true;
            }
        }
        if self.busy {
            return false;
        }
        // The run drives the band target itself.
        self.desired_side = None;
        let (schedule, status) = match kind {
            RunKind::Anneal => (
                Schedule::default(),
                "Annealing: gentler shakes, tighter band…",
            ),
            RunKind::Squeeze => (
                Schedule::gentle(),
                "Gently squeezing: a slow, soft band and no shakes…",
            ),
            RunKind::SqueezeDown => (
                Schedule::squeeze_down(),
                "Squeezing down: squeeze, relax, squeeze a little lower…",
            ),
        };
        let seed = (js_sys::Math::random() * u64::MAX as f64) as u64;
        let floor = (n as f64).sqrt();
        self.anneal = Some(Anneal::new(schedule, self.physics.side(), floor, seed));
        self.run_kind = kind;
        self.set_pause(false);
        self.set_status(status, false);
        self.refresh_readout();
        true
    }

    fn share_arrangement(&self) -> Arrangement {
        match &self.last_report {
            Some(r) if r.valid => r.arrangement.clone(),
            _ => self.physics.arrangement(),
        }
    }

    fn copy_share(&self, ctx: &Context<Self>) {
        let copy = web_sys::window()
            .zip(self.short_share.as_ref())
            .map(|(window, url)| write_clipboard(&window, url))
            .unwrap_or_else(|| js_sys::Promise::reject(&wasm_bindgen::JsValue::UNDEFINED));
        // Safari may reject a write after the network await loses the user
        // gesture. The visible Copy link button retries within a fresh gesture;
        // the selectable URL remains usable even without the Clipboard API.
        ctx.link().send_future(async move {
            Msg::Shared(
                wasm_bindgen_futures::JsFuture::from(copy)
                    .await
                    .map(|_| ())
                    .map_err(|_| ()),
            )
        });
    }

    /// Point the address bar at a share link for the current solution (the
    /// validated arrangement if there is one, else the live scene) without
    /// adding a history entry. Returns the path written.
    fn update_share_url(&self, n: u32) -> Option<String> {
        let arrangement = self.share_arrangement();
        let path = format!("/play/{n}?s={}", share::encode(&arrangement));
        web_sys::window()?
            .history()
            .ok()?
            .replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path))
            .ok()?;
        Some(path)
    }

    /// Manual input takes over from an annealing run.
    fn stop_anneal(&mut self) {
        if self.anneal.take().is_some() {
            // Release the band the run was tightening; a slider handler that
            // called this may set its own value right after.
            let mut params = self.physics.params();
            params.band_tension = 0.0;
            self.physics.set_params(params);
            let stopped = match self.run_kind {
                RunKind::Anneal => "Anneal stopped.",
                RunKind::Squeeze => "Gentle squeeze stopped.",
                RunKind::SqueezeDown => "Squeeze down stopped.",
            };
            self.set_status(stopped, false);
        }
        self.stop_relax();
    }

    /// Cancel a relax in progress and release the band. A submission waiting
    /// on its measurement is dropped too, so a later settle of a different
    /// scene can't submit it.
    fn stop_relax(&mut self) {
        if self.relax.take().is_some() {
            let mut params = self.physics.params();
            params.band_tension = 0.0;
            self.physics.set_params(params);
            self.submit_after_measure = false;
            self.set_status("Relaxing stopped. Settle & measure when ready.", false);
        }
    }

    /// Settle before measuring. If anything overlaps, relax the band outward
    /// until nothing does, so the measured packing doesn't pop the squares
    /// apart; a clear scene is measured right away.
    fn settle(&mut self, ctx: &Context<Self>) -> bool {
        if self.busy {
            return false;
        }
        if geometry::worst_violation(&self.physics.arrangement()) <= RELAX_TOL {
            return self.begin_measure(ctx);
        }
        self.relax = Some(Relax {
            start_side: self.physics.side(),
            elapsed: 0.0,
            clear_frames: 0,
        });
        self.desired_side = None;
        self.set_pause(false);
        self.set_status("Relaxing the box until nothing overlaps…", false);
        true
    }

    /// Grow the band's target while relaxing, and measure once the scene
    /// has stayed clear for a few frames (or the relax runs out of time).
    fn tick_relax(&mut self, ctx: &Context<Self>, dt: f64) {
        let Some(relax) = self.relax.as_mut() else {
            return;
        };
        relax.elapsed += dt;
        let clear = geometry::worst_violation(&self.physics.arrangement()) <= RELAX_TOL;
        relax.clear_frames = if clear { relax.clear_frames + 1 } else { 0 };
        let mut params = self.physics.params();
        let clear = relax.clear_frames >= RELAX_CLEAR_FRAMES;
        if clear || relax.elapsed >= RELAX_MAX_SECS {
            self.relax = None;
            // Zero tension holds the relaxed size.
            params.band_tension = 0.0;
            self.physics.set_params(params);
            if clear {
                self.begin_measure(ctx);
            } else {
                // Measuring a scene that still overlaps would pop the
                // squares apart, which is what relaxing is meant to avoid.
                self.auto_measured = true;
                self.submit_after_measure = false;
                self.set_status(
                    "Couldn't relax out of the overlap; glue or a jam is holding it. Loosen it, then Settle & measure.",
                    true,
                );
            }
            return;
        }
        params.band_tension = RELAX_TENSION;
        params.target_side = relax.start_side + RELAX_RATE * relax.elapsed;
        self.physics.set_params(params);
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
        self.apply_scrub();
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
        let glues = self.physics.glues();
        canvas::draw(
            &canvas,
            &Scene {
                glues: &glues,
                glue_tool: self.glue_tool,
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
            || self.relax.is_some()
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

    /// Picking radius in world units; a fingertip is wider than a cursor.
    fn reach(&self, canvas: &HtmlCanvasElement, e: &PointerEvent) -> f64 {
        let px = if e.pointer_type() == "touch" {
            22.0
        } else {
            12.0
        };
        canvas::px_to_world(canvas, px, self.view_side.get().max(self.physics.side()))
    }

    /// Open the glue tool on a double tap: end any drag, pause the scene,
    /// and take the target under the tap as the first pick.
    fn open_glue(&mut self, p: (f64, f64), reach: f64) {
        self.stop_anneal();
        self.selected = None;
        self.dragging = false;
        self.rotating = false;
        self.last_pointer = None;
        self.push_mouse();
        self.glue_resume = !self.physics.paused();
        self.set_pause(true);
        let bodies = self.physics.bodies();
        let side = self.physics.side();
        // On an existing link, open with nothing picked so the next tap
        // removes it rather than starting a new glue at the same spot.
        let on_link = glue::glue_at(&bodies, side, &self.physics.glues(), p, reach).is_some();
        self.glue_tool = Some(if on_link {
            None
        } else {
            glue::pick(&bodies, side, p, reach, |_| true)
        });
        self.glue_prompt();
    }

    fn glue_prompt(&mut self) {
        let prompt = if matches!(self.glue_tool, Some(Some(_))) {
            "Glue: now tap a target on another square, or a wall. Escape cancels."
        } else {
            "Glue: tap a corner, midpoint, edge, or wall. Tap a link to remove it."
        };
        self.set_status(prompt, false);
    }

    /// A tap with the glue tool open. With no first pick, tapping a link
    /// removes it; otherwise the first pick is glued to a target on another
    /// square or a wall. Another target becomes the first pick instead, and
    /// tapping empty space closes the tool.
    fn glue_tap(&mut self, p: (f64, f64), reach: f64) {
        let bodies = self.physics.bodies();
        let side = self.physics.side();
        let mut glues = self.physics.glues();
        let first = self.glue_tool.flatten();
        let second =
            first.and_then(|a| glue::pick(&bodies, side, p, reach, |f| glue::compatible(a, f)));
        let removed = first
            .is_none()
            .then(|| glue::glue_at(&bodies, side, &glues, p, reach))
            .flatten();
        if let (Some(a), Some(b)) = (first, second) {
            glues.push(Glue { a, b });
            self.apply_glues(&glues, "Glued. Double-tap to add another.");
        } else if let Some(i) = removed {
            glues.remove(i);
            self.apply_glues(&glues, "Glue removed.");
        } else if let Some(f) = glue::pick(&bodies, side, p, reach, |_| true) {
            self.glue_tool = Some(Some(f));
            self.glue_prompt();
            return;
        } else {
            self.set_status("Glue cancelled.", false);
        }
        self.close_glue();
    }

    fn apply_glues(&mut self, glues: &[Glue], done: &str) {
        match self.physics.set_glues(glues) {
            Ok(()) => self.set_status(done, false),
            Err(e) => self.set_status(&format!("Can't glue that: {e}"), true),
        }
    }

    /// Close the glue tool, resuming the simulation if opening it paused it.
    fn close_glue(&mut self) {
        let resume = self.glue_resume;
        self.drop_glue_tool();
        if resume {
            self.set_pause(false);
        }
    }

    /// Close the glue tool without touching the pause state.
    fn drop_glue_tool(&mut self) {
        self.glue_tool = None;
        self.glue_resume = false;
    }

    /// Let the band's target catch up with the requested size as far as the
    /// band pressure reaches, as the squares yield or push back.
    fn apply_scrub(&mut self) {
        let Some(desired) = self.desired_side else {
            return;
        };
        let mut params = self.physics.params();
        // Zero tension holds the current size.
        if params.band_tension == 0.0 {
            return;
        }
        let target = scrub_target(desired, self.physics.side(), params.band_tension);
        if target != params.target_side {
            params.target_side = target;
            self.physics.set_params(params);
        }
    }

    fn apply_import(&mut self, a: &Arrangement) {
        self.desired_side = None;
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
                    #[cfg(all(test, target_arch = "wasm32"))]
                    TEST_REPORT.with(|r| r.replace(Some(report.arrangement.clone())));
                    // Settle at the measured packing; don't resume squeezing.
                    self.desired_side = None;
                    // Loading clears glue; the measured packing has the same
                    // squares, so every link still applies.
                    let glues = self.physics.glues();
                    self.physics.load(&report.arrangement);
                    let _ = self.physics.set_glues(&glues);
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
        // Every settle (automatic or Settle & measure) updates the URL, so a
        // reload or copied address reproduces the current solution.
        self.update_share_url(ctx.props().n);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_reach_scales_with_band_pressure() {
        // At tension 30 the target may lead a 3.0 container by 0.3 either way.
        assert!((scrub_target(1.0, 3.0, 30.0) - 2.7).abs() < 1e-12);
        assert!((scrub_target(5.0, 3.0, 30.0) - 3.3).abs() < 1e-12);
        // Full pressure reaches a whole unit; zero pressure holds the side.
        assert!((scrub_target(1.0, 3.0, 100.0) - 2.0).abs() < 1e-12);
        assert_eq!(scrub_target(1.0, 3.0, 0.0), 3.0);
        // Within reach, the request is used as-is.
        assert_eq!(scrub_target(2.9, 3.0, 30.0), 2.9);
    }
}
