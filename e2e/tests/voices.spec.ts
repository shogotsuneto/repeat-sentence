import { test, expect, openTab, DEFAULT_VOICES } from "./fixtures";

test("first visit seeds one voice per English accent", async ({ page }) => {
  await page.goto("/");
  await openTab(page, "Voices");
  await expect(page.getByText("Voice pool (3)")).toBeVisible();
  for (const label of ["Samantha (en-US)", "Daniel (en-GB)", "Karen (en-AU)"]) {
    await expect(page.locator("li", { hasText: label })).toHaveCount(1);
  }
});

test.describe("with duplicate voice names", () => {
  test.use({
    fakeVoices: {
      voices: [
        ...DEFAULT_VOICES,
        // Same URI twice: the same voice to the Speech API → listed once.
        { voiceURI: "test.en-GB.Daniel", name: "Daniel", lang: "en-GB" },
        // Same name, different URI → told apart by the differing segment.
        { voiceURI: "test.en-GB.Daniel2", name: "Daniel", lang: "en-GB" },
      ],
    },
  });

  test("labels stay unique and the raw list shows everything", async ({ page }) => {
    await page.goto("/");
    await openTab(page, "Voices");
    const options = await page.locator("#voice-select option").allTextContents();
    expect(new Set(options).size).toBe(options.length);
    expect(options.filter((o) => o.startsWith("Daniel"))).toEqual([
      "Daniel (en-GB) · Daniel",
      "Daniel (en-GB) · Daniel2",
    ]);
    await expect(page.getByText("What this browser reports (6 voices)")).toBeVisible();
  });
});

test("a preset reads questions with its voice and rate", async ({ page, spoken }) => {
  await page.goto("/");
  await openTab(page, "Voices");
  await page.getByRole("button", { name: "Remove all" }).click();
  await page.locator("#voice-select").selectOption({ label: "Karen (en-AU)" });
  await page.locator("#rate").fill("1.2");
  await page.getByRole("button", { name: "Add to pool" }).click();
  await expect(page.locator("li", { hasText: "Karen (en-AU)" })).toContainText("1.20×");

  await openTab(page, "Practice");
  await page.getByRole("button", { name: "Start" }).click();
  await expect.poll(async () => (await spoken()).length).toBe(1);
  const [u] = await spoken();
  expect(u.voice).toBe("test.en-AU.Karen");
  expect(u.rate).toBeCloseTo(1.2);
});
