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

async function audioCtx() {
  if (!ctx) ctx = new (window.AudioContext || window.webkitAudioContext)();
  // Without user activation `resume()` can stay pending indefinitely; don't
  // let that stall the flow.
  if (ctx.state === "suspended") {
    await Promise.race([ctx.resume(), new Promise((r) => setTimeout(r, 300))]);
  }
  return ctx;
}

// One reusable <audio> element for generated (Kokoro) prompts. iOS only lets
// a media element play outside a user gesture once it has played inside
// one, so `prime()` unlocks it on the Start click. Media elements also keep
// playing with the iPhone's silent switch on, unlike Web Audio.
let player = null;
const PLAY_START_TIMEOUT_MS = 2000;
// The in-flight `playUrl`, if any: { resolve }.
let playing = null;
// 0.1 s of silence as a WAV blob URL (8 kHz, 8-bit mono: silence is 0x80).
function silenceUrl() {
  const n = 800;
  const bytes = new Uint8Array(44 + n).fill(0x80);
  const v = new DataView(bytes.buffer);
  const text = (off, str) => [...str].forEach((c, i) => v.setUint8(off + i, c.charCodeAt(0)));
  text(0, "RIFF");
  v.setUint32(4, 36 + n, true);
  text(8, "WAVEfmt ");
  v.setUint32(16, 16, true); // fmt chunk size
  v.setUint16(20, 1, true); // PCM
  v.setUint16(22, 1, true); // mono
  v.setUint32(24, 8000, true); // sample rate
  v.setUint32(28, 8000, true); // byte rate
  v.setUint16(32, 1, true); // block align
  v.setUint16(34, 8, true); // bits per sample
  text(36, "data");
  v.setUint32(40, n, true);
  return URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
}

function audioPlayer() {
  player ??= new Audio();
  return player;
}

// Call from a user gesture (the Start click): unlocks audio on iOS, and if
// microphone permission hasn't been granted yet, asks now — then releases
// the mic straight away — so the prompt doesn't interrupt the first attempt.
// Must run synchronously inside a user gesture (click handler).
export function unlockPlayback() {
  const p = audioPlayer();
  if (!p.src) {
    p.src = silenceUrl();
    p.play().catch(() => {});
  }
}

export async function prime() {
  unlockPlayback();
  await audioCtx();
  if (!recordingSupported()) return;
  try {
    const status = await navigator.permissions?.query({ name: "microphone" });
    if (status?.state === "granted") return;
  } catch {
    // Permissions API without "microphone" (older Safari/Firefox): ask anyway.
  }
  const s = await navigator.mediaDevices.getUserMedia({ audio: true });
  s.getTracks().forEach((t) => t.stop());
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
  if (playing) {
    const p = playing;
    playing = null;
    p.stop();
    p.resolve(false);
  }
}

// Plays an audio URL (a generated prompt). Resolves true when it played to
// the end, false when stopped by `stopSpeaking` or a newer prompt — the same
// contract as `speak`.
//
// Uses the shared <audio> element first (plays through the iPhone's silent
// switch). If that hasn't started within PLAY_START_TIMEOUT_MS — autoplay
// blocked, or a tab the browser won't start media in — it falls back to
// Web Audio rather than leaving the flow waiting forever.
export function playUrl(url) {
  stopSpeaking();
  return new Promise((resolve, reject) => {
    const p = audioPlayer();
    let source = null; // Web Audio fallback, once started
    const mine = {
      resolve,
      stop: () => {
        clearTimeout(watchdog);
        p.pause();
        source?.stop();
      },
    };
    playing = mine;
    const finish = (ok, err) => {
      if (playing !== mine) return;
      playing = null;
      mine.stop();
      if (err) reject(err);
      else resolve(ok);
    };
    const fallback = async () => {
      if (playing !== mine) return;
      p.pause();
      try {
        const c = await audioCtx();
        const buf = await c.decodeAudioData(await (await fetch(url)).arrayBuffer());
        if (playing !== mine) return;
        source = c.createBufferSource();
        source.buffer = buf;
        source.connect(c.destination);
        source.onended = () => finish(true);
        source.start();
      } catch (e) {
        finish(false, new Error(`Couldn't play the generated audio: ${e.message}`));
      }
    };
    const watchdog = setTimeout(fallback, PLAY_START_TIMEOUT_MS);
    p.onplaying = () => clearTimeout(watchdog);
    p.onended = () => finish(true);
    p.onerror = () => {
      clearTimeout(watchdog);
      fallback();
    };
    p.src = url;
    p.play().catch((e) => {
      // Superseded by a newer prompt: that one owns the element now.
      if (e.name === "AbortError") return;
      clearTimeout(watchdog);
      fallback();
    });
  });
}

