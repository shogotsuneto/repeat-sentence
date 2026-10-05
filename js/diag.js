// Crash diagnostics, bound into Rust by `src/diag.rs`.
//
// When iOS runs low on memory it kills the tab's process and Safari reloads
// the page: the app just restarts, and the console (even a connected Web
// Inspector) is gone. To see what led up to it, events are appended to a
// small ring buffer in localStorage — synchronously, so each entry survives
// a sudden kill — and each page load records whether the previous one ended
// normally.

const LOG_KEY = "repeat-sentence.diag.log.v1";
const SESSION_KEY = "repeat-sentence.diag.session.v1";
const MAX_ENTRIES = 300;

function read(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(key)) ?? fallback;
  } catch {
    return fallback;
  }
}

function write(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage full or unavailable: diagnostics are best effort.
  }
}

let entries = read(LOG_KEY, []);
const sessionId = Date.now();

export function log(msg) {
  entries.push({ t: Date.now(), s: sessionId, m: String(msg).slice(0, 2000) });
  if (entries.length > MAX_ENTRIES) entries = entries.slice(-MAX_ENTRIES);
  write(LOG_KEY, entries);
}

// The previous page load's session, if it never recorded a normal end
// (`pagehide`). `kind` tells the two ways that happens apart:
// - "background": it was hidden at the time. iOS routinely discards
//   backgrounded pages / home-screen apps to save memory, without
//   `pagehide` — expected, not a crash.
// - "crash": it was on screen, i.e. the page was killed while in use
//   (e.g. memory pressure).
const previous = read(SESSION_KEY, null);
const ended = previous && !previous.ended ? previous : null;
const endKind = ended?.hidden ? "background" : "crash";

export function previousEnd() {
  if (!ended) return null;
  const last = entries.filter((e) => e.s === ended.id).at(-1);
  return {
    kind: endKind,
    startedAt: ended.id,
    lastEventAt: last?.t ?? ended.id,
    lastEvent: last?.m ?? "",
  };
}

export function entriesText() {
  return entries
    .map((e) => {
      const d = new Date(e.t);
      const time = `${d.toLocaleDateString()} ${d.toLocaleTimeString()}.${String(d.getMilliseconds()).padStart(3, "0")}`;
      return `${time}  ${e.m}`;
    })
    .join("\n");
}

export function clearLog() {
  entries = [];
  write(LOG_KEY, entries);
}

function memory() {
  // Chrome only; Safari exposes no memory figures to pages.
  const m = performance.memory;
  return m ? ` heap=${Math.round(m.usedJSHeapSize / 1e6)}MB` : "";
}

// --- session bookkeeping & global hooks ---
const session = { id: sessionId, ended: false, hidden: document.visibilityState === "hidden" };
const saveSession = () => write(SESSION_KEY, session);
saveSession();
if (ended) {
  const at = new Date(ended.id).toLocaleTimeString();
  log(
    endKind === "background"
      ? `previous session (${at}) was closed while in the background`
      : `previous session (${at}) ended unexpectedly`,
  );
}
log(`start ${navigator.userAgent}${memory()}`);

addEventListener("pagehide", (e) => {
  log(`pagehide persisted=${e.persisted}`);
  session.ended = true;
  saveSession();
});
// Coming back from the back/forward cache is still the same session.
addEventListener("pageshow", (e) => {
  if (e.persisted) {
    session.ended = false;
    saveSession();
  }
});
document.addEventListener("visibilitychange", () => {
  session.hidden = document.visibilityState === "hidden";
  saveSession();
  log(`visibility ${document.visibilityState}${memory()}`);
});
addEventListener("error", (e) => log(`error ${e.message} @ ${e.filename}:${e.lineno}`));
addEventListener("unhandledrejection", (e) => log(`unhandled rejection ${e.reason?.stack ?? e.reason}`));
