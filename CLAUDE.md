# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A PTE Core "Repeat Sentence" practice app: a client-side Rust/WebAssembly app using [Leptos](https://leptos.dev/) 0.8 (CSR) bundled with [Trunk](https://trunk-rs.github.io/trunk/), deployed to GitHub Pages on push to `main`. Static files (`index.html`, `input.css`, `favicon.svg`) live under `static/`; `Trunk.toml` points `target` at `static/index.html`.

## Commands

```bash
trunk serve                     # dev server with hot reload
trunk build --release           # production build (dist/)
cargo test                      # native unit tests (model.rs, sentences.rs)
cargo clippy --target wasm32-unknown-unknown -- -D warnings
leptosfmt src/*.rs && cargo fmt # format (rust-analyzer.toml wires leptosfmt in)
```

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, wasm clippy with `-D warnings`, and `cargo test`.

## Architecture

- `app.rs` — `App` root. Owns `AppState` (settings, imported sets, browser voices) provided via context; `Effect`s persist settings/sets to localStorage on change. Tabs are **kept mounted and hidden** (`class:hidden`), so switching tabs never interrupts a recording or loses session history. On first visit (no stored settings) the voice pool is seeded with `suggest_voices`.
- `practice.rs` — the practice loop. `Session` is a `Copy` bundle of signals with the flow methods (`next`, `record`, `replay`, `stop`, `primary`). Async flows run in `spawn_local`; **cancellation uses a version counter** (`ver`): each flow captures it at start and bails once it changes (same pattern as pomodoro-leptos-csr's run loop). Interrupting also calls `audio::stop_speaking` / `cancel_recording` so the awaited promise settles.
- `audio.rs` + `js/audio.js` — the browser-audio layer. JS owns the browser objects (utterances, the mic `MediaStream`, `MediaRecorder`, `AudioContext`/analyser for silence detection) and returns Promises; Rust wraps them as typed async fns. **The mic is held only while recording**: `record()` opens it after the prompt finishes (during the configurable `record_delay_ms` pause, which runs concurrently so mic start-up time is absorbed into it) and releases it when recording ends, so other apps / Bluetooth earphones switching devices aren't blocked and headsets don't drop to call-quality audio during the prompt. `prime()` runs from the Start click to unlock audio and settle mic permission up front (asking, then releasing at once, only if not yet granted). Overlapping recordings are guarded: one shared `getUserMedia` in flight, and a superseded recording never releases a newer one's mic. `AudioContext.resume()` is awaited with a timeout because it can stay pending without user activation. JS keeps a reference to the pending utterance because Chrome otherwise GCs it and never fires `end`.
- `model.rs` — pure domain types (`Settings`, `VoicePreset`, `SentenceSet`, `VoiceInfo`) and logic: `sentence_pool`, `ShuffleBag` (draw without replacement, no back-to-back repeats), `suggest_voices`, novelty-voice filtering. Randomness is injected (`FnMut() -> f64`, `js_sys::Math::random` in the app) so it's testable.
- `sentences.rs` — built-in sentences and the import parser (`parse_lines`: one sentence per line). CSV import was dropped deliberately — only the sentence text is ever used, so plain text covers it.
- `history.rs` + `js/history.js` — persistent practice history in IndexedDB (`repeat-sentence` DB): an `attempts` store (metadata, auto-increment `id`) and an `audio` store (the recording `Blob`, keyed by attempt id), so listing never loads audio. Each recording is saved in the background after it finishes (`practice.rs` `Session::keep`); `history_tab.rs` turns a saved recording into a blob URL only when played and revokes it on unmount. `AppState.history` mirrors the store (newest first) and `AppState.counts` derives per-sentence practice counts from it.
- `storage.rs` — settings and imported sentences as localStorage JSON under versioned keys (`repeat-sentence.*.v1`). `Settings` is `#[serde(default)]` so added fields stay backward compatible.
- `panic_hook.rs` — reports panics (message + JS stack) via `console.error`; replaces the archived `console_error_panic_hook` crate.
- `ui.rs` — Tailwind class-string constants and the `Toggle` component. Styling is **Tailwind v4** via Trunk's built-in `tailwind-css` asset (version pinned in `Trunk.toml [tools]`); `@source "../src"` in `static/input.css` scans the Rust sources. Repeated utility runs are DRY'd in Rust (constants/components), not `@apply` classes.

Voice presets are keyed by `voiceURI`; presets whose voice isn't available in the current browser are skipped at pick time (and flagged in the Voices tab). In-session recordings are blob URLs revoked when the session list is cleared; the persisted copies live in IndexedDB.

## Design decisions (and alternatives considered)

- **Leptos 0.8 (latest stable)** rather than 0.9 beta.
- **CSR + Trunk** rather than SSR/hydration (cargo-leptos) — GitHub Pages is static hosting and the app has no server data; SSG would add build complexity for no gain.
- **JS module for audio** rather than raw `web-sys` — callback/closure lifetimes for MediaRecorder, analyser polling and speech events are much simpler in JS, and Promises give Rust a linear async flow.
- **localStorage for settings/sentences, IndexedDB for history** — settings are a few KB of text; history holds audio blobs and grows without bound.
- **Tab state in a signal** rather than `leptos_router` — avoids GitHub Pages sub-path / 404 handling for a single-screen app.