function micLive() {
  return !!stream && stream.getAudioTracks().some((t) => t.readyState === "live");
}

// The mic is held only while recording: other apps (and the OS, when moving
// Bluetooth earphones between devices) can't use it while we hold it, and
// Bluetooth headsets drop to low-quality call audio while it is open.
let opening = null;

async function openMic() {
  if (micLive()) return;
  if (!recordingSupported()) throw new Error("Recording is not supported in this browser");
  // Share one in-flight request so overlapping callers don't open two streams.
  opening ??= navigator.mediaDevices
    .getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } })
    .then((s) => {
      stream = s;
    })
    .finally(() => {
      opening = null;
    });
  await opening;
}

// Release only when no newer recording has taken over the mic.
function releaseIfIdle() {
  if (!active) releaseMic();
}

function releaseMic() {
  stream?.getTracks().forEach((t) => t.stop());
  stream = null;
}

async function beep(durationMs) {
  const c = await audioCtx();
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
  await new Promise((r) => setTimeout(r, durationMs + 40));
}

// Waits `delayMs` after the prompt (opening the mic meanwhile), optionally
// beeps, and records until `maxMs` passes, `stopRecording` is called, or —
// when `silenceMs` > 0 — the input stays silent for `silenceMs` (the exam
// closes the mic the same way). The mic is released when it ends.
// `onStart()` fires when recording actually begins; `onTick(level, elapsedMs,
// silentMs)` drives the meter, with level 0..1, or -1 while it's unknown.
// Resolves `{ url, mime, durationMs, reason, levelUnknownMs, contextStates }`,
// or null when cancelled.
export async function record(maxMs, silenceMs, delayMs, beepMs, onStart, onTick) {
  if (active) active.finish("cancel");
  // Claim the slot before the awaits so a cancel during mic start-up lands.
  let cancelled = false;
  const pending = {
    recorder: null,
    finish: () => {
      cancelled = true;
      if (active === pending) active = null;
    },
  };
  active = pending;
  let c;
  try {
    // The delay runs from the end of the prompt, so mic start-up time is
    // absorbed into it rather than added on top.
    await Promise.all([openMic(), new Promise((r) => setTimeout(r, delayMs))]);
    if (!cancelled && beepMs > 0) await beep(beepMs);
    c = await audioCtx();
  } catch (e) {
    if (active === pending) active = null;
    releaseIfIdle();
    throw e;
  }
  if (cancelled) {
    releaseIfIdle();
    return null;
  }
  return new Promise((resolve, reject) => {
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
      c.onstatechange = null;
      releaseIfIdle();
      if (failure) return reject(failure);
      if (reason === "cancel") return resolve(null);
      const mime = recorder.mimeType || chunks[0]?.type || "audio/webm";
      const blob = new Blob(chunks, { type: mime });
      resolve({
        url: URL.createObjectURL(blob),
        mime,
        durationMs: Math.round(end - start),
        reason,
        levelUnknownMs: unknownMs,
        contextStates: [...states].join(","),
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

    // The level comes from Web Audio, separately from the recorder. If the
    // AudioContext isn't running (e.g. iOS interrupts it when the mic
    // switches Bluetooth earphones to call mode), the analyser gets nothing
    // while the recording itself is fine. Treat that as "level unknown" —
    // not silence — so auto-stop doesn't cut the answer off. A live mic is
    // never exactly zero (there's always some noise), so until a non-zero
    // sample arrives the level is unknown too.
    let heard = false;
    let lastResume = 0;
    let unknownMs = 0;
    const states = new Set([c.state]);
    c.onstatechange = () => states.add(c.state);
    const timer = setInterval(() => {
      const now = performance.now();
      const elapsed = now - start;
      analyser.getFloatTimeDomainData(buf);
      let sum = 0;
      for (const x of buf) {
        sum += x * x;
        if (x !== 0) heard = true;
      }
      const running = c.state === "running";
      if (!running && now - lastResume > 1000) {
        lastResume = now;
        c.resume().catch(() => {});
      }
      if (running && heard) {
        const rms = Math.sqrt(sum / buf.length);
        if (rms > SILENCE_RMS) lastVoice = now;
        onTick(Math.min(1, Math.sqrt(rms) * 2.5), elapsed, now - lastVoice);
      } else {
        // Unknown: hold the silence clock rather than let it run.
        lastVoice = now;
        unknownMs += TICK_MS;
        onTick(-1, elapsed, 0);
      }
      if (elapsed >= maxMs) finish("timeout");
      else if (silenceMs > 0 && now - lastVoice >= silenceMs) finish("silence");
    }, TICK_MS);

    active = { recorder, finish };
    recorder.start();
    onStart();
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
