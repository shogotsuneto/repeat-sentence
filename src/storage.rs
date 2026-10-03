// localStorage persistence. The data is a few KB of text (settings, voice
// presets, imported sentences), so a synchronous key/value store is enough;
// recordings are deliberately never persisted.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::model::{SentenceSet, Settings};

// Bump the suffix for incompatible format changes; additive changes are
// absorbed by `#[serde(default)]`.
const SETTINGS_KEY: &str = "repeat-sentence.settings.v1";
const SETS_KEY: &str = "repeat-sentence.sets.v1";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    let raw = local_storage()?.get_item(key).ok()??;
    serde_json::from_str(&raw)
        .inspect_err(|e| leptos::logging::warn!("ignoring unreadable {key}: {e}"))
        .ok()
}

fn save<T: Serialize>(key: &str, value: &T) {
    let (Some(store), Ok(json)) = (local_storage(), serde_json::to_string(value)) else {
        return;
    };
    if let Err(e) = store.set_item(key, &json) {
        leptos::logging::warn!("failed to save {key}: {e:?}");
    }
}

/// `None` on first visit, so the app can seed defaults that depend on the
/// browser (e.g. suggested voices).
pub fn load_settings() -> Option<Settings> {
    load(SETTINGS_KEY)
}

pub fn save_settings(settings: &Settings) {
    save(SETTINGS_KEY, settings);
}

pub fn load_sets() -> Vec<SentenceSet> {
    load(SETS_KEY).unwrap_or_default()
}

pub fn save_sets(sets: &[SentenceSet]) {
    save(SETS_KEY, &sets);
}

/// Monotonic-enough id for presets and sets: ms timestamp plus a counter to
/// separate items created in the same millisecond.
pub fn new_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    (js_sys::Date::now() as u64) * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}
