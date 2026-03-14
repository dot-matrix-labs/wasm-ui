# Prior Art: Wasm UI Frameworks Evaluated Against Project Goals

**Evaluation date:** March 2026
**Project goal:** GPU-rendered forms library targeting mobile-first PWAs, operating within Wasm's grow-only linear memory model.

This document evaluates Rust, C++, and Go frameworks that can target WebAssembly, scored against our project requirements. The evaluation criteria are derived from the memory constraints, failure modes, and architectural implications analyzed in `docs/technical/rust-wasm-memory-constraints.md`.

---

## 1. Evaluation Criteria

| # | Criterion | Weight | Why It Matters |
|---|---|---|---|
| C1 | **Linear memory fragmentation** | High | Wasm memory grows but never shrinks. Frameworks that keep most state in linear memory and use scattered allocations will fragment over long sessions. |
| C2 | **Rendering architecture** | High | Immediate-mode avoids persistent widget trees that fragment memory. Retained-mode must carefully manage object lifetimes. |
| C3 | **Per-frame allocations** | High | Zero-allocation steady state is critical for 60fps on mobile. Cloning buffers or diffing virtual DOM trees each frame is expensive. |
| C4 | **Cross-boundary overhead** | Medium | JS↔Wasm data copies (events, vertex buffers, DOM mutations) add latency and memory pressure. |
| C5 | **Binary size (gzipped)** | Medium | Large binaries consume code memory on mobile, slow first load, and count against PWA install budgets. |
| C6 | **Mobile support** | Critical | Touch events, virtual keyboard activation, IME (CJK), iOS Safari quirks. |
| C7 | **Accessibility** | Critical | WCAG 2.1 AA. Screen reader support is a hard requirement. |
| C8 | **PWA compatibility** | Medium | Offline operation, installability, service worker integration. |
| C9 | **Text rendering** | Medium | Glyph atlas strategy affects GPU memory budget. SDF preferred for resolution independence. |
| C10 | **Maturity / community** | Low | Ecosystem health, production deployments, bus factor. |

---

## 2. Framework Profiles

### Rust Frameworks

