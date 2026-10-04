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
    #[wasm_bindgen(js_name = playUrl)]
    fn play_url_js(url: &str) -> js_sys::Promise;
    /// Call synchronously from a click handler before playing generated
    /// audio outside the practice flow (e.g. previews).
    #[wasm_bindgen(js_name = unlockPlayback)]
    pub fn unlock_playback();
    #[wasm_bindgen(js_name = prime)]
    fn prime_js() -> js_sys::Promise;
    #[wasm_bindgen(js_name = record)]
    fn record_js(
        max_ms: u32,
        silence_ms: u32,
        delay_ms: u32,
        beep_ms: u32,
        on_start: &Closure<dyn FnMut()>,
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

pub(crate) fn js_error(e: JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|err| String::from(err.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}

pub(crate) async fn call(promise: js_sys::Promise) -> Result<JsValue, String> {
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
/// Plays generated audio with the same contract as [`speak`]; stopped by
/// [`stop_speaking`].
pub async fn play_url(url: &str) -> Result<bool, String> {
    call(play_url_js(url))
        .await
        .map(|v| v.as_bool().unwrap_or(false))
}

pub async fn speak(text: &str, voice_uri: &str, rate: f32) -> Result<bool, String> {
    call(speak_js(text, voice_uri, rate))
        .await
        .map(|v| v.as_bool().unwrap_or(false))
}

/// Call from the Start click: unlocks audio playback and gets microphone
/// permission up front (asking now if needed, then releasing the mic).
pub async fn prime() -> Result<(), String> {
    call(prime_js()).await.map(|_| ())
}

/// Waits `delay_ms` (opening the mic meanwhile), beeps for `beep_ms`
/// (0 = no beep), then records until
/// stopped, timed out, or silent for `silence_ms` (0 = never). The mic is
/// held only for the duration of this call. `on_start` fires when recording
/// begins; `on_tick(level, elapsed_ms, silent_ms)` every ~50 ms after.
/// `Ok(None)` means it was cancelled and nothing was kept.
pub async fn record(
    max_ms: u32,
    silence_ms: u32,
    delay_ms: u32,
    beep_ms: u32,
    on_start: impl FnMut() + 'static,
    on_tick: impl FnMut(f64, f64, f64) + 'static,
) -> Result<Option<Recording>, String> {
    let start_cb = Closure::<dyn FnMut()>::new(on_start);
    let tick_cb = Closure::<dyn FnMut(f64, f64, f64)>::new(on_tick);
    // The closures must outlive the promise: JS stops calling them before it
    // settles.
    let v = call(record_js(
        max_ms, silence_ms, delay_ms, beep_ms, &start_cb, &tick_cb,
    ))
    .await?;
    drop((start_cb, tick_cb));
    if v.is_null() || v.is_undefined() {
        return Ok(None);
    }
    serde_wasm_bindgen::from_value(v)
        .map(Some)
        .map_err(|e| e.to_string())
}
