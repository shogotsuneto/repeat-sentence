// Domain types and pure helpers. Nothing here touches the browser, so it is
// all covered by native `cargo test`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::sentences::BUILTIN;

/// A speech-synthesis voice as reported by the browser. `uri` is the stable
/// identifier (`SpeechSynthesisVoice.voiceURI`) presets are keyed on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub uri: String,
    pub name: String,
    pub lang: String,
    #[serde(default)]
    pub local: bool,
}

impl VoiceInfo {
    /// `en_US` (Android) and `en-US` both normalize to `en-US`.
    pub fn lang_tag(&self) -> String {
        self.lang.replace('_', "-")
    }

    pub fn is_english(&self) -> bool {
        self.lang_tag().to_ascii_lowercase().starts_with("en")
    }

    /// macOS ships joke voices (Bubbles, Zarvox, ...) that are useless for
    /// listening practice; they are hidden unless the user asks for all.
    pub fn is_novelty(&self) -> bool {
        let base = self.name.split(" (").next().unwrap_or(&self.name);
        NOVELTY_VOICES.contains(&base)
    }

    /// Apple platforms list the same voice name at several quality levels
    /// (e.g. `com.apple.voice.compact.en-US.Samantha` and
    /// `com.apple.voice.enhanced.en-US.Samantha`); only the URI tells them
    /// apart.
    pub fn quality(&self) -> Option<&'static str> {
        let uri = self.uri.to_ascii_lowercase();
        let name = self.name.to_ascii_lowercase();
        if uri.contains(".premium.") || name.contains("premium") {
            Some("Premium")
        } else if uri.contains(".enhanced.") || name.contains("enhanced") {
            Some("Enhanced")
        } else {
            None
        }
    }

    /// The name without a trailing language description, which the label
    /// already shows as a tag ("Daniel (English (United Kingdom))" → "Daniel").
    fn short_name(&self) -> &str {
        match self.name.split_once(" (English") {
            Some((base, _)) if !base.is_empty() => base,
            _ => &self.name,
        }
    }

    pub fn label(&self) -> String {
        let name = self.short_name();
        match self.quality() {
            // Don't repeat a quality the name already carries ("Ava (Premium)").
            Some(q) if !name.contains(q) => format!("{name} · {q} ({})", self.lang_tag()),
            _ => format!("{name} ({})", self.lang_tag()),
        }
    }
}

/// Labels for `voices`, made unique. Some platforms list a voice twice
/// under the same name (e.g. two "Daniel (en-GB)" with different URIs);
/// colliding labels get the first URI segment that tells them apart
/// ("compact" / "enhanced"), else on-device / network, else a number.
pub fn unique_labels(voices: &[VoiceInfo]) -> Vec<String> {
    let base: Vec<String> = voices.iter().map(VoiceInfo::label).collect();
    let mut out = base.clone();
    for (i, label) in base.iter().enumerate() {
        let group: Vec<usize> = (0..voices.len()).filter(|&j| &base[j] == label).collect();
        if group.len() < 2 {
            continue;
        }
        let segments = |k: usize| -> Vec<&str> { voices[k].uri.split(['.', '/', ':']).collect() };
        let differing = (0..)
            .take_while(|&n| group.iter().any(|&k| segments(k).len() > n))
            .find(|&n| {
                let first = segments(group[0]).get(n).copied();
                group.iter().any(|&k| segments(k).get(n).copied() != first)
            });
        let distinct = |tag: &dyn Fn(usize) -> String| {
            let tags: Vec<String> = group.iter().map(|&k| tag(k)).collect();
            let unique = tags
                .iter()
                .enumerate()
                .all(|(a, t)| tags.iter().skip(a + 1).all(|u| u != t));
            unique.then(|| tag(i))
        };
        let suffix = differing
            .and_then(|n| distinct(&|k| segments(k).get(n).unwrap_or(&"").to_string()))
            .filter(|t| !t.is_empty())
            .or_else(|| {
                distinct(&|k| {
                    if voices[k].local {
                        "on-device"
                    } else {
                        "network"
                    }
                    .to_string()
                })
            })
            .unwrap_or_else(|| {
                let pos = group.iter().position(|&k| k == i).unwrap_or(0);
                format!("#{}", pos + 1)
            });
        out[i] = format!("{label} · {suffix}");
    }
    out
}

