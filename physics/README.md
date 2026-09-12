# Browser physics

`kernel.wgsl` runs oriented-square SAT contacts, damping, gravity, attraction,
and a mouse spring as WebGPU compute. `engine.js` manages ping-pong storage
buffers and bounded readback, and supplies a CPU fallback with the same model.
The simulation advances at 120 Hz in at most six substeps per display frame.
Contacts are stiff penalty springs, intentionally not exact constraints.

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
