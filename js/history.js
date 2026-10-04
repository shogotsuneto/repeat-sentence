// Practice history in IndexedDB, bound into Rust by `src/history.rs`.
// Attempt metadata and the recorded audio live in separate stores so listing
// the history never loads audio; a recording is turned into a blob URL only
// when it is played.

const DB_NAME = "repeat-sentence";
// Bump to run `onupgradeneeded`; keep the upgrade idempotent.
const DB_VERSION = 1;
const ATTEMPTS = "attempts"; // { id (auto), atMs, text, source, voiceLabel, rate, durationMs, reason, mime }
const AUDIO = "audio"; // Blob, keyed by the attempt's id

let dbPromise = null;

function openDb() {
  dbPromise ??= new Promise((resolve, reject) => {
    if (!("indexedDB" in window)) return reject(new Error("IndexedDB is not available"));
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(ATTEMPTS)) {
        db.createObjectStore(ATTEMPTS, { keyPath: "id", autoIncrement: true });
      }
      if (!db.objectStoreNames.contains(AUDIO)) db.createObjectStore(AUDIO);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  }).catch((e) => {
    dbPromise = null;
    throw e;
  });
  return dbPromise;
}

function complete(tx) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error("Transaction aborted"));
  });
}

// Stores the attempt and the recording behind `audioUrl` (a live blob URL).
// Resolves the new attempt id.
export async function saveAttempt(meta, audioUrl) {
  // Read the blob before opening the transaction: a transaction auto-commits
  // if it is left idle across an unrelated await.
  const blob = await (await fetch(audioUrl)).blob();
  const db = await openDb();
  const tx = db.transaction([ATTEMPTS, AUDIO], "readwrite");
  const req = tx.objectStore(ATTEMPTS).add(meta);
  req.onsuccess = () => tx.objectStore(AUDIO).put(blob, req.result);
  await complete(tx);
  // Ask the browser not to evict our data under storage pressure.
  navigator.storage?.persist?.().catch(() => {});
  return req.result;
}

export async function listAttempts() {
  const db = await openDb();
  const tx = db.transaction(ATTEMPTS, "readonly");
  const req = tx.objectStore(ATTEMPTS).getAll();
  await complete(tx);
  return req.result;
}

// A fresh blob URL for the attempt's recording (revoke it when done), or
// null if the audio is missing.
export async function audioUrl(id) {
  const db = await openDb();
  const tx = db.transaction(AUDIO, "readonly");
  const req = tx.objectStore(AUDIO).get(id);
  await complete(tx);
  return req.result ? URL.createObjectURL(req.result) : null;
}

export async function deleteAttempt(id) {
  const db = await openDb();
  const tx = db.transaction([ATTEMPTS, AUDIO], "readwrite");
  tx.objectStore(ATTEMPTS).delete(id);
  tx.objectStore(AUDIO).delete(id);
  await complete(tx);
}

export async function clearAttempts() {
  const db = await openDb();
  const tx = db.transaction([ATTEMPTS, AUDIO], "readwrite");
  tx.objectStore(ATTEMPTS).clear();
  tx.objectStore(AUDIO).clear();
  await complete(tx);
}

// Bytes this origin is using, when the browser reports it.
export async function storageUsage() {
  try {
    return (await navigator.storage?.estimate?.())?.usage ?? null;
  } catch {
    return null;
  }
}