const NOVELTY_VOICES: &[&str] = &[
    "Albert",
    "Bad News",
    "Bahh",
    "Bells",
    "Boing",
    "Bubbles",
    "Cellos",
    "Eddy",
    "Flo",
    "Fred",
    "Good News",
    "Grandma",
    "Grandpa",
    "Jester",
    "Junior",
    "Kathy",
    "Organ",
    "Ralph",
    "Reed",
    "Rocko",
    "Sandy",
    "Shelley",
    "Superstar",
    "Trinoids",
    "Whisper",
    "Wobble",
    "Zarvox",
];

/// Accents worth covering, in the order suggestions are listed.
const ACCENTS: &[&str] = &[
    "en-US", "en-GB", "en-AU", "en-CA", "en-IN", "en-IE", "en-NZ", "en-ZA",
];

/// Picks one good-sounding voice per English accent, preferring the
/// higher-quality network / neural voices browsers expose.
pub fn suggest_voices(voices: &[VoiceInfo]) -> Vec<VoiceInfo> {
    fn score(v: &VoiceInfo) -> u8 {
        let n = v.name.to_ascii_lowercase();
        let quality = match v.quality() {
            Some("Premium") => 3,
            Some(_) => 2,
            None if ["natural", "neural", "online", "google"]
                .iter()
                .any(|k| n.contains(k)) =>
            {
                2
            }
            None => 0,
        };
        quality * 2 + u8::from(v.local)
    }

    ACCENTS
        .iter()
        .filter_map(|accent| {
            voices
                .iter()
                .filter(|v| !v.is_novelty() && v.lang_tag().eq_ignore_ascii_case(accent))
                .max_by_key(|v| score(v))
                .cloned()
        })
        .collect()
}

/// Which speech engine reads a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Engine {
    /// The browser's Web Speech API; `voice_uri` is a `voiceURI`.
    #[default]
    Browser,
    /// On-device Kokoro model; `voice_uri` is a Kokoro voice id (`af_heart`).
    Kokoro,
}

/// One entry in the user's voice pool: a voice at a given speaking rate.
/// Each question picks one of these at random.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoicePreset {
    pub id: u64,
    /// Older presets predate Kokoro and are all browser voices.
    #[serde(default)]
    pub engine: Engine,
    pub voice_uri: String,
    pub voice_label: String,
    pub rate: f32,
}

/// A Kokoro voice. `grade` is the model card's overall grade (A best).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KokoroVoice {
    pub id: &'static str,
    pub name: &'static str,
    pub accent: &'static str,
    pub female: bool,
    pub grade: &'static str,
    /// Included by "Add recommended Kokoro voices".
    pub recommended: bool,
}

impl KokoroVoice {
    pub fn label(&self) -> String {
        let gender = if self.female { "F" } else { "M" };
        format!("Kokoro {} ({}, {gender})", self.name, self.accent)
    }

    pub fn find(id: &str) -> Option<&'static KokoroVoice> {
        KOKORO_VOICES.iter().find(|v| v.id == id)
    }
}

const fn kv(
    id: &'static str,
    name: &'static str,
    accent: &'static str,
    female: bool,
    grade: &'static str,
    recommended: bool,
) -> KokoroVoice {
    KokoroVoice {
        id,
        name,
        accent,
        female,
        grade,
        recommended,
    }
}

