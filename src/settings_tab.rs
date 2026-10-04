// Settings tab: timing and exam-condition options for the practice loop.

use leptos::prelude::*;

use crate::app::AppState;
use crate::model::{Reveal, Settings};
use crate::ui::{CARD, HEADING, INPUT, MUTED, Toggle};

/// A number input bound to one `u32` field of `Settings`, clamped to
/// `min..=max`.
#[component]
fn NumberField(
    #[prop(into)] label: String,
    #[prop(into)] hint: String,
    min: u32,
    max: u32,
    get: fn(&Settings) -> u32,
    set: fn(&mut Settings, u32),
) -> impl IntoView {
    let app = expect_context::<AppState>();
    view! {
        <label class="flex items-center justify-between gap-4">
            <span>
                <span class="block text-sm font-medium">{label}</span>
                <span class=format!("block {MUTED}")>{hint}</span>
            </span>
            <input
                type="number"
                class=format!("{INPUT} w-20 text-right")
                min=min
                max=max
                prop:value=move || app.settings.with(get).to_string()
                on:change=move |ev| {
                    if let Ok(n) = event_target_value(&ev).parse::<u32>() {
                        app.settings.update(|s| set(s, n.clamp(min, max)));
                    }
                }
            />
        </label>
    }
}

/// Like `NumberField`, but edits a millisecond field in seconds with
/// half-second steps.
#[component]
fn SecondsField(
    #[prop(into)] label: String,
    #[prop(into)] hint: String,
    max_ms: u32,
    get: fn(&Settings) -> u32,
    set: fn(&mut Settings, u32),
) -> impl IntoView {
    let app = expect_context::<AppState>();
    view! {
        <label class="flex items-center justify-between gap-4">
            <span>
                <span class="block text-sm font-medium">{label}</span>
                <span class=format!("block {MUTED}")>{hint}</span>
            </span>
            <input
                type="number"
                class=format!("{INPUT} w-20 text-right")
                min="0"
                max=f64::from(max_ms) / 1000.0
                step="0.5"
                prop:value=move || (f64::from(app.settings.with(get)) / 1000.0).to_string()
                on:change=move |ev| {
                    if let Ok(secs) = event_target_value(&ev).parse::<f64>() {
                        let ms = (secs.max(0.0) * 1000.0).round() as u32;
                        app.settings.update(|s| set(s, ms.min(max_ms)));
                    }
                }
            />
        </label>
    }
}

#[component]
pub fn SettingsTab() -> impl IntoView {
    let app = expect_context::<AppState>();
    let s = app.settings;

    view! {
        <div class="flex flex-col gap-6">
            <section class=format!("{CARD} flex flex-col gap-5")>
                <h2 class=HEADING>"Recording"</h2>
                <Toggle
                    label="Record automatically after the prompt"
                    hint="Like the exam: the microphone opens as soon as the sentence ends."
                    checked=Signal::derive(move || s.with(|s| s.auto_record))
                    on_change=Callback::new(move |v| s.update(|s| s.auto_record = v))
                />
                <SecondsField
                    label="Delay before recording (s)"
                    hint="Pause between the end of the sentence and the beep / start of recording."
                    max_ms=10_000
                    get=|s| s.record_delay_ms
                    set=|s, v| s.record_delay_ms = v
                />
                <Toggle
                    label="Beep before recording"
                    hint="A short tone marks the start of recording."
                    checked=Signal::derive(move || s.with(|s| s.beep))
                    on_change=Callback::new(move |v| s.update(|s| s.beep = v))
                />
                <NumberField
                    label="Maximum recording length (s)"
                    hint="The exam allows 15 seconds."
                    min=3
                    max=60
                    get=|s| s.max_record_secs
                    set=|s, v| s.max_record_secs = v
                />
                <NumberField
                    label="Stop after silence (s)"
                    hint="The exam closes the mic after 3 seconds of silence. 0 turns this off."
                    min=0
                    max=15
                    get=|s| s.silence_stop_secs
                    set=|s, v| s.silence_stop_secs = v
                />
            </section>

            <section class=format!("{CARD} flex flex-col gap-5")>
                <h2 class=HEADING>"Prompt"</h2>
                <NumberField
                    label="Delay before the prompt (s)"
                    hint="Pause after pressing Start / Next."
                    min=0
                    max=10
                    get=|s| s.pre_delay_secs
                    set=|s, v| s.pre_delay_secs = v
                />
                <Toggle
                    label="Show the sentence while listening"
                    hint="Off by default: the text appears only after your attempt."
                    checked=Signal::derive(move || s.with(|s| s.reveal == Reveal::Always))
                    on_change=Callback::new(move |v: bool| {
                        s.update(|s| {
                            s.reveal = if v { Reveal::Always } else { Reveal::AfterAttempt };
                        })
                    })
                />
            </section>
        </div>
    }
}
