// Rust face of `js/audio.js`. The JS side owns the browser objects
// (utterances, MediaRecorder, AudioContext) and their event plumbing; this
// module turns its Promises into typed async functions.

use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::model::VoiceInfo;

#[wasm_bindgen(module = "/js/audio.js")]
extern "C" {
    #[wasm_bindgen(js_name = speechSupported)]
    pub fn speech_supported() -> bool;
    #[wasm_bindgen(js_name = recordingSupported)]
    pub fn recording_supported() -> bool;
    #[wasm_bindgen(js_name = loadVoices)]
    fn load_voices_js(timeout_ms: u32) -> js_sys::Promise;
    #[wasm_bindgen(js_name = onVoicesChanged)]
    fn on_voices_changed_js(cb: &Closure<dyn FnMut(JsValue)>);
    #[wasm_bindgen(js_name = speak)]
    fn speak_js(text: &str, voice_uri: &str, rate: f32) -> js_sys::Promise;
    #[wasm_bindgen(js_name = stopSpeaking)]
    pub fn stop_speaking();
    #[wasm_bindgen(js_name = micReady)]
    pub fn mic_ready() -> bool;
    #[wasm_bindgen(js_name = ensureMic)]
    fn ensure_mic_js() -> js_sys::Promise;
    #[wasm_bindgen(js_name = beep)]
    fn beep_js(duration_ms: u32) -> js_sys::Promise;
    #[wasm_bindgen(js_name = record)]
    fn record_js(
        max_ms: u32,
        silence_ms: u32,
        on_tick: &Closure<dyn FnMut(f64, f64, f64)>,
    ) -> js_sys::Promise;
    #[wasm_bindgen(js_name = stopRecording)]
    pub fn stop_recording();
    #[wasm_bindgen(js_name = cancelRecording)]
    pub fn cancel_recording();
    #[wasm_bindgen(js_name = revokeUrl)]
    pub fn revoke_url(url: &str);
}

/// A finished recording. `url` is a blob URL owned by the caller — release
/// it with [`revoke_url`] when the attempt is discarded.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub url: String,
    pub mime: String,
    pub duration_ms: u32,
    /// Why it stopped: `manual`, `timeout` or `silence`.
    pub reason: String,
}

fn js_error(e: JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|err| String::from(err.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}

async fn call(promise: js_sys::Promise) -> Result<JsValue, String> {
    JsFuture::from(promise).await.map_err(js_error)
}

fn voices_from_js(v: JsValue) -> Vec<VoiceInfo> {
    serde_wasm_bindgen::from_value(v).unwrap_or_default()
}

pub async fn load_voices() -> Vec<VoiceInfo> {
    call(load_voices_js(2000))
        .await
        .map(voices_from_js)
        .unwrap_or_default()
}

/// Registers an app-lifetime listener; the closure is intentionally leaked.
pub fn on_voices_changed(mut f: impl FnMut(Vec<VoiceInfo>) + 'static) {
    let cb = Closure::<dyn FnMut(JsValue)>::new(move |v| f(voices_from_js(v)));
    on_voices_changed_js(&cb);
    cb.forget();
}

/// `Ok(true)` when the sentence was read to the end, `Ok(false)` when it was
/// cancelled. An empty `voice_uri` uses the browser's default voice.
pub async fn speak(text: &str, voice_uri: &str, rate: f32) -> Result<bool, String> {
    call(speak_js(text, voice_uri, rate))
        .await
        .map(|v| v.as_bool().unwrap_or(false))
}

pub async fn ensure_mic() -> Result<(), String> {
    call(ensure_mic_js()).await.map(|_| ())
}

pub async fn beep(duration_ms: u32) {
    let _ = call(beep_js(duration_ms)).await;
}

/// Records until stopped, timed out, or silent for `silence_ms` (0 = never).
/// `on_tick(level, elapsed_ms, silent_ms)` fires every ~50 ms. `Ok(None)`
/// means the recording was cancelled and nothing was kept.
pub async fn record(
    max_ms: u32,
    silence_ms: u32,
    on_tick: impl FnMut(f64, f64, f64) + 'static,
) -> Result<Option<Recording>, String> {
    let cb = Closure::<dyn FnMut(f64, f64, f64)>::new(on_tick);
    // `cb` must outlive the promise: JS stops ticking before it settles.
    let v = call(record_js(max_ms, silence_ms, &cb)).await?;
    drop(cb);
    if v.is_null() || v.is_undefined() {
        return Ok(None);
    }
    serde_wasm_bindgen::from_value(v)
        .map(Some)
        .map_err(|e| e.to_string())
}
