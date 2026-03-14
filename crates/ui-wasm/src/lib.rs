mod atlas;
mod demo;
mod http;
mod renderer;

use demo::DemoApp;
use renderer::Renderer;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen]
pub struct WasmApp {
    demo: DemoApp,
    renderer: Renderer,
    width: f32,
    height: f32,
    scale: f32,
}

#[wasm_bindgen]
impl WasmApp {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement, width: f32, height: f32, scale: f32) -> Result<WasmApp, JsValue> {
        console_error_panic_hook::set_once();
        let renderer = Renderer::new(&canvas, width, height)?;
        Ok(Self {
            demo: DemoApp::new(width, height),
            renderer,
            width,
            height,
            scale,
        })
    }

    pub fn resize(&mut self, width: f32, height: f32, scale: f32) {
        self.width = width;
        self.height = height;
        self.scale = scale;
        self.renderer.resize(width, height);
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        self.renderer.set_font_bytes(bytes);
    }

    /// Load a fallback font from raw bytes.  Missing glyphs in the primary font
    /// are looked up in fallbacks in the order they were added.
    /// Task 2.6: Font Fallback Chain.
    pub fn add_fallback_font_bytes(&mut self, bytes: Vec<u8>) {
        self.renderer.add_fallback_font_bytes(bytes);
    }

    pub fn frame(&mut self, timestamp_ms: f64) -> Result<JsValue, JsValue> {
        let output = self.demo.frame(self.width, self.height, self.scale, timestamp_ms);
        self.renderer.render(&output.batch, &output.text_runs)?;
        Ok(output.a11y_json)
    }

    pub fn handle_pointer_down(&mut self, x: f32, y: f32, button: u16, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.demo.handle_pointer_down(x, y, button, ctrl, alt, shift, meta);
    }

    pub fn handle_pointer_up(&mut self, x: f32, y: f32, button: u16, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.demo.handle_pointer_up(x, y, button, ctrl, alt, shift, meta);
    }

    pub fn handle_pointer_move(&mut self, x: f32, y: f32, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.demo.handle_pointer_move(x, y, ctrl, alt, shift, meta);
    }

    pub fn handle_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.demo.handle_wheel(x, y, dx, dy, ctrl, alt, shift, meta);
    }

    pub fn handle_key_down(&mut self, code: String, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.demo.handle_key_down(code, ctrl, alt, shift, meta);
    }

    pub fn handle_key_up(&mut self, code: String, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.demo.handle_key_up(code, ctrl, alt, shift, meta);
    }

    pub fn handle_text_input(&mut self, text: String) {
        self.demo.handle_text_input(text);
    }

    pub fn handle_composition_start(&mut self) {
        self.demo.handle_composition_start();
    }

    pub fn handle_composition_update(&mut self, text: String) {
        self.demo.handle_composition_update(text);
    }

    pub fn handle_composition_end(&mut self, text: String) {
        self.demo.handle_composition_end(text);
    }

    pub fn handle_paste(&mut self, text: String) {
        self.demo.handle_paste(text);
    }

    pub fn take_clipboard_request(&mut self) -> Option<String> {
        self.demo.take_clipboard_request()
    }

    /// Task 3.5: Pass prefers-reduced-motion from JS.
    pub fn set_reduce_motion(&mut self, reduce: bool) {
        self.demo.set_reduce_motion(reduce);
    }

    /// Task 3.6: Pass CSS env() safe area insets from JS.
    pub fn set_safe_area_insets(&mut self, top: f32, right: f32, bottom: f32, left: f32) {
        self.demo.set_safe_area_insets(top, right, bottom, left);
    }

    /// Task 6.5: Switch between light and dark theme.
    pub fn set_dark_mode(&mut self, dark: bool) {
        self.demo.set_dark_mode(dark);
    }

    /// Task 6.1: Handle autofill values from password manager.
    pub fn handle_autofill(&mut self, field: String, value: String) {
        self.demo.handle_autofill(field, value);
    }
}

