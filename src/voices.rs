// Voices tab: build the pool of (voice, rate) presets that questions are
// randomly read with — browser voices and on-device Kokoro voices.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::AppState;
use crate::model::{
    Engine, KOKORO_VOICES, KokoroBackend, KokoroVoice, VoiceInfo, VoicePreset, suggest_voices,
    unique_labels,
};
use crate::storage::new_id;
use crate::ui::{
    BADGE, BTN, BTN_DANGER, BTN_SMALL, CARD, CHECKBOX, HEADING, INPUT, MUTED, rate_label,
};
use crate::{audio, kokoro};

const SAMPLE: &str = "The lecture will begin in the main hall at nine o'clock.";

/// Reads the sample sentence with a voice. Call from a click handler.
fn preview(app: AppState, engine: Engine, voice: String, rate: f32) {
    match engine {
        Engine::Browser => spawn_local(async move {
            let _ = audio::speak(SAMPLE, &voice, rate).await;
        }),
        Engine::Kokoro => {
            // Still inside the click: lets the generated audio play on iOS.
            audio::unlock_playback();
            spawn_local(async move {
                if app.ensure_kokoro().await.is_ok() {
                    let pending = kokoro::start_generate(SAMPLE, &voice, rate);
                    if let Ok(url) = kokoro::finish_generate(pending).await {
                        let _ = audio::play_url(&url).await;
                    }
                }
            });
        }
    }
}

/// Adds a preset unless the same voice at the same rate is already pooled.
fn add_preset(app: AppState, engine: Engine, voice: &str, label: String, rate: f32) {
    let duplicate = app.settings.with_untracked(|s| {
        s.presets
            .iter()
            .any(|p| p.engine == engine && p.voice_uri == voice && (p.rate - rate).abs() < 0.001)
    });
    if !duplicate {
        app.settings.update(|s| {
            s.presets.push(VoicePreset {
                id: new_id(),
                engine,
                voice_uri: voice.to_string(),
                voice_label: label,
                rate,
            })
        });
    }
}

/// A 0.5×–1.5× rate slider with its label.
#[component]
fn RateSlider(#[prop(into)] id: String, rate: RwSignal<f32>) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-2">
            <label class="text-sm font-medium" for=id.clone()>
                "Rate "
                <span class="tabular-nums text-sky-600">{move || rate_label(rate.get())}</span>
            </label>
            <input
                id=id
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
    }
}

