// Thin browser-audio layer bound into Rust by `src/audio.rs`: speech
// synthesis, microphone recording with silence detection, and a cue tone.
// Anything asynchronous returns a Promise so the Rust side can `.await` the
// practice flow as straight-line code.

// RMS level below which input counts as silence. Speech into a laptop mic
// with noise suppression sits well above this; room noise well below.
const SILENCE_RMS = 0.015;
const TICK_MS = 50;

let ctx = null;
let stream = null;
// The in-flight recording, if any: { recorder, finish(reason) }.
let active = null;
// Chrome garbage-collects a queued utterance (and never fires its `end`
// event) unless something keeps a reference to it.
let utterance = null;

function audioCtx() {
  if (!ctx) ctx = new (window.AudioContext || window.webkitAudioContext)();
  if (ctx.state === "suspended") ctx.resume();
  return ctx;
}

export function speechSupported() {
  return "speechSynthesis" in window && "SpeechSynthesisUtterance" in window;
}

export function recordingSupported() {
  return !!navigator.mediaDevices?.getUserMedia && "MediaRecorder" in window;
}

function voiceList() {
  return speechSynthesis.getVoices().map((v) => ({
    uri: v.voiceURI,
    name: v.name,
    lang: v.lang,
    local: v.localService,
  }));
}

// Voices load asynchronously in most browsers; resolve once they arrive or
// after `timeoutMs` with whatever is there.
export function loadVoices(timeoutMs) {
  return new Promise((resolve) => {
    if (!speechSupported()) return resolve([]);
    if (speechSynthesis.getVoices().length) return resolve(voiceList());
    const done = () => {
      clearTimeout(timer);
      speechSynthesis.removeEventListener("voiceschanged", done);
      resolve(voiceList());
    };
    const timer = setTimeout(done, timeoutMs);
    speechSynthesis.addEventListener("voiceschanged", done);
  });
}

export function onVoicesChanged(cb) {
  if (speechSupported()) {
    speechSynthesis.addEventListener("voiceschanged", () => cb(voiceList()));
  }
}

// Resolves true when the utterance finished, false when it was cancelled
// (by `stopSpeaking` or a newer `speak`). An empty or unknown `voiceUri`
// falls back to the browser's default English voice.
export function speak(text, voiceUri, rate) {
  return new Promise((resolve, reject) => {
    if (!speechSupported()) return reject(new Error("Speech synthesis is not supported in this browser"));
    speechSynthesis.cancel();
    const u = new SpeechSynthesisUtterance(text);
    const voice = speechSynthesis.getVoices().find((v) => v.voiceURI === voiceUri);
    if (voice) {
      u.voice = voice;
      u.lang = voice.lang;
    } else {
      u.lang = "en-US";
    }
    u.rate = rate;
    u.onend = () => {
      if (utterance === u) utterance = null;
      resolve(true);
    };
    u.onerror = (e) => {
      if (utterance === u) utterance = null;
      if (e.error === "interrupted" || e.error === "canceled") resolve(false);
      else reject(new Error(`Speech synthesis failed: ${e.error}`));
    };
    utterance = u;
    // Give `cancel()` a moment to settle (Chrome otherwise sometimes drops
    // the next utterance), and un-stick a queue Chrome left paused.
    setTimeout(() => {
      if (utterance !== u) return resolve(false);
      speechSynthesis.resume();
      speechSynthesis.speak(u);
    }, 60);
  });
}

export function stopSpeaking() {
  utterance = null;
  if (speechSupported()) speechSynthesis.cancel();
}

function micLive() {
  return !!stream && stream.getAudioTracks().some((t) => t.readyState === "live");
}

export function micReady() {
  return micLive();
}

// Opens the microphone once and keeps it open, so recording can start the
// instant the prompt ends. Call from a user gesture: it also unlocks the
// AudioContext on iOS.
export async function ensureMic() {
  audioCtx();
  if (micLive()) return;
  if (!recordingSupported()) throw new Error("Recording is not supported in this browser");
  stream = await navigator.mediaDevices.getUserMedia({
    audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  });
}

export function beep(durationMs) {
  const c = audioCtx();
  const t = c.currentTime;
  const end = t + durationMs / 1000;
  const osc = c.createOscillator();
  const gain = c.createGain();
  osc.frequency.value = 880;
  // Exponential ramps avoid audible clicks at the edges.
  gain.gain.setValueAtTime(0.0001, t);
  gain.gain.exponentialRampToValueAtTime(0.2, t + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.0001, end);
  osc.connect(gain).connect(c.destination);
  osc.start(t);
  osc.stop(end + 0.02);
  return new Promise((r) => setTimeout(r, durationMs + 40));
}

// Records until `maxMs` passes, `stopRecording` is called, or — when
// `silenceMs` > 0 — the input stays silent for `silenceMs` (the exam closes
// the mic the same way). `onTick(level 0..1, elapsedMs, silentMs)` drives the
// meter. Resolves `{ url, mime, durationMs, reason }`, or null when cancelled.
export function record(maxMs, silenceMs, onTick) {
  if (active) active.finish("cancel");
  return new Promise((resolve, reject) => {
    if (!micLive()) return reject(new Error("Microphone is not ready"));
    const c = audioCtx();
    const source = c.createMediaStreamSource(stream);
    const analyser = c.createAnalyser();
    analyser.fftSize = 2048;
    source.connect(analyser);
    const buf = new Float32Array(analyser.fftSize);

    const recorder = new MediaRecorder(stream);
    const chunks = [];
    const start = performance.now();
    let lastVoice = start;
    let end = start;
    let reason = null;
    let failure = null;

    const finish = (r) => {
      if (reason) return;
      reason = r;
      end = performance.now();
      clearInterval(timer);
      source.disconnect();
      if (active?.recorder === recorder) active = null;
      if (recorder.state !== "inactive") recorder.stop();
      else settle();
    };
    const settle = () => {
      if (failure) return reject(failure);
      if (reason === "cancel") return resolve(null);
      const mime = recorder.mimeType || chunks[0]?.type || "audio/webm";
      const blob = new Blob(chunks, { type: mime });
      resolve({
        url: URL.createObjectURL(blob),
        mime,
        durationMs: Math.round(end - start),
        reason,
      });
    };

    recorder.ondataavailable = (e) => {
      if (e.data.size) chunks.push(e.data);
    };
    recorder.onstop = settle;
    recorder.onerror = (e) => {
      failure = new Error(`Recording failed: ${e.error?.message ?? "unknown error"}`);
      finish("error");
    };

    const timer = setInterval(() => {
      analyser.getFloatTimeDomainData(buf);
      let sum = 0;
      for (const x of buf) sum += x * x;
      const rms = Math.sqrt(sum / buf.length);
      const now = performance.now();
      if (rms > SILENCE_RMS) lastVoice = now;
      const elapsed = now - start;
      const silent = now - lastVoice;
      onTick(Math.min(1, Math.sqrt(rms) * 2.5), elapsed, silent);
      if (elapsed >= maxMs) finish("timeout");
      else if (silenceMs > 0 && silent >= silenceMs) finish("silence");
    }, TICK_MS);

    active = { recorder, finish };
    recorder.start();
  });
}

export function stopRecording() {
  active?.finish("manual");
}

export function cancelRecording() {
  active?.finish("cancel");
}

export function revokeUrl(url) {
  URL.revokeObjectURL(url);
}
