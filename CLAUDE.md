# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A PTE Core "Repeat Sentence" practice app: a client-side Rust/WebAssembly app using [Leptos](https://leptos.dev/) 0.8 (CSR) bundled with [Trunk](https://trunk-rs.github.io/trunk/), deployed to GitHub Pages on push to `main`. Static files (`index.html`, `input.css`, `favicon.svg`) live under `static/`; `Trunk.toml` points `target` at `static/index.html`.

## Commands

```bash
trunk serve                     # dev server with hot reload
trunk build --release           # production build (dist/)
cargo test                      # native unit tests (pure logic: model, sentences, ui, practice helpers)
(cd e2e && npx playwright test) # browser e2e tests (needs `npm ci` + `npx playwright install chromium` once)
cargo clippy --target wasm32-unknown-unknown -- -D warnings
leptosfmt src/*.rs && cargo fmt # format (rust-analyzer.toml wires leptosfmt in)
```

`trunk serve`'s file watcher sometimes stops picking up changes (notably edits to `js/*.js`); if the served `dist/snippets/**` is stale, restart `trunk serve`.

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, wasm clippy with `-D warnings`, `cargo test`, and the Playwright e2e suite, on pull requests and pushes to `main`.

**E2E tests** (`e2e/`): Playwright + Chromium against `dist/` built by Trunk and served by `e2e/serve.mjs` (a dependency-free static server — Python's `http.server` resets connections under parallel load). `tests/fixtures.ts` installs fakes before the app loads: a scripted `speechSynthesis` (fixed voice list, records utterances in `window.__spoken`) and a `getUserMedia` oscillator whose level tests control (`mic.setLevel(0)` = silence) while counting live tracks (`mic.live()`), so tests can assert the mic is held only while recording. Options passed with `test.use` must not be bare arrays (Playwright treats `[value, options]` as a tuple) — hence `fakeVoices: { voices }`. Kokoro is not exercised beyond "nothing is downloaded until opt-in" (the model is 90+ MB).

## Architecture

- `app.rs` — `App` root. Owns `AppState` (settings, imported sets, browser voices) provided via context; `Effect`s persist settings/sets to localStorage on change. Tabs are **kept mounted and hidden** (`class:hidden`), so switching tabs never interrupts a recording or loses session history. On first visit (no stored settings) the voice pool is seeded with `suggest_voices`.
- `practice.rs` — the practice loop. `Session` is a `Copy` bundle of signals with the flow methods (`next`, `record`, `replay`, `stop`, `primary`). Async flows run in `spawn_local`; **cancellation uses a version counter** (`ver`): each flow captures it at start and bails once it changes (same pattern as pomodoro-leptos-csr's run loop). Interrupting also calls `audio::stop_speaking` / `cancel_recording` so the awaited promise settles.
- `audio.rs` + `js/audio.js` — the browser-audio layer. JS owns the browser objects (utterances, the mic `MediaStream`, `MediaRecorder`, `AudioContext`/analyser for silence detection) and returns Promises; Rust wraps them as typed async fns. **The mic is held only while recording**: `record()` opens it after the prompt finishes (during the configurable `record_delay_ms` pause, which runs concurrently so mic start-up time is absorbed into it) and releases it when recording ends, so other apps / Bluetooth earphones switching devices aren't blocked and headsets don't drop to call-quality audio during the prompt. `prime()` runs from the Start click to unlock audio and settle mic permission up front (asking, then releasing at once, only if not yet granted). Overlapping recordings are guarded: one shared `getUserMedia` in flight, and a superseded recording never releases a newer one's mic. `AudioContext.resume()` is awaited with a timeout because it can stay pending without user activation. JS keeps a reference to the pending utterance because Chrome otherwise GCs it and never fires `end`.
- `model.rs` — pure domain types (`Settings`, `VoicePreset`, `SentenceSet`, `VoiceInfo`) and logic: `sentence_pool`, `ShuffleBag` (draw without replacement, no back-to-back repeats), `suggest_voices`, novelty-voice filtering. Randomness is injected (`FnMut() -> f64`, `js_sys::Math::random` in the app) so it's testable.
- `sentences.rs` — built-in sentences and the import parser (`parse_lines`: one sentence per line). CSV import was dropped deliberately — only the sentence text is ever used, so plain text covers it.
- `kokoro.rs` + `js/kokoro.js` — optional on-device neural TTS. `kokoro-js` (pinned, from jsDelivr) is dynamically imported only when the user opts in; the model comes from the Hugging Face Hub and Transformers.js caches it in Cache Storage (`transformers-cache`), so `Settings.kokoro_enabled` makes later visits load it in the background (only if the pool has Kokoro presets). Backends are `device/dtype` keys: Auto picks WebGPU fp32 → WASM q8. **fp16 is offered only as a manual choice: on an iPhone it produced heavy noise. WebGPU with quantized weights is very slow (~30 s/sentence).** After a successful load, cached weights for other dtypes are pruned from `transformers-cache`. Generation is serialized and the last few results are cached as WAV blob URLs (instant replay). `practice.rs` `Session::prepare` starts generation before the pre-prompt delay so the two overlap; if Kokoro fails to load the item falls back to the browser voice. Presets carry `engine: Engine` (`#[serde(default)]` = Browser for old data).
- Generated prompts play via `audio::play_url` on one shared `<audio>` element, unlocked inside the Start / Preview click (`unlockPlayback`) because iOS only lets a media element play outside a gesture once it has played inside one; media elements also ignore the iPhone silent switch, unlike Web Audio. If playback hasn't started within 2 s (autoplay blocked, background tab) it falls back to Web Audio so the flow never hangs.
- `history.rs` + `js/history.js` — persistent practice history in IndexedDB (`repeat-sentence` DB): an `attempts` store (metadata, auto-increment `id`) and an `audio` store (the recording `Blob`, keyed by attempt id), so listing never loads audio. Recordings can be deleted on their own (`deleteAudio` / `clearAudio`) while attempts stay; `listAttempts` derives `hasAudio` per attempt from the audio store's keys (`AttemptRecord.has_audio` is `skip_serializing`, never stored). History rows are keyed by `(id, has_audio)` so they re-render when audio is removed. Each recording is saved in the background after it finishes (`practice.rs` `Session::keep`); `history_tab.rs` turns a saved recording into a blob URL only when played and revokes it on unmount. `AppState.history` mirrors the store (newest first) and `AppState.counts` derives per-sentence practice counts from it.
- `storage.rs` — settings and imported sentences as localStorage JSON under versioned keys (`repeat-sentence.*.v1`). `Settings` is `#[serde(default)]` so added fields stay backward compatible.
- `panic_hook.rs` — reports panics (message + JS stack) via `console.error`; replaces the archived `console_error_panic_hook` crate.
- `ui.rs` — Tailwind class-string constants and the `Toggle` component. Styling is **Tailwind v4** via Trunk's built-in `tailwind-css` asset (version pinned in `Trunk.toml [tools]`); `@source "../src"` in `static/input.css` scans the Rust sources. Repeated utility runs are DRY'd in Rust (constants/components), not `@apply` classes.

Voice presets are keyed by `voiceURI`; presets whose voice isn't available in the current browser are skipped at pick time (and flagged in the Voices tab). In-session recordings are blob URLs revoked when the session list is cleared; the persisted copies live in IndexedDB.

## Design decisions (and alternatives considered)

- **Leptos 0.8 (latest stable)** rather than 0.9 beta.
- **CSR + Trunk** rather than SSR/hydration (cargo-leptos) — GitHub Pages is static hosting and the app has no server data; SSG would add build complexity for no gain.
- **JS module for audio** rather than raw `web-sys` — callback/closure lifetimes for MediaRecorder, analyser polling and speech events are much simpler in JS, and Promises give Rust a linear async flow.
- **localStorage for settings/sentences, IndexedDB for history** — settings are a few KB of text; history holds audio blobs and grows without bound.
- **Kokoro via kokoro-js from a CDN, opt-in** rather than bundling — the model alone is 90–330 MB, so it's downloaded only on request; the library is pinned to an exact version.
- **Tab state in a signal** rather than `leptos_router` — avoids GitHub Pages sub-path / 404 handling for a single-screen app.
