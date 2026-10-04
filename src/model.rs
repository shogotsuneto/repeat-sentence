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

    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.lang_tag())
    }
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
        let premium = [
            "natural", "neural", "online", "google", "premium", "enhanced",
        ]
        .iter()
        .any(|k| n.contains(k));
        u8::from(premium) * 2 + u8::from(v.local)
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

/// One entry in the user's voice pool: a voice at a given speaking rate.
/// Each question picks one of these at random.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoicePreset {
    pub id: u64,
    pub voice_uri: String,
    pub voice_label: String,
    pub rate: f32,
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
    pub max_record_secs: u32,
    /// Stop recording after this long without voice. 0 disables it.
    pub silence_stop_secs: u32,
    /// Pause between pressing Next and the prompt starting.
    pub pre_delay_secs: u32,
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
            max_record_secs: 15,
            silence_stop_secs: 3,
            pre_delay_secs: 1,
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
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"auto_record":false}"#).unwrap();
        assert!(!s.auto_record);
        assert_eq!(s.max_record_secs, 15);
    }
}
