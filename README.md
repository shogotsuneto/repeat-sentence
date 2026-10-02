# repeat-sentence

A browser app for practising the **Repeat Sentence** task of PTE Core, built with [Leptos](https://leptos.dev/) (CSR) and WebAssembly.

[Live app](https://shogotsuneto.github.io/repeat-sentence/)

## Features

- A sentence is read aloud with the Web Speech API, then recording starts immediately — like the exam, it stops after 15 s or 3 s of silence (both configurable)
- Each question picks a **random voice preset** (voice + speaking rate) from a pool you build; on first visit the pool is seeded with one voice per English accent (US, UK, AU, IN, …)
- 48 built-in sentences; add your own from **CSV** or **plain-text** files (one sentence per line), or by pasting
- Sentences are drawn without repeats until the whole pool has been used
- Sentence text stays hidden until your attempt is over; replay the prompt, play back or download your recording, record again
- Keyboard: <kbd>Space</kbd> start / next / stop recording, <kbd>Esc</kbd> stop
- Settings, voice presets and imported sentences persist in `localStorage`. Recordings live only in memory and are never uploaded.

Available voices depend on the browser and OS (Chrome and Edge offer high-quality online voices; Safari uses the system voices).

### Import formats

- **Text** (`.txt`, or anything not `.csv`): one sentence per line. Blank lines and lines starting with `#` are skipped.
- **CSV**: uses the column headed `sentence` / `sentences` / `text` (case-insensitive). With no such header, the column with the most words is used, so `id,sentence` files without a header also work.

## Development

Requires the `wasm32-unknown-unknown` target and [Trunk](https://trunk-rs.github.io/trunk/). Trunk downloads the Tailwind CSS standalone CLI itself — no Node needed.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

trunk serve            # dev server with hot reload at http://localhost:8080
trunk build --release  # production build into dist/
cargo test             # unit tests for the pure logic
```

## Deployment

Pushing to `main` builds and deploys to GitHub Pages via `.github/workflows/deploy.yml` (Settings → Pages → Source must be **GitHub Actions**).
