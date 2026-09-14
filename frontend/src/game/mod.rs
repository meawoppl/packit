//! The play screen: Rust physics on a canvas, plus the solver and leaderboard flow.

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests;
mod canvas;
mod files;
mod glue;
mod picker;
mod tap;
mod touch;

use crate::account::{Account, SignInAsk};
use crate::anneal::{Anneal, Command, Schedule};
use crate::benchmark::Benchmark;
use crate::{api, Route};
use canvas::Scene;
use gloo_events::{EventListener, EventListenerOptions};
use gloo_render::{request_animation_frame, AnimationFrame};
use physics::{Backend, Feature, Glue, Physics, SettlePhase, SETTLE_GLUE_ERROR};
use shared::{board, Arrangement, BoardCode, BoardLink, KnownRecord, ScoreEntry, SubmitScore};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::JsCast;
use web_sys::{
    Event, HtmlCanvasElement, HtmlElement, HtmlInputElement, KeyboardEvent, PointerEvent,
    WheelEvent,
};
use yew::context::ContextHandle;
use yew::prelude::*;
use yew_router::prelude::*;

const SHARE_NOTE: &str =
    "Sign in to share. This board is kept as it is now and shared once you sign in.";
const SUBMIT_NOTE: &str =
    "Sign in to submit. This board is kept as it is now and submitted once you sign in.";
const SUBMIT_UNCERTIFIED_NOTE: &str =
    "Sign in to submit, then press Submit packing to settle and submit.";
const EXPIRED_SHARE_NOTE: &str = "You're signed out. Sign in again to share this board.";
const EXPIRED_SUBMIT_NOTE: &str = "You're signed out. Sign in again to submit this board.";
const UNSHARED_NOTE: &str = "Not signed in, so nothing was shared.";
const UNSUBMITTED_NOTE: &str = "Not signed in, so nothing was submitted.";

#[cfg(all(test, target_arch = "wasm32"))]
thread_local! {
    /// Physics of the most recently created `Game`, so browser tests can
    /// observe the simulation behind the mounted component.
    static TEST_PHYSICS: std::cell::RefCell<Option<Physics>> = const { std::cell::RefCell::new(None) };
    /// The last validated arrangement, for exact comparisons in browser tests.
    static TEST_REPORT: std::cell::RefCell<Option<Arrangement>> = const { std::cell::RefCell::new(None) };
    /// The viewport extent the last frame was drawn at.
    static TEST_EXTENT: Cell<f64> = const { Cell::new(0.0) };
    static TEST_PAN: Cell<(f64,f64)> = const { Cell::new((0.0,0.0)) };
}

const STEP_HZ: f64 = 120.0;
const MAX_SUBSTEPS: u32 = 6;
/// Frames of near-zero motion before "Settle" runs by itself.
const SETTLE_FRAMES: u32 = 150;
const NUDGE: f32 = 0.025;
const ROTATE_STEP: f32 = 0.04;
/// Turn per tap of the on-screen turn buttons (touch has no wheel or keys).
const TOUCH_TURN: f32 = 0.12;
/// How far the effective size target may lead the actual container at full
/// band tension (100); lower tension shortens the reach proportionally.
const MAX_SCRUB: f64 = 1.0;
/// Farthest certification may nudge any square, in square widths, to clear
/// the numerical overlap a settled contact leaves. Past this the packing
/// isn't certified rather than moved.
const CERTIFY_MOVE: f64 = 1e-4;

/// Largest residual `glues` would leave on `arrangement`, measured on a
/// scratch simulation so the live one is untouched.
fn glue_error(arrangement: &Arrangement, glues: &[Glue]) -> f64 {
    let scratch = Physics::new_in(
        arrangement.shape,
        arrangement.container,
        arrangement.n,
        arrangement.side,
    );
    scratch.load(arrangement);
    match scratch.set_glues(glues) {
        Ok(()) => scratch.violations().max_glue_error,
        Err(_) => f64::INFINITY,
    }
}

/// Whether two arrangements are the same packing to within rounding, so a
/// solver result for one describes the other.
fn same_packing(a: &Arrangement, b: &Arrangement) -> bool {
    const SAME: f64 = 1e-9;
    a.container == b.container
        && a.shape == b.shape
        && (a.side - b.side).abs() <= SAME
        && a.squares.len() == b.squares.len()
        && a.squares.iter().zip(&b.squares).all(|(p, q)| {
            (p.cx - q.cx).abs() <= SAME
                && (p.cy - q.cy).abs() <= SAME
                && (p.theta - q.theta).abs() <= SAME
        })
}
/// The band's size target for a requested `desired` side: it can only run
/// ahead of the actual `side` as far as the band pressure reaches.
fn scrub_target(desired: f64, side: f64, tension: f32) -> f64 {
    let reach = MAX_SCRUB * tension as f64 / 100.0;
    desired.clamp(side - reach, side + reach)
}

#[derive(Properties, PartialEq)]
pub struct GameProps {
    #[prop_or_default]
    pub shape: shared::Shape,
    #[prop_or_default]
    pub container: shared::Shape,
    pub n: u32,
}

pub enum Msg {
    Frame(f64),
    Stepped,
    GpuReady,
    Records(Result<Vec<KnownRecord>, String>),
    PointerDown(PointerEvent),
    PointerMove(PointerEvent),
    PointerUp(PointerEvent),
    Wheel(usize, f32),
    /// On-screen turn buttons: turn the selected square by this many radians.
    Turn(f32),
    Key(KeyboardEvent),
    TargetSide(f64),
    CornerStart(u8, PointerEvent),
    CornerMove(PointerEvent),
    CornerEnd(PointerEvent, bool),
    CornerNudge,
    /// A range-stepping key went down on the Squeeze slider. Its `change`
    /// then fires on every step, so the key's release ends the squeeze
    /// instead.
    SqueezeKey,
    /// The Squeeze slider's `change`.
    SqueezeChange,
    /// The Squeeze slider let go: pointer up or cancel, a stepping key's
    /// release, or blur.
    SqueezeRelease,
    BandTension(f32),
    Attraction(bool),
    EdgeAttraction(f32),
    Damping(f32),
    Stiffness(f32),
    TogglePause,
    Shake,
    Reset,
    ResetView,
    Anneal,
    Squeeze,
    SqueezeDown,
    Measure,
    /// Load the solver's smaller packing of the certified scene.
    Tighten,
    Refine,
    Submit,
    /// A submission's result, with the board it sent.
    Submitted(BoardCode, Result<ScoreEntry, api::Failure>),
    Export,
    Share,
    /// Clipboard result for a share link; `Err` if it could not be copied.
    Shared(Result<(), ()>),
    /// A short link's result, with the board it was for.
    ShareCreated(BoardCode, Result<BoardLink, api::RetryError>),
    CopyShare,
    Account(Account),
    /// The answer to the sign-in ask with this number: whether it signed in.
    SignedIn(u32, bool),
    ImportPick,
    ImportFile(Event),
    Imported(Result<String, String>),
    /// Remove every glue link.
    ClearGlue,
}

/// A Share or Submit pressed without a session: the board, frozen as it
/// was then.
enum Pending {
    Share(BoardCode),
    Submit(BoardCode),
}

