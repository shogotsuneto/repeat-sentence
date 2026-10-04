// History tab: every saved attempt, grouped by day, with on-demand playback.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

use crate::app::AppState;
use crate::model::AttemptRecord;
use crate::practice::{file_extension, stop_reason};
use crate::ui::{BADGE, BTN_DANGER, BTN_SMALL, CARD, HEADING, MUTED, clock, rate_label};
use crate::{audio, history};

const PAGE: usize = 30;

fn date_of(ms: f64) -> js_sys::Date {
    js_sys::Date::new(&JsValue::from_f64(ms))
}

/// Local calendar day, for grouping (`YYYY-MM-DD`).
fn day_key(ms: f64) -> String {
    let d = date_of(ms);
    format!(
        "{:04}-{:02}-{:02}",
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date()
    )
}

fn locale() -> String {
    web_sys::window()
        .and_then(|w| w.navigator().language())
        .unwrap_or_else(|| "en".into())
}

fn format_with(ms: f64, key: &str, style: &str) -> String {
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &key.into(), &style.into());
    let d = date_of(ms);
    if key == "dateStyle" {
        d.to_locale_date_string(&locale(), &opts).into()
    } else {
        d.to_locale_time_string_with_options(&locale(), &opts)
            .into()
    }
}

fn format_bytes(bytes: f64) -> String {
    if bytes < 1_000_000.0 {
        format!("{:.0} KB", bytes / 1000.0)
    } else {
        format!("{:.1} MB", bytes / 1_000_000.0)
    }
}

/// Loads a saved recording only when asked to, and releases its blob URL
/// when the row goes away.
#[component]
fn SavedAudio(id: u32, mime: String) -> impl IntoView {
    let url = RwSignal::new(None::<String>);
    let failed = RwSignal::new(false);
    // Shared with the cleanup, which runs after the signals are disposed.
    let held = Arc::new(Mutex::new(None::<String>));
    let held_cleanup = held.clone();
    on_cleanup(move || {
        if let Some(u) = held_cleanup.lock().ok().and_then(|mut h| h.take()) {
            audio::revoke_url(&u);
        }
    });

    let load = move |_| {
        let held = held.clone();
        spawn_local(async move {
            match history::audio_url(id).await {
                Ok(Some(u)) => {
                    if let Ok(mut h) = held.lock() {
                        *h = Some(u.clone());
                    }
                    url.set(Some(u));
                }
                _ => failed.set(true),
            }
        });
    };

    view! {
        {move || match url.get() {
            Some(u) => {
                view! {
                    <div class="flex items-center gap-3">
                        <audio class="h-10 flex-1" controls autoplay src=u.clone()></audio>
                        <a
                            class=BTN_SMALL
                            href=u
                            download=format!("repeat-sentence-{id}.{}", file_extension(&mime))
                        >
                            "Download"
                        </a>
                    </div>
                }
                    .into_any()
            }
            None if failed.get() => {
                view! { <span class=MUTED>"Recording unavailable"</span> }.into_any()
            }
            None => {
                view! {
                    <button class=BTN_SMALL on:click=load.clone()>
                        "▶ Play"
                    </button>
                }
                    .into_any()
            }
        }}
    }
}

#[component]
fn AttemptRow(record: AttemptRecord) -> impl IntoView {
    let app = expect_context::<AppState>();
    let id = record.id;
    let delete = move |_| {
        let Some(id) = id else {
            return;
        };
        spawn_local(async move {
            match history::delete(id).await {
                Ok(()) => app.history.update(|h| h.retain(|r| r.id != Some(id))),
                Err(e) => app.history_error.set(Some(format!("Couldn't delete: {e}"))),
            }
        });
    };
    let delete_audio = move |_| {
        let Some(id) = id else {
            return;
        };
        spawn_local(async move {
            match history::delete_audio(id).await {
                Ok(()) => app.history.update(|h| {
                    if let Some(r) = h.iter_mut().find(|r| r.id == Some(id)) {
                        r.has_audio = false;
                    }
                }),
                Err(e) => app.history_error.set(Some(format!("Couldn't delete: {e}"))),
            }
        });
    };

    view! {
        <li class=format!("{CARD} flex flex-col gap-2 p-4")>
            <div class="flex items-start gap-3">
                <p class="flex-1 font-medium">{record.text.clone()}</p>
                <span class="shrink-0 text-xs tabular-nums text-zinc-400">
                    {format_with(record.at_ms, "timeStyle", "short")}
                </span>
            </div>
            <div class="flex flex-wrap gap-2">
                <span class=BADGE>{record.voice_label.clone()}</span>
                <span class=BADGE>{rate_label(record.rate)}</span>
                <span class=BADGE>{clock(record.duration_ms)}</span>
                <span class=BADGE>{stop_reason(&record.reason)}</span>
                <span class=BADGE>{record.source.clone()}</span>
            </div>
            <div class="flex flex-wrap items-center justify-between gap-3">
                {match (id, record.has_audio) {
                    (Some(id), true) => {
                        view! {
                            <SavedAudio id=id mime=record.mime.clone() />
                            <button
                                class=BTN_SMALL
                                on:click=delete_audio
                                title="Delete the recording but keep this attempt in the history"
                            >
                                "Delete audio"
                            </button>
                        }
                            .into_any()
                    }
                    _ => view! { <span class=MUTED>"Audio deleted"</span> }.into_any(),
                }}
                <button
                    class=BTN_DANGER
                    on:click=delete
                    title="Delete this attempt and its recording"
                >
                    "Delete"
                </button>
            </div>
        </li>
    }
}