/// Kokoro v1.0's English voices (it has no Australian / Indian ones).
pub const KOKORO_VOICES: &[KokoroVoice] = &[
    kv("af_heart", "Heart", "en-US", true, "A", true),
    kv("af_bella", "Bella", "en-US", true, "A-", true),
    kv("af_nicole", "Nicole", "en-US", true, "B-", false),
    kv("af_aoede", "Aoede", "en-US", true, "C+", false),
    kv("af_kore", "Kore", "en-US", true, "C+", false),
    kv("af_sarah", "Sarah", "en-US", true, "C+", false),
    kv("af_alloy", "Alloy", "en-US", true, "C", false),
    kv("af_nova", "Nova", "en-US", true, "C", false),
    kv("af_sky", "Sky", "en-US", true, "C-", false),
    kv("af_jessica", "Jessica", "en-US", true, "D", false),
    kv("af_river", "River", "en-US", true, "D", false),
    kv("am_fenrir", "Fenrir", "en-US", false, "C+", true),
    kv("am_michael", "Michael", "en-US", false, "C+", true),
    kv("am_puck", "Puck", "en-US", false, "C+", false),
    kv("am_echo", "Echo", "en-US", false, "D", false),
    kv("am_eric", "Eric", "en-US", false, "D", false),
    kv("am_liam", "Liam", "en-US", false, "D", false),
    kv("am_onyx", "Onyx", "en-US", false, "D", false),
    kv("am_santa", "Santa", "en-US", false, "D-", false),
    kv("am_adam", "Adam", "en-US", false, "F+", false),
    kv("bf_emma", "Emma", "en-GB", true, "B-", true),
    kv("bf_isabella", "Isabella", "en-GB", true, "C", true),
    kv("bf_alice", "Alice", "en-GB", true, "D", false),
    kv("bf_lily", "Lily", "en-GB", true, "D", false),
    kv("bm_fable", "Fable", "en-GB", false, "C", true),
    kv("bm_george", "George", "en-GB", false, "C", true),
    kv("bm_lewis", "Lewis", "en-GB", false, "D+", false),
    kv("bm_daniel", "Daniel", "en-GB", false, "D", false),
];

/// How the Kokoro model runs. `Auto` picks the best the device supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum KokoroBackend {
    #[default]
    Auto,
    WebGpuFp16,
    WebGpuFp32,
    WasmQ8,
}

impl KokoroBackend {
    pub const ALL: [KokoroBackend; 4] = [
        KokoroBackend::Auto,
        KokoroBackend::WebGpuFp16,
        KokoroBackend::WebGpuFp32,
        KokoroBackend::WasmQ8,
    ];

    /// The `device/dtype` key `js/kokoro.js` understands; `None` for Auto.
    pub fn key(self) -> Option<&'static str> {
        match self {
            KokoroBackend::Auto => None,
            KokoroBackend::WebGpuFp16 => Some("webgpu/fp16"),
            KokoroBackend::WebGpuFp32 => Some("webgpu/fp32"),
            KokoroBackend::WasmQ8 => Some("wasm/q8"),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.key() == Some(key))
    }

    pub fn describe(self) -> &'static str {
        match self {
            KokoroBackend::Auto => "Auto (best for this device)",
            KokoroBackend::WebGpuFp16 => "WebGPU · fp16 — 163 MB, fast",
            KokoroBackend::WebGpuFp32 => "WebGPU · fp32 — 326 MB, fast",
            KokoroBackend::WasmQ8 => "CPU (WASM) · q8 — 92 MB, slow",
        }
    }
}

/// When the sentence text is shown on the practice screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Reveal {
    /// Hidden until the attempt is over — the exam condition.
    #[default]
    AfterAttempt,
    Always,
}

/// Persisted user settings. `#[serde(default)]` keeps older stored records
/// loadable when fields are added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub builtin_enabled: bool,
    pub presets: Vec<VoicePreset>,
    /// Start recording as soon as the prompt finishes playing.
    pub auto_record: bool,
    /// Short tone right before recording starts.
    pub beep: bool,
    /// Pause between the end of the prompt and the start of recording.
    pub record_delay_ms: u32,
    pub max_record_secs: u32,
    /// Stop recording after this long without voice. 0 disables it.
    pub silence_stop_secs: u32,
    /// Pause between pressing Next and the prompt starting.
    pub pre_delay_secs: u32,
    pub kokoro_backend: KokoroBackend,
    /// Set once the Kokoro model has loaded successfully: it is cached, so
    /// the app loads it again in the background on later visits.
    pub kokoro_enabled: bool,
    pub reveal: Reveal,
}

