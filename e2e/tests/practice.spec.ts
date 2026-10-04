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
