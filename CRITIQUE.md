# Critique: GPU Forms UI (Rust + WASM)

*Perspective: engineer who built browsers, web infrastructure, and web standards.*

---

## Executive Summary

This project renders HTML-style form widgets entirely on a `<canvas>` via WebGL2, bypassing the DOM. It's an impressive amount of working code for an early prototype. But from a web platform perspective, it re-implements a subset of what browsers already do — badly — while discarding decades of battle-tested infrastructure for accessibility, internationalization, security, input handling, and compositing. The result is a system that looks like a form but doesn't behave like one.

---

## 1. Accessibility Is Fundamentally Broken

**The A11y tree is a JSON blob on `window.__a11y`. Nothing consumes it.**

Screen readers (NVDA, JAWS, VoiceOver, TalkBack) interact with the browser's accessibility tree via platform APIs (MSAA/UIA on Windows, ATK on Linux, NSAccessibility on macOS). The browser builds this tree from DOM semantics. A `<canvas>` element exposes exactly one node to the accessibility tree: "canvas." Your JSON object sitting on a JS global is invisible to every assistive technology on every platform.

To make canvas-based UIs accessible, you need to either:

1. Maintain a shadow DOM of ARIA-annotated elements positioned over the canvas (how Google Docs, Figma, and Rive do it), or
2. Use the experimental [Accessibility Object Model (AOM)](https://wicg.github.io/aom/) `ElementInternals` API — which is not yet shipping in most browsers.

Without one of these, this project is **unusable** by blind users, users with motor impairments who rely on switch access, and users of voice control software. This isn't a nice-to-have. In many jurisdictions (ADA, EAA, EN 301 549) shipping an inaccessible web form is a legal liability.

**Keyboard navigation is incomplete.** Tab cycles through widgets, but there's no `aria-activedescendant` equivalent, no `role="application"` signaling, no live regions for status messages, and no focus ring rendering that respects `prefers-reduced-motion` or OS high-contrast mode.

---

## 2. Text Input Is a Minefield

Text input is the single hardest thing to get right on the web. Browsers have tens of thousands of lines of code for it, honed over decades. This project's `TextBuffer` + `apply_text_events` reimplements a fraction of it.

### What's missing or broken:

- **`e.keyCode` is deprecated.** (`app.js:38`) The `keyCode` property was formally deprecated in the UI Events spec (which I can say from direct experience is a pain to maintain). Use `e.code` (physical key) or `e.key` (logical key). `keyCode` values differ across keyboard layouts, so your `map_key` function (mapping `65` to `KeyCode::A`) breaks on AZERTY, Dvorak, and every non-Latin layout. A French user pressing `Q` to type `A` will trigger `keyCode=81`, not `65`. Your Ctrl+A select-all won't work.

- **IME composition is fragile.** You handle `compositionstart`/`update`/`end`, which is good. But you also listen to `beforeinput` and forward `e.data` as text input unconditionally. During IME composition, `beforeinput` fires *alongside* composition events. You'll double-insert characters for CJK input. Real browsers gate text insertion on `inputType === 'insertText'` and suppress it during active composition. This is a showstopper for ~1.5 billion CJK users.

- **No `contenteditable` fallback for mobile.** On mobile browsers, the virtual keyboard needs a focused DOM element to appear. A `<canvas>` won't trigger it. Your text inputs are untypable on iOS and Android without a hidden `<input>` or `<textarea>` to proxy focus and keyboard events. This is how every canvas-based editor (Monaco, CodeMirror in canvas mode, Flutter Web) solves it.

- **Monospace-assumed glyph metrics.** `position_to_index` and `index_to_position` use `char_width = font_size * 0.6`. This is a hardcoded monospace approximation. With a proportional font (which `fontdue` will happily rasterize), click-to-place-caret and selection rendering will be wrong for every character. The renderer's `push_text_quads` uses real `glyph.advance` for positioning, so there's a fundamental disagreement between where text is drawn and where the UI thinks it is.

- **No BiDi support.** Right-to-left text (Arabic, Hebrew) will render LTR. The Unicode Bidirectional Algorithm is complex but mandatory for internationalized forms. Caret movement in mixed-direction text is a notoriously hard problem that browsers solve with platform-specific heuristics.

- **No system text selection integration.** Users can't use OS-level "find on page" (Ctrl+F) to locate text in your forms because it's all on a canvas. This breaks a fundamental user expectation.

---

## 3. The Rendering Architecture Has Scaling Problems

### Atlas management

The text atlas (`atlas.rs`) is a single 1024x1024 R8 texture with a naive row-packing allocator. When it runs out of space, it **clears the entire atlas and starts over** (line 119). This means:

- At ~16px font size, you get roughly 4000 glyphs before eviction. That sounds like a lot until you consider CJK character sets (tens of thousands of unique glyphs). A Chinese-language form will thrash the atlas every frame.
- The atlas is keyed on `char` alone, not `(char, font_size)`. If you render the same character at two different sizes (which you do — labels are 16px, inline labels are 13px), only the first size gets cached. The second render will use the wrong-sized glyph.

Production text atlases (e.g., in Chromium's GPU text renderer, Skia, or Pathfinder) use LRU eviction, multi-page atlases, or SDF (signed distance field) rendering. SDF would also give you resolution-independent text for free, which matters for pinch-zoom on mobile.

### Per-frame full re-upload

Every frame, `render()` clones the entire batch, appends all text quads, and uploads all vertex/index data via `bufferData` with `DYNAMIC_DRAW`. For 100 widgets this is fine. For 1000 (a realistic complex form with repeating groups), you're uploading hundreds of KB per frame. A real renderer would use persistent mapped buffers (WebGL2 doesn't have these, but you can simulate with `bufferSubData` and double-buffering) or only re-upload dirty regions.

### No instancing

Each quad is 4 vertices + 6 indices. WebGL2 supports instanced rendering (`drawElementsInstanced`), which would let you draw all solid quads in one call with a single quad VBO and per-instance position/color/UV data. This would cut vertex data by ~4x.

### Uniform lookups every draw call

`get_uniform_location` is called inside `bind_text_texture` and `unbind_text_texture`, which are called per draw command per frame. Uniform locations are stable for the lifetime of a program — cache them once at init time.

---

## 4. The Select Widget Is Not a Select Widget

Your `select()` widget cycles through options on click (`(pos + 1) % options.len()`). This is not how any select/dropdown/combobox works on any platform. Users expect:

- A dropdown overlay showing all options simultaneously
- Keyboard navigation within the dropdown (arrow keys, type-ahead search)
- Click-outside-to-dismiss
- Scrolling for long option lists

What you've built is closer to a toggle button. This would fail usability testing immediately. It also has `A11yRole::ComboBox` which is semantically wrong for a cycling button — a combobox implies an expandable list.

---

## 5. Security Concerns

### Password fields render in plaintext

There's no concept of a masked/password input. `text_input("Password", ...)` renders the password as visible text on the canvas. The WebGL framebuffer is readable by any JS on the page. In contrast, browser `<input type="password">` has specific protections: masking, resistance to shoulder-surfing, integration with password managers, and autofill APIs.

### No autofill integration

Browser autofill (credit cards, addresses, passwords) relies on DOM `<input>` elements with `autocomplete` attributes. Password managers rely on the same. Your forms are invisible to all of these. Users will not be able to use 1Password, LastPass, Chrome's built-in password manager, or any browser autofill with your forms.

### Clipboard access is fire-and-forget

`handleClipboard()` calls `navigator.clipboard.writeText()` with an empty `catch {}`. The Clipboard API requires a user gesture and a secure context. On browsers that enforce this strictly (Safari), your clipboard integration will silently fail. You also don't request clipboard-read permission, so Ctrl+V works via the `paste` event but programmatic read doesn't.

### No CSP considerations

The inline event handling and `eval`-free design is good, but there's no discussion of Content Security Policy. The WASM module loads via a plain ES module import, which is fine, but worth documenting.

---

## 6. Event Handling Gaps

- **No `preventDefault()`.** Your keyboard handler doesn't prevent default browser behavior. Pressing Tab will both cycle your internal focus AND move browser focus away from the canvas. Pressing Backspace may navigate the browser back. Space on a focused button may scroll the page. The `beforeinput` handler doesn't call `preventDefault()` either, which means the browser will try to insert text into... nothing, since the canvas isn't editable. This happens to be harmless but is accidental.

- **No pointer capture.** When dragging to select text, if the pointer leaves the canvas, you stop receiving `pointermove` events. Browsers solve this with `setPointerCapture()`. Without it, drag-to-select breaks at the canvas boundary.

- **No touch events / gesture handling.** `pointerdown`/`pointermove` don't distinguish touch from mouse. There's no pinch-to-zoom, no long-press-to-select, no scroll inertia. Mobile users will find the forms unusable.

- **`e.preventDefault()` is never called on `wheel`**, so the page will scroll behind your canvas while you're trying to scroll a dropdown (if you ever build a real one).

---

## 7. Layout System Is Too Primitive

The layout is a single vertical stack with fixed 24px margins. There's no:

- Horizontal layout (side-by-side fields)
- Flex-like or grid-like distribution
- Responsive breakpoints
- Scroll containers (what happens when your form is taller than the viewport?)
- `overflow` handling of any kind
- Margin collapse, padding, or box model

For a forms library, this means you can't build a two-column form, a form with an inline "first name / last name" row, or a form that scrolls. The web's layout engines (CSS flexbox, grid) are extraordinarily complex precisely because real-world forms need this flexibility.

---

## 8. The `rest.rs` Module Is Dead Code

`HttpClient` is a trait with no implementations. `Request`, `Response`, and `RetryPolicy` are defined but never used in the actual application. The demo mocks submissions with a timer. This isn't necessarily wrong for a prototype, but the trait signature (`fn request(&mut self, ...) -> Result<Response, String>`) is synchronous, which is incompatible with WASM's single-threaded async model. A real implementation would need to use `wasm-bindgen-futures` and `fetch`.

---

## 9. The `im` Crate Dependency Is Questionable

You're using `im` (persistent/immutable data structures) for `FormState.fields`, but then cloning the entire `FormState` on every mutation (`(*self.state).clone()`). The point of persistent data structures is structural sharing — you get that with `im::HashMap`. But wrapping the whole thing in `Arc<FormState>` and doing full clones of the outer struct on every `set_value` call undermines the benefit. You're paying the overhead of a persistent data structure (larger nodes, more allocations) without getting the wins (cheap snapshots). A plain `HashMap` with explicit snapshots for undo would be simpler and faster.

---

## 10. The CDP Test Runner Is Admirable but Fragile

Building your own Chrome DevTools Protocol client in raw Rust with manual WebSocket framing and base64 decoding is genuinely impressive as engineering exercise. But:

- The WebSocket implementation doesn't handle fragmented frames, continuation frames, or close frames correctly.
- The base64 decoder is hand-rolled — this is a one-line call with the `base64` crate and eliminates a class of potential bugs.
- The Chrome process management assumes specific binary paths and doesn't handle zombie processes if the test runner panics.
- There are no assertions on visual output (pixel comparison) — just "did it render without crashing."

For CI, Playwright or `wasm-pack test --headless --chrome` would be more reliable with less code. But I understand the appeal of zero-dep testing.

---

## 11. What This Project Gets Right

To be fair:

- **The crate separation (ui-core / ui-wasm) is clean.** Keeping the platform-agnostic UI logic separate from the WebGL renderer is the right architecture. You could port to native OpenGL, Metal, or Vulkan without touching ui-core.
- **Immediate-mode API is well-designed.** The `button()` / `checkbox()` / `text_input()` API returning interaction results is idiomatic imgui-style design and pleasant to use.
- **Grapheme-aware text editing.** Using `unicode-segmentation` for caret movement is correct and something many projects get wrong.
- **Draw command batching.** Merging consecutive quads with the same material into a single draw call is the right optimization.
- **Optimistic submit with rollback.** The form state management with snapshots, retry, and rollback is a thoughtful design for real-world form UX.
- **Hit-test grid.** Spatial partitioning for pointer dispatch is the right approach and will scale well.

---

## Bottom Line

This is a well-structured prototype that demonstrates the mechanical feasibility of GPU-rendered forms. But it's solving a problem that doesn't need solving for 99% of web applications, while sacrificing the platform guarantees that users depend on: accessibility, internationalization, security integration (autofill, password managers), and input correctness across devices and languages.

If the goal is a learning exercise or a specialized embedded UI (kiosk, game UI, digital signage), it's a solid foundation. If the goal is to replace HTML forms in production web applications, it would need to reimplement most of what a browser already provides — and at that point, the question is: why not use the browser?

The projects that have successfully gone this route (Flutter Web, Figma, Google Docs canvas mode) employ teams of dozens and still struggle with accessibility, text input, and platform integration years into development. That's not a reason not to try. But it's important to enter this space with clear eyes about the scope of what "render your own UI on canvas" actually entails.