impl Default for Settings {
    fn default() -> Self {
        // Mirrors the exam: recording opens right after the audio, closes
        // after 15 s or 3 s of silence.
        Self {
            builtin_enabled: true,
            presets: Vec::new(),
            auto_record: true,
            beep: true,
            record_delay_ms: 1500,
            max_record_secs: 15,
            silence_stop_secs: 3,
            pre_delay_secs: 1,
            kokoro_backend: KokoroBackend::Auto,
            kokoro_enabled: false,
            reveal: Reveal::AfterAttempt,
        }
    }
}

/// A named group of user-imported sentences that can be toggled on and off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentenceSet {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub sentences: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PoolEntry {
    pub text: String,
    pub source: String,
}

pub const BUILTIN_SOURCE: &str = "Built-in";

/// Every sentence currently eligible for practice, de-duplicated across sets.
pub fn sentence_pool(settings: &Settings, sets: &[SentenceSet]) -> Vec<PoolEntry> {
    let builtin = settings
        .builtin_enabled
        .then(|| BUILTIN.iter().map(|s| (s.to_string(), BUILTIN_SOURCE)))
        .into_iter()
        .flatten();
    let imported = sets
        .iter()
        .filter(|s| s.enabled)
        .flat_map(|s| s.sentences.iter().map(|t| (t.clone(), s.name.as_str())));

    let mut seen = std::collections::HashSet::new();
    builtin
        .chain(imported)
        .filter(|(text, _)| seen.insert(text.clone()))
        .map(|(text, source)| PoolEntry {
            text,
            source: source.to_string(),
        })
        .collect()
}

/// One saved practice attempt (the recording itself is stored separately,
/// keyed by `id`). Field names are camelCase on the JS / IndexedDB side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttemptRecord {
    /// Assigned by IndexedDB on insert; `None` for an attempt that couldn't
    /// be saved. Omitted when serializing so the store auto-increments it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// Unix epoch milliseconds.
    pub at_ms: f64,
    pub text: String,
    pub source: String,
    pub voice_label: String,
    pub rate: f32,
    pub duration_ms: u32,
    /// Why recording stopped: `manual`, `timeout` or `silence`.
    pub reason: String,
    pub mime: String,
}

/// How many times each sentence (by text) has been practised.
pub fn practice_counts(history: &[AttemptRecord]) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for r in history {
        *counts.entry(r.text.clone()).or_default() += 1;
    }
    counts
}

/// Draws indices without replacement so every sentence comes up once before
/// any repeats, and never the same one twice in a row across refills.
#[derive(Debug, Default)]
pub struct ShuffleBag {
    queue: Vec<usize>,
    len: usize,
    last: Option<usize>,
}

impl ShuffleBag {
    pub fn reset(&mut self) {
        self.queue.clear();
        self.last = None;
    }

    pub fn next(&mut self, len: usize, rand: &mut impl FnMut() -> f64) -> Option<usize> {
        if len == 0 {
            return None;
        }
        if len != self.len {
            self.reset();
            self.len = len;
        }
        if self.queue.is_empty() {
            self.queue = (0..len).collect();
            shuffle(&mut self.queue, rand);
            // `pop` takes from the end; keep the previous pick off that slot.
            if len > 1 && self.queue.last() == self.last.as_ref() {
                self.queue.swap(0, len - 1);
            }
        }
        let picked = self.queue.pop();
        self.last = picked;
        picked
    }
}

/// Uniform index in `0..len` from a `[0, 1)` random source. `len` must be > 0.
pub fn pick_index(len: usize, rand: &mut impl FnMut() -> f64) -> usize {
    ((rand() * len as f64) as usize).min(len - 1)
}

