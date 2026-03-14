# Getting Started

Build your first GPU-accelerated form in 5 minutes.

## Prerequisites
- Rust 1.75+
- wasm-pack
- A web server (e.g. `python3 -m http.server`)

## Build

```sh
wasm-pack build crates/ui-wasm --target web --release
cd examples/web && python3 -m http.server 8080
```

## Your First Form

```rust
// In crates/ui-wasm/src/demo.rs, add to your frame function:
ui.label("My Form");
ui.text_input("Name", &mut name_buffer, "Enter your name");
if ui.button("Submit") {
    // handle submission
}
```

## API Reference

- `ui.label(text)` — render a text label
- `ui.text_input(label, buffer, placeholder)` — text input field
- `ui.password_input(label, buffer, placeholder)` — masked password field
- `ui.button(label) -> bool` — returns true when clicked
- `ui.checkbox(label, checked) -> bool` — toggle checkbox
- `ui.begin_row(gap)` / `ui.end_row()` — horizontal layout
- `ui.dropdown(label, options, selected) -> usize` — dropdown select

## Dark Mode

The app automatically responds to `prefers-color-scheme`. You can also call `app.set_dark_mode(true)` from JavaScript.

## PWA

The app ships with a service worker and web manifest. To customize:
- Edit `examples/web/manifest.json` for app metadata
- Edit `examples/web/sw.js` for caching strategy
