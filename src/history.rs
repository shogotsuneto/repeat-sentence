// Rust face of `js/history.js`: practice attempts and their recordings
// persisted in IndexedDB.

use wasm_bindgen::prelude::*;

use crate::audio::call;
use crate::model::AttemptRecord;

#[wasm_bindgen(module = "/js/history.js")]
extern "C" {
    #[wasm_bindgen(js_name = saveAttempt)]
    fn save_attempt_js(meta: JsValue, audio_url: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = listAttempts)]
    fn list_attempts_js() -> js_sys::Promise;
    #[wasm_bindgen(js_name = audioUrl)]
    fn audio_url_js(id: u32) -> js_sys::Promise;
    #[wasm_bindgen(js_name = deleteAttempt)]
    fn delete_attempt_js(id: u32) -> js_sys::Promise;
    #[wasm_bindgen(js_name = clearAttempts)]
    fn clear_attempts_js() -> js_sys::Promise;
    #[wasm_bindgen(js_name = storageUsage)]
    fn storage_usage_js() -> js_sys::Promise;
}

/// Saves the attempt with the recording behind `audio_url` (a live blob
/// URL). Returns the record with its new `id`.
pub async fn save(mut record: AttemptRecord, audio_url: &str) -> Result<AttemptRecord, String> {
    let meta = serde_wasm_bindgen::to_value(&record).map_err(|e| e.to_string())?;
    let id = call(save_attempt_js(meta, audio_url)).await?;
    record.id = id.as_f64().map(|n| n as u32);
    Ok(record)
}

/// All saved attempts, newest first.
pub async fn list() -> Result<Vec<AttemptRecord>, String> {
    let v = call(list_attempts_js()).await?;
    let mut records: Vec<AttemptRecord> =
        serde_wasm_bindgen::from_value(v).map_err(|e| e.to_string())?;
    records.sort_by(|a, b| b.at_ms.total_cmp(&a.at_ms));
    Ok(records)
}

/// A fresh blob URL for a saved recording; release it with
/// [`crate::audio::revoke_url`].
pub async fn audio_url(id: u32) -> Result<Option<String>, String> {
    call(audio_url_js(id)).await.map(|v| v.as_string())
}

pub async fn delete(id: u32) -> Result<(), String> {
    call(delete_attempt_js(id)).await.map(|_| ())
}

pub async fn clear() -> Result<(), String> {
    call(clear_attempts_js()).await.map(|_| ())
}

/// Bytes used by this site's storage, when the browser reports it.
pub async fn storage_usage() -> Option<f64> {
    call(storage_usage_js()).await.ok()?.as_f64()
}
