// The practice loop: pick a sentence and a voice preset at random, read it
// aloud, record the repeat immediately afterwards, then review.

use gloo_timers::future::TimeoutFuture;
use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::AppState;
use crate::audio;
use crate::model::{PoolEntry, Reveal, ShuffleBag, pick_index, sentence_pool};
use crate::ui::{BADGE, BTN, BTN_PRIMARY, BTN_SMALL, CARD, MUTED, clock, rate_label};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    /// Short pause before the prompt plays.
    Waiting,
    Speaking,
    Recording,
    Review,
}

/// One question: the sentence plus the voice it is read with.
#[derive(Clone, PartialEq, Debug)]
struct Item {
    text: String,
    source: String,
    voice_uri: String,
    voice_label: String,
    rate: f32,
}

#[derive(Clone, PartialEq, Debug)]
struct Attempt {
    id: u32,
    item: Item,
    rec: audio::Recording,
}

/// The practice screen's signals bundled for the async flow. Every flow
/// captures `ver` when it starts and bails out once it no longer matches, so
/// pressing Next mid-flow cleanly supersedes the old one.
#[derive(Clone, Copy)]
struct Session {
    app: AppState,
    pool: Memo<Vec<PoolEntry>>,
    bag: StoredValue<ShuffleBag>,
    ver: StoredValue<u32>,
    phase: RwSignal<Phase>,
    item: RwSignal<Option<Item>>,
    /// The latest attempt at the current item.
    attempt: RwSignal<Option<Attempt>>,
    /// All attempts this session, newest first.
    history: RwSignal<Vec<Attempt>>,
    level: RwSignal<f64>,
    elapsed_ms: RwSignal<u32>,
    silent_ms: RwSignal<u32>,
    error: RwSignal<Option<String>>,
}

impl Session {
    fn bump(self) -> u32 {
        self.ver.update_value(|v| *v += 1);
        self.ver.get_value()
    }

    fn is_current(self, v: u32) -> bool {
        self.ver.get_value() == v
    }

    fn interrupt(self) {
        audio::stop_speaking();
        audio::cancel_recording();
    }

    fn pick(self) -> Option<Item> {
        let mut rand = js_sys::Math::random;
        let pool = self.pool.get_untracked();
        let mut idx = None;
        self.bag
            .update_value(|b| idx = b.next(pool.len(), &mut rand));
        let entry = pool.get(idx?)?.clone();

        let settings = self.app.settings.get_untracked();
        let voices = self.app.voices.get_untracked();
        // Presets whose voice this browser lacks (e.g. created on another
        // device) are skipped. Before voices load, trust them all.
        let usable: Vec<_> = settings
            .presets
            .iter()
            .filter(|p| voices.is_empty() || voices.iter().any(|v| v.uri == p.voice_uri))
            .collect();
        let (voice_uri, voice_label, rate) = if usable.is_empty() {
            (String::new(), "Browser default voice".to_string(), 1.0)
        } else {
            let p = usable[pick_index(usable.len(), &mut rand)];
            (p.voice_uri.clone(), p.voice_label.clone(), p.rate)
        };
        Some(Item {
            text: entry.text,
            source: entry.source,
            voice_uri,
            voice_label,
            rate,
        })
    }

    /// Start (or skip to) a new question.
    fn next(self) {
        self.interrupt();
        let v = self.bump();
        let Some(item) = self.pick() else {
            self.phase.set(Phase::Idle);
            self.error.set(Some(
                "No sentences are enabled. Turn some on in the Sentences tab.".into(),
            ));
            return;
        };
        self.error.set(None);
        self.item.set(Some(item.clone()));
        self.attempt.set(None);
        let settings = self.app.settings.get_untracked();

        spawn_local(async move {
            if settings.auto_record {
                // Opening the mic up front (from the click's user gesture)
                // means recording can start the instant the prompt ends.
                if let Err(e) = audio::ensure_mic().await {
                    self.error.set(Some(format!("Microphone unavailable: {e}")));
                }
                if !self.is_current(v) {
                    return;
                }
            }
            if settings.pre_delay_secs > 0 {
                self.phase.set(Phase::Waiting);
                TimeoutFuture::new(settings.pre_delay_secs * 1000).await;
                if !self.is_current(v) {
                    return;
                }
            }
            self.phase.set(Phase::Speaking);
            match audio::speak(&item.text, &item.voice_uri, item.rate).await {
                Ok(true) => {}
                // Cancelled: whoever cancelled it now owns the phase.
                Ok(false) => return,
                Err(e) => {
                    if self.is_current(v) {
                        self.error.set(Some(e));
                        self.phase.set(Phase::Review);
                    }
                    return;
                }
            }
            if !self.is_current(v) {
                return;
            }
            if settings.auto_record && audio::mic_ready() {
                self.record(v).await;
            } else {
                self.phase.set(Phase::Review);
            }
        });
    }

