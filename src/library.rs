// Sentences tab: toggle the built-in set, import CSV / text files or pasted
// lines as named sets, and manage them.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::app::AppState;
use crate::model::{SentenceSet, sentence_pool};
use crate::sentences::{BUILTIN, parse_file, parse_lines, set_name_from_file};
use crate::storage::new_id;
use crate::ui::{BTN, BTN_DANGER, CARD, CHECKBOX, HEADING, INPUT, MUTED};

async fn read_file(file: &web_sys::File) -> Result<String, String> {
    JsFuture::from(file.text())
        .await
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| format!("Couldn't read {}", file.name()))
}

#[component]
fn SentenceList(sentences: Vec<String>) -> impl IntoView {
    view! {
        <details class="mt-2">
            <summary class=format!("cursor-pointer {MUTED}")>"Show sentences"</summary>
            <ol class="mt-2 list-decimal space-y-1 pl-6 text-sm">
                {sentences.into_iter().map(|s| view! { <li>{s}</li> }).collect_view()}
            </ol>
        </details>
    }
}

#[component]
pub fn Library() -> impl IntoView {
    let app = expect_context::<AppState>();
    let notice = RwSignal::new(None::<(bool, String)>);
    let paste_name = RwSignal::new(String::new());
    let paste_text = RwSignal::new(String::new());

    let add_set = move |name: String, sentences: Vec<String>| -> String {
        let n = sentences.len();
        app.sets.update(|sets| {
            sets.push(SentenceSet {
                id: new_id(),
                name: name.clone(),
                enabled: true,
                sentences,
            })
        });
        format!("Imported {n} sentences as “{name}”.")
    };

    let on_files = move |ev: leptos::ev::Event| {
        let Some(input) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let files: Vec<web_sys::File> = input
            .files()
            .map(|list| (0..list.length()).filter_map(|i| list.get(i)).collect())
            .unwrap_or_default();
        input.set_value("");
        spawn_local(async move {
            let mut messages = Vec::new();
            let mut ok = true;
            for file in files {
                let name = file.name();
                let parsed = read_file(&file)
                    .await
                    .and_then(|content| parse_file(&name, &content));
                match parsed {
                    Ok(s) if s.is_empty() => {
                        ok = false;
                        messages.push(format!("No sentences found in {name}."));
                    }
                    Ok(s) => messages.push(add_set(set_name_from_file(&name), s)),
                    Err(e) => {
                        ok = false;
                        messages.push(format!("{name}: {e}"));
                    }
                }
            }
            notice.set(Some((ok, messages.join(" "))));
        });
    };

    let on_paste = move |_| {
        let sentences = parse_lines(&paste_text.get_untracked());
        if sentences.is_empty() {
            notice.set(Some((
                false,
                "Paste at least one sentence (one per line).".into(),
            )));
            return;
        }
        let name = paste_name.get_untracked().trim().to_string();
        let name = if name.is_empty() {
            format!("Pasted set {}", app.sets.with_untracked(Vec::len) + 1)
        } else {
            name
        };
        notice.set(Some((true, add_set(name, sentences))));
        paste_text.set(String::new());
        paste_name.set(String::new());
    };

    let pool_size = Memo::new(move |_| {
        app.settings
            .with(|s| app.sets.with(|sets| sentence_pool(s, sets).len()))
    });

    view! {
        <div class="flex flex-col gap-6">
            <p class=MUTED>
                {move || {
                    format!("{} sentences are currently in the practice pool.", pool_size.get())
                }}
            </p>

            <section class=CARD>
                <label class="flex cursor-pointer items-center gap-3">
                    <input
                        type="checkbox"
                        class=CHECKBOX
                        prop:checked=move || app.settings.with(|s| s.builtin_enabled)
                        on:change=move |ev| {
                            app.settings.update(|s| s.builtin_enabled = event_target_checked(&ev))
                        }
                    />
                    <span class=HEADING>"Built-in sentences"</span>
                    <span class=MUTED>{format!("{} sentences", BUILTIN.len())}</span>
                </label>
                <SentenceList sentences=BUILTIN.iter().map(|s| s.to_string()).collect() />
            </section>

            <section class="flex flex-col gap-3">
                <h2 class=HEADING>"Your sets"</h2>
                <Show
                    when=move || app.sets.with(|s| !s.is_empty())
                    fallback=|| view! { <p class=MUTED>"Nothing imported yet."</p> }
                >
                    <For
                        each=move || app.sets.get()
                        key=|set| (set.id, set.enabled, set.name.clone())
                        let(set)
                    >
                        {
                            let id = set.id;
                            view! {
                                <div class=CARD>
                                    <div class="flex items-center gap-3">
                                        <input
                                            type="checkbox"
                                            class=CHECKBOX
                                            prop:checked=set.enabled
                                            on:change=move |ev| {
                                                let on = event_target_checked(&ev);
                                                app.sets
                                                    .update(|sets| {
                                                        if let Some(s) = sets.iter_mut().find(|s| s.id == id) {
                                                            s.enabled = on;
                                                        }
                                                    });
                                            }
                                        />
                                        <span class="flex-1 font-semibold">{set.name.clone()}</span>
                                        <span class=MUTED>
                                            {format!("{} sentences", set.sentences.len())}
                                        </span>
                                        <button
                                            class=BTN_DANGER
                                            on:click=move |_| {
                                                app.sets.update(|sets| sets.retain(|s| s.id != id))
                                            }
                                        >
                                            "Delete"
                                        </button>
                                    </div>
                                    <SentenceList sentences=set.sentences.clone() />
                                </div>
                            }
                        }
                    </For>
                </Show>
            </section>

            <section class=format!("{CARD} flex flex-col gap-4")>
                <h2 class=HEADING>"Import"</h2>
                <div class="flex flex-col gap-2">
                    <label class="text-sm font-medium" for="import-file">
                        "From files (.csv or .txt)"
                    </label>
                    <input
                        id="import-file"
                        type="file"
                        multiple
                        accept=".csv,.txt,text/csv,text/plain"
                        class="text-sm file:mr-3 file:rounded-full file:border-0 file:bg-sky-600 file:px-4 file:py-2 file:text-sm file:font-medium file:text-white hover:file:bg-sky-500"
                        on:change=on_files
                    />
                    <p class=MUTED>
                        "Text: one sentence per line; blank lines and lines starting with # are ignored. "
                        "CSV: the column headed “sentence” or “text” is used; without a header, the column with the most words. "
                        "Each file becomes its own set."
                    </p>
                </div>
                <div class="flex flex-col gap-2">
                    <label class="text-sm font-medium" for="paste-text">
                        "Or paste sentences (one per line)"
                    </label>
                    <input
                        class=INPUT
                        placeholder="Set name (optional)"
                        prop:value=paste_name
                        on:input=move |ev| paste_name.set(event_target_value(&ev))
                    />
                    <textarea
                        id="paste-text"
                        rows="5"
                        class=INPUT
                        placeholder="The lecture has been moved to Room 204.\nPlease bring your student card to the exam."
                        prop:value=paste_text
                        on:input=move |ev| paste_text.set(event_target_value(&ev))
                    ></textarea>
                    <div>
                        <button class=BTN on:click=on_paste>
                            "Add set"
                        </button>
                    </div>
                </div>
                {move || {
                    notice
                        .get()
                        .map(|(ok, msg)| {
                            view! {
                                <p
                                    class="text-sm"
                                    class=("text-emerald-600", ok)
                                    class=("text-rose-600", !ok)
                                >
                                    {msg}
                                </p>
                            }
                        })
                }}
            </section>
        </div>
    }
}
