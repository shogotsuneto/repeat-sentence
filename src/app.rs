use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::audio;
use crate::library::Library;
use crate::model::{SentenceSet, Settings, VoiceInfo, VoicePreset, suggest_voices};
use crate::practice::Practice;
use crate::settings_tab::SettingsTab;
use crate::storage;
use crate::voices::Voices;

/// App-wide state, provided as context. Everything persisted lives here;
/// `Effect`s write it back to localStorage on change.
#[derive(Clone, Copy)]
pub struct AppState {
    pub settings: RwSignal<Settings>,
    pub sets: RwSignal<Vec<SentenceSet>>,
    /// Voices this browser offers. Empty until they load (or if unsupported).
    pub voices: RwSignal<Vec<VoiceInfo>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Practice,
    Sentences,
    Voices,
    Settings,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Practice, Tab::Sentences, Tab::Voices, Tab::Settings];

    fn label(self) -> &'static str {
        match self {
            Tab::Practice => "Practice",
            Tab::Sentences => "Sentences",
            Tab::Voices => "Voices",
            Tab::Settings => "Settings",
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    let stored = storage::load_settings();
    let first_visit = stored.is_none();
    let state = AppState {
        settings: RwSignal::new(stored.unwrap_or_default()),
        sets: RwSignal::new(storage::load_sets()),
        voices: RwSignal::new(Vec::new()),
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
                <nav class="flex gap-1 rounded-full bg-zinc-200/70 p-1 dark:bg-zinc-800">
                    {Tab::ALL
                        .into_iter()
                        .map(|t| {
                            view! {
                                <button
                                    class="rounded-full px-3 py-1.5 text-sm font-medium text-zinc-600 dark:text-zinc-300"
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
                <div class:hidden=move || tab.get() != Tab::Settings>
                    <SettingsTab />
                </div>
            </main>

            <footer class="text-center text-xs text-zinc-500">
                "Everything stays in your browser — recordings are never uploaded. "
                <a class="underline" href="https://github.com/shogotsuneto/repeat-sentence">
                    "Source"
                </a>
            </footer>
        </div>
    }
}
