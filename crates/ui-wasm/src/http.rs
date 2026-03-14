//! Real async HTTP implementation for production use.
//!
//! The demo in `demo.rs` simulates network calls with mock timers
//! (`PendingMock` / `complete_at`) for self-contained offline demos.
//! This module provides the real `fetch`-based implementation using
//! `wasm-bindgen-futures` for use in production form submissions.

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

/// POST a JSON body to `url` and return the response body string on success,
/// or an error string describing the failure.
pub async fn post_json(url: &str, body: &str) -> Result<String, String> {
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&JsValue::from_str(body));

    let request = Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("Request error: {:?}", e))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Header error: {:?}", e))?;

    let window = web_sys::window().ok_or("no window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch error: {:?}", e))?;

    let resp: Response = resp_value.dyn_into().map_err(|_| "not a Response")?;
    let ok = resp.ok();
    let text = JsFuture::from(resp.text().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("text() error: {:?}", e))?;
    let body_str = text.as_string().unwrap_or_default();

    if ok {
        Ok(body_str)
    } else {
        Err(format!("HTTP {}: {}", resp.status(), body_str))
    }
}
