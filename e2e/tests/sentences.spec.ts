import path from "node:path";
import { test, expect, openTab } from "./fixtures";

const EXAMPLE = path.join(__dirname, "../../examples/everyday.txt");

test("imports a text file as a set and keeps it after reload", async ({ page }) => {
  await page.goto("/");
  await openTab(page, "Sentences");
  await page.locator("#import-file").setInputFiles(EXAMPLE);
  await expect(page.getByText("Imported 28 sentences as “everyday”.")).toBeVisible();
  await expect(page.getByText("76 sentences are currently in the practice pool.")).toBeVisible();

  await page.reload();
  await openTab(page, "Sentences");
  await expect(page.getByText("76 sentences are currently in the practice pool.")).toBeVisible();

  // Turning the built-in set off leaves only the imported sentences.
  await page.getByRole("checkbox").first().uncheck();
  await expect(page.getByText("28 sentences are currently in the practice pool.")).toBeVisible();
});

test("adds pasted lines as a set and deletes it", async ({ page }) => {
  await page.goto("/");
  await openTab(page, "Sentences");
  await page.getByPlaceholder("Set name (optional)").fill("Mine");
  await page.locator("#paste-text").fill("First pasted sentence.\n\n# comment\nSecond pasted sentence.\n");
  await page.getByRole("button", { name: "Add set" }).click();
  await expect(page.getByText("Imported 2 sentences as “Mine”.")).toBeVisible();
  await expect(page.getByText("50 sentences are currently in the practice pool.")).toBeVisible();

  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("48 sentences are currently in the practice pool.")).toBeVisible();
});
