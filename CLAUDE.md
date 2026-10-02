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
- `audio.rs` + `js/audio.js` — the browser-audio layer. JS owns the browser objects (utterances, the long-lived mic `MediaStream`, `MediaRecorder`, `AudioContext`/analyser for silence detection) and returns Promises; Rust wraps them as typed async fns. The mic is opened once (from the Start click's user gesture) and kept open so recording starts the instant the prompt ends. JS keeps a reference to the pending utterance because Chrome otherwise GCs it and never fires `end`.
- `model.rs` — pure domain types (`Settings`, `VoicePreset`, `SentenceSet`, `VoiceInfo`) and logic: `sentence_pool`, `ShuffleBag` (draw without replacement, no back-to-back repeats), `suggest_voices`, novelty-voice filtering. Randomness is injected (`FnMut() -> f64`, `js_sys::Math::random` in the app) so it's testable.
- `sentences.rs` — built-in sentences and the import parser (`parse_lines`: one sentence per line). CSV import was dropped deliberately — only the sentence text is ever used, so plain text covers it.
- `storage.rs` — localStorage JSON under versioned keys (`repeat-sentence.*.v1`). `Settings` is `#[serde(default)]` so added fields stay backward compatible.
- `ui.rs` — Tailwind class-string constants and the `Toggle` component. Styling is **Tailwind v4** via Trunk's built-in `tailwind-css` asset (version pinned in `Trunk.toml [tools]`); `@source "../src"` in `static/input.css` scans the Rust sources. Repeated utility runs are DRY'd in Rust (constants/components), not `@apply` classes.

Voice presets are keyed by `voiceURI`; presets whose voice isn't available in the current browser are skipped at pick time (and flagged in the Voices tab). Recordings are blob URLs kept only in memory and revoked when cleared/superseded.

## Design decisions (and alternatives considered)

- **Leptos 0.8 (latest stable)** rather than 0.9 beta.
- **CSR + Trunk** rather than SSR/hydration (cargo-leptos) — GitHub Pages is static hosting and the app has no server data; SSG would add build complexity for no gain.
- **JS module for audio** rather than raw `web-sys` — callback/closure lifetimes for MediaRecorder, analyser polling and speech events are much simpler in JS, and Promises give Rust a linear async flow.
- **localStorage** rather than IndexedDB — data is a few KB of text; recordings are intentionally not persisted.
- **Tab state in a signal** rather than `leptos_router` — avoids GitHub Pages sub-path / 404 handling for a single-screen app.
