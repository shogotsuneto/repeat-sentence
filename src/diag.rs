// Rust face of `js/diag.js`: a crash-surviving event log in localStorage,
// for debugging pages the browser kills and reloads (e.g. iOS under memory
// pressure). Logs enough to see what was happening; sentence text is
// truncated (`snippet`).

use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/diag.js")]
extern "C" {
    #[wasm_bindgen(js_name = log)]
    fn log_js(msg: &str);
    #[wasm_bindgen(js_name = previousEnd)]
    fn previous_end_js() -> JsValue;
    #[wasm_bindgen(js_name = entriesText)]
    pub fn entries_text() -> String;
    #[wasm_bindgen(js_name = clearLog)]
    pub fn clear_log();
}

pub fn log(msg: impl AsRef<str>) {
    log_js(msg.as_ref());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndKind {
    /// Closed while hidden: iOS discards backgrounded pages and home-screen
    /// apps without notice. Expected.
    Background,
    /// Killed while on screen (e.g. memory pressure).
    Crash,
}

/// The previous page load, if it ended without a normal `pagehide`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousEnd {
    pub kind: EndKind,
    pub started_at: f64,
    pub last_event_at: f64,
    pub last_event: String,
}

pub fn previous_end() -> Option<PreviousEnd> {
    serde_wasm_bindgen::from_value(previous_end_js())
        .ok()
        .flatten()
}

/// Keeps log lines short and avoids dumping whole sentences.
pub fn snippet(text: &str) -> String {
    let mut s: String = text.chars().take(40).collect();
    if text.chars().count() > 40 {
        s.push('…');
    }
    s
}
