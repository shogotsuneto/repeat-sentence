// Shared test setup. Headless Chromium on CI has neither speech-synthesis
// voices nor a microphone, so both are replaced with deterministic fakes
// before the app loads:
//
// - speechSynthesis: a fixed voice list; `speak` "plays" for SPEAK_MS, then
//   fires `end`. Every utterance is recorded in `window.__spoken`.
// - getUserMedia: an oscillator stream whose loudness tests control with
//   `mic.setLevel(0..1)` (0 = silence). Live / opened track counts are
//   tracked so tests can assert when the app holds the microphone.

import { test as base, expect, type Page } from "@playwright/test";

export type FakeVoice = { voiceURI: string; name: string; lang: string; localService?: boolean };

export const DEFAULT_VOICES: FakeVoice[] = [
  { voiceURI: "test.en-US.Samantha", name: "Samantha", lang: "en-US" },
  { voiceURI: "test.en-GB.Daniel", name: "Daniel", lang: "en-GB" },
  { voiceURI: "test.en-AU.Karen", name: "Karen", lang: "en-AU" },
  { voiceURI: "test.ja-JP.Kyoko", name: "Kyoko", lang: "ja-JP" },
];

// Long enough for tests to observe the "Listen" phase.
const SPEAK_MS = 1500;

function installFakes({ voices, speakMs }: { voices: FakeVoice[]; speakMs: number }) {
  const w = window as any;

  // --- speech synthesis ---
  const fakeVoices = voices.map((v) => ({ default: false, localService: true, ...v }));
  w.__spoken = [];
  let pending: { u: any; timer: number } | null = null;
  class FakeUtterance {
    text: string;
    voice: any = null;
    lang = "";
    rate = 1;
    pitch = 1;
    onend: ((e: any) => void) | null = null;
    onerror: ((e: any) => void) | null = null;
    constructor(text: string) {
      this.text = text;
    }
  }
  const synth = {
    getVoices: () => fakeVoices,
    speak(u: any) {
      w.__spoken.push({ text: u.text, voice: u.voice?.voiceURI ?? null, rate: u.rate });
      const timer = window.setTimeout(() => {
        pending = null;
        u.onend?.({});
      }, speakMs);
      pending = { u, timer };
    },
    cancel() {
      if (!pending) return;
      clearTimeout(pending.timer);
      const { u } = pending;
      pending = null;
      u.onerror?.({ error: "interrupted" });
    },
    resume() {},
    pause() {},
    addEventListener() {},
    removeEventListener() {},
  };
  Object.defineProperty(window, "speechSynthesis", { value: synth, configurable: true });
  Object.defineProperty(window, "SpeechSynthesisUtterance", { value: FakeUtterance, configurable: true });

  // --- microphone ---
  const mic = { level: 0.5, live: 0, opened: 0, gains: [] as GainNode[] };
  w.__mic = mic;
  w.__setMicLevel = (level: number) => {
    mic.level = level;
    mic.gains.forEach((g) => (g.gain.value = level));
  };
  navigator.mediaDevices.getUserMedia = async () => {
    const ctx = new AudioContext();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    gain.gain.value = mic.level;
    const dest = ctx.createMediaStreamDestination();
    osc.connect(gain).connect(dest);
    osc.start();
    mic.gains.push(gain);
    mic.opened++;
    mic.live++;
    for (const track of dest.stream.getTracks()) {
      const stop = track.stop.bind(track);
      let stopped = false;
      track.stop = () => {
        if (!stopped) {
          stopped = true;
          mic.live--;
        }
        stop();
      };
    }
    return dest.stream;
  };
  // Pretend permission is already granted, so `prime()` doesn't open the mic.
  const query = navigator.permissions.query.bind(navigator.permissions);
  navigator.permissions.query = (desc: any) =>
    desc?.name === "microphone" ? Promise.resolve({ state: "granted" } as PermissionStatus) : query(desc);
}

type Fixtures = {
  // Wrapped in an object: Playwright reads an array passed to `test.use`
  // as a [value, options] tuple.
  fakeVoices: { voices: FakeVoice[] };
  mic: { setLevel: (level: number) => Promise<void>; live: () => Promise<number>; opened: () => Promise<number> };
  spoken: () => Promise<{ text: string; voice: string | null; rate: number }[]>;
};

export const test = base.extend<Fixtures>({
  fakeVoices: [{ voices: DEFAULT_VOICES }, { option: true }],
  page: async ({ page, fakeVoices }, use) => {
    await page.addInitScript(installFakes, { voices: fakeVoices.voices, speakMs: SPEAK_MS });
    await use(page);
  },
  mic: async ({ page }, use) => {
    await use({
      setLevel: (level) => page.evaluate((l) => (window as any).__setMicLevel(l), level),
      live: () => page.evaluate(() => (window as any).__mic.live),
      opened: () => page.evaluate(() => (window as any).__mic.opened),
    });
  },
  spoken: async ({ page }, use) => {
    await use(() => page.evaluate(() => (window as any).__spoken));
  },
});

export { expect };

export async function openTab(page: Page, name: "Practice" | "Sentences" | "Voices" | "History" | "Settings") {
  await page.getByRole("navigation").getByRole("button", { name, exact: true }).click();
}

/** The practice status line ("Ready", "Listen", "Recording 0:02 / 0:15", …). */
export function status(page: Page) {
  return page.locator("#practice .tabular-nums").first();
}

/** The revealed sentence on the practice card (absent while it's hidden). */
export function sentence(page: Page) {
  return page.locator("#practice p.leading-relaxed");
}

/** The visible tab panel (all panels stay mounted; inactive ones are hidden). */
export function panel(page: Page) {
  return page.locator("main > div:not(.hidden)");
}
