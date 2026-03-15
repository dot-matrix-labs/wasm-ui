# Chromium-Driven Testing (No Third Party)

This project includes a minimal Chromium driver (`cdp-runner`) that uses the Chrome DevTools Protocol over a raw WebSocket implementation built with Rust stdlib only.

## Build WASM
```bash
cargo install wasm-pack
cd crates/ui-wasm
wasm-pack build --target web --out-dir ../../examples/web/pkg
```

## Run the Chromium Test Runner
```bash
cargo run -p cdp-runner
```

The runner will:
- Start a local HTTP server (`python3 -m http.server`) serving `examples/web`.
- Launch Chromium headless with remote debugging enabled.
- Connect to CDP and drive input events directly.
- Assert accessibility JSON contains expected text.
- Trigger optimistic submit and rollback behavior.

## Cargo Test Integration
```bash
cargo test -p cdp-runner
```

This launches Chromium in a clean temporary profile per run and fails if Chromium is not found.

## Environment Variables
- `CDP_CHROME_BIN`: path to Chromium/Chrome binary.
- `CDP_URL`: page URL (default `http://127.0.0.1:8000/index.html`).
- `CDP_PORT`: remote debugging port (default `9222`).
- `CDP_HEADLESS`: `1` or `0`.
- `CDP_NO_SERVER`: `1` to skip the HTTP server.
- `CDP_NO_CHROME`: `1` to skip launching Chromium (tests will fail if set).

## Visual Regression Testing

The CDP runner includes pixel-diff visual regression testing. Each screenshot
captured during a test run is compared against a committed baseline image in
`tests/screenshots/baseline/`.

### Generating / Updating Baselines

```bash
CDP_SCREENSHOT_DIR=screenshots CDP_UPDATE_BASELINES=1 cargo test -p cdp-runner -- --nocapture
```

This captures screenshots and copies them into `tests/screenshots/baseline/`.
Commit the updated baseline PNGs alongside your code changes.

### How Comparison Works

- Each screenshot is compared pixel-by-pixel against its baseline PNG.
- Per-channel tolerance of 3 (out of 255) absorbs minor anti-aliasing variance.
- The test fails if more than 0.1% of pixels differ beyond tolerance.
- On failure, three debug images are written to the screenshot output directory:
  - `<name>_baseline.png` -- the expected reference image
  - `<name>_actual.png` -- what the test produced
  - `<name>_diff.png` -- changed pixels highlighted in magenta

### Environment Variables

| Variable | Default | Purpose |
|---|---|---|
| `CDP_SCREENSHOT_DIR` | (none) | Directory for captured screenshots |
| `CDP_BASELINE_DIR` | `tests/screenshots/baseline` | Directory containing baseline PNGs |
| `CDP_UPDATE_BASELINES` | `0` | Set to `1` to overwrite baselines instead of comparing |
| `CDP_DIFF_THRESHOLD` | `0.001` | Max fraction of differing pixels before failure (0.001 = 0.1%) |

### Deterministic Rendering

For baselines to be meaningful, rendering must be deterministic:
- Fixed viewport size (1280x720) and device pixel ratio
- Headless Chromium with a pinned version in CI
- No cursor blink or animations during capture
- Baselines must be regenerated when the CI OS image or Chromium version changes

## Notes
- The test runner uses coordinate-based clicks based on the immediate-mode layout (1280x720).
- No DOM form elements are used; all interactions are through GPU-rendered widgets.
