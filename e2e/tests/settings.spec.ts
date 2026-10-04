import { test, expect, openTab, sentence } from "./fixtures";

test("settings persist across reloads", async ({ page }) => {
  await page.goto("/");
  await openTab(page, "Settings");
  const delay = page.getByLabel("Delay before recording (s)");
  await expect(delay).toHaveValue("1.5");
  await delay.fill("2.5");
  await delay.blur();
  await page.getByLabel("Show the sentence while listening").check();

  await page.reload();
  await openTab(page, "Settings");
  await expect(page.getByLabel("Delay before recording (s)")).toHaveValue("2.5");
  await expect(page.getByLabel("Show the sentence while listening")).toBeChecked();
});

test("showing the sentence while listening reveals it during the prompt", async ({ page, spoken }) => {
  await page.goto("/");
  await openTab(page, "Settings");
  await page.getByLabel("Show the sentence while listening").check();
  await openTab(page, "Practice");
  await page.getByRole("button", { name: "Start" }).click();
  await expect.poll(async () => (await spoken()).length).toBe(1);
  const [u] = await spoken();
  await expect(sentence(page)).toHaveText(u.text);
});