    async fn record(self, v: u32) {
        let settings = self.app.settings.get_untracked();
        if settings.beep {
            audio::beep(120).await;
            if !self.is_current(v) {
                return;
            }
        }
        self.level.set(0.0);
        self.elapsed_ms.set(0);
        self.silent_ms.set(0);
        self.phase.set(Phase::Recording);

        let (level, elapsed_ms, silent_ms) = (self.level, self.elapsed_ms, self.silent_ms);
        let result = audio::record(
            settings.max_record_secs * 1000,
            settings.silence_stop_secs * 1000,
            move |lvl, el, si| {
                level.set(lvl);
                elapsed_ms.set(el as u32);
                silent_ms.set(si as u32);
            },
        )
        .await;

        if !self.is_current(v) {
            if let Ok(Some(rec)) = result {
                audio::revoke_url(&rec.url);
            }
            return;
        }
        match result {
            Ok(Some(rec)) => {
                if let Some(item) = self.item.get_untracked() {
                    let id = self.history.with_untracked(|h| h.len() as u32 + 1);
                    let attempt = Attempt { id, item, rec };
                    self.history.update(|h| h.insert(0, attempt.clone()));
                    self.attempt.set(Some(attempt));
                }
            }
            Ok(None) => {}
            Err(e) => self.error.set(Some(e)),
        }
        self.level.set(0.0);
        self.phase.set(Phase::Review);
    }

    /// Record another attempt at the current item without replaying it.
    fn record_again(self) {
        self.interrupt();
        let v = self.bump();
        spawn_local(async move {
            if let Err(e) = audio::ensure_mic().await {
                self.error.set(Some(format!("Microphone unavailable: {e}")));
                return;
            }
            if self.is_current(v) {
                self.record(v).await;
            }
        });
    }

    /// Play the prompt again for review (does not start recording).
    fn replay(self) {
        let Some(item) = self.item.get_untracked() else {
            return;
        };
        self.interrupt();
        let v = self.bump();
        self.phase.set(Phase::Speaking);
        spawn_local(async move {
            let res = audio::speak(&item.text, &item.voice_uri, item.rate).await;
            if self.is_current(v) {
                if let Err(e) = res {
                    self.error.set(Some(e));
                }
                self.phase.set(Phase::Review);
            }
        });
    }

    fn stop(self) {
        match self.phase.get_untracked() {
            // Keeps what was recorded so far.
            Phase::Recording => audio::stop_recording(),
            Phase::Waiting | Phase::Speaking => {
                self.bump();
                self.interrupt();
                self.phase.set(Phase::Review);
            }
            Phase::Idle | Phase::Review => {}
        }
    }

    /// What Space does: start / next, or stop while recording.
    fn primary(self) {
        match self.phase.get_untracked() {
            Phase::Idle | Phase::Review => self.next(),
            Phase::Recording => self.stop(),
            Phase::Waiting | Phase::Speaking => {}
        }
    }

    fn clear_history(self) {
        self.history.update(|h| {
            for a in h.drain(..) {
                audio::revoke_url(&a.rec.url);
            }
        });
        self.attempt.set(None);
    }
}

fn file_extension(mime: &str) -> &'static str {
    match mime {
        m if m.contains("mp4") || m.contains("aac") => "m4a",
        m if m.contains("ogg") => "ogg",
        m if m.contains("wav") => "wav",
        _ => "webm",
    }
}

fn stop_reason(reason: &str) -> &'static str {
    match reason {
        "silence" => "stopped after silence",
        "timeout" => "time limit reached",
        _ => "stopped manually",
    }
}