#[component]
pub fn HistoryTab() -> impl IntoView {
    let app = expect_context::<AppState>();
    let shown = RwSignal::new(PAGE);
    let confirm_clear = RwSignal::new(false);
    let confirm_clear_audio = RwSignal::new(false);
    let usage = RwSignal::new(None::<f64>);

    // Refresh the storage estimate whenever the history changes.
    Effect::new(move |_| {
        app.history.track();
        spawn_local(async move { usage.set(history::storage_usage().await) });
    });

    let today = move || {
        let key = day_key(js_sys::Date::now());
        app.history
            .with(|h| h.iter().filter(|r| day_key(r.at_ms) == key).count())
    };

    // The first `shown` attempts, grouped by day (history is newest first).
    let days = move || {
        app.history.with(|h| {
            let mut groups: Vec<(String, Vec<AttemptRecord>)> = Vec::new();
            for r in h.iter().take(shown.get()) {
                let key = day_key(r.at_ms);
                match groups.last_mut() {
                    Some((k, rows)) if *k == key => rows.push(r.clone()),
                    _ => groups.push((key, vec![r.clone()])),
                }
            }
            groups
        })
    };

    let clear_all = move |_| {
        if !confirm_clear.get_untracked() {
            confirm_clear.set(true);
            return;
        }
        confirm_clear.set(false);
        spawn_local(async move {
            match history::clear().await {
                Ok(()) => app.history.set(Vec::new()),
                Err(e) => app.history_error.set(Some(format!("Couldn't delete: {e}"))),
            }
        });
    };

    let clear_audio = move |_| {
        if !confirm_clear_audio.get_untracked() {
            confirm_clear_audio.set(true);
            return;
        }
        confirm_clear_audio.set(false);
        spawn_local(async move {
            match history::clear_audio().await {
                Ok(()) => app
                    .history
                    .update(|h| h.iter_mut().for_each(|r| r.has_audio = false)),
                Err(e) => app.history_error.set(Some(format!("Couldn't delete: {e}"))),
            }
        });
    };
    let any_audio = move || app.history.with(|h| h.iter().any(|r| r.has_audio));

    view! {
        <div class="flex flex-col gap-6">
            {move || {
                app.history_error
                    .get()
                    .map(|e| view! { <p class="text-sm text-rose-600 dark:text-rose-400">{e}</p> })
            }} <section class=format!("{CARD} grid grid-cols-2 gap-4 sm:grid-cols-4")>
                <div>
                    <p class=MUTED>"Attempts"</p>
                    <p class="text-2xl font-semibold tabular-nums">
                        {move || app.history.with(Vec::len)}
                    </p>
                </div>
                <div>
                    <p class=MUTED>"Sentences practised"</p>
                    <p class="text-2xl font-semibold tabular-nums">
                        {move || app.counts.with(|c| c.len())}
                    </p>
                </div>
                <div>
                    <p class=MUTED>"Today"</p>
                    <p class="text-2xl font-semibold tabular-nums">{today}</p>
                </div>
                <div>
                    <p class=MUTED>"Storage used"</p>
                    <p class="text-2xl font-semibold tabular-nums">
                        {move || usage.get().map(format_bytes).unwrap_or_else(|| "—".into())}
                    </p>
                </div>
            </section> <section class="flex flex-col gap-3">
                <div class="flex items-center justify-between">
                    <h2 class=HEADING>"History"</h2>
                    <div class="flex flex-wrap justify-end gap-2">
                        <Show when=any_audio>
                            <button
                                class=BTN_DANGER
                                on:click=clear_audio
                                on:blur=move |_| confirm_clear_audio.set(false)
                                title="Delete every recording but keep the history"
                            >
                                {move || {
                                    if confirm_clear_audio.get() {
                                        "Click again to delete all audio"
                                    } else {
                                        "Delete all audio"
                                    }
                                }}
                            </button>
                        </Show>
                        <Show when=move || app.history.with(|h| !h.is_empty())>
                            <button
                                class=BTN_DANGER
                                on:click=clear_all
                                on:blur=move |_| confirm_clear.set(false)
                            >
                                {move || {
                                    if confirm_clear.get() {
                                        "Click again to delete everything"
                                    } else {
                                        "Delete all"
                                    }
                                }}
                            </button>
                        </Show>
                    </div>
                </div>
                <Show
                    when=move || app.history.with(|h| !h.is_empty())
                    fallback=|| {
                        view! {
                            <p class=MUTED>
                                "No attempts yet. Every recording you make is saved here, on this device only."
                            </p>
                        }
                    }
                >
                    <For
                        each=days
                        key=|(day, rows)| {
                            (
                                day.clone(),
                                rows.iter().map(|r| (r.id, r.has_audio)).collect::<Vec<_>>(),
                            )
                        }
                        let((day, rows))
                    >
                        <div class="flex flex-col gap-2">
                            <h3 class="mt-2 text-sm font-semibold text-zinc-500">
                                {rows
                                    .first()
                                    .map(|r| format_with(r.at_ms, "dateStyle", "full"))
                                    .unwrap_or(day)}
                            </h3>
                            <ul class="flex flex-col gap-2">
                                <For
                                    each=move || rows.clone()
                                    key=|r| (r.id, r.has_audio)
                                    let(record)
                                >
                                    <AttemptRow record=record />
                                </For>
                            </ul>
                        </div>
                    </For>
                    <Show when=move || app.history.with(|h| h.len() > shown.get())>
                        <button class=BTN_SMALL on:click=move |_| shown.update(|n| *n += PAGE)>
                            "Show more"
                        </button>
                    </Show>
                </Show>
            </section>
        </div>
    }
}
