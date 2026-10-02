// Voices tab: build the pool of (voice, rate) presets that questions are
// randomly read with.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::AppState;
use crate::audio;
use crate::model::{VoiceInfo, VoicePreset, suggest_voices};
use crate::storage::new_id;
use crate::ui::{BTN, BTN_DANGER, BTN_SMALL, CARD, CHECKBOX, HEADING, INPUT, MUTED, rate_label};

const SAMPLE: &str = "The lecture will begin in the main hall at nine o'clock.";

fn preview(voice_uri: String, rate: f32) {
    spawn_local(async move {
        let _ = audio::speak(SAMPLE, &voice_uri, rate).await;
    });
}

#[component]
pub fn Voices() -> impl IntoView {
    let app = expect_context::<AppState>();
    let show_all = RwSignal::new(false);
    let selected = RwSignal::new(String::new());
    let rate = RwSignal::new(1.0f32);

    // English, non-novelty voices by default; sorted by accent then name.
    let choices = Memo::new(move |_| {
        let all = show_all.get();
        let mut v: Vec<VoiceInfo> = app
            .voices
            .get()
            .into_iter()
            .filter(|v| all || (v.is_english() && !v.is_novelty()))
            .collect();
        v.sort_by(|a, b| a.lang_tag().cmp(&b.lang_tag()).then(a.name.cmp(&b.name)));
        v
    });
    // Keep the selection valid as the choice list changes.
    Effect::new(move |_| {
        let c = choices.get();
        if !c.iter().any(|v| v.uri == selected.get_untracked()) {
            selected.set(c.first().map(|v| v.uri.clone()).unwrap_or_default());
        }
    });

    let add_preset = move |voice: &VoiceInfo, rate: f32| {
        let duplicate = app.settings.with_untracked(|s| {
            s.presets
                .iter()
                .any(|p| p.voice_uri == voice.uri && (p.rate - rate).abs() < 0.001)
        });
        if !duplicate {
            app.settings.update(|s| {
                s.presets.push(VoicePreset {
                    id: new_id(),
                    voice_uri: voice.uri.clone(),
                    voice_label: voice.label(),
                    rate,
                })
            });
        }
    };

    let add_selected = move |_| {
        let uri = selected.get_untracked();
        if let Some(v) = app
            .voices
            .with_untracked(|vs| vs.iter().find(|v| v.uri == uri).cloned())
        {
            add_preset(&v, rate.get_untracked());
        }
    };
    let add_suggested = move |_| {
        let r = rate.get_untracked();
        for v in app.voices.with_untracked(|vs| suggest_voices(vs)) {
            add_preset(&v, r);
        }
    };

    let is_available = move |uri: &str| {
        app.voices
            .with(|vs| vs.is_empty() || vs.iter().any(|v| v.uri == uri))
    };

    view! {
        <div class="flex flex-col gap-6">
            <section class="flex flex-col gap-3">
                <div class="flex items-center justify-between">
                    <h2 class=HEADING>
                        "Voice pool "
                        <span class=MUTED>
                            {move || format!("({})", app.settings.with(|s| s.presets.len()))}
                        </span>
                    </h2>
                    <Show when=move || app.settings.with(|s| !s.presets.is_empty())>
                        <button
                            class=BTN_DANGER
                            on:click=move |_| app.settings.update(|s| s.presets.clear())
                        >
                            "Remove all"
                        </button>
                    </Show>
                </div>
                <p class=MUTED>
                    "Each question is read with a preset picked at random from this pool. "
                    "Add the same voice at several rates to vary speed. "
                    "Available voices depend on your browser and OS — presets missing here are skipped."
                </p>
                <Show
                    when=move || app.settings.with(|s| !s.presets.is_empty())
                    fallback=|| {
                        view! {
                            <p class=format!(
                                "{CARD} {MUTED}",
                            )>
                                "The pool is empty, so the browser's default voice is used at 1.00×."
                            </p>
                        }
                    }
                >
                    <ul class="flex flex-col gap-2">
                        <For
                            each=move || app.settings.with(|s| s.presets.clone())
                            key=|p| p.id
                            let(p)
                        >
                            {
                                let id = p.id;
                                let uri = p.voice_uri.clone();
                                let uri_check = uri.clone();
                                view! {
                                    <li class=format!("{CARD} flex items-center gap-3 p-3")>
                                        <div class="flex-1">
                                            <span class="font-medium">{p.voice_label.clone()}</span>
                                            <span class="ml-2 text-sm text-zinc-500">
                                                {rate_label(p.rate)}
                                            </span>
                                            <Show when=move || !is_available(&uri_check)>
                                                <span class="ml-2 text-xs text-amber-600">
                                                    "not available in this browser"
                                                </span>
                                            </Show>
                                        </div>
                                        <button
                                            class=BTN_SMALL
                                            on:click=move |_| preview(uri.clone(), p.rate)
                                        >
                                            "Preview"
                                        </button>
                                        <button
                                            class=BTN_DANGER
                                            on:click=move |_| {
                                                app.settings.update(|s| s.presets.retain(|p| p.id != id))
                                            }
                                        >
                                            "Remove"
                                        </button>
                                    </li>
                                }
                            }
                        </For>
                    </ul>
                </Show>
            </section>

            <section class=format!("{CARD} flex flex-col gap-4")>
                <h2 class=HEADING>"Add a preset"</h2>
                <Show
                    when=move || app.voices.with(|v| !v.is_empty())
                    fallback=|| {
                        view! {
                            <p class=MUTED>
                                "Loading voices… (none may be available in this browser)"
                            </p>
                        }
                    }
                >
                    <div class="flex flex-col gap-2">
                        <label class="text-sm font-medium" for="voice-select">
                            "Voice"
                        </label>
                        <select
                            id="voice-select"
                            class=INPUT
                            on:change=move |ev| selected.set(event_target_value(&ev))
                        >
                            <For each=move || choices.get() key=|v| v.uri.clone() let(v)>
                                {
                                    let uri = v.uri.clone();
                                    view! {
                                        <option
                                            value=v.uri.clone()
                                            selected=move || selected.get() == uri
                                        >
                                            {v.label()}
                                        </option>
                                    }
                                }
                            </For>
                        </select>
                        <label class="flex items-center gap-2 text-sm">
                            <input
                                type="checkbox"
                                class=CHECKBOX
                                prop:checked=show_all
                                on:change=move |ev| show_all.set(event_target_checked(&ev))
                            />
                            "Show all voices (other languages and novelty voices)"
                        </label>
                    </div>
                    <div class="flex flex-col gap-2">
                        <label class="text-sm font-medium" for="rate">
                            "Rate "
                            <span class="tabular-nums text-sky-600">
                                {move || rate_label(rate.get())}
                            </span>
                        </label>
                        <input
                            id="rate"
                            type="range"
                            min="0.5"
                            max="1.5"
                            step="0.05"
                            class="accent-sky-600"
                            prop:value=move || rate.get().to_string()
                            on:input=move |ev| {
                                if let Ok(r) = event_target_value(&ev).parse() {
                                    rate.set(r);
                                }
                            }
                        />
                        <p class=MUTED>
                            "PTE speakers talk at a natural pace; 0.9×–1.1× covers most of it."
                        </p>
                    </div>
                    <div class="flex flex-wrap gap-2">
                        <button
                            class=BTN
                            on:click=move |_| preview(
                                selected.get_untracked(),
                                rate.get_untracked(),
                            )
                        >
                            "Preview"
                        </button>
                        <button class=BTN on:click=add_selected>
                            "Add to pool"
                        </button>
                        <button
                            class=BTN
                            on:click=add_suggested
                            title="One voice per English accent (US, UK, AU, …) at the selected rate"
                        >
                            "Add suggested voices"
                        </button>
                    </div>
                </Show>
            </section>
        </div>
    }
}
