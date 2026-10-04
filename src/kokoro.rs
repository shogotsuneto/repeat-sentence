// Rust face of `js/kokoro.js`: the on-device Kokoro TTS model.

use wasm_bindgen::prelude::*;

use crate::audio::call;
use crate::model::KokoroBackend;

#[wasm_bindgen(module = "/js/kokoro.js")]
extern "C" {
    #[wasm_bindgen(js_name = detectBackend)]
    fn detect_backend_js() -> js_sys::Promise;
    #[wasm_bindgen(js_name = loadedBackend)]
    fn loaded_backend_js() -> Option<String>;
    #[wasm_bindgen(js_name = load)]
    fn load_js(backend: &str, on_progress: &Closure<dyn FnMut(f64, f64)>) -> js_sys::Promise;
    #[wasm_bindgen(js_name = isCached)]
    fn is_cached_js(backend: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = forget)]
    fn forget_js() -> js_sys::Promise;
    #[wasm_bindgen(js_name = generate)]
    fn generate_js(text: &str, voice: &str, speed: f32) -> js_sys::Promise;
}

/// Loading state of the model, for the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    NotLoaded,
    /// Download / initialisation in progress; `None` before byte counts arrive.
    Loading(Option<f64>),
    Ready(KokoroBackend),
    Failed(String),
}

/// The backend `Auto` resolves to on this device.
pub async fn detect_backend() -> KokoroBackend {
    call(detect_backend_js())
        .await
        .ok()
        .and_then(|v| v.as_string())
        .and_then(|k| KokoroBackend::from_key(&k))
        .unwrap_or(KokoroBackend::WasmQ8)
}

/// `Auto` resolved to a concrete backend for this device.
pub async fn resolve(backend: KokoroBackend) -> KokoroBackend {
    match backend {
        KokoroBackend::Auto => detect_backend().await,
        b => b,
    }
}

/// Whether `backend`'s weights are already downloaded.
pub async fn is_cached(backend: KokoroBackend) -> bool {
    let Some(key) = resolve(backend).await.key() else {
        return false;
    };
    call(is_cached_js(key))
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub fn loaded_backend() -> Option<KokoroBackend> {
    loaded_backend_js().and_then(|k| KokoroBackend::from_key(&k))
}

/// Loads the model (downloading it on first use). `on_progress(fraction)`
/// reports download progress. Returns the concrete backend used.
pub async fn load(
    backend: KokoroBackend,
    mut on_progress: impl FnMut(f64) + 'static,
) -> Result<KokoroBackend, String> {
    let backend = resolve(backend).await;
    let key = backend.key().unwrap_or("wasm/q8");
    let cb = Closure::<dyn FnMut(f64, f64)>::new(move |done: f64, total: f64| {
        if total > 0.0 {
            on_progress(done / total);
        }
    });
    call(load_js(key, &cb)).await?;
    drop(cb);
    Ok(backend)
}

/// Starts generating `text` and returns the pending promise, so generation
/// can overlap other waiting (e.g. the pre-prompt delay). Resolves a WAV
/// blob URL owned by the JS cache.
pub fn start_generate(text: &str, voice: &str, speed: f32) -> js_sys::Promise {
    generate_js(text, voice, speed)
}

pub async fn finish_generate(pending: js_sys::Promise) -> Result<String, String> {
    call(pending)
        .await?
        .as_string()
        .ok_or_else(|| "Kokoro returned no audio".to_string())
}

/// Unloads the model and deletes its cached download.
pub async fn forget() -> Result<(), String> {
    call(forget_js()).await.map(|_| ())
}
