use leptos::prelude::*;
use leptos::task::spawn_local;

use std::collections::HashMap;

use crate::history_tab::HistoryTab;
use crate::library::Library;
use crate::model::{
    AttemptRecord, SentenceSet, Settings, VoiceInfo, VoicePreset, practice_counts, suggest_voices,
};
use crate::practice::Practice;
use crate::settings_tab::SettingsTab;
use crate::storage;
use crate::voices::Voices;
use crate::{audio, history};

/// App-wide state, provided as context. Everything persisted lives here;
/// `Effect`s write it back to localStorage on change.
#[derive(Clone, Copy)]
pub struct AppState {
    pub settings: RwSignal<Settings>,
    pub sets: RwSignal<Vec<SentenceSet>>,
    /// Voices this browser offers. Empty until they load (or if unsupported).
    pub voices: RwSignal<Vec<VoiceInfo>>,
    /// Saved attempts (IndexedDB), newest first.
    pub history: RwSignal<Vec<AttemptRecord>>,
    /// Set when history can't be loaded or saved (e.g. private browsing).
    pub history_error: RwSignal<Option<String>>,
    /// Attempts per sentence text, derived from `history`.
    pub counts: Memo<HashMap<String, u32>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Practice,
    Sentences,
    Voices,
    History,
    Settings,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Practice,
        Tab::Sentences,
        Tab::Voices,
        Tab::History,
        Tab::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Practice => "Practice",
            Tab::Sentences => "Sentences",
            Tab::Voices => "Voices",
            Tab::History => "History",
            Tab::Settings => "Settings",
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    let stored = storage::load_settings();
    let first_visit = stored.is_none();
    let history = RwSignal::new(Vec::new());
    let state = AppState {
        settings: RwSignal::new(stored.unwrap_or_default()),
        sets: RwSignal::new(storage::load_sets()),
        voices: RwSignal::new(Vec::new()),
        history,
        history_error: RwSignal::new(None),
        counts: Memo::new(move |_| history.with(|h| practice_counts(h))),
    };
    provide_context(state);

    Effect::new(move |_| state.settings.with(storage::save_settings));
    Effect::new(move |_| state.sets.with(|s| storage::save_sets(s)));

    spawn_local(async move {
        let voices = audio::load_voices().await;
        // First visit: start with one voice per English accent so questions
        // vary out of the box.
        if first_visit && state.settings.with_untracked(|s| s.presets.is_empty()) {
            let presets: Vec<_> = suggest_voices(&voices)
                .into_iter()
                .map(|v| VoicePreset {
                    id: storage::new_id(),
                    voice_uri: v.uri.clone(),
                    voice_label: v.label(),
                    rate: 1.0,
                })
                .collect();
            state.settings.update(|s| s.presets = presets);
        }
        state.voices.set(voices);
    });
    audio::on_voices_changed(move |v| state.voices.set(v));
    spawn_local(async move {
        match history::list().await {
            // Attempts saved before the list arrived are already in it.
            Ok(saved) => state.history.set(saved),
            Err(e) => state
                .history_error
                .set(Some(format!("History is unavailable: {e}"))),
        }
    });

    let tab = RwSignal::new(Tab::Practice);
    let speech_ok = audio::speech_supported();
    let recording_ok = audio::recording_supported();

    // Panels stay mounted and are only hidden, so switching tabs never
    // interrupts a recording or drops the session history.
    view! {
        <div class="mx-auto flex min-h-dvh max-w-3xl flex-col gap-6 px-4 py-6">
            <header class="flex flex-wrap items-center justify-between gap-3">
                <h1 class="text-xl font-bold tracking-tight">
                    "Repeat Sentence"
                    <span class="ml-2 text-sm font-normal text-zinc-500">"PTE Core practice"</span>
                </h1>
                <nav class="flex max-w-full gap-1 overflow-x-auto rounded-full bg-zinc-200/70 p-1 dark:bg-zinc-800">
                    {Tab::ALL
                        .into_iter()
                        .map(|t| {
                            view! {
                                <button
                                    class="shrink-0 rounded-full px-2.5 py-1.5 text-sm font-medium text-zinc-600 sm:px-3 dark:text-zinc-300"
                                    class=(
                                        [
                                            "bg-white",
                                            "text-zinc-900",
                                            "shadow-sm",
                                            "dark:bg-zinc-950",
                                            "dark:text-white",
                                        ],
                                        move || tab.get() == t,
                                    )
                                    on:click=move |_| tab.set(t)
                                >
                                    {t.label()}
                                </button>
                            }
                        })
                        .collect_view()}
                </nav>
            </header>

            {(!speech_ok || !recording_ok)
                .then(|| {
                    view! {
                        <p class="rounded-lg border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
                            {match (speech_ok, recording_ok) {
                                (false, false) => {
                                    "This browser supports neither speech synthesis nor recording. Try a recent Chrome, Edge or Safari."
                                }
                                (false, _) => "This browser doesn't support speech synthesis.",
                                _ => "This browser doesn't support recording; you can still listen.",
                            }}
                        </p>
                    }
                })}

            <main class="flex-1">
                <div class:hidden=move || tab.get() != Tab::Practice>
                    <Practice />
                </div>
                <div class:hidden=move || tab.get() != Tab::Sentences>
                    <Library />
                </div>
                <div class:hidden=move || tab.get() != Tab::Voices>
                    <Voices />
                </div>
                <div class:hidden=move || tab.get() != Tab::History>
                    <HistoryTab />
                </div>
                <div class:hidden=move || tab.get() != Tab::Settings>
                    <SettingsTab />
                </div>
            </main>

            <footer class="text-center text-xs text-zinc-500">
                "Everything stays in your browser — recordings and history are never uploaded. "
                <a class="underline" href="https://github.com/shogotsuneto/repeat-sentence">
                    "Source"
                </a>
            </footer>
        </div>
    }
}
