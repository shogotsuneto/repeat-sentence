import { test, expect, openTab, panel, sentence, status } from "./fixtures";

test("reads a sentence, records the repeat, and saves it to history", async ({ page, mic, spoken }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Start" }).click();

  // The prompt is read once, with the sentence hidden while listening.
  await expect(status(page)).toHaveText("Listen");
  await expect(sentence(page)).toHaveCount(0);
  expect(await mic.live()).toBe(0); // mic is not held during the prompt

  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  expect(await mic.live()).toBe(1);

  await page.keyboard.press("Space"); // stop recording
  await expect(status(page)).toHaveText("Review");
  await expect(page.getByText(/Your answer · 0:0\d · stopped manually/)).toBeVisible();
  expect(await mic.live()).toBe(0); // released as soon as recording ends

  // The revealed sentence is the one that was spoken.
  const [first] = await spoken();
  await expect(sentence(page)).toHaveText(first.text);
  await expect(page.getByText("This session (1)")).toBeVisible();

  await openTab(page, "History");
  await expect(panel(page).getByText(first.text, { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "▶ Play" })).toBeVisible();
});

test("stops recording after 3 s of silence", async ({ page, mic }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Start" }).click();
  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  await mic.setLevel(0);
  await expect(status(page)).toHaveText("Review", { timeout: 8_000 });
  await expect(page.getByText(/stopped after silence/).first()).toBeVisible();
  expect(await mic.live()).toBe(0);
});

test("Next during the prompt skips to a new sentence without recording", async ({ page, mic, spoken }) => {
  await page.goto("/");
  // A long pre-prompt pause keeps us in "Get ready…" long enough to skip.
  await openTab(page, "Settings");
  await page.getByLabel("Delay before the prompt (s)").fill("3");
  await page.getByLabel("Delay before the prompt (s)").blur();
  await openTab(page, "Practice");

  await page.getByRole("button", { name: "Start" }).click();
  await expect(status(page)).toHaveText("Get ready…");
  await page.getByRole("button", { name: "Skip" }).click();
  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  expect((await spoken()).length).toBe(1); // the skipped one was never read
  expect(await mic.opened()).toBe(1);
});

test("deleting audio keeps the attempt in history", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Start" }).click();
  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  await page.keyboard.press("Space");
  await expect(status(page)).toHaveText("Review");

  await openTab(page, "History");
  await page.getByRole("button", { name: "Delete audio" }).click();
  await expect(page.getByText("Audio deleted")).toBeVisible();

  await page.reload();
  await openTab(page, "History");
  await expect(page.getByText("Audio deleted")).toBeVisible();
  await expect(page.getByRole("button", { name: "Delete all audio" })).toHaveCount(0);
  await expect(page.locator("li", { hasText: "Audio deleted" })).toHaveCount(1);
});

test("pressing Next never reveals the next sentence before it is attempted", async ({ page, spoken }) => {
  await page.goto("/");
  // Record every text the sentence slot shows, as it happens.
  await page.evaluate(() => {
    const w = window as any;
    w.__shown = [];
    new MutationObserver(() => {
      const t = document.querySelector("#practice p.leading-relaxed")?.textContent;
      if (t && w.__shown.at(-1) !== t) w.__shown.push(t);
    }).observe(document.getElementById("practice")!, { childList: true, subtree: true, characterData: true });
  });

  await page.getByRole("button", { name: "Start" }).click();
  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  await page.keyboard.press("Space");
  await expect(status(page)).toHaveText("Review");
  const [first] = await spoken();

  await page.getByRole("button", { name: "Next" }).click();
  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  const shown: string[] = await page.evaluate(() => (window as any).__shown);
  const second = (await spoken())[1];
  expect(second.text).not.toBe(first.text);
  expect(shown).toEqual([first.text]); // the second sentence never appeared
});

test("doesn't treat a dead level meter as silence", async ({ page }) => {
  await page.goto("/");
  // Simulate the analyser receiving nothing (e.g. an AudioContext iOS has
  // interrupted while the recorder itself keeps capturing).
  await page.evaluate(() => {
    AnalyserNode.prototype.getFloatTimeDomainData = function (buf: Float32Array) {
      buf.fill(0);
    };
  });
  await page.getByRole("button", { name: "Start" }).click();
  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  await expect(page.getByText(/Input level unavailable/)).toBeVisible();
  await page.waitForTimeout(4_500); // past the 3 s silence limit
  await expect(status(page)).toContainText("Recording");

  await page.keyboard.press("Space");
  await expect(status(page)).toHaveText("Review");
  await expect(page.getByText(/stopped manually/).first()).toBeVisible();
});

test("keeps recording while the audio context is interrupted, and logs it", async ({ page }) => {
  await page.goto("/");
  // iOS reports "interrupted" when the audio session is taken over, e.g. by
  // Bluetooth earphones switching to call mode as the mic opens.
  await page.evaluate(() => {
    Object.defineProperty(BaseAudioContext.prototype, "state", { get: () => "interrupted" });
    BaseAudioContext.prototype.resume = () => Promise.resolve();
  });
  await page.getByRole("button", { name: "Start" }).click();
  await expect(status(page)).toContainText("Recording", { timeout: 10_000 });
  await expect(page.getByText(/Input level unavailable/)).toBeVisible();
  await page.waitForTimeout(4_500);
  await expect(status(page)).toContainText("Recording");
  await page.keyboard.press("Space");
  await expect(status(page)).toHaveText("Review");

  await openTab(page, "Settings");
  await page.getByText("Show event log").click();
  await expect(page.locator("pre")).toContainText(/recorded \d+ ms \(manual, .*level unknown \d+ ms; audio context interrupted\)/);
});
