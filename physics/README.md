# Rust browser physics

`Physics` is a cloneable `Rc<RefCell<_>>` simulation handle. It runs immediately
on the Rust CPU fallback and can asynchronously initialize WebGPU through
`wgpu`. `kernel.wgsl` computes oriented-square SAT contacts, damping, gravity,
attraction, and a bounded mouse spring. The CPU implementation uses the same
model. Both use 120 Hz substeps, capped at six per frame, for up to 100 squares.
Contacts are stiff penalty springs, intentionally not exact constraints.
Off-center collisions and wall corners impart torque using unit-square inverse
inertia 6. Face contacts distribute pressure and damping over their contact
patch so near-flat resting faces do not jitter between support corners.

`edge_attraction` (0–40, default 0) pulls nearby facing edge midpoints toward
flush alignment and applies an aligning torque. Square pairs and all four walls
use the same 0.75-unit range and falloff. It acts across gaps; penalty contacts
handle overlaps. Attraction to moving walls also pulls on the outer band.
`shake_scaled(seed, strength)` scales a deterministic shake from zero to full
strength; `shake(seed)` is the full-strength version.

Dragging uses a spring capped at 40 force units, so neighbors push back even
when the pointer is far away. Mouse-target updates never increment the body
revision. Direct edits increment it; completed GPU readbacks preserve edited
components while merging motion into unchanged components. No state borrow
survives an await. Concurrent steps are ignored, cancelled mappings unmap their
buffers, and device loss or unavailable WebGPU selects CPU fallback. Call
`dispose` at UI teardown; dropping the final GPU handle also destroys its device.

Wheel, Q/E, and shift-drag use `turn(index, delta)` rather than pose edits.
It applies a transient angular spring (60 gain, 8 damping, acceleration capped
at 30 rad/s²) for half a second after the latest input. Target lead is bounded
to 0.35 radians to prevent wind-up when wedged. Contacts can resist and transfer
the turn; inputs never increment the body revision. Pause, reset, load, and
dispose clear the spring.

Outer band tension couples the top/right walls to one side-length spring;
bottom/left edges stay anchored. `target_side` sets its rest length; contact
pressure can expand it. Zero tension fixes the current side. The scalar band
coordinate runs on CPU after each CPU substep or GPU readback batch. Body
integration remains GPU computed. Gameplay uses f32 coordinates; submissions
must use the separately refined and validated f64 solver arrangement. The Yew
play screen (`frontend/src/game/`) drives `Physics` directly.

`contact_forces()` exposes the last substep's net collision and edge-attraction
force per body, separately from body state. GPU output uses the two spare f32
slots of the existing 32-byte body stride; telemetry never participates in pose
merging and clears after direct edits. `mouse_force()` exposes the current
capped mouse spring. The canvas uses logarithmically scaled arrows during
interaction; zero net contact force correctly produces no arrow even when
opposing contacts cancel. Outer-band marks depict spring effort and direction,
with a dashed rest boundary and a solid actual boundary.

## Tests

Run host physics tests with `cargo test -p physics`. Browser tests are also Rust,
using `wasm-bindgen-test`; CI runs them in headless Chrome with a software GPU.
The physics tests exercise GPU/CPU parity, readback edits, drag resistance, band
contraction, cancellation, missing WebGPU, and device-loss fallback. The frontend
tests mount the play screen and drive it with real DOM events (first-render
readout, pause, drag pushing a neighbor). None write scores.

For local browser tests, install Chrome and a ChromeDriver with the same major
version (wasm-pack caches drivers under `~/.cache/.wasm-pack/`), then run from the
repository root (adjust paths in `physics/webdriver.json` if needed):

```sh
cargo install wasm-bindgen-cli --version 0.2.128 --locked
CHROMEDRIVER=/path/to/chromedriver \
WASM_BINDGEN_TEST_WEBDRIVER_JSON="$PWD/physics/webdriver.json" \
WASM_BINDGEN_USE_BROWSER=1 WASM_BINDGEN_TEST_TIMEOUT=60 \
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p physics -p frontend --target wasm32-unknown-unknown --locked
```

The test runner version must match `wasm-bindgen` in `Cargo.lock`. These tests
require a WebGPU adapter; they fail instead of silently skipping GPU checks.
Chrome's software-adapter flags in `webdriver.json` are only for testing.
