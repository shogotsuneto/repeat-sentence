import { test, expect, openTab, status, panel } from "./fixtures";

test("loads with the built-in sentences and switches tabs", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle("Repeat Sentence");
  await expect(status(page)).toHaveText("Ready");
  await expect(page.getByText("48 sentences in pool")).toBeVisible();

  for (const tab of ["Sentences", "Voices", "History", "Settings", "Practice"] as const) {
    await openTab(page, tab);
    await expect(panel(page)).toHaveCount(1);
  }
  await expect(page.getByRole("button", { name: "Start" })).toBeVisible();
});

test("downloads nothing from the model CDN / hub until Kokoro is opted into", async ({ page }) => {
  const external: string[] = [];
  page.on("request", (r) => {
    const url = r.url();
    if (/huggingface\.co|cdn\.jsdelivr\.net/.test(url)) external.push(url);
  });
  await page.goto("/");
  await openTab(page, "Voices");
  await expect(page.getByRole("button", { name: "Download model" })).toBeVisible();
  await page.waitForTimeout(500);
  expect(external).toEqual([]);
});