fn shuffle<T>(items: &mut [T], rand: &mut impl FnMut() -> f64) {
    for i in (1..items.len()).rev() {
        items.swap(i, pick_index(i + 1, rand));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic `[0, 1)` source (LCG) so tests are reproducible.
    fn rng(seed: u64) -> impl FnMut() -> f64 {
        let mut s = seed;
        move || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn voice(name: &str, lang: &str) -> VoiceInfo {
        VoiceInfo {
            uri: name.into(),
            name: name.into(),
            lang: lang.into(),
            local: true,
        }
    }

    #[test]
    fn shuffle_bag_covers_all_before_repeating() {
        let mut bag = ShuffleBag::default();
        let mut r = rng(1);
        for _ in 0..5 {
            let mut round: Vec<_> = (0..7).map(|_| bag.next(7, &mut r).unwrap()).collect();
            round.sort();
            assert_eq!(round, (0..7).collect::<Vec<_>>());
        }
    }

    #[test]
    fn shuffle_bag_never_repeats_back_to_back() {
        let mut bag = ShuffleBag::default();
        let mut r = rng(42);
        let mut prev = None;
        for _ in 0..500 {
            let n = bag.next(3, &mut r);
            assert_ne!(n, prev);
            prev = n;
        }
    }

    #[test]
    fn shuffle_bag_handles_empty_and_resize() {
        let mut bag = ShuffleBag::default();
        let mut r = rng(7);
        assert_eq!(bag.next(0, &mut r), None);
        assert_eq!(bag.next(1, &mut r), Some(0));
        assert_eq!(bag.next(1, &mut r), Some(0));
        assert!(bag.next(4, &mut r).unwrap() < 4);
    }

    #[test]
    fn pick_index_stays_in_range() {
        assert_eq!(pick_index(3, &mut || 0.0), 0);
        assert_eq!(pick_index(3, &mut || 0.999_999), 2);
        assert_eq!(pick_index(3, &mut || 1.0), 2);
    }

    #[test]
    fn pool_dedupes_and_respects_toggles() {
        let mut settings = Settings::default();
        let sets = vec![
            SentenceSet {
                id: 1,
                name: "a".into(),
                enabled: true,
                sentences: vec!["One.".into(), BUILTIN[0].into()],
            },
            SentenceSet {
                id: 2,
                name: "b".into(),
                enabled: false,
                sentences: vec!["Two.".into()],
            },
        ];
        let pool = sentence_pool(&settings, &sets);
        assert_eq!(pool.len(), BUILTIN.len() + 1);
        assert_eq!(pool[0].source, BUILTIN_SOURCE);

        settings.builtin_enabled = false;
        let pool = sentence_pool(&settings, &sets);
        let texts: Vec<_> = pool.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, vec!["One.", BUILTIN[0]]);
        assert!(pool.iter().all(|p| p.source == "a"));
    }

    #[test]
    fn voice_classification() {
        assert!(voice("Daniel", "en_GB").is_english());
        assert_eq!(voice("Daniel", "en_GB").lang_tag(), "en-GB");
        assert!(!voice("Kyoko", "ja-JP").is_english());
        assert!(voice("Zarvox", "en-US").is_novelty());
        assert!(voice("Eddy (English (US))", "en-US").is_novelty());
        assert!(!voice("Samantha", "en-US").is_novelty());
        assert_eq!(
            voice("Daniel (English (United Kingdom))", "en-GB").label(),
            "Daniel (en-GB)"
        );
    }

    #[test]
    fn apple_voice_quality_from_uri() {
        let v = |uri: &str, name: &str| VoiceInfo {
            uri: uri.into(),
            name: name.into(),
            lang: "en-US".into(),
            local: true,
        };
        let compact = v("com.apple.voice.compact.en-US.Samantha", "Samantha");
        let enhanced = v("com.apple.voice.enhanced.en-US.Samantha", "Samantha");
        let premium = v("com.apple.voice.premium.en-US.Zoe", "Zoe (Premium)");
        assert_eq!(compact.quality(), None);
        assert_eq!(compact.label(), "Samantha (en-US)");
        assert_eq!(enhanced.label(), "Samantha · Enhanced (en-US)");
        assert_eq!(premium.label(), "Zoe (Premium) (en-US)");
        // Same name at two quality levels: the better one is suggested.
        let picked = suggest_voices(&[compact, enhanced.clone()]);
        assert_eq!(picked, vec![enhanced]);
    }

    #[test]
    fn duplicate_labels_are_disambiguated() {
        let v = |uri: &str, name: &str, local: bool| VoiceInfo {
            uri: uri.into(),
            name: name.into(),
            lang: "en-GB".into(),
            local,
        };
        // Differ in a URI segment.
        let labels = unique_labels(&[
            v("com.apple.speech.synthesis.voice.daniel", "Daniel", true),
            v("com.apple.voice.compact.en-GB.Daniel", "Daniel", true),
            v("com.apple.voice.compact.en-GB.Arthur", "Arthur", true),
        ]);
        assert_eq!(
            labels,
            vec![
                "Daniel (en-GB) · speech",
                "Daniel (en-GB) · voice",
                "Arthur (en-GB)"
            ]
        );
        // Same URI shape: fall back to on-device / network, then a number.
        let labels = unique_labels(&[v("Daniel", "Daniel", true), v("Daniel", "Daniel", false)]);
        assert_eq!(
            labels,
            vec!["Daniel (en-GB) · on-device", "Daniel (en-GB) · network"]
        );
        let labels = unique_labels(&[v("Daniel", "Daniel", true), v("Daniel", "Daniel", true)]);
        assert_eq!(labels, vec!["Daniel (en-GB) · #1", "Daniel (en-GB) · #2"]);
    }

    #[test]
    fn suggestions_pick_best_voice_per_accent() {
        let voices = vec![
            voice("Bubbles", "en-US"),
            voice("Samantha", "en-US"),
            VoiceInfo {
                local: false,
                ..voice("Google US English", "en-US")
            },
            voice("Daniel", "en-GB"),
            voice("Karen", "en-AU"),
            voice("Kyoko", "ja-JP"),
        ];
        let names: Vec<_> = suggest_voices(&voices)
            .into_iter()
            .map(|v| v.name)
            .collect();
        assert_eq!(names, vec!["Google US English", "Daniel", "Karen"]);
    }

    fn attempt(text: &str) -> AttemptRecord {
        AttemptRecord {
            id: None,
            at_ms: 0.0,
            text: text.into(),
            source: "s".into(),
            voice_label: "v".into(),
            rate: 1.0,
            duration_ms: 1000,
            reason: "manual".into(),
            mime: "audio/webm".into(),
        }
    }

    #[test]
    fn counts_attempts_per_sentence() {
        let counts = practice_counts(&[attempt("A."), attempt("B."), attempt("A.")]);
        assert_eq!(counts.get("A."), Some(&2));
        assert_eq!(counts.get("B."), Some(&1));
        assert_eq!(counts.get("C."), None);
    }

    #[test]
    fn attempt_record_omits_missing_id() {
        let json = serde_json::to_value(attempt("A.")).unwrap();
        assert!(json.get("id").is_none());
        assert_eq!(json["atMs"], 0.0);
        assert_eq!(json["voiceLabel"], "v");
        let back: AttemptRecord = serde_json::from_value(
            serde_json::json!({"id": 7, "atMs": 1.0, "text": "A.", "source": "s",
                "voiceLabel": "v", "rate": 1.0, "durationMs": 5, "reason": "silence", "mime": "m"}),
        )
        .unwrap();
        assert_eq!(back.id, Some(7));
    }

    #[test]
    fn kokoro_voices_and_backends() {
        let ids: std::collections::HashSet<_> = KOKORO_VOICES.iter().map(|v| v.id).collect();
        assert_eq!(ids.len(), KOKORO_VOICES.len());
        assert_eq!(
            KokoroVoice::find("bf_emma").unwrap().label(),
            "Kokoro Emma (en-GB, F)"
        );
        assert!(KOKORO_VOICES.iter().filter(|v| v.recommended).count() >= 4);
        for b in KokoroBackend::ALL {
            if let Some(k) = b.key() {
                assert_eq!(KokoroBackend::from_key(k), Some(b));
            }
        }
    }

    #[test]
    fn presets_without_engine_are_browser_voices() {
        let p: VoicePreset = serde_json::from_str(
            r#"{"id":1,"voice_uri":"Samantha","voice_label":"Samantha (en-US)","rate":1.0}"#,
        )
        .unwrap();
        assert_eq!(p.engine, Engine::Browser);
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"auto_record":false}"#).unwrap();
        assert!(!s.auto_record);
        assert_eq!(s.max_record_secs, 15);
        assert_eq!(s.record_delay_ms, 1500);
    }
}
