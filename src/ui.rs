// Shared Tailwind utility runs, DRY'd in Rust rather than via `@apply`
// classes. Strings decorate whichever element fits; `@source "../src"` in
// `static/input.css` makes Tailwind scan them like any other class name.

use leptos::prelude::*;

pub const CARD: &str = "rounded-2xl border border-zinc-200 bg-white p-5 shadow-sm dark:border-zinc-800 dark:bg-zinc-900";
pub const MUTED: &str = "text-sm text-zinc-500 dark:text-zinc-400";
pub const HEADING: &str = "text-lg font-semibold";
pub const INPUT: &str = "rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm focus:border-sky-500 focus:outline-none dark:border-zinc-700 dark:bg-zinc-950";
pub const BTN: &str = "inline-flex items-center justify-center gap-1.5 rounded-full border border-zinc-300 px-4 py-2 text-sm font-medium hover:bg-zinc-100 disabled:cursor-not-allowed disabled:opacity-40 dark:border-zinc-700 dark:hover:bg-zinc-800";
pub const BTN_PRIMARY: &str = "inline-flex items-center justify-center gap-1.5 rounded-full bg-sky-600 px-6 py-2.5 text-base font-semibold text-white shadow-sm hover:bg-sky-500 disabled:cursor-not-allowed disabled:opacity-40";
pub const BTN_DANGER: &str = "inline-flex items-center justify-center rounded-full border border-rose-300 px-3 py-1 text-xs font-medium text-rose-600 hover:bg-rose-50 dark:border-rose-800 dark:text-rose-400 dark:hover:bg-rose-950";
pub const BTN_SMALL: &str = "inline-flex items-center justify-center rounded-full border border-zinc-300 px-3 py-1 text-xs font-medium hover:bg-zinc-100 disabled:opacity-40 dark:border-zinc-700 dark:hover:bg-zinc-800";
pub const BADGE: &str = "inline-block rounded-full bg-zinc-100 px-2 py-0.5 text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300";
pub const CHECKBOX: &str = "h-4 w-4 accent-sky-600";

/// A labelled checkbox bound to a getter/setter pair.
#[component]
pub fn Toggle(
    #[prop(into)] label: String,
    #[prop(into)] checked: Signal<bool>,
    #[prop(into)] on_change: Callback<bool>,
    #[prop(optional, into)] hint: Option<String>,
) -> impl IntoView {
    view! {
        <label class="flex cursor-pointer items-start gap-3">
            <input
                type="checkbox"
                class=format!("{CHECKBOX} mt-0.5 shrink-0")
                prop:checked=checked
                on:change=move |ev| on_change.run(event_target_checked(&ev))
            />
            <span>
                <span class="block text-sm font-medium">{label}</span>
                {hint.map(|h| view! { <span class=format!("block {MUTED}")>{h}</span> })}
            </span>
        </label>
    }
}

/// Formats milliseconds as `m:ss`.
pub fn clock(ms: u32) -> String {
    let secs = ms / 1000;
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn rate_label(rate: f32) -> String {
    format!("{rate:.2}×")
}