/// The scheduled runs that share one slot: shaking anneal or gentle squeeze.
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
    /// The band's target while a squeeze is held, else the live side.
    size_value: f64,
    /// The Squeeze slider's thumb: the requested side while held, else the
    /// live side, so it springs back with the box.
    squeeze: f64,
    /// The Squeeze slider's range: the usual bounds widened to take in the
    /// live side, frozen while a squeeze is held so the track doesn't move
    /// under the pointer.
    squeeze_range: (f64, f64),
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
    /// Side length the last frame was drawn at, which pointer mapping
    /// shares. It holds for a whole drag, so growth under the pointer can't
    /// rescale the view and chase it.
    extent: Rc<Cell<f64>>,
    pan: Rc<Cell<(f64, f64)>>,
    manual_view: bool,
    touch: touch::Touch,
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
    /// Credit drawn beside the best-known square.
    best_label: String,
    /// The settled packing certified where it stands: the score, and what
    /// Submit and Share send. Any change to the scene clears it.
    certified: Option<Arrangement>,
    /// A smaller valid packing the solver found for the certified one, which
    /// Tighten loads on request.
    tighter: Option<Arrangement>,
    bound: String,
    status: String,
    status_error: bool,
    auto_measured: bool,
    settled_frames: u32,
    account: Account,
    _account: ContextHandle<Account>,
    /// Sent once, and only if the sign-in it asked for succeeds. It goes
    /// with the screen when it unmounts.
    pending: Option<Pending>,
    /// Numbers sign-in asks, so an answer to a replaced one is ignored.
    ask_id: u32,
    /// Why a sign-in is needed, or what became of the request that needed it.
    sign_in_note: Option<&'static str>,
    submitting: bool,
    sharing: bool,
    short_share: Option<String>,
    share_status: String,
    /// A Share's final failure, shown as an alert until the next Share.
    share_error: Option<&'static str>,
    /// Set when the screen unmounts, so a Share stops retrying.
    unmounted: Rc<Cell<bool>>,
    submit_after_measure: bool,
    pending_import: Option<Arrangement>,
    anneal: Option<Anneal>,
    /// Which button started the active scheduled run.
    run_kind: RunKind,
    /// Container side requested with the Squeeze slider while it's held;
    /// the band's target follows it only as far as the band pressure
    /// reaches. Letting go clears it.
    desired_side: Option<f64>,
    /// Set while a range-stepping key is held on the Squeeze slider.
    squeeze_key: bool,
    corner_drag: Option<CornerDrag>,
    corner_nudge: Option<f64>,
    readout: Readout,
    /// Detects the double tap that opens the glue tool.
    taps: tap::DoubleTap,
    /// The glue tool: `None` when closed, `Some(None)` waiting for a first
    /// target, `Some(Some(a))` holding it.
    glue_tool: Option<Option<Feature>>,
    /// Whether closing the glue tool resumes the simulation it paused.
    glue_resume: bool,
    /// Set while this screen's Settle runs in the physics.
    settling: bool,
}

/// Pointer movement is measured in the view captured at press time, so a
/// moving spring boundary never changes what the same finger position requests.
struct CornerDrag {
    pointer: i32,
    element: HtmlElement,
    start: (f64, f64),
    side: f64,
    scale: f64,
    corner: u8,
    moved: bool,
}

fn corner_positions(container: shared::Shape) -> Vec<(f64, f64)> {
    if container.is_square() {
        vec![(-0.5, 0.5), (0.5, 0.5), (-0.5, -0.5), (0.5, -0.5)]
    } else {
        container
            .container_vertices(1.0)
            .into_iter()
            .map(|(x, y)| (x - 0.5, y - 0.5))
            .collect()
    }
}
fn corner_target_in(
    container: shared::Shape,
    side: f64,
    scale: f64,
    corner: u8,
    dx: f64,
    dy: f64,
) -> f64 {
    if container.is_square() {
        return corner_target(side, scale, corner, dx, dy);
    }
    let (x, y) = corner_positions(container)[corner as usize];
    (side + (dx * x - dy * y) / (scale * (x * x + y * y))).clamp(0.1, 1000.0)
}
fn corner_target(side: f64, scale: f64, corner: u8, dx: f64, dy: f64) -> f64 {
    let sx = if corner & 1 == 0 { -1.0 } else { 1.0 };
    let sy = if corner & 2 == 0 { -1.0 } else { 1.0 };
    (side + (sx * dx + sy * dy) / scale).clamp(0.5, 1000.0)
}

fn initial_side(n: u32) -> f64 {
    (n as f64).sqrt().ceil() + 0.5
}

/// Query string of `/play/:n`; `s` carries a board code.
#[derive(serde::Deserialize)]
struct PlayQuery {
    s: Option<String>,
}

fn input_value(e: &Event) -> String {
    e.target_unchecked_into::<HtmlInputElement>().value()
}

/// Keys that step a range input; any other key leaves a squeeze alone.
fn steps_range(e: &KeyboardEvent) -> bool {
    matches!(
        e.key().as_str(),
        "ArrowUp"
            | "ArrowDown"
            | "ArrowLeft"
            | "ArrowRight"
            | "PageUp"
            | "PageDown"
            | "Home"
            | "End"
    )
}

