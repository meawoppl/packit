# Rust browser physics

`Physics` is a cloneable `Rc<RefCell<_>>` simulation handle. It runs immediately
on the Rust CPU fallback and can asynchronously initialize WebGPU through
`wgpu`. `kernel.wgsl` computes oriented-square SAT contacts, damping, gravity,
attraction, and a bounded mouse spring. The CPU implementation uses the same
model. Both use 120 Hz substeps, capped at six per frame, for up to 100 squares.
Contacts are stiff penalty springs, intentionally not exact constraints.

Dragging uses a spring capped at 40 force units, so neighbors push back even
when the pointer is far away. Mouse-target updates never increment the body
revision. Direct edits increment it; completed GPU readbacks preserve edited
components while merging motion into unchanged components. No state borrow
survives an await. Concurrent steps are ignored, cancelled mappings unmap their
buffers, and device loss or unavailable WebGPU selects CPU fallback. Call
`dispose` at UI teardown; dropping the final GPU handle also destroys its device.

Outer band tension couples the top/right walls to one side-length spring;
bottom/left edges stay anchored. `target_side` sets its rest length; contact
pressure can expand it. Zero tension fixes the current side. The scalar band
coordinate runs on CPU after each CPU substep or GPU readback batch. Body
integration remains GPU computed. Gameplay uses f32 coordinates; submissions
must use the separately refined and validated f64 solver arrangement.

The JavaScript implementation is temporarily retained as the live frontend's
reference; the companion Yew port removes it and uses `Physics` directly.

## Tests

Run host physics tests with `cargo test -p physics`. Browser tests are also Rust,
using `wasm-bindgen-test`; CI runs them in headless Chrome with a software GPU.
They exercise GPU/CPU parity, readback edits, drag resistance, band contraction,
cancellation, missing WebGPU, and device-loss fallback. They write no scores.

For local browser tests, install Chrome and a matching ChromeDriver, then run
from the repository root (adjust paths in `physics/webdriver.json` if needed):

```sh
cargo install wasm-bindgen-cli --version 0.2.128 --locked
CHROMEDRIVER=/path/to/chromedriver \
WASM_BINDGEN_TEST_WEBDRIVER_JSON="$PWD/physics/webdriver.json" \
WASM_BINDGEN_USE_BROWSER=1 WASM_BINDGEN_TEST_TIMEOUT=60 \
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p physics --target wasm32-unknown-unknown --lib --locked
```

The test runner version must match `wasm-bindgen` in `Cargo.lock`. These tests
require a WebGPU adapter; they fail instead of silently skipping GPU checks.
Chrome's software-adapter flags in `webdriver.json` are only for testing.