#### egui
- **Rendering:** Immediate mode. Tessellates all UI into triangle meshes per frame. WebGL via glow backend.
- **Binary size:** ~1.5–2 MB uncompressed (glow). Significantly larger with wgpu (Naga shader transpiler).
- **Mobile:** Touch events work. Virtual keyboard / IME support is limited on web — no iOS Safari handling.
- **Accessibility:** AccessKit on native. Web: experimental screen reader (manual enable). No ARIA. Canvas = no screen reader access.
- **Memory:** Minimal UI state (immediate mode). Font atlas overflow is a known issue (#5256) with CJK.
- **Text:** CPU-rasterized glyph atlas with 4 subpixel variants per glyph. Not SDF.
- **Community:** ~22K stars. v0.33.x. Used by Rerun.io. Single maintainer (emilk).

#### Dioxus
- **Rendering:** Retained mode, virtual DOM with fiber-like diffing. Web target renders to **real DOM** via web-sys.
- **Binary size:** ~50 KB gzipped (claimed).
- **Mobile:** Android/iOS via native webview. Virtual keyboard handled by platform webview.
- **Accessibility:** Full ARIA through real DOM. First-party component library (Radix-based) with WAI-ARIA compliance.
- **Memory:** DOM nodes are the primary memory consumers (browser GC, not linear memory). VDOM diffing in linear memory.
- **Text:** Browser-native (DOM).
- **Community:** ~35K stars. v0.7.3. Actively developed by DioxusLabs.

#### Leptos
- **Rendering:** Fine-grained reactivity, no virtual DOM. Renders to **real DOM**. Reactive signals update individual nodes.
- **Binary size:** ~12 KB gzipped (islands mode). ~274 KB uncompressed (non-islands). Supports wasm binary splitting.
- **Mobile:** Web-first. Mobile via responsive web or Tauri webview. No native mobile target.
- **Accessibility:** Full DOM = standard HTML accessibility, ARIA, screen readers work normally.
- **Memory:** No virtual DOM tree in memory. Fine-grained signals minimize retained state.
- **Text:** Browser-native (DOM).
- **Community:** ~20K stars. v0.8.x. Active development by Greg Johnston.

#### Yew
- **Rendering:** Retained mode, virtual DOM with diffing (React-like). Renders to **real DOM**.
- **Binary size:** ~136 KB gzipped (RustMart example). TodoMVC: ~1.7 MB uncompressed.
- **Mobile:** Web-only. Mobile via responsive web.
- **Accessibility:** DOM-based = standard HTML accessibility and ARIA.
- **Memory:** Virtual DOM requires maintaining two trees in linear memory.
- **Text:** Browser-native (DOM).
- **Community:** ~32K stars. Pre-1.0. Most mature Rust wasm web framework by age, but development has slowed.

#### Iced
- **Rendering:** Retained mode (Elm architecture). Desktop: wgpu (GPU). Web: DOM (iced_web) — two very different backends.
- **Binary size:** Unknown. wgpu backend would be very large for wasm.
- **Mobile:** Experimental. Desktop-first. No mobile touch/keyboard handling.
- **Accessibility:** Unknown / undocumented. wgpu backend has none.
- **Memory:** Elm architecture centralizes state. wgpu has GPU buffer allocations.
- **Text:** wgpu backend: custom rendering. DOM backend: browser-native.
- **Community:** ~30K stars. Used by COSMIC desktop (System76). API unstable.

#### Makepad
- **Rendering:** Immediate-style with retained scene graph. Entirely GPU-rendered via custom shaders. Wasm uses WebGL.
- **Binary size:** ~500 KB compressed. Loads in under 1 second.
- **Mobile:** Strong: iOS (down to iPhone 6), tvOS, Android. Touch events handled natively.
- **Accessibility:** None documented. GPU-rendered canvas = no screen reader access on web.
- **Memory:** GPU-centric — most visual data in GPU buffers. CPU allocations minimized.
- **Text:** SDF rendering on GPU. Resolution-independent. Custom shader-based.
- **Community:** ~5K stars. 1.0 released ~2025. Active development by Rik Arends.

#### Xilem
- **Rendering:** Declarative/reactive (SwiftUI-inspired) with retained widget layer (Masonry). GPU via Vello (compute renderer) on wgpu. Also has xilem_web (DOM target).
- **Binary size:** Unknown. Vello + wgpu = large. xilem_web = smaller.
- **Mobile:** Not supported. Desktop-first. Web backend experimental, "not recommended for production."
- **Accessibility:** AccessKit in Masonry (desktop). Web canvas: none. xilem_web: standard browser a11y.
- **Memory:** View tree diffing against retained widget tree. Vello uses GPU compute shaders.
- **Text:** Parley for layout, Vello for rendering. GPU compute-based path rendering.
- **Community:** ~3K stars. v0.1.0. Alpha quality. Linebender project (Raph Levien).

#### Slint
- **Rendering:** Retained mode with declarative .slint markup. Multiple renderers: FemtoVG (OpenGL ES), Skia, software. Wasm uses FemtoVG → WebGL canvas.
- **Binary size:** Runtime fits in <300 KB RAM (embedded claim). Web binary size unknown but designed for embedded.
- **Mobile:** Android/iOS support with safe area and virtual keyboard area support (v1.15).
- **Accessibility:** Basic infrastructure, keyboard navigation. Screen readers: partial. Web (canvas): no accessibility.
- **Memory:** Designed for embedded (<300 KB RAM). Reactive property bindings. Minimal allocations.
- **Text:** FemtoVG for OpenGL ES rendering. System fonts via platform integration.
- **Community:** ~18K stars. v1.15.1. Commercial (SixtyFPS GmbH). Dual-licensed GPLv3 + commercial.

### C++ Frameworks

#### Dear ImGui (via Emscripten)
- **Rendering:** Immediate mode. Outputs vertex buffers; user provides backend. Wasm typically uses WebGL via Emscripten.
- **Binary size:** 325–780 KB compressed depending on features.
- **Mobile:** No built-in support. No virtual keyboard, no IME. Touch must be manually mapped.
- **Accessibility:** None. No screen reader, no ARIA. Known gap with active discussion (#4122, #8022).
- **Memory:** Allocates on first use; typical frames allocate nothing. C++ allocator via Emscripten's malloc.
- **Text:** CPU-rasterized bitmap font atlas. Custom TrueType loading. No SDF by default.
- **Community:** ~64K stars. Extremely mature. De facto standard for immediate-mode GUI in C++.

#### Qt for WebAssembly
- **Rendering:** Retained mode (Qt Widgets or Qt Quick/QML). Renders to canvas via WebGL. Full Qt pipeline compiled to wasm.
- **Binary size:** 2–5 MB gzipped for minimal app. Safari has issues with modules this large.
- **Mobile:** No virtual keyboard on web. Some mobile GPUs blacklisted. Touch limited.
- **Accessibility:** Not available in wasm build. Canvas = no screen reader access.
- **Memory:** Full Qt runtime in linear memory. Heavy baseline footprint. Emscripten ALLOW_MEMORY_GROWTH.
- **Text:** FreeType/HarfBuzz compiled to wasm. System font access limited.
- **Community:** Massive (Qt itself). Wasm officially supported since Qt 5.13. Commercial backing. Web is not primary focus.

### Go Frameworks

#### Gio
- **Rendering:** Immediate mode. GPU vector renderer (Pathfinder-based, migrating to piet-gpu compute). Wasm support experimental.
- **Binary size:** ~1.3–2 MB baseline (Go runtime alone). App code adds more.
- **Mobile:** Android/iOS are primary targets with good native support. Web/wasm is secondary.
- **Accessibility:** Unknown / undocumented for web target.
- **Memory:** Go runtime with GC compiled to wasm. GC overhead in wasm. Full Go runtime included.
- **Text:** Vector outline rendering (no texture atlas). Resolution-independent.
- **Community:** ~1.5K stars. Pre-1.0. Active development by Elias Naur. Small community.

#### Vecty
- **Rendering:** Retained mode, virtual DOM. Renders to **real DOM** via syscall/js.
- **Binary size:** ~250–300 KB gzipped estimated. TinyGo can reduce further.
- **Mobile:** Web-only. Mobile via responsive web.
- **Accessibility:** DOM-based = standard HTML accessibility.
- **Memory:** Go runtime + GC in wasm. VDOM in linear memory. DOM nodes managed by browser.
- **Text:** Browser-native (DOM).
- **Community:** ~2.8K stars. Experimental, pre-1.0. Development slow.

---

## 3. Scorecard

Scores: **1** = poor/missing, **2** = limited, **3** = adequate, **4** = good, **5** = excellent.

Weights: Critical = 3×, High = 2×, Medium = 1×, Low = 0.5×.

| Framework | C1 Frag | C2 Arch | C3 Alloc | C4 Boundary | C5 Size | C6 Mobile | C7 A11y | C8 PWA | C9 Text | C10 Maturity | **Weighted** |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **egui** | 4 | 5 | 4 | 3 | 3 | 2 | 1 | 2 | 3 | 4 | **55.0** |
| **Dioxus** | 4 | 3 | 3 | 2 | 5 | 4 | 5 | 3 | 3 | 4 | **69.0** |
| **Leptos** | 5 | 3 | 4 | 2 | 5 | 3 | 5 | 4 | 3 | 4 | **71.5** |
| **Yew** | 3 | 3 | 2 | 2 | 3 | 3 | 5 | 3 | 3 | 4 | **59.5** |
| **Iced** | 3 | 3 | 3 | 2 | 2 | 1 | 1 | 2 | 3 | 3 | **42.5** |
| **Makepad** | 4 | 4 | 4 | 3 | 4 | 5 | 1 | 2 | 5 | 3 | **62.0** |
| **Xilem** | 3 | 4 | 3 | 2 | 2 | 1 | 3 | 1 | 4 | 2 | **44.0** |
| **Slint** | 4 | 3 | 4 | 3 | 4 | 4 | 2 | 2 | 3 | 4 | **60.0** |
| **Dear ImGui** | 4 | 5 | 5 | 3 | 4 | 1 | 1 | 2 | 3 | 5 | **57.5** |
| **Qt Wasm** | 2 | 3 | 2 | 2 | 1 | 1 | 1 | 1 | 4 | 4 | **37.0** |
| **Gio** | 3 | 4 | 3 | 2 | 2 | 4 | 1 | 2 | 4 | 2 | **48.5** |
| **Vecty** | 3 | 3 | 2 | 2 | 3 | 3 | 4 | 3 | 3 | 2 | **53.0** |

### Ranking by Weighted Score

| Rank | Framework | Score | Best For |
|---|---|---|---|
| 1 | **Leptos** | 71.5 | DOM-based apps with minimal wasm memory footprint |
| 2 | **Dioxus** | 69.0 | Full-stack apps with native mobile via webview |
| 3 | **Makepad** | 62.0 | GPU-rendered cross-platform apps (if a11y is not required) |
| 4 | **Slint** | 60.0 | Embedded/industrial UI with web as secondary target |
| 5 | **Yew** | 59.5 | React-like Rust web apps |
| 6 | **Dear ImGui** | 57.5 | Internal tools, debug UIs, game dev overlays |
| 7 | **egui** | 55.0 | Rust-native tools with web as secondary target |
| 8 | **Vecty** | 53.0 | Simple Go web apps |
| 9 | **Gio** | 48.5 | Native mobile apps in Go (web secondary) |
| 10 | **Xilem** | 44.0 | Future potential (alpha quality today) |
| 11 | **Iced** | 42.5 | Rust desktop apps (web is an afterthought) |
| 12 | **Qt Wasm** | 37.0 | Porting existing Qt apps to web (not greenfield) |

---

## 4. Analysis

### The DOM vs Canvas divide

The single most important architectural divide is whether a framework renders to the **DOM** or to a **canvas/WebGL surface**.

**DOM-based frameworks** (Leptos, Dioxus, Yew, Vecty) win on:
- **Accessibility** — screen readers work for free via ARIA and semantic HTML
- **Memory fragmentation** — most memory lives on the browser's GC heap, not in Wasm linear memory
- **Text rendering** — browser handles fonts, shaping, BiDi, IME natively
- **Binary size** — no rendering pipeline in the Wasm module

**Canvas/GPU frameworks** (egui, Makepad, Dear ImGui, our project) win on:
- **Rendering control** — pixel-perfect, cross-platform consistent appearance
- **Performance ceiling** — batched GPU rendering beats DOM layout for complex UIs
- **Per-frame allocation** — immediate-mode can achieve zero-allocation steady state

Our project explicitly chose canvas/GPU rendering for rendering control and performance determinism. This means we inherit the canvas side's weaknesses — accessibility and text rendering require significant custom work (the DOM mirror and glyph atlas).

### Why not just use Leptos or Dioxus?

They score highest, but they solve a different problem. DOM-based frameworks delegate rendering, text, and accessibility to the browser — which is excellent until you need:
- Pixel-identical rendering across platforms
- Sub-millisecond hit testing on hundreds of widgets
- GPU-batched rendering with single-digit draw calls
- Full control over text layout for specialized form rendering

For a forms library that must look and behave identically across browsers and devices, DOM rendering introduces browser-specific layout quirks, inconsistent form control styling, and the overhead of the browser's layout engine on every frame.

### Why not Makepad?

Makepad is the closest prior art to our project: GPU-rendered, immediate-mode-influenced, excellent mobile support, SDF text, small binary. It scores well on most technical criteria. The blockers are:

1. **No accessibility** — Makepad has no screen reader support and no accessibility architecture. For a forms library, WCAG 2.1 AA is a hard requirement, not a nice-to-have.
2. **Custom DSL** — Makepad uses its own shader language and layout DSL. This creates a steep learning curve and vendor lock-in.
3. **Small community** — ~5K stars, single-company project. Higher bus factor risk.

### Why not egui?

egui is the most mature immediate-mode Rust framework with web support. The blockers:

1. **No web accessibility** — same fundamental problem as Makepad. AccessKit works on native but not web.
2. **Font atlas limitations** — known overflow issues with CJK character sets (#5256), which our forms must handle.
3. **No mobile input story** — virtual keyboard, IME, and iOS Safari quirks are not handled.

### Why build our own?

No existing framework satisfies all three critical requirements simultaneously:
1. GPU-rendered canvas (for rendering control and performance)
2. Accessible via DOM mirror (for WCAG 2.1 AA compliance)
3. Mobile-first text input (virtual keyboard, IME, iOS Safari)

Our architecture combines the GPU rendering approach of egui/Makepad/Dear ImGui with a DOM accessibility mirror (our custom `AccessibilityMirror`) and a hidden textarea proxy for mobile input. This is the same architectural pattern used by Figma (C++→Wasm with DOM accessibility overlay) and Google Docs (canvas rendering with ARIA mirror).

The tradeoff is engineering cost: we must build and maintain the accessibility mirror, glyph atlas, and mobile input proxy ourselves. The memory constraints paper documents why this is viable within Wasm's limitations, and the engineering spec (`ENGINEERING.md`) defines the patterns that keep memory under control.

---

## 5. Key Takeaways for Implementation

| Lesson | Source Framework | Application to Our Project |
|---|---|---|
| SDF text atlas for resolution independence | Makepad | Adopt SDF rendering (Phase 2.1) to reduce per-glyph atlas memory |
| Fine-grained reactivity over VDOM diffing | Leptos | Avoid virtual DOM patterns; our immediate-mode approach already avoids this |
| AccessKit for native accessibility | egui, Xilem | Study AccessKit's ARIA mapping for our DOM mirror implementation |
| Wasm binary splitting | Leptos | Consider code splitting if binary exceeds 500 KB budget |
| Embedded memory budgets | Slint | Apply Slint's <300 KB mindset to our buffer pre-allocation strategy |
| GPU compute rendering | Xilem/Vello | Future option if WebGPU adoption reaches critical mass |
| Component library with ARIA | Dioxus | Model our widget accessibility semantics on Radix/WAI-ARIA patterns |