/// `element.focus({preventScroll: true})`; focusing must not scroll the page,
/// or pointer-to-world coordinates shift mid-drag. The pinned web-sys has no
/// `FocusOptions`, so the options object is built by hand.
fn focus_without_scroll(canvas: &HtmlElement) {
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
        let side = initial_side(n)
            * if ctx.props().shape.is_square() {
                1.0
            } else {
                2.0 * ctx.props().shape.radius()
            };
        let side = side
            / if ctx.props().container.is_square() {
                1.0
            } else {
                ctx.props().container.apothem() * 2.0f64.sqrt()
            };
        let physics = Physics::new_in(ctx.props().shape, ctx.props().container, n, side);
        // Pushing hard against the walls grows the box around its center,
        // which the canvas keeps fixed on screen.
        physics.set_drag_expansion(true);
        #[cfg(all(test, target_arch = "wasm32"))]
        TEST_PHYSICS.with(|p| p.replace(Some(physics.clone())));
        let gpu = physics.clone();
        ctx.link().send_future(async move {
            gpu.init_gpu().await;
            Msg::GpuReady
        });
        let shape = ctx.props().shape;
        let container = ctx.props().container;
        ctx.link().send_future(async move {
            Msg::Records(api::known_records_in(shape, container).await)
        });
        let (account, account_handle) = ctx
            .link()
            .context::<Account>(ctx.link().callback(Msg::Account))
            .expect("the play screen is inside an AccountProvider");
        let mut game = Self {
            physics,
            canvas: NodeRef::default(),
            file_input: NodeRef::default(),
            frame: None,
            wheel: None,
            view_side: Rc::new(Cell::new(side)),
            extent: Rc::new(Cell::new(side * ctx.props().container.extent())),
            pan: Rc::new(Cell::new((0.0, 0.0))),
            manual_view: false,
            touch: touch::Touch::default(),
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
            best_label: String::new(),
            certified: None,
            tighter: None,
            bound: String::new(),
            status: "Find your rhythm. Then squeeze a little.".into(),
            status_error: false,
            auto_measured: false,
            settled_frames: 0,
            account,
            _account: account_handle,
            pending: None,
            ask_id: 0,
            sign_in_note: None,
            submitting: false,
            sharing: false,
            short_share: None,
            share_status: String::new(),
            share_error: None,
            unmounted: Rc::new(Cell::new(false)),
            submit_after_measure: false,
            pending_import: None,
            anneal: None,
            run_kind: RunKind::Anneal,
            desired_side: None,
            squeeze_key: false,
            corner_drag: None,
            corner_nudge: None,
            readout: Readout::default(),
            taps: tap::DoubleTap::default(),
            glue_tool: None,
            glue_resume: false,
            settling: false,
        };
        // A board opens paused and unvalidated, even if it was validated
        // when stored; it has to be measured again here.
        let code = ctx
            .link()
            .location()
            .and_then(|l| l.query::<PlayQuery>().ok())
            .and_then(|q| q.s);
        if let Some(code) = code {
            match board::decode(&code, n) {
                Ok(board)
                    if board.arrangement.shape == ctx.props().shape
                        && board.arrangement.container == ctx.props().container =>
                {
                    // Loading clears glue, so the board's glue goes on after it.
                    game.physics.load(&board.arrangement);
                    game.physics.set_paused(true);
                    game.view_side.set(game.physics.side());
                    match game.physics.set_glues(&board.glues) {
                        Ok(()) => game.set_status(
                            "Board loaded, not yet validated. Settle to check it.",
                            false,
                        ),
                        Err(e) => game.set_status(&format!("Board glue: {e}"), true),
                    }
                }
                Ok(_) => game.set_status(
                    "This board uses a different shape. Open its matching shape first.",
                    true,
                ),
                Err(e) => game.set_status(&format!("Board link: {e}"), true),
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
            let (physics, extent, pan, link) = (
                self.physics.clone(),
                self.extent.clone(),
                self.pan.clone(),
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
                    let p = canvas::to_world(
                        &target,
                        (e.client_x() as f64, e.client_y() as f64),
                        extent.get(),
                        physics.side(),
                    );
                    let offset = pan.get();
                    let p = (p.0 + offset.0, p.1 + offset.1);
                    if let Some(i) = canvas::hit_for(physics.shape(), &physics.bodies(), p) {
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
                | Msg::PointerUp(_)
                | Msg::Key(_)
                | Msg::ClearGlue
                | Msg::Submitted(..)
                | Msg::Export
                | Msg::Share
                | Msg::Shared(_)
                | Msg::ShareCreated(..)
                | Msg::CopyShare
                | Msg::Account(_)
                | Msg::SignedIn(..)
                | Msg::ImportPick
        ) {
            self.drop_glue_tool();
        }
        // A different control takes ownership of the scene; a captured corner
        // must not revive its old request on a later pointer event.
        if (self.corner_drag.is_some() || self.corner_nudge.is_some())
            && !matches!(
                msg,
                Msg::Frame(_)
                    | Msg::Stepped
                    | Msg::GpuReady
                    | Msg::Records(_)
                    | Msg::CornerStart(..)
                    | Msg::CornerMove(_)
                    | Msg::CornerEnd(..)
                    | Msg::TargetSide(_)
            )
        {
            self.end_corner();
            self.corner_nudge = None;
            self.desired_side = None;
            let mut params = self.physics.params();
            params.band_tension = 0.0;
            self.physics.set_params(params);
        }
        if self.touch.active()
            && matches!(
                msg,
                Msg::Reset
                    | Msg::ResetView
                    | Msg::Measure
                    | Msg::Tighten
                    | Msg::TogglePause
                    | Msg::Shake
                    | Msg::Anneal
                    | Msg::Squeeze
                    | Msg::SqueezeDown
                    | Msg::Imported(_)
                    | Msg::TargetSide(_)
                    | Msg::CornerStart(..)
                    | Msg::Turn(_)
                    | Msg::Wheel(..)
            )
        {
            self.clear_touch();
            self.dragging = false;
        }
        match msg {
            Msg::ResetView => {
                self.pan.set((0.0, 0.0));
                self.manual_view = false;
                self.view_side.set(self.physics.side());
                self.draw();
                true
            }
            Msg::Frame(time) => {
                self.frame = None;
                let elapsed = self
                    .last_time
                    .map_or(0.0, |last| ((time - last) / 1000.0).clamp(0.0, 0.05));
                self.last_time = Some(time);
                // Either can change the status, which the readout doesn't track.
                let changed = self.tick_corner_nudge(elapsed)
                    | self.tick_anneal(elapsed)
                    | self.tick_settle(ctx);
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
                        return changed;
                    }
                }
                self.after_frame(ctx) || changed
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
                        if let Some(r) = &self.record {
                            self.best_label = best_label(r);
                        } else {
                            self.record_note = "No reference loaded for this game.".into();
                        }
                    }
                    Err(_) => self.record_note = "Reference records unavailable.".into(),
                }
                self.refresh_readout()
            }
            Msg::PointerDown(e) => {
                if e.pointer_type() == "touch" {
                    return self.touch_down(&e);
                }
                if self.touch.active() {
                    return false;
                }
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
                self.selected = canvas::hit_for(self.physics.shape(), &bodies, p);
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
                if e.pointer_type() == "touch" {
                    return self.touch_move(&e);
                }
                if self.touch.active() {
                    return false;
                }
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
            Msg::PointerUp(e) => {
                if e.pointer_type() == "touch" {
                    return self.touch_up(&e);
                }
                if self.touch.active() {
                    return false;
                }
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
            Msg::CornerStart(corner, e) => {
                if self.busy || self.corner_drag.is_some() || !e.is_primary() || e.button() != 0 {
                    return false;
                }
                e.prevent_default();
                let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
                    return false;
                };
                let element = e.target_unchecked_into::<HtmlElement>();
                let _ = element.set_pointer_capture(e.pointer_id());
                focus_without_scroll(&element);
                self.stop_anneal();
                if self.corner_nudge.take().is_some() {
                    self.desired_side = None;
                    let mut params = self.physics.params();
                    params.band_tension = 0.0;
                    self.physics.set_params(params);
                }
                self.dragging = false;
                self.rotating = false;
                self.push_mouse();
                self.corner_drag = Some(CornerDrag {
                    pointer: e.pointer_id(),
                    element,
                    start: (e.client_x() as f64, e.client_y() as f64),
                    side: self.physics.side(),
                    scale: canvas.get_bounding_client_rect().width() * 0.91 / self.extent.get(),
                    corner,
                    moved: false,
                });
                true
            }
            Msg::CornerMove(e) => {
                let Some(drag) = self.corner_drag.as_mut() else {
                    return false;
                };
                if drag.pointer != e.pointer_id() {
                    return false;
                }
                e.prevent_default();
                let dx = e.client_x() as f64 - drag.start.0;
                let dy = e.client_y() as f64 - drag.start.1;
                if !drag.moved && dx.hypot(dy) < 3.0 {
                    return false;
                }
                drag.moved = true;
                let target = corner_target_in(
                    self.physics.container(),
                    drag.side,
                    drag.scale,
                    drag.corner,
                    dx,
                    dy,
                );
                self.squeeze_to(target)
            }
            Msg::CornerEnd(e, cancelled) => {
                if self
                    .corner_drag
                    .as_ref()
                    .is_none_or(|d| d.pointer != e.pointer_id())
                {
                    return false;
                }
                let drag = self.end_corner().unwrap();
                if !cancelled && !drag.moved {
                    return self.nudge_corner();
                }
                self.release_squeeze();
                true
            }
            Msg::CornerNudge => {
                if self.busy {
                    return false;
                }
                self.nudge_corner()
            }
            Msg::TargetSide(side) => self.squeeze_to(side),
            Msg::SqueezeKey => {
                self.squeeze_key = true;
                false
            }
            Msg::SqueezeChange => !self.squeeze_key && self.release_squeeze(),
            Msg::SqueezeRelease => {
                self.squeeze_key = false;
                self.release_squeeze()
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
                self.pan.set((0.0, 0.0));
                self.manual_view = false;
                self.stop_anneal();
                self.desired_side = None;
                let side = initial_side(n)
                    * if ctx.props().shape.is_square() {
                        1.0
                    } else {
                        2.0 * ctx.props().shape.radius()
                    };
                let side = side
                    / if ctx.props().container.is_square() {
                        1.0
                    } else {
                        ctx.props().container.apothem() * 2.0f64.sqrt()
                    };
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
                self.settle()
            }
            Msg::Tighten => self.tighten(ctx),
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
            Msg::Submit => {
                // The board is the certified packing with the glue it has
                // now.
                let board = self.certified.as_ref().map(|a| self.board_code(a));
                if self.account.username.is_none() {
                    // Only a certified packing can be frozen; without one,
                    // Submit is pressed again after signing in.
                    match board {
                        Some(board) => {
                            self.ask_sign_in(ctx, Some(Pending::Submit(board)), SUBMIT_NOTE)
                        }
                        None => self.ask_sign_in(ctx, None, SUBMIT_UNCERTIFIED_NOTE),
                    }
                    return true;
                }
                match board {
                    Some(board) => self.send_submit(ctx, board),
                    None => {
                        self.submit_after_measure = true;
                        // A Settle in progress measures (and so submits) once
                        // it settles; restarting it would drop this request.
                        if !self.settling {
                            ctx.link().send_message(Msg::Measure);
                        }
                    }
                }
                true
            }
            Msg::Submitted(board, result) => {
                self.submitting = false;
                match result {
                    Ok(entry) => self.set_status(
                        &format!(
                            "Saved! Rank #{} for {n} {}.",
                            entry.rank,
                            ctx.props().shape.plural()
                        ),
                        false,
                    ),
                    Err(api::Failure::SignedOut(_)) => {
                        self.ask_sign_in(ctx, Some(Pending::Submit(board)), EXPIRED_SUBMIT_NOTE)
                    }
                    Err(e) => self.set_status(&e.to_string(), true),
                }
                true
            }
            Msg::Export => {
                let value = serde_json::to_value(serde_json::json!({
                    "arrangement": self.board_arrangement()
                }));
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
                // Freeze the requested board, including the solver's f64
                // precision and the glue. Later edits never change what this
                // link contains, even if it waits for a sign-in.
                let body = self.board_code(&self.board_arrangement());
                if self.account.username.is_none() {
                    self.ask_sign_in(ctx, Some(Pending::Share(body)), SHARE_NOTE);
                } else {
                    self.start_share(ctx, body);
                }
                true
            }
            Msg::ShareCreated(body, result) => {
                match result {
                    Ok(link) => {
                        self.short_share = Some(link.url);
                        // Process the response in the mounted component before
                        // starting a clipboard write, so unmounts discard it.
                        self.copy_share(ctx);
                    }
                    Err(api::RetryError::Cancelled) => return false,
                    Err(api::RetryError::SignedOut) => {
                        self.sharing = false;
                        self.share_status.clear();
                        self.ask_sign_in(ctx, Some(Pending::Share(body)), EXPIRED_SHARE_NOTE);
                    }
                    Err(api::RetryError::Failed) => {
                        self.sharing = false;
                        self.share_status.clear();
                        self.share_error = Some(
                            "Couldn't create a short link. Check your connection, then press Share to try again.",
                        );
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
            Msg::Account(account) => {
                self.account = account;
                false
            }
            Msg::SignedIn(id, signed_in) => {
                if id != self.ask_id {
                    return false;
                }
                let pending = self.pending.take();
                self.sign_in_note = match (signed_in, pending) {
                    (true, Some(Pending::Share(body))) => {
                        self.start_share(ctx, body);
                        None
                    }
                    (true, Some(Pending::Submit(arrangement))) => {
                        self.send_submit(ctx, arrangement);
                        None
                    }
                    (true, None) | (false, None) => None,
                    (false, Some(Pending::Share(_))) => Some(UNSHARED_NOTE),
                    (false, Some(Pending::Submit(_))) => Some(UNSUBMITTED_NOTE),
                };
                true
            }
            Msg::Shared(copied) => {
                self.sharing = false;
                self.share_status = match copied {
                    Ok(()) => "Board link copied to the clipboard.".into(),
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
        let status_class = classes!("pg-status", self.status_error.then_some("pg-invalid"));
        // Compare the refined f64 side once validated; otherwise the live side.
        let (bench_side, validated) = match &self.certified {
            Some(arrangement) => (arrangement.side, true),
            None => (self.physics.side(), false),
        };
        html! {
            <div class="packing-game">
                <div class="pg-layout">
                    <div class="pg-stage">
                <div class="pg-top">
                    <Benchmark
                        side={bench_side}
                        reference_side={self.record.as_ref().map(|r| r.side)}
                        proven={self.record.as_ref().is_some_and(|r| r.proven_optimal)}
                        {validated} />

                </div>

                        <div class="pg-board">
                            <div class="pg-canvas-frame" style={self.manual_view.then_some("overflow:clip")}>
                            <canvas ref={self.canvas.clone()} tabindex="0"
                                aria-label={format!("{} packing playfield. Drag to move. Select a piece, then use arrow keys to move and Q or E to rotate. Double-click to glue two features; Escape cancels.",ctx.props().shape.plural())}
                                onpointerdown={link.callback(Msg::PointerDown)}
                                onpointermove={link.callback(Msg::PointerMove)}
                                onpointerup={link.callback(Msg::PointerUp)}
                                onpointercancel={link.callback(Msg::PointerUp)}
                                onlostpointercapture={link.callback(Msg::PointerUp)}
                                onkeydown={link.callback(Msg::Key)} />
                            { for corner_positions(self.physics.container()).into_iter().enumerate().map(|(corner, (px,py))| {
                                let name = if self.physics.container().is_square() { ["top left","top right","bottom left","bottom right"][corner].to_string() } else { format!("{}",corner+1) };
                                let x = 50.0 + 91.0 * (px * self.physics.side()-self.pan.get().0) / self.extent.get();
                                let y = 50.0 - 91.0 * (py * self.physics.side()-self.pan.get().1) / self.extent.get();
                                let angle = py.atan2(px).to_degrees();
                                let corner = corner as u8;
                                html! { <button class="pg-corner" disabled={self.busy}
                                    aria-label={format!("Squeeze {name} corner")}
                                    title="Drag inward to squeeze; release to settle. Tap to nudge."
                                    style={format!("left:{x}%;top:{y}%")}
                                    onpointerdown={link.callback(move |e| Msg::CornerStart(corner, e))}
                                    onpointermove={link.callback(Msg::CornerMove)}
                                    onpointerup={link.callback(|e| Msg::CornerEnd(e, false))}
                                    onpointercancel={link.callback(|e| Msg::CornerEnd(e, true))}
                                    onlostpointercapture={link.callback(|e| Msg::CornerEnd(e, true))}
                                    onclick={link.batch_callback(|e: MouseEvent| (e.detail() == 0).then_some(Msg::CornerNudge))}>
                                    <span style={format!("display:block;transform:rotate({}deg)",-angle)}>{"←"}</span>
                                </button> }
                            }) }
                            </div>
                            <div class="pg-board-footer">
                                <span class="pg-corner-hint">{ "Drag a corner inward · release to settle" }</span>
                                <span class="pg-turn">
                                    <button aria-label="Turn left" onclick={link.callback(|_| Msg::Turn(TOUCH_TURN))}>{ "⟲" }</button>
                                    <button aria-label="Turn right" onclick={link.callback(|_| Msg::Turn(-TOUCH_TURN))}>{ "⟳" }</button>
                                </span>
                            </div>
                        </div>
                    </div>
                    <aside class="pg-sidebar">
                        <section class="pg-panel pg-submit">
                    <picker::Picker n={n} shape={ctx.props().shape} container={ctx.props().container} />
                            <h2>{ "Your packing" }</h2>
                            <div class="pg-stat">{ &r.side }<small>{ " side length" }</small></div>
                            <div class="pg-meter"><span style={format!("width: {}", r.meter)}></span></div>
                            <div class="pg-row"><span>{ "Area filled" }</span><strong>{ &r.density }</strong></div>
                            <div class="pg-help">{ &r.record }</div>
                            <div class="pg-actions">
                                <button class="pg-primary" disabled={self.busy} onclick={link.callback(|_| Msg::Measure)}>
                                    { if self.settling { "Settling…" } else { "Settle" } }
                                </button>
                                { self.tighter.as_ref().map(|t| html! {
                                    <button disabled={self.busy} onclick={link.callback(|_| Msg::Tighten)}>
                                        { format!("Tighten to {:.4}", t.side) }
                                    </button>
                                }) }
                                <button disabled={self.sharing} onclick={link.callback(|_| Msg::Share)}>{ if self.sharing { "Sharing…" } else { "Share" } }</button>
                            </div>
                            <p class="pg-share-status pg-help" role="status" aria-live="polite">{ &self.share_status }</p>
                            { self.share_error.map(|error| html! { <p class="pg-share-error" role="alert">{ error }</p> }) }
                            { self.sign_in_note.map(|note| html! { <p class="pg-sign-in" role="alert">{ note }</p> }) }
                            { if let Some(url) = &self.short_share {
                                html! {
                                    <div class="pg-share-result">
                                        <label class="pg-help" for="pg-share-link">{ "Board link" }</label>
                                        <input id="pg-share-link" class="pg-field" type="url" readonly=true value={url.clone()}
                                            onclick={Callback::from(|e: MouseEvent| e.target_unchecked_into::<HtmlInputElement>().select())} />
                                        <button disabled={self.sharing} onclick={link.callback(|_| Msg::CopyShare)}>{ "Copy link" }</button>
                                    </div>
                                }
                            } else { Html::default() } }
                            <p class="pg-help">{ &self.bound }</p>
                            <p class={status_class} role="status" aria-live="polite">{ &self.status }</p>
                            <button class="pg-primary pg-wide" disabled={self.submitting} onclick={link.callback(|_| Msg::Submit)}>
                                { "Submit packing" }
                            </button>
                            <p class="pg-help">
                                <Link<Route> to={Route::leaderboard_in(ctx.props().shape,ctx.props().container,n)}>{ "See the leaderboard ↗" }</Link<Route>>
                            </p>
                        </section>
                        <details class="pg-panel pg-advanced">
                            <summary>{ "Advanced" }</summary>
                            <div class="pg-squeeze">
                                <label class="pg-row" for="pg-size" title="Hold to squeeze; release to settle">
                                    { "Squeeze " }<output>{ format!("{:.3}", r.size_value) }</output>
                                </label>
                                <input id="pg-size" type="range" min={r.squeeze_range.0.to_string()} max={r.squeeze_range.1.to_string()} step="0.001"
                                    value={r.squeeze.to_string()}
                                    oninput={link.callback(|e: InputEvent| Msg::TargetSide(input_value(&e).parse().unwrap_or(1.0)))}
                                    onchange={link.callback(|_| Msg::SqueezeChange)}
                                    onpointerup={link.callback(|_| Msg::SqueezeRelease)}
                                    onpointercancel={link.callback(|_| Msg::SqueezeRelease)}
                                    onkeydown={link.batch_callback(|e: KeyboardEvent| steps_range(&e).then_some(Msg::SqueezeKey))}
                                    onkeyup={link.batch_callback(|e: KeyboardEvent| steps_range(&e).then_some(Msg::SqueezeRelease))}
                                    onblur={link.callback(|_| Msg::SqueezeRelease)} />
                            </div>

                            <label class="pg-row" for="pg-band">
                                { "Outer band tension " }
                                <output>{ if params.band_tension > 0.0 { format!("{}", params.band_tension) } else { "Off".into() } }</output>
                            </label>
                            <input id="pg-band" type="range" min="0" max="100" step="1" value={params.band_tension.to_string()}
                                oninput={link.callback(|e: InputEvent| Msg::BandTension(input_value(&e).parse().unwrap_or(0.0)))} />
                            <p class="pg-help">{ "Squeezing animates the band with live pressure. Squares push back, and higher pressure lets the size target run further ahead of the container. Zero holds the current size. The Squeeze slider engages pressure at 30 if it's off, keeps a pressure set here while held, and releases the band when let go." }</p>
                            <label class="pg-row">
                                { "Piece attraction " }
                                <input type="checkbox" checked={params.attraction}
                                    onchange={link.callback(|e: Event| Msg::Attraction(e.target_unchecked_into::<HtmlInputElement>().checked()))} />
                            </label>
                            <label class="pg-row" for="pg-edge-attraction">
                                { "Edge attraction " }
                                <output>{ if params.edge_attraction > 0.0 { format!("{}",params.edge_attraction) } else { "Off".into() } }</output>
                            </label>
                            <input id="pg-edge-attraction" disabled={!(ctx.props().shape.is_square() && ctx.props().container.is_square())} type="range" min="0" max="40" step="1" value={params.edge_attraction.to_string()}
                                oninput={link.callback(|e: InputEvent| Msg::EdgeAttraction(input_value(&e).parse().unwrap_or(0.0)))} />
                            <p class="pg-help">{ if ctx.props().shape.is_square() && ctx.props().container.is_square() { "Pull nearby facing edges together and turn them toward a flush fit, including the outer band. Zero turns it off." } else { "Optional edge attraction is available for squares in a square. Polygon contacts and glue still apply." } }</p>
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
                                <button onclick={link.callback(|_| Msg::ResetView)}>{ "Fit board" }</button>
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
                            <div class="pg-actions">
                                <button onclick={link.callback(|_| Msg::Export)}>{ "Export" }</button>
                                <button onclick={link.callback(|_| Msg::ImportPick)}>{ "Import" }</button>
                                <input ref={self.file_input.clone()} type="file" accept="application/json,.json" hidden=true
                                    onchange={link.callback(Msg::ImportFile)} />
                            </div>
                        </details>
                        <section class="pg-instructions">
                            <div class="pg-board-notes">
                                <span class="pg-mode">{ &r.mode }</span>
                                { (!self.physics.glues().is_empty()).then(|| html! {
                                    <button class="pg-clear-glue" onclick={link.callback(|_| Msg::ClearGlue)}>{ "Clear glue" }</button>
                                }) }
                                <span class="pg-hint-mouse">{ "drag · wheel to rotate · shift-drag to spin · double-click to glue" }</span>
                                <span class="pg-hint-touch">{ "drag pieces · two fingers on a piece to turn · pinch/pan empty space · double-tap to glue" }</span>
                            </div>
                        <p class="pg-help">{ "Force arrows: blue = net contact and edge pull · gold = mouse spring. Dashed band = target size." }</p>
                        <details class="pg-details">
                            <summary>{ "How to play & what the score means" }</summary>
                            <p>{ "Each piece has edge length 1. Make the container smaller while keeping every piece inside and avoiding overlap. Dragging and rotating resume physics, push neighbors, and resist blocked motion. Drag a corner inward to squeeze the packing; let go and the box springs back out from the squares' pressure until nothing overlaps, then settles. Turn on forces, or use Q/E to rotate a selected square. Arrow keys nudge it. Space pauses." }</p>
                            <p>{ "The simulation has springy contacts. “Settle” lets the contacts resolve, opening the box only while squares still overlap, then pauses the scene and checks its geometry in place. Only an independently validated arrangement can be submitted. A best-known packing is an upper bound, not necessarily a proven optimum. A numerical match is not an exact proof." }</p>
                            <p>
                                <a href={match ctx.props().shape { shared::Shape::Square => "https://kingbird.myphotos.cc/packing/squares_in_squares.html", shared::Shape::Triangle => "https://erich-friedman.github.io/packing/triinsqu/", shared::Shape::Pentagon => "https://erich-friedman.github.io/packing/peninsqu/", shared::Shape::Hexagon => "https://erich-friedman.github.io/packing/hexinsqu/" }} target="_blank" rel="noopener">
                                    { "Explore the research records ↗" }
                                </a>
                            </p>
                        </details>
                        </section>
                    </aside>
                </div>
            </div>
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.unmounted.set(true);
        self.physics.dispose();
    }
}

impl Game {
    fn nudge_corner(&mut self) -> bool {
        self.squeeze_to(
            (self.physics.side() - 0.05)
                .max(self.physics.container().area_bound(self.physics.shape(), 1)),
        );
        self.corner_nudge = Some(0.25);
        true
    }

    fn tick_corner_nudge(&mut self, elapsed: f64) -> bool {
        let Some(remaining) = self.corner_nudge.as_mut() else {
            return false;
        };
        *remaining -= elapsed;
        if *remaining > 0.0 {
            return false;
        }
        self.corner_nudge = None;
        self.release_squeeze()
    }

    fn squeeze_to(&mut self, side: f64) -> bool {
        let side = side.max(self.physics.container().area_bound(self.physics.shape(), 1));
        self.corner_nudge = None;
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
    fn end_corner(&mut self) -> Option<CornerDrag> {
        let drag = self.corner_drag.take()?;
        let _ = drag.element.release_pointer_capture(drag.pointer);
        Some(drag)
    }

    /// Advance an annealing run by `dt` seconds and apply its commands.
    /// Returns whether the run ended in a settle, which sets the status.
    fn tick_anneal(&mut self, dt: f64) -> bool {
        let Some(anneal) = self.anneal.as_mut() else {
            return false;
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
                    // The run ends here: a later band update would cancel
                    // the settle. Release the band, then settle and measure.
                    self.anneal = None;
                    let mut params = self.physics.params();
                    params.band_tension = 0.0;
                    self.physics.set_params(params);
                    self.settle();
                    return true;
                }
            }
        }
        false
    }

    /// Start a scheduled run of `kind`, or stop it if it is the active one.
    /// Clicking the other run's button switches runs.
    fn toggle_run(&mut self, n: u32, kind: RunKind) -> bool {
        // A run takes over from a Settle in progress.
        self.stop_settle();
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
        let floor = self.physics.container().area_bound(self.physics.shape(), n);
        self.anneal = Some(Anneal::new(schedule, self.physics.side(), floor, seed));
        self.run_kind = kind;
        self.set_pause(false);
        self.set_status(status, false);
        self.refresh_readout();
        true
    }

    /// The current solution: the certified arrangement if there is one,
    /// else the live scene.
    fn board_arrangement(&self) -> Arrangement {
        self.certified
            .clone()
            .unwrap_or_else(|| self.physics.arrangement())
    }

    /// `arrangement` with the scene's glue as it is now, as a board code.
    fn board_code(&self, arrangement: &Arrangement) -> BoardCode {
        BoardCode {
            n: arrangement.n,
            code: board::encode(arrangement, &self.physics.glues()),
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
        self.stop_settle();
    }

    /// Cancel a Settle in progress. A submission waiting on its measurement
    /// is dropped too, so a later settle of a different scene can't submit it.
    fn stop_settle(&mut self) {
        if std::mem::take(&mut self.settling) {
            self.physics.cancel_settle();
            self.submit_after_measure = false;
            self.set_status("Settle stopped. Press Settle when ready.", false);
        }
    }

    /// Settle: the physics lets the constraints resolve (opening the box
    /// only while squares still overlap), then the settled pose is measured.
    fn settle(&mut self) -> bool {
        if self.busy {
            return false;
        }
        self.clear_touch();
        // The physics drops mouse and rotation input; end the drag here too.
        // The selection stays, so keys and turn buttons still act on it.
        self.dragging = false;
        self.rotating = false;
        self.last_pointer = None;
        self.desired_side = None;
        self.physics.begin_settle();
        self.settling = true;
        self.invalidate();
        self.set_status("Settling…", false);
        true
    }

    /// Let go of the Squeeze slider: release the band and Settle, so the box
    /// springs back out from the squares' pressure until nothing overlaps.
    /// Only a squeeze that moved the slider is released, and only once.
    fn release_squeeze(&mut self) -> bool {
        // Cleared before settling, so the scrub can't drive the band again.
        if self.desired_side.take().is_none() {
            return false;
        }
        let mut params = self.physics.params();
        params.band_tension = 0.0;
        self.physics.set_params(params);
        self.settle();
        true
    }

    /// Follow the physics Settle each frame: measure once it settles, and
    /// explain when it can't. Returns whether it changed the status.
    fn tick_settle(&mut self, ctx: &Context<Self>) -> bool {
        if !self.settling {
            return false;
        }
        let status = self.physics.settle_status();
        match status.phase {
            SettlePhase::Running => return false,
            // Direct interaction cancelled it.
            SettlePhase::Idle => {
                self.settling = false;
                self.set_status("Settle stopped. Press Settle when ready.", false);
            }
            SettlePhase::Settled => {
                self.settling = false;
                self.begin_measure(ctx);
            }
            SettlePhase::Blocked | SettlePhase::TimedOut => {
                self.settling = false;
                self.auto_measured = true;
                self.submit_after_measure = false;
                let why = if status.max_glue_error > SETTLE_GLUE_ERROR {
                    "some glue can't be satisfied"
                } else if status.phase == SettlePhase::TimedOut {
                    "the squares kept moving"
                } else {
                    "squares still overlap"
                };
                self.set_status(
                    &format!(
                        "Couldn't settle: {why}. Adjust the packing, then press Settle again."
                    ),
                    true,
                );
            }
        }
        true
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
        // Hold the scale during a drag: a refit would move the pointer's
        // world position outward and the box would chase it. The box may
        // outgrow the view until the square is let go.
        if !self.manual_view && !self.dragging && self.corner_drag.is_none() {
            if extent > self.view_side.get() {
                self.view_side
                    .set(self.view_side.get() + (extent - self.view_side.get()) * 0.12);
            }
            self.extent.set(
                self.view_side.get().max(self.physics.side()) * self.physics.container().extent(),
            );
        }
        #[cfg(all(test, target_arch = "wasm32"))]
        {
            TEST_EXTENT.with(|e| e.set(self.extent.get()));
            TEST_PAN.with(|p| p.set(self.pan.get()));
        }
        let forces = self.physics.contact_forces();
        let show_forces = self.dragging
            || self.rotating
            || (self.selected.is_some() && self.physics.motion() > 0.002 * bodies.len() as f32);
        let glues = self.physics.glues();
        let violations = self.physics.violations();
        let now_ms = web_sys::window()
            .and_then(|w| w.performance())
            .map_or(0.0, |p| p.now());
        canvas::draw(
            &canvas,
            &Scene {
                shape: self.physics.shape(),
                container: self.physics.container(),
                glues: &glues,
                glue_tool: self.glue_tool,
                bodies: &bodies,
                side: self.physics.side(),
                view_side: self.extent.get(),
                pan: self.pan.get(),
                band_on: params.band_tension > 0.0,
                band_tension: params.band_tension,
                target_side: params.target_side,
                forces: show_forces.then_some(&forces),
                mouse_force: self.physics.mouse_force(),
                selected: self.selected,
                tether: (self.dragging && !self.touch.active()).then_some(self.mouse),
                violations: &violations,
                now_ms,
                best: self
                    .record
                    .as_ref()
                    .filter(|_| self.certified.is_some())
                    .map(|r| (r.side, self.best_label.as_str())),
            },
        );
    }

    fn check_settled(&mut self, ctx: &Context<Self>) {
        // An annealing run measures on its own schedule, and a held squeeze
        // when it's let go.
        if self.physics.paused()
            || self.dragging
            || self.corner_drag.is_some()
            || self.corner_nudge.is_some()
            || self.busy
            || self.auto_measured
            || self.anneal.is_some()
            || self.settling
            || self.desired_side.is_some()
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
        let density = n * self.physics.shape().area()
            / (side * side * self.physics.container().area())
            * 100.0;
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
            size_value: if self.desired_side.is_some() {
                params.target_side
            } else {
                side
            },
            squeeze: self.desired_side.unwrap_or(side),
            squeeze_range: if self.desired_side.is_some() {
                self.readout.squeeze_range
            } else {
                // Loaded scenes may sit outside the usual bounds.
                let low = (self
                    .physics
                    .container()
                    .area_bound(self.physics.shape(), n as u32)
                    * 1000.0)
                    .ceil()
                    / 1000.0;
                (low.min(side), (n.sqrt().ceil() + 3.0).max(side))
            },
            anneal: self.anneal.as_ref().map(|a| (a.progress() * 100.0) as u32),
        };
        let changed = next != self.readout;
        self.readout = next;
        changed
    }

    fn world(&self, canvas: &HtmlCanvasElement, e: &PointerEvent) -> (f64, f64) {
        self.client_world(canvas, (e.client_x() as f64, e.client_y() as f64))
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
        self.certified = None;
        self.tighter = None;
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
        canvas::px_to_world(canvas, px, self.extent.get())
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
        let on_link = glue::glue_at(
            self.physics.shape(),
            self.physics.container(),
            &bodies,
            side,
            &self.physics.glues(),
            p,
            reach,
        )
        .is_some();
        self.glue_tool = Some(if on_link {
            None
        } else {
            glue::pick(
                self.physics.shape(),
                self.physics.container(),
                &bodies,
                side,
                p,
                reach,
                |_| true,
            )
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
        let second = first.and_then(|a| {
            glue::pick(
                self.physics.shape(),
                self.physics.container(),
                &bodies,
                side,
                p,
                reach,
                |f| glue::compatible(a, f),
            )
        });
        let removed = first
            .is_none()
            .then(|| {
                glue::glue_at(
                    self.physics.shape(),
                    self.physics.container(),
                    &bodies,
                    side,
                    &glues,
                    p,
                    reach,
                )
            })
            .flatten();
        if let (Some(a), Some(b)) = (first, second) {
            glues.push(Glue { a, b });
            self.apply_glues(&glues, "Glued. Double-tap to add another.");
        } else if let Some(i) = removed {
            glues.remove(i);
            self.apply_glues(&glues, "Glue removed.");
        } else if let Some(f) = glue::pick(
            self.physics.shape(),
            self.physics.container(),
            &bodies,
            side,
            p,
            reach,
            |_| true,
        ) {
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
        if a.shape != self.physics.shape() || a.container != self.physics.container() {
            self.set_status(
                "Import uses a different shape. Choose that shape first.",
                true,
            );
            return;
        }
        self.clear_touch();
        self.pan.set((0.0, 0.0));
        self.manual_view = false;
        self.desired_side = None;
        self.physics.load(a);
        self.view_side.set(self.physics.side());
        self.invalidate();
        self.set_status("Imported. Settle to validate.", false);
        // The Squeeze slider shows the new side with the status, not a frame later.
        self.refresh_readout();
    }

    /// Certify the settled packing where it stands: that's the score, and
    /// what Submit and Share send. The solver then reports how it compares,
    /// but its own packing is never loaded or credited to this one.
    fn refine(&mut self, ctx: &Context<Self>) {
        let submit = std::mem::take(&mut self.submit_after_measure);
        self.busy = false;
        self.tighter = None;
        let live = self.physics.arrangement();
        let glues = self.physics.glues();
        let Some((certified, moved)) = self.certify(&live, &glues) else {
            self.certified = None;
            self.set_status(
                "Couldn't certify: squares still overlap. Adjust the packing, then press Settle again.",
                true,
            );
            return;
        };
        #[cfg(all(test, target_arch = "wasm32"))]
        TEST_REPORT.with(|r| r.replace(Some(certified.clone())));
        // Stay at the certified packing; don't resume squeezing.
        self.desired_side = None;
        let side = certified.side;
        let mut status = format!("Ready · {side:.9} side");
        if moved > 0.0 {
            status += &format!(" · nudged {moved:.1e} to clear contacts");
        }
        status += ".";
        match solver::refine(&certified) {
            Ok(report) => {
                self.bound = format!(
                    "Lower bound {:.6} · gap {:.3}%",
                    report.lower_bound,
                    100.0 * (side / report.lower_bound - 1.0)
                );
                // An exact form or residual describes the solver's packing,
                // so it's shown only when that's the one displayed.
                if report.valid && same_packing(&report.arrangement, &certified) {
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
                    status += &format!(
                        " Contact residual {:.1e}. {exact}",
                        report.contacts.max_residual
                    );
                } else if report.valid && report.arrangement.side < side - 1e-6 {
                    status += &format!(
                        " The solver can tighten this to {:.6}. Press Tighten to use it.",
                        report.arrangement.side
                    );
                    self.tighter = Some(report.arrangement);
                }
            }
            Err(_) => self.bound.clear(),
        }
        self.set_status(&status, false);
        if submit {
            let board = self.board_code(&certified);
            self.send_submit(ctx, board);
        }
        self.certified = Some(certified);
    }

    /// `live` certified where it stands, and the farthest any square was
    /// nudged to clear the settle's numerical contact overlap. Every glue
    /// must still hold on it, which is checked before the live scene is
    /// touched; only a nudged packing is loaded.
    fn certify(&mut self, live: &Arrangement, glues: &[Glue]) -> Option<(Arrangement, f64)> {
        let (certified, moved) =
            shared::geometry::certify(live, shared::VALIDATION_TOL, CERTIFY_MOVE)?;
        if glue_error(&certified, glues) > SETTLE_GLUE_ERROR {
            return None;
        }
        if moved > 0.0 {
            // Loading clears glue; the nudged packing has the same squares.
            self.physics.load(&certified);
            let _ = self.physics.set_glues(glues);
            self.physics.set_paused(true);
        }
        Some((certified, moved))
    }

    /// On request, jump to the solver's smaller packing and certify it like
    /// any other, unless the jump would break a glue link, in which case
    /// the live scene is never touched.
    fn tighten(&mut self, ctx: &Context<Self>) -> bool {
        let Some(tighter) = self.tighter.take() else {
            return false;
        };
        let glues = self.physics.glues();
        if glue_error(&tighter, &glues) > SETTLE_GLUE_ERROR {
            self.set_status(
                "Tightening would break a glue link, so the packing stays as it is.",
                true,
            );
            return true;
        }
        // Loading clears glue; the smaller packing has the same squares.
        self.physics.load(&tighter);
        let _ = self.physics.set_glues(&glues);
        self.physics.set_paused(true);
        self.certified = None;
        self.begin_measure(ctx)
    }

    /// Submit the frozen `board` for the signed-in account, which names it.
    fn send_submit(&mut self, ctx: &Context<Self>, board: BoardCode) {
        self.submitting = true;
        self.sign_in_note = None;
        ctx.link().send_future(async move {
            let result = api::submit_score(SubmitScore {
                board: board.clone(),
            })
            .await;
            Msg::Submitted(board, result)
        });
    }

    /// Create a short link for the frozen `body`, retrying as the policy
    /// allows. Every attempt sends the same code.
    fn start_share(&mut self, ctx: &Context<Self>, body: BoardCode) {
        self.sharing = true;
        self.short_share = None;
        self.share_error = None;
        self.sign_in_note = None;
        self.share_status = "Creating a short link…".into();
        let unmounted = self.unmounted.clone();
        ctx.link().send_future(async move {
            let result = api::save_board(body.clone(), move || unmounted.get()).await;
            Msg::ShareCreated(body, result)
        });
    }

    /// Hold `pending` and ask the account for a sign-in. The request goes
    /// out only when this ask is answered yes; a newer ask replaces it.
    fn ask_sign_in(&mut self, ctx: &Context<Self>, pending: Option<Pending>, note: &'static str) {
        self.ask_id += 1;
        let id = self.ask_id;
        self.pending = pending;
        self.sign_in_note = Some(note);
        self.account.ask.emit(SignInAsk {
            reason: note,
            done: ctx
                .link()
                .callback(move |signed_in| Msg::SignedIn(id, signed_in)),
        });
    }
}

/// "Best known 3.7071 · found by … · proved optimal by …", naming everyone
/// the sources credit, and every candidate when they disagree.
fn best_label(r: &KnownRecord) -> String {
    let mut label = format!("Best known {:.4}", r.side);
    if !r.packing_by.is_empty() {
        let join = if r.packing_disputed { " or " } else { ", " };
        label += &format!(" · found by {}", r.packing_by.join(join));
    }
    if r.proof_trivial {
        label += " · optimal";
    } else if !r.proof_by.is_empty() {
        label += &format!(" · proved optimal by {}", r.proof_by.join(", "));
    } else if r.proven_optimal {
        label += " · proved optimal";
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glue_error_is_measured_on_the_candidate() {
        let sq = |cx| shared::Placement {
            cx,
            cy: 0.5,
            theta: 0.0,
        };
        let touching = Arrangement {
            container: shared::Shape::Square,
            shape: shared::Shape::Square,
            n: 2,
            side: 2.0,
            squares: vec![sq(0.5), sq(1.5)],
        };
        // Square 1's right edge on square 2's left edge.
        let glue = Glue {
            a: Feature::Edge { square: 0, edge: 0 },
            b: Feature::Edge { square: 1, edge: 2 },
        };
        assert!(glue_error(&touching, &[glue]) < 1e-6);
        assert_eq!(glue_error(&touching, &[]), 0.0);
        let apart = Arrangement {
            container: shared::Shape::Square,
            shape: shared::Shape::Square,
            side: 3.0,
            squares: vec![sq(0.5), sq(2.5)],
            ..touching
        };
        assert!(glue_error(&apart, &[glue]) > 0.9);
    }

    #[test]
    fn same_packing_allows_only_rounding() {
        let sq = |cx| shared::Placement {
            cx,
            cy: 0.5,
            theta: 0.0,
        };
        let a = Arrangement {
            container: shared::Shape::Square,
            shape: shared::Shape::Square,
            n: 2,
            side: 2.0,
            squares: vec![sq(0.5), sq(1.5)],
        };
        let mut b = a.clone();
        b.squares[1].cx += 1e-12;
        assert!(same_packing(&a, &b));
        b.squares[1].cx += 1e-6;
        assert!(!same_packing(&a, &b), "a slid square");
        let mut c = a.clone();
        c.side -= 1e-6;
        assert!(!same_packing(&a, &c), "a tighter box");
    }

    #[test]
    fn best_label_credits_finders_and_provers() {
        let record = |packing_by: &[&str], disputed, proof_by: &[&str], trivial| KnownRecord {
            n: 10,
            side: 3.0 + std::f64::consts::FRAC_1_SQRT_2,
            side_expr: None,
            proven_optimal: trivial || !proof_by.is_empty(),
            source: String::new(),
            packing_by: packing_by.iter().map(|s| s.to_string()).collect(),
            packing_disputed: disputed,
            proof_by: proof_by.iter().map(|s| s.to_string()).collect(),
            proof_trivial: trivial,
        };
        assert_eq!(
            best_label(&record(
                &["Frits Göbel"],
                false,
                &["Walter Stromquist"],
                false
            )),
            "Best known 3.7071 · found by Frits Göbel · proved optimal by Walter Stromquist"
        );
        assert_eq!(
            best_label(&record(
                &["Evert Stenlund", "Frits Göbel"],
                true,
                &[],
                false
            )),
            "Best known 3.7071 · found by Evert Stenlund or Frits Göbel"
        );
        assert_eq!(
            best_label(&record(&[], false, &[], true)),
            "Best known 3.7071 · optimal"
        );
    }

    #[test]
    fn each_corner_projects_inward_outward_and_tangential_motion() {
        for corner in 0..4 {
            let x = if corner & 1 == 0 { 10.0 } else { -10.0 };
            let y = if corner & 2 == 0 { 10.0 } else { -10.0 };
            assert_eq!(corner_target(3.0, 100.0, corner, x, y), 2.8);
            assert_eq!(corner_target(3.0, 100.0, corner, -x, -y), 3.2);
            assert_eq!(corner_target(3.0, 100.0, corner, x, -y), 3.0);
        }
    }

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
