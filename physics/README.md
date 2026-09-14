# Rust browser physics

`Physics` is a cloneable `Rc<RefCell<_>>` simulation handle. It runs immediately
on the Rust CPU fallback and can asynchronously initialize WebGPU through
`wgpu`. `kernel.wgsl` computes oriented-square SAT contacts, damping,
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

`set_glues(&[Glue])` replaces up to 4096 explicit feature contacts atomically.
Corners and edge midpoints are points; square edges and walls are segments.
Point–point links pull to coincidence. Point–segment links slide along the
segment, pulling toward the endpoint if they slide off. Segment–segment links
align opposing normals and close their gap while allowing tangential sliding.
Springs are capped at 80 force units, with an alignment couple capped at 12.
Forces act at the features and transmit torque; moving walls receive the opposite
reaction. Invalid indices, same-object links and reversed duplicates are rejected.
Each link is weighted by the larger incident-body degree, bounding total
explicit stiffness and damping even for redundant point unions.
Inputs do not change revisions; reset, load and dispose clear links, pause keeps
them. GPU glue input uses a separate bounded storage buffer; body stride is unchanged.

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
and glue force per body, separately from body state. GPU output uses the two spare f32
slots of the existing 32-byte body stride; telemetry never participates in pose
merging and clears after direct edits. `mouse_force()` exposes the current
capped mouse spring. The canvas combines these as an unoriented directional-load
tensor, filtered over 240 ms, and draws a translucent ellipse over each piece.
Its short axis follows the dominant load; force reversals do not flip it, and
release fades it away. This is a visual cue, not a stress measurement: opposing
contacts can cancel in the net telemetry. The dashed boundary shows the target
and the solid boundary shows the actual container.

## Centered interaction and settling

The simplified UI can opt into `set_drag_expansion(true)`. Sustained mouse
resistance above 8 force units, a target lead above 0.18 units, and a chain of
contacts to a wall must persist for 150 ms before the box grows. Growth is
limited to 0.5 side units per simulated second and stops on release. Free-space
motion and brief bumps do not expand the container. Growth releases band
tension and updates its target to hold the expanded size. The load overlay uses
the completed batch's telemetry until the next batch refreshes it.

The renderer must keep the box center fixed: screen position is canvas center
plus `(local_position - side / 2) * scale` (with the screen y-axis flipped).
Growth by a side increment translates body positions and the stored mouse
target by half that increment on each axis. Velocities and angles do not change;
the centered world positions are preserved. `frame_shift()` is the cumulative
translation per axis over the instance's lifetime. `arrangement()` still uses
`[0, side]^2`. These frame shifts do not increment the direct-edit revision.
The legacy tension slider remains a separate bottom-left-anchored control.

`begin_settle()` releases the mouse and turn input, wakes the simulation,
disables attraction/compression, and increases damping. Contacts and glue stay
active. The box stays fixed while penetration improves by more than 10%
(or 1e-6, whichever is larger) per observation window. If progress stalls for
half a second, the centered box opens at 0.08 side units per second until
clear. `settle_status()` reports `Running` until geometric depth is at most
`SETTLE_DEPTH` (1e-5), glue error is at most `SETTLE_GLUE_ERROR` (1e-3), and
motion is at most 0.002 per square, continuously for half a second. It then
pauses with `Settled`. After 12 simulated seconds, remaining violations produce
`Blocked`; persistent motion produces `TimedOut`. Both stop and pause without
loading a solver pose. `cancel_settle()` and direct controls stop the run;
saved force settings return with band tension zero.
Do not call `set_paused` or `set_params` after starting a run unless cancelling
it is intended. The controller advances inside `step()`; the UI must not also
drive a competing relaxation controller.

`violations()` measures one current snapshot. `pairs` and `wall_contacts` give
positive penetration depths; `max_depth` is their maximum. `glues` gives each
unsatisfied glue's index, separation and angular error; `max_glue_error` is the
maximum separation or half angular error. `bodies` and `walls` include both
geometric and glue error for highlighting. The glue reporter shares contact
geometry with the CPU force calculation. No extra GPU buffer/readback is needed.

`Settled` is not a validity certificate and cannot bound how far an optimizer
might move a packing. The UI must certify separately with f64 validation, retain
the live pose, and reject a material refinement displacement instead of snapping
to another solution. Mouse capture and frontend drag bookkeeping must also be
released before starting the controller.

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
WASM_BINDGEN_USE_BROWSER=1 WASM_BINDGEN_TEST_TIMEOUT=120 \
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p physics -p frontend --target wasm32-unknown-unknown --locked
```

The test runner version must match `wasm-bindgen` in `Cargo.lock`. These tests
require a WebGPU adapter; they fail instead of silently skipping GPU checks.
Chrome's software-adapter flags in `webdriver.json` are only for testing.

Manual Settle first uses `begin_settle_with_pressure`: a slack, clear scene gets
gentle inward band tension (15) toward its shape-specific area bound. Existing
overlap or unsatisfied glue is resolved first without more compression. Pressure
relaxes toward a local equilibrium, measured by stable poses and boundary size
over two half-second windows, then releases into the normal strict contact and
motion checks. The compression time budget includes travel for roomy imports;
exhaustion reports TimedOut rather than claiming equilibrium. This is a local
force relaxation, not a global optimum guarantee. Automatic measurement and
squeeze release keep using `begin_settle` so they do not restart compression.
