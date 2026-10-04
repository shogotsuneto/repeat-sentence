// Kokoro-82M neural TTS running on-device via kokoro-js (Transformers.js /
// ONNX Runtime Web), bound into Rust by `src/kokoro.rs`. The library is
// loaded from a CDN and the model from the Hugging Face Hub only when the
// user opts in; Transformers.js caches the model in Cache Storage, so later
// loads work offline and skip the download.

const LIB_URL = "https://cdn.jsdelivr.net/npm/kokoro-js@1.2.1/dist/kokoro.web.js";
const MODEL_ID = "onnx-community/Kokoro-82M-v1.0-ONNX";

let tts = null;
let loaded = null; // "device/dtype" of the loaded model
let loading = null; // in-flight load: { key, promise }
// Generation runs one at a time: concurrent session runs aren't safe.
let queue = Promise.resolve();
// Recently generated audio, so replaying a prompt is instant.
const cache = new Map(); // key -> blob URL
const CACHE_SIZE = 8;

// Best backend for this device: WebGPU with fp32 (~1 s per sentence). fp16
// is half the download but produced heavy noise on an iPhone, so it's only
// offered as a manual choice; WebGPU with quantized weights is very slow.
// Without WebGPU fall back to WASM + q8 (smallest download, but several
// seconds per sentence).
export async function detectBackend() {
  try {
    if (await navigator.gpu?.requestAdapter()) return "webgpu/fp32";
  } catch {
    // WebGPU present but unusable: fall through.
  }
  return "wasm/q8";
}

// Transformers.js names the weights file by dtype.
const MODEL_FILE = { fp32: "model.onnx", fp16: "model_fp16.onnx", q8: "model_quantized.onnx" };

// Whether the weights for `backend` ("device/dtype") are already
// downloaded, so loading won't hit the network for the big file.
export async function isCached(backend) {
  try {
    const file = MODEL_FILE[backend.split("/")[1]];
    const cache = await globalThis.caches?.open("transformers-cache");
    const keys = (await cache?.keys()) ?? [];
    return keys.some((req) => req.url.endsWith(`/onnx/${file}`));
  } catch {
    return false;
  }
}

// Drops cached weights for other dtypes (e.g. fp16 after switching to fp32)
// so an old download doesn't keep taking up 100+ MB.
async function pruneOtherWeights(dtype) {
  try {
    const cache = await globalThis.caches?.open("transformers-cache");
    if (!cache) return;
    const keep = `/onnx/${MODEL_FILE[dtype]}`;
    for (const req of await cache.keys()) {
      if (/\/onnx\/model(_\w+)?\.onnx$/.test(req.url) && !req.url.endsWith(keep)) {
        await cache.delete(req);
      }
    }
  } catch {
    // Best effort.
  }
}

export function loadedBackend() {
  return loaded;
}

// Loads the model for `backend` ("device/dtype"). `onProgress(loaded, total)`
// reports download bytes of the model weights (the .onnx file is ~all of
// the download; counting the small files too makes progress jump back as
// each new file's size becomes known). Re-calling with the same backend
// while loading shares the in-flight load.
export function load(backend, onProgress) {
  if (loaded === backend) return Promise.resolve();
  if (loading?.key === backend) return loading.promise;
  const [device, dtype] = backend.split("/");
  const files = new Map(); // file -> [loaded, total]
  const promise = (async () => {
    const { KokoroTTS } = await import(LIB_URL);
    const model = await KokoroTTS.from_pretrained(MODEL_ID, {
      device,
      dtype,
      progress_callback: (p) => {
        if (p.status !== "progress" || !p.total || !p.file.endsWith(".onnx")) return;
        files.set(p.file, [p.loaded, p.total]);
        let done = 0;
        let total = 0;
        for (const [l, t] of files.values()) {
          done += l;
          total += t;
        }
        onProgress(done, total);
      },
    });
    // Release the previous backend's model before swapping.
    await tts?.model?.dispose?.();
    tts = model;
    loaded = backend;
    clearCache();
    pruneOtherWeights(dtype);
  })().finally(() => {
    if (loading?.promise === promise) loading = null;
  });
  loading = { key: backend, promise };
  return promise;
}

function clearCache() {
  for (const url of cache.values()) URL.revokeObjectURL(url);
  cache.clear();
}

// Resolves a blob URL of a WAV for `text` read by `voice` at `speed`. The
// URL stays owned by the cache — don't revoke it.
export function generate(text, voice, speed) {
  const key = `${voice}|${speed}|${text}`;
  const hit = cache.get(key);
  if (hit) {
    // Refresh recency.
    cache.delete(key);
    cache.set(key, hit);
    return Promise.resolve(hit);
  }
  const run = queue.then(async () => {
    if (!tts) throw new Error("Kokoro model is not loaded");
    const audio = await tts.generate(text, { voice, speed });
    const url = URL.createObjectURL(audio.toBlob());
    cache.set(key, url);
    while (cache.size > CACHE_SIZE) {
      const [oldKey, oldUrl] = cache.entries().next().value;
      cache.delete(oldKey);
      URL.revokeObjectURL(oldUrl);
    }
    return url;
  });
  // Keep the queue going even if this run fails.
  queue = run.catch(() => {});
  return run;
}

// Unloads the model and deletes the downloaded files from Cache Storage.
export async function forget() {
  await loading?.promise.catch(() => {});
  await tts?.model?.dispose?.();
  tts = null;
  loaded = null;
  clearCache();
  // Transformers.js caches the model; kokoro-js caches voice embeddings.
  await Promise.all(
    ["transformers-cache", "kokoro-voices"].map((name) => globalThis.caches?.delete(name)),
  );
}
