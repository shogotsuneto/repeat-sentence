# repeat-sentence

A browser app for practising the **Repeat Sentence** task of PTE Core, built with [Leptos](https://leptos.dev/) (CSR) and WebAssembly.

[Live app](https://shogotsuneto.github.io/repeat-sentence/)

## Features

- A sentence is read aloud with the Web Speech API, then — after a short pause and a beep (1.5 s by default, adjustable) — recording starts; like the exam, it stops after 15 s or 3 s of silence (both configurable)
- Each question picks a **random voice preset** (voice + speaking rate) from a pool you build; on first visit the pool is seeded with one voice per English accent (US, UK, AU, IN, …). Enhanced / Premium system voices are labelled as such
- **Kokoro neural voices** (optional): natural US / UK voices generated on-device by the open [Kokoro-82M](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX) model via [kokoro-js](https://www.npmjs.com/package/kokoro-js) — handy where the browser's built-in voices are poor (e.g. iPhone). Downloaded on request from Hugging Face (326 MB with WebGPU fp32, 92 MB for the slower CPU fallback) and cached by the browser
- 48 built-in sentences; add your own from **plain-text** files (one sentence per line) or by pasting
- Sentences are drawn without repeats until the whole pool has been used
- Sentence text stays hidden until your attempt is over; replay the prompt, play back or download your recording, record again
- Keyboard: <kbd>Space</kbd> start / next / stop recording, <kbd>Esc</kbd> stop
- **History**: every attempt (sentence, voice, rate, length, time) and its recording is saved on the device in IndexedDB — replay or download past answers, see per-sentence practice counts; delete individual attempts or everything, or delete just the recordings (one or all) to free space while keeping the history
- The microphone is held only while recording, so earphones can switch devices freely between questions
- Settings, voice presets and imported sentences persist in `localStorage`. Nothing you practise is ever uploaded.

Available voices depend on the browser and OS (Chrome and Edge offer high-quality online voices; Safari uses the system voices).

### Import format

Plain text (`.txt`): one sentence per line. Blank lines and lines starting with `#` are skipped; duplicates are dropped. Each file becomes its own toggleable set.

[`examples/everyday.txt`](examples/everyday.txt) is a ready-to-import set of everyday sentences (shopping, transport, eating out, appointments, small talk) that also shows the format.

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