#[component]
pub fn Practice() -> impl IntoView {
    let app = expect_context::<AppState>();
    let pool = Memo::new(move |_| {
        app.settings
            .with(|s| app.sets.with(|sets| sentence_pool(s, sets)))
    });
    let s = Session {
        app,
        pool,
        bag: StoredValue::new(ShuffleBag::default()),
        ver: StoredValue::new(0),
        phase: RwSignal::new(Phase::Idle),
        item: RwSignal::new(None),
        attempt: RwSignal::new(None),
        history: RwSignal::new(Vec::new()),
        level: RwSignal::new(0.0),
        elapsed_ms: RwSignal::new(0),
        silent_ms: RwSignal::new(0),
        error: RwSignal::new(None),
    };
    // A different pool invalidates the bag's indices.
    Effect::new(move |_| {
        pool.track();
        s.bag.update_value(ShuffleBag::reset);
    });

    // Space = primary action, Escape = stop. Ignored while typing or when a
    // control has focus (Space already activates the focused control).
    let handle = window_event_listener(ev::keydown, move |e| {
        let target_is_control = e
            .target()
            .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok())
            .is_some_and(|el| {
                matches!(
                    el.tag_name().as_str(),
                    "INPUT" | "TEXTAREA" | "SELECT" | "BUTTON" | "AUDIO" | "A" | "SUMMARY"
                )
            });
        if target_is_control || e.repeat() {
            return;
        }
        // Only while the Practice panel is visible.
        let visible = leptos::prelude::document()
            .get_element_by_id("practice")
            .is_some_and(|el| el.get_client_rects().length() > 0);
        if !visible {
            return;
        }
        match e.code().as_str() {
            "Space" => {
                e.prevent_default();
                s.primary();
            }
            "Escape" => s.stop(),
            _ => {}
        }
    });
    on_cleanup(move || handle.remove());

    let phase = s.phase;
    let reveal_text =
        move || phase.get() == Phase::Review || app.settings.with(|st| st.reveal == Reveal::Always);
    let max_ms = move || app.settings.with(|st| st.max_record_secs * 1000);
    let silence_ms = move || app.settings.with(|st| st.silence_stop_secs * 1000);

    let status = move || match phase.get() {
        Phase::Idle => "Ready".to_string(),
        Phase::Waiting => "Get ready…".to_string(),
        Phase::Speaking => "Listen".to_string(),
        Phase::Recording => format!(
            "Recording  {} / {}",
            clock(s.elapsed_ms.get()),
            clock(max_ms())
        ),
        Phase::Review => "Review".to_string(),
    };

    view! {
        <div id="practice" class="flex flex-col gap-6">
            <section class=format!("{CARD} flex flex-col items-center gap-5 py-8 text-center")>
                <div class="flex items-center gap-2 text-sm font-semibold uppercase tracking-widest">
                    <span class=move || {
                        let color = match phase.get() {
                            Phase::Idle => "bg-zinc-400",
                            Phase::Waiting => "bg-sky-500",
                            Phase::Speaking => "bg-sky-500 animate-pulse",
                            Phase::Recording => "bg-rose-500 animate-pulse",
                            Phase::Review => "bg-emerald-500",
                        };
                        format!("inline-block h-2.5 w-2.5 rounded-full {color}")
                    }></span>
                    <span class="tabular-nums">{status}</span>
                </div>

                // Level meter and auto-stop hint while recording.
                <div
                    class="w-full max-w-sm"
                    class:invisible=move || phase.get() != Phase::Recording
                >
                    <div class="h-2 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
                        <div
                            class="h-full rounded-full bg-rose-500 transition-[width] duration-75"
                            style:width=move || format!("{:.0}%", s.level.get() * 100.0)
                        ></div>
                    </div>
                    <div class="mt-1 h-2 overflow-hidden rounded-full bg-zinc-100 dark:bg-zinc-800/50">
                        <div
                            class="h-full rounded-full bg-zinc-400"
                            style:width=move || {
                                format!(
                                    "{:.0}%",
                                    s.elapsed_ms.get() as f64 / max_ms().max(1) as f64 * 100.0,
                                )
                            }
                        ></div>
                    </div>
                    <p class=format!(
                        "mt-1 h-5 {MUTED}",
                    )>
                        {move || {
                            let (silent, limit) = (s.silent_ms.get(), silence_ms());
                            (limit > 0 && silent >= 1000)
                                .then(|| {
                                    format!(
                                        "Silence — auto-stop in {:.1}s",
                                        limit.saturating_sub(silent) as f64 / 1000.0,
                                    )
                                })
                        }}
                    </p>
                </div>

                <div class="min-h-24 flex w-full items-center justify-center px-2">
                    {move || match s.item.get() {
                        None => {
                            view! {
                                <p class=MUTED>
                                    "Press Start (or Space). A sentence is read once — repeat it exactly as you heard it."
                                </p>
                            }
                                .into_any()
                        }
                        Some(item) if reveal_text() => {
                            view! {
                                <p class="text-xl font-medium leading-relaxed sm:text-2xl">
                                    {item.text}
                                </p>
                            }
                                .into_any()
                        }
                        Some(_) => {
                            view! {
                                <p class="select-none text-xl tracking-[0.3em] text-zinc-300 dark:text-zinc-700">
                                    "• • • • • • • •"
                                </p>
                            }
                                .into_any()
                        }
                    }}
                </div>

                {move || {
                    s.item
                        .get()
                        .map(|item| {
                            view! {
                                <div class="flex flex-wrap justify-center gap-2">
                                    <span class=BADGE>{item.voice_label}</span>
                                    <span class=BADGE>{rate_label(item.rate)}</span>
                                    <span class=BADGE>{item.source}</span>
                                </div>
                            }
                        })
                }}

                <div class="flex flex-wrap items-center justify-center gap-3">
                    <button
                        class=BTN_PRIMARY
                        on:click=move |_| s.primary()
                        disabled=move || matches!(phase.get(), Phase::Waiting | Phase::Speaking)
                    >
                        {move || match phase.get() {
                            Phase::Idle => "Start",
                            Phase::Recording => "Stop recording",
                            _ => "Next",
                        }}
                    </button>
                    <Show when=move || matches!(phase.get(), Phase::Waiting | Phase::Speaking)>
                        <button class=BTN on:click=move |_| s.stop()>
                            "Stop"
                        </button>
                        <button class=BTN on:click=move |_| s.next()>
                            "Skip"
                        </button>
                    </Show>
                    <Show when=move || phase.get() == Phase::Review>
                        <button class=BTN on:click=move |_| s.replay()>
                            "Replay prompt"
                        </button>
                        <button class=BTN on:click=move |_| s.record_again()>
                            {move || {
                                if s.attempt.with(Option::is_some) {
                                    "Record again"
                                } else {
                                    "Record"
                                }
                            }}
                        </button>
                    </Show>
                </div>

                {move || {
                    s.attempt
                        .get()
                        .filter(|_| phase.get() == Phase::Review)
                        .map(|a| {
                            view! {
                                <div class="flex w-full max-w-md flex-col items-center gap-1">
                                    <audio class="w-full" controls src=a.rec.url.clone()></audio>
                                    <p class=MUTED>
                                        {format!(
                                            "Your answer · {} · {}",
                                            clock(a.rec.duration_ms),
                                            stop_reason(&a.rec.reason),
                                        )}
                                    </p>
                                </div>
                            }
                        })
                }}

                {move || {
                    s.error
                        .get()
                        .map(|e| {
                            view! { <p class="text-sm text-rose-600 dark:text-rose-400">{e}</p> }
                        })
                }}

                <p class="text-xs text-zinc-400">
                    {move || format!("{} sentences in pool", pool.with(Vec::len))}
                    " · Space: start / next / stop · Esc: stop"
                </p>
            </section>

            <section class="flex flex-col gap-3">
                <div class="flex items-center justify-between">
                    <h2 class="text-lg font-semibold">
                        "This session "
                        <span class=MUTED>{move || format!("({})", s.history.with(Vec::len))}</span>
                    </h2>
                    <Show when=move || s.history.with(|h| !h.is_empty())>
                        <button class=BTN_SMALL on:click=move |_| s.clear_history()>
                            "Clear"
                        </button>
                    </Show>
                </div>
                <Show
                    when=move || s.history.with(|h| !h.is_empty())
                    fallback=|| {
                        view! {
                            <p class=MUTED>
                                "Recordings appear here. They are kept only until you close or reload the page."
                            </p>
                        }
                    }
                >
                    <ul class="flex flex-col gap-3">
                        <For each=move || s.history.get() key=|a| a.id let(a)>
                            <li class=format!("{CARD} flex flex-col gap-2 p-4")>
                                <p class="font-medium">
                                    <span class="mr-2 text-zinc-400">{format!("#{}", a.id)}</span>
                                    {a.item.text.clone()}
                                </p>
                                <p class=MUTED>
                                    {format!(
                                        "{} · {} · {}",
                                        a.item.voice_label,
                                        rate_label(a.item.rate),
                                        clock(a.rec.duration_ms),
                                    )}
                                </p>
                                <div class="flex items-center gap-3">
                                    <audio
                                        class="h-10 flex-1"
                                        controls
                                        src=a.rec.url.clone()
                                    ></audio>
                                    <a
                                        class=BTN_SMALL
                                        href=a.rec.url.clone()
                                        download=format!(
                                            "repeat-sentence-{}.{}",
                                            a.id,
                                            file_extension(&a.rec.mime),
                                        )
                                    >
                                        "Download"
                                    </a>
                                </div>
                            </li>
                        </For>
                    </ul>
                </Show>
            </section>
        </div>
    }
}
