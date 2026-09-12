# Browser physics

`kernel.wgsl` runs oriented-square SAT contacts, damping, gravity, attraction,
and a mouse spring as WebGPU compute. `engine.js` manages ping-pong storage
buffers and bounded readback, and supplies a CPU fallback with the same model.
The simulation advances at 120 Hz in at most six substeps per display frame.
Contacts are stiff penalty springs, intentionally not exact constraints.
Dragging wakes paused scenes and uses a spring capped at 40 force units, so
neighbors push back even if the pointer moves far away. A dotted tether shows
the pointer target; clicking off-center preserves the grab offset.

Outer band tension couples the top/right walls to one side-length spring;
the bottom/left edges stay anchored. The target-size slider sets its rest
length, and contact pressure can expand the band. Zero tension fixes the
current size. This single boundary coordinate runs on the CPU, after each
CPU substep or GPU readback batch (up to six substeps); square contacts remain
GPU computed. The viewport stays fixed while the band contracts. Measuring
still refines and validates a separate f64 arrangement before submission.

The scene is limited to the shared `MAX_N` (100). Pair evaluation is O(n²).
Rendering uses canvas; GPU acceleration is specifically in the physics compute.
GPU availability is shown in the UI. Device loss switches to CPU. GPU and CPU
transcendental precision differ, so their trajectories need not be identical.

The renderer stores float32 positions, while the solver retains a separate
float64 arrangement for submissions. A scene edit increments a revision to
prevent an older GPU readback from overwriting the edit. Reset, import, and
unmount release or invalidate the relevant state.

Run CPU checks with `node --test physics/engine.test.js` (also in CI).
For a real browser/GPU/database roundtrip, start the server against a local
Postgres database, then:

```sh
npm install --prefix /tmp/packit-browser playwright
NODE_PATH=/tmp/packit-browser/node_modules PACKIT_URL=http://localhost:8093 \
  node physics/browser.test.cjs
```

The optional browser check uses `/usr/bin/google-chrome` by default; override
with `CHROME_PATH`. It enables the software WebGPU adapter for headless testing,
compares one GPU/CPU step within rough-physics precision, tests edits during GPU
readback, imports the five-square packing, refines/submits it to the local DB,
checks its detail view, verifies a mobile viewport, and tests CPU fallback.
It writes one score with a unique `browser-<timestamp>` player name. Use a test
DB, not a production endpoint.

Read-only mouse and boundary regression (WebGPU and CPU fallback):

```sh
NODE_PATH=/tmp/packit-browser/node_modules PACKIT_URL=http://localhost:8093 \
  node physics/interaction.test.cjs
```
