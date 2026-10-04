// The practice loop: pick a sentence and a voice preset at random, read it
// aloud, record the repeat immediately afterwards, then review.

use gloo_timers::future::TimeoutFuture;
use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::app::AppState;
use crate::model::{
    AttemptRecord, Engine, PoolEntry, Reveal, ShuffleBag, pick_index, sentence_pool,
};
use crate::ui::{BADGE, BTN, BTN_PRIMARY, BTN_SMALL, CARD, MUTED, clock, rate_label};
use crate::{audio, diag, history, kokoro};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    /// Short pause before the prompt plays.
    Waiting,
    /// Loading the Kokoro model or generating the prompt's audio.
    Preparing,
    Speaking,
    /// Prompt done; waiting for the microphone (and beep) before recording.
    OpeningMic,
    Recording,
    Review,
}

/// One question: the sentence plus the voice it is read with.
#[derive(Clone, PartialEq, Debug)]
struct Item {
    text: String,
    source: String,
    engine: Engine,
    voice_uri: String,
    voice_label: String,
    rate: f32,
}

/// An attempt made in this session. `url` is the live blob URL of the
/// recording; the same attempt is also saved to the persistent history.
#[derive(Clone, PartialEq, Debug)]
struct Attempt {
    /// Sequence number within this session.
    seq: u32,
    record: AttemptRecord,
    url: String,
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
    session: RwSignal<Vec<Attempt>>,
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
        // device) are skipped. Before voices load, trust them all. Kokoro
        // presets need the model to have been downloaded on this device.
        let usable: Vec<_> = settings
            .presets
            .iter()
            .filter(|p| match p.engine {
                Engine::Browser => voices.is_empty() || voices.iter().any(|v| v.uri == p.voice_uri),
                Engine::Kokoro => settings.kokoro_enabled,
            })
            .collect();
        let (engine, voice_uri, voice_label, rate) = if usable.is_empty() {
            Self::default_voice()
        } else {
            let p = usable[pick_index(usable.len(), &mut rand)];
            (p.engine, p.voice_uri.clone(), p.voice_label.clone(), p.rate)
        };
        Some(Item {
            text: entry.text,
            source: entry.source,
            engine,
            voice_uri,
            voice_label,
            rate,
        })
    }

    fn default_voice() -> (Engine, String, String, f32) {
        (
            Engine::Browser,
            String::new(),
            "Browser default voice".to_string(),
            1.0,
        )
    }

    /// For a Kokoro item, loads the model and starts generating the prompt
    /// (so it overlaps whatever waiting follows). If Kokoro can't load, the
    /// item falls back to the browser's default voice.
    async fn prepare(self, v: u32, item: &mut Item) -> Option<js_sys::Promise> {
        if item.engine != Engine::Kokoro {
            return None;
        }
        if self
            .app
            .kokoro
            .with_untracked(|k| !matches!(k, kokoro::Status::Ready(_)))
        {
            self.phase.set(Phase::Preparing);
        }
        match self.app.ensure_kokoro().await {
            Ok(()) => Some(kokoro::start_generate(
                &item.text,
                &item.voice_uri,
                item.rate,
            )),
            Err(e) => {
                if self.is_current(v) {
                    self.error.set(Some(format!(
                        "Kokoro unavailable ({e}); using the browser voice."
                    )));
                    (item.engine, item.voice_uri, item.voice_label, item.rate) =
                        Self::default_voice();
                    self.item.set(Some(item.clone()));
                }
                None
            }
        }
    }

    /// Reads the item aloud with its engine; same contract as
    /// [`audio::speak`] (`Ok(false)` = cancelled).
    async fn say(
        self,
        v: u32,
        item: &Item,
        pending: Option<js_sys::Promise>,
    ) -> Result<bool, String> {
        match item.engine {
            Engine::Browser => {
                self.phase.set(Phase::Speaking);
                audio::speak(&item.text, &item.voice_uri, item.rate).await
            }
            Engine::Kokoro => {
                let pending = pending.unwrap_or_else(|| {
                    kokoro::start_generate(&item.text, &item.voice_uri, item.rate)
                });
                self.phase.set(Phase::Preparing);
                let waited = js_sys::Date::now();
                let url = kokoro::finish_generate(pending).await?;
                diag::log(format!(
                    "kokoro audio ready (waited {:.0} ms)",
                    js_sys::Date::now() - waited
                ));
                if !self.is_current(v) {
                    return Ok(false);
                }
                self.phase.set(Phase::Speaking);
                audio::play_url(&url).await
            }
        }
    }

    /// Start (or skip to) a new question.
    fn next(self) {
        self.interrupt();
        let v = self.bump();
        let Some(mut item) = self.pick() else {
            self.phase.set(Phase::Idle);
            self.error.set(Some(
                "No sentences are enabled. Turn some on in the Sentences tab.".into(),
            ));
            return;
        };
        self.error.set(None);
        // Leave Review *before* swapping in the new item: Review reveals the
        // sentence, and the async flow below only moves on after awaits (or
        // after a CPU-bound Kokoro generation has blocked rendering), so the
        // new sentence would otherwise show up for a moment — or longer.
        self.phase.set(Phase::Waiting);
        diag::log(format!(
            "next: {:?} {} @{:.2} \"{}\"",
            item.engine,
            item.voice_label,
            item.rate,
            diag::snippet(&item.text)
        ));
        self.item.set(Some(item.clone()));
        self.attempt.set(None);
        let settings = self.app.settings.get_untracked();

        spawn_local(async move {
            // While we still have the click's user gesture: unlock audio and
            // settle mic permission so no prompt interrupts the attempt. The
            // mic itself is only opened once the prompt has finished.
            let primed = audio::prime().await;
            if let (true, Err(e)) = (settings.auto_record, primed) {
                self.error.set(Some(format!("Microphone unavailable: {e}")));
            }
            if !self.is_current(v) {
                return;
            }
            let pending = self.prepare(v, &mut item).await;
            if !self.is_current(v) {
                return;
            }
            if settings.pre_delay_secs > 0 {
                self.phase.set(Phase::Waiting);
                TimeoutFuture::new(settings.pre_delay_secs * 1000).await;
                if !self.is_current(v) {
                    return;
                }
            }
            match self.say(v, &item, pending).await {
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
            if settings.auto_record && audio::recording_supported() {
                self.record(v).await;
            } else {
                self.phase.set(Phase::Review);
            }
        });
    }

    async fn record(self, v: u32) {
        let settings = self.app.settings.get_untracked();
        self.level.set(0.0);
        self.elapsed_ms.set(0);
        self.silent_ms.set(0);
        self.phase.set(Phase::OpeningMic);

        let (phase, level, elapsed_ms, silent_ms) =
            (self.phase, self.level, self.elapsed_ms, self.silent_ms);
        let result = audio::record(
            settings.max_record_secs * 1000,
            settings.silence_stop_secs * 1000,
            settings.record_delay_ms,
            if settings.beep { 120 } else { 0 },
            move || phase.set(Phase::Recording),
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
        match &result {
            Ok(Some(rec)) => diag::log(format!(
                "recorded {} ms ({}, {})",
                rec.duration_ms, rec.reason, rec.mime
            )),
            Ok(None) => diag::log("recording cancelled"),
            Err(e) => diag::log(format!("recording failed: {e}")),
        }
        match result {
            Ok(Some(rec)) => {
                if let Some(item) = self.item.get_untracked() {
                    self.keep(item, rec);
                }
            }
            Ok(None) => {}
            Err(e) => self.error.set(Some(e)),
        }
        self.level.set(0.0);
        self.phase.set(Phase::Review);
    }

    /// Adds a finished recording to the session list and saves it to the
    /// persistent history in the background.
    fn keep(self, item: Item, rec: audio::Recording) {
        let record = AttemptRecord {
            id: None,
            at_ms: js_sys::Date::now(),
            text: item.text,
            source: item.source,
            voice_label: item.voice_label,
            rate: item.rate,
            duration_ms: rec.duration_ms,
            reason: rec.reason,
            mime: rec.mime,
            // Set once saved.
            has_audio: false,
        };
        let seq = self.session.with_untracked(|h| h.len() as u32 + 1);
        let attempt = Attempt {
            seq,
            record: record.clone(),
            url: rec.url,
        };
        self.session.update(|h| h.insert(0, attempt.clone()));
        self.attempt.set(Some(attempt.clone()));

        let app = self.app;
        spawn_local(async move {
            match history::save(record, &attempt.url).await {
                Ok(saved) => app.history.update(|h| h.insert(0, saved)),
                Err(e) => app
                    .history_error
                    .set(Some(format!("Couldn't save to history: {e}"))),
            }
        });
    }

    /// Record another attempt at the current item without replaying it.
    fn record_again(self) {
        self.interrupt();
        let v = self.bump();
        spawn_local(async move {
            if let Err(e) = audio::prime().await {
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
        let Some(mut item) = self.item.get_untracked() else {
            return;
        };
        self.interrupt();
        let v = self.bump();
        spawn_local(async move {
            let pending = self.prepare(v, &mut item).await;
            let res = self.say(v, &item, pending).await;
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
            Phase::Waiting | Phase::Preparing | Phase::Speaking | Phase::OpeningMic => {
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
            Phase::Waiting | Phase::Preparing | Phase::Speaking | Phase::OpeningMic => {}
        }
    }

    /// Clears the session list only; saved history is kept.
    fn clear_session(self) {
        self.session.update(|h| {
            for a in h.drain(..) {
                audio::revoke_url(&a.url);
            }
        });
        self.attempt.set(None);
    }
}

pub fn file_extension(mime: &str) -> &'static str {
    match mime {
        m if m.contains("mp4") || m.contains("aac") => "m4a",
        m if m.contains("ogg") => "ogg",
        m if m.contains("wav") => "wav",
        _ => "webm",
    }
}

pub fn stop_reason(reason: &str) -> &'static str {
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
        session: RwSignal::new(Vec::new()),
        level: RwSignal::new(0.0),
        elapsed_ms: RwSignal::new(0),
        silent_ms: RwSignal::new(0),
        error: RwSignal::new(None),
    };
    // Phase changes and errors go to the crash log.
    Effect::new(move |prev: Option<Phase>| {
        let p = s.phase.get();
        if prev.is_some_and(|prev| prev != p) {
            diag::log(format!("phase {p:?}"));
        }
        p
    });
    Effect::new(move |_| {
        if let Some(e) = s.error.get() {
            diag::log(format!("error shown: {e}"));
        }
    });
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
        Phase::Preparing => match app.kokoro.get() {
            kokoro::Status::Loading(Some(f)) => {
                format!("Loading voice model… {:.0}%", f * 100.0)
            }
            kokoro::Status::Loading(None) => "Loading voice model…".to_string(),
            _ => "Preparing audio…".to_string(),
        },
        Phase::Speaking => "Listen".to_string(),
        Phase::OpeningMic => "Get ready to speak…".to_string(),
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
                            Phase::Preparing => "bg-sky-300 animate-pulse",
                            Phase::Speaking => "bg-sky-500 animate-pulse",
                            Phase::OpeningMic => "bg-rose-300",
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
                            let text = item.text.clone();
                            let times = move || {
                                app.counts.with(|c| c.get(&text).copied().unwrap_or(0))
                            };
                            view! {
                                <div class="flex flex-wrap justify-center gap-2">
                                    <span class=BADGE>{item.voice_label}</span>
                                    <span class=BADGE>{rate_label(item.rate)}</span>
                                    <span class=BADGE>{item.source}</span>
                                    <span class=BADGE>
                                        {move || match times() {
                                            0 => "New".to_string(),
                                            n => format!("Practised {n}×"),
                                        }}
                                    </span>
                                </div>
                            }
                        })
                }}

                <div class="flex flex-wrap items-center justify-center gap-3">
                    <button
                        class=BTN_PRIMARY
                        on:click=move |_| s.primary()
                        disabled=move || {
                            matches!(
                                phase.get(),
                                Phase::Waiting
                                | Phase::Preparing
                                | Phase::Speaking
                                | Phase::OpeningMic
                            )
                        }
                    >
                        {move || match phase.get() {
                            Phase::Idle => "Start",
                            Phase::Recording => "Stop recording",
                            _ => "Next",
                        }}
                    </button>
                    <Show when=move || {
                        matches!(
                            phase.get(),
                            Phase::Waiting | Phase::Preparing | Phase::Speaking | Phase::OpeningMic
                        )
                    }>
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
                                    <audio class="w-full" controls src=a.url.clone()></audio>
                                    <p class=MUTED>
                                        {format!(
                                            "Your answer · {} · {}",
                                            clock(a.record.duration_ms),
                                            stop_reason(&a.record.reason),
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
                        <span class=MUTED>{move || format!("({})", s.session.with(Vec::len))}</span>
                    </h2>
                    <Show when=move || s.session.with(|h| !h.is_empty())>
                        <button class=BTN_SMALL on:click=move |_| s.clear_session()>
                            "Clear"
                        </button>
                    </Show>
                </div>
                <Show
                    when=move || s.session.with(|h| !h.is_empty())
                    fallback=|| {
                        view! {
                            <p class=MUTED>
                                "Recordings from this session appear here. Every attempt is also saved to the History tab."
                            </p>
                        }
                    }
                >
                    <ul class="flex flex-col gap-3">
                        <For each=move || s.session.get() key=|a| a.seq let(a)>
                            <li class=format!("{CARD} flex flex-col gap-2 p-4")>
                                <p class="font-medium">
                                    <span class="mr-2 text-zinc-400">{format!("#{}", a.seq)}</span>
                                    {a.record.text.clone()}
                                </p>
                                <p class=MUTED>
                                    {format!(
                                        "{} · {} · {}",
                                        a.record.voice_label,
                                        rate_label(a.record.rate),
                                        clock(a.record.duration_ms),
                                    )}
                                </p>
                                <div class="flex items-center gap-3">
                                    <audio class="h-10 flex-1" controls src=a.url.clone()></audio>
                                    <a
                                        class=BTN_SMALL
                                        href=a.url.clone()
                                        download=format!(
                                            "repeat-sentence-{}.{}",
                                            a.seq,
                                            file_extension(&a.record.mime),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_extension_follows_mime() {
        assert_eq!(file_extension("audio/webm;codecs=opus"), "webm");
        assert_eq!(file_extension("audio/mp4"), "m4a");
        assert_eq!(file_extension("audio/ogg"), "ogg");
        assert_eq!(file_extension(""), "webm");
    }

    #[test]
    fn stop_reasons_are_readable() {
        assert_eq!(stop_reason("silence"), "stopped after silence");
        assert_eq!(stop_reason("timeout"), "time limit reached");
        assert_eq!(stop_reason("manual"), "stopped manually");
    }
}
