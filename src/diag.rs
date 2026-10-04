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
    #[wasm_bindgen(js_name = previousCrash)]
    fn previous_crash_js() -> JsValue;
    #[wasm_bindgen(js_name = entriesText)]
    pub fn entries_text() -> String;
    #[wasm_bindgen(js_name = clearLog)]
    pub fn clear_log();
}

pub fn log(msg: impl AsRef<str>) {
    log_js(msg.as_ref());
}

/// The previous page load, if it ended without a normal `pagehide`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviousCrash {
    pub started_at: f64,
    pub last_event_at: f64,
    pub last_event: String,
}

pub fn previous_crash() -> Option<PreviousCrash> {
    serde_wasm_bindgen::from_value(previous_crash_js())
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