#[component]
pub fn Voices() -> impl IntoView {
    let app = expect_context::<AppState>();
    let show_all = RwSignal::new(false);
    let selected = RwSignal::new(String::new());
    let rate = RwSignal::new(1.0f32);

    // Voices are addressed by URI, so entries repeating a URI are the same
    // voice as far as the Speech API is concerned: keep one.
    let distinct = Memo::new(move |_| {
        let mut seen = std::collections::HashSet::new();
        app.voices.with(|vs| {
            vs.iter()
                .filter(|v| seen.insert(v.uri.clone()))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    // URI -> label, unique even when names repeat.
    let labels = Memo::new(move |_| {
        distinct.with(|vs| {
            vs.iter()
                .map(|v| v.uri.clone())
                .zip(unique_labels(vs))
                .collect::<std::collections::HashMap<_, _>>()
        })
    });
    let label_of = move |v: &VoiceInfo| {
        labels.with_untracked(|l| l.get(&v.uri).cloned().unwrap_or_else(|| v.label()))
    };
    // English, non-novelty voices by default; sorted by accent then name.
    let choices = Memo::new(move |_| {
        let all = show_all.get();
        let mut v: Vec<VoiceInfo> = distinct
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

    let add_selected = move |_| {
        let uri = selected.get_untracked();
        if let Some(v) = app
            .voices
            .with_untracked(|vs| vs.iter().find(|v| v.uri == uri).cloned())
        {
            add_preset(
                app,
                Engine::Browser,
                &v.uri,
                label_of(&v),
                rate.get_untracked(),
            );
        }
    };
    let add_suggested = move |_| {
        let r = rate.get_untracked();
        for v in app.voices.with_untracked(|vs| suggest_voices(vs)) {
            add_preset(app, Engine::Browser, &v.uri, label_of(&v), r);
        }
    };

    let unavailable = move |engine: Engine, uri: &str| match engine {
        Engine::Browser => app
            .voices
            .with(|vs| !vs.is_empty() && !vs.iter().any(|v| v.uri == uri))
            .then_some("not available in this browser"),
        Engine::Kokoro => (!app.settings.with(|s| s.kokoro_enabled))
            .then_some("download the Kokoro model below to use"),
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
                <p class=MUTED>
                    "iPhone / iPad: Safari offers websites only some of the voices installed under "
                    "Settings → Accessibility → Read & Speak (older iOS: Spoken Content) → Voices, "
                    "and Siri voices never. If a voice isn't listed below, this app can't use it — "
                    "try the Kokoro voices further down instead."
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
                                let engine = p.engine;
                                let uri = p.voice_uri.clone();
                                let uri_check = uri.clone();
                                view! {
                                    <li class=format!("{CARD} flex items-center gap-3 p-3")>
                                        <div class="flex-1">
                                            <span class="font-medium">{p.voice_label.clone()}</span>
                                            <span class="ml-2 text-sm text-zinc-500">
                                                {rate_label(p.rate)}
                                            </span>
                                            {move || {
                                                unavailable(engine, &uri_check)
                                                    .map(|why| {
                                                        view! {
                                                            <span class="ml-2 text-xs text-amber-600">{why}</span>
                                                        }
                                                    })
                                            }}
                                        </div>
                                        <button
                                            class=BTN_SMALL
                                            on:click=move |_| preview(app, engine, uri.clone(), p.rate)
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
                <h2 class=HEADING>"Add a browser voice"</h2>
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
                                            {label_of(&v)}
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
                    <RateSlider id="rate" rate=rate />
                    <div class="flex flex-wrap gap-2">
                        <button
                            class=BTN
                            on:click=move |_| preview(
                                app,
                                Engine::Browser,
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
                <details>
                    <summary class=format!(
                        "cursor-pointer {MUTED}",
                    )>
                        {move || {
                            format!(
                                "What this browser reports ({} voices)",
                                app.voices.with(Vec::len),
                            )
                        }}
                    </summary>
                    <p class=format!(
                        "mt-2 {MUTED}",
                    )>
                        "Raw speechSynthesis.getVoices() output, unfiltered — useful for checking which installed voices Safari actually exposes."
                    </p>
                    <ul class="mt-2 flex flex-col gap-1 font-mono text-xs break-all select-text">
                        {move || {
                            app.voices
                                .get()
                                .into_iter()
                                .map(|v| {
                                    view! {
                                        <li>
                                            {format!(
                                                "{} | {} | {} | {}",
                                                v.name,
                                                v.lang,
                                                if v.local { "local" } else { "network" },
                                                v.uri,
                                            )}
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ul>
                </details>
            </section>

            <KokoroPanel />
        </div>
    }
}

/// Opt-in on-device neural voices: download / load the model, then add
/// Kokoro voices to the pool.
#[component]
fn KokoroPanel() -> impl IntoView {
    let app = expect_context::<AppState>();
    let detected = RwSignal::new(None::<KokoroBackend>);
    spawn_local(async move { detected.set(Some(kokoro::detect_backend().await)) });
    let selected = RwSignal::new(
        KOKORO_VOICES
            .first()
            .map(|v| v.id.to_string())
            .unwrap_or_default(),
    );
    let rate = RwSignal::new(1.0f32);
    let status = app.kokoro;
    let backend = move || app.settings.with(|s| s.kokoro_backend);

    let load = move |_| {
        spawn_local(async move {
            let _ = app.ensure_kokoro().await;
        })
    };
    let forget = move |_| {
        spawn_local(async move {
            let _ = kokoro::forget().await;
            status.set(kokoro::Status::NotLoaded);
            app.settings.update(|s| s.kokoro_enabled = false);
        })
    };
    let add_selected = move |_| {
        if let Some(v) = KokoroVoice::find(&selected.get_untracked()) {
            add_preset(app, Engine::Kokoro, v.id, v.label(), rate.get_untracked());
        }
    };
    let add_recommended = move |_| {
        let r = rate.get_untracked();
        for v in KOKORO_VOICES.iter().filter(|v| v.recommended) {
            add_preset(app, Engine::Kokoro, v.id, v.label(), r);
        }
    };
    // Loaded with the backend the settings ask for (Auto accepts any).
    let ready = move || match status.get() {
        kokoro::Status::Ready(b) => backend() == KokoroBackend::Auto || backend() == b,
        _ => false,
    };
    let usable = move || ready() || app.settings.with(|s| s.kokoro_enabled);

    view! {
        <section class=format!("{CARD} flex flex-col gap-4")>
            <div class="flex items-center gap-2">
                <h2 class=HEADING>"Kokoro neural voices"</h2>
                <span class=BADGE>"on-device"</span>
            </div>
            <p class=MUTED>
                "Natural-sounding US and UK voices generated on this device by the open Kokoro-82M model "
                "— useful where the browser's own voices are robotic (e.g. iPhone). "
                "The model is downloaded once from Hugging Face and cached by the browser; "
                "nothing you practise is sent anywhere."
            </p>

            <label class="flex flex-col gap-2">
                <span class="text-sm font-medium">"Runs on"</span>
                <select
                    class=INPUT
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        let b = KokoroBackend::ALL
                            .into_iter()
                            .find(|b| format!("{b:?}") == v)
                            .unwrap_or_default();
                        app.settings.update(|s| s.kokoro_backend = b);
                    }
                >
                    {KokoroBackend::ALL
                        .into_iter()
                        .map(|b| {
                            view! {
                                <option value=format!("{b:?}") selected=move || backend() == b>
                                    {move || match (b, detected.get()) {
                                        (KokoroBackend::Auto, Some(d)) => {
                                            format!("Auto → {}", d.describe())
                                        }
                                        _ => b.describe().to_string(),
                                    }}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
                <span class=MUTED>
                    "WebGPU reads a sentence in about a second; the CPU fallback can take several seconds per sentence."
                </span>
            </label>

            <div class="flex flex-wrap items-center gap-3">
                {move || match status.get() {
                    kokoro::Status::Loading(f) => {
                        let pct = f.map(|f| f * 100.0).unwrap_or(0.0);
                        view! {
                            <div class="flex w-full flex-col gap-1">
                                <div class="h-2 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
                                    <div
                                        class="h-full rounded-full bg-sky-500 transition-[width]"
                                        style:width=format!("{pct:.0}%")
                                    ></div>
                                </div>
                                <span class=MUTED>
                                    {match f {
                                        Some(_) => {
                                            format!("Downloading / loading model… {pct:.0}%")
                                        }
                                        None => "Loading model…".to_string(),
                                    }}
                                </span>
                            </div>
                        }
                            .into_any()
                    }
                    kokoro::Status::Ready(b) if ready() => {
                        view! {
                            <span class="text-sm text-emerald-600 dark:text-emerald-400">
                                {format!("✓ Ready · {}", b.describe())}
                            </span>
                        }
                            .into_any()
                    }
                    other => {
                        let failed = match other {
                            kokoro::Status::Failed(e) => Some(e),
                            _ => None,
                        };
                        let label = if app.settings.with_untracked(|s| s.kokoro_enabled) {
                            "Load model"
                        } else {
                            "Download model"
                        };
                        view! {
                            <button class=BTN on:click=load>
                                {label}
                            </button>
                            {failed
                                .map(|e| {
                                    view! {
                                        <span class="text-sm text-rose-600 dark:text-rose-400">
                                            {format!("Failed: {e}")}
                                        </span>
                                    }
                                })}
                        }
                            .into_any()
                    }
                }} <Show when=move || app.settings.with(|s| s.kokoro_enabled)>
                    <button class=BTN_DANGER on:click=forget>
                        "Delete downloaded model"
                    </button>
                </Show>
            </div>

            <Show when=usable>
                <div class="flex flex-col gap-2">
                    <label class="text-sm font-medium" for="kokoro-voice">
                        "Voice"
                    </label>
                    <select
                        id="kokoro-voice"
                        class=INPUT
                        on:change=move |ev| selected.set(event_target_value(&ev))
                    >
                        {KOKORO_VOICES
                            .iter()
                            .map(|v| {
                                let id = v.id;
                                view! {
                                    <option value=id selected=move || selected.get() == id>
                                        {format!("{} — grade {}", v.label(), v.grade)}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </div>
                <RateSlider id="kokoro-rate" rate=rate />
                <div class="flex flex-wrap gap-2">
                    <button
                        class=BTN
                        on:click=move |_| preview(
                            app,
                            Engine::Kokoro,
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
                        on:click=add_recommended
                        title="The best-rated US and UK voices at the selected rate"
                    >
                        "Add recommended voices"
                    </button>
                </div>
            </Show>
        </section>
    }
}
