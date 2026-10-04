import { test, expect, openTab } from "./fixtures";

const SESSION_KEY = "repeat-sentence.diag.session.v1";

test("a normal reload is not reported as a crash", async ({ page }) => {
  await page.goto("/");
  await page.reload();
  await expect(page.getByText("ended unexpectedly")).toHaveCount(0);
});

test("a killed page is reported on the next load, with the event log", async ({ page }) => {
  // A process kill skips `pagehide`. Simulate it: right before the next load's
  // scripts run, mark the previous session as never having ended.
  await page.addInitScript((key) => {
    if (!sessionStorage.getItem("simulateCrash")) return;
    sessionStorage.removeItem("simulateCrash");
    const s = JSON.parse(localStorage.getItem(key) ?? "null");
    if (s) localStorage.setItem(key, JSON.stringify({ ...s, ended: false }));
  }, SESSION_KEY);

  await page.goto("/");
  await page.getByRole("button", { name: "Start" }).click(); // leave some events behind
  await expect(page.locator("#practice .tabular-nums").first()).toHaveText("Listen");
  await page.evaluate(() => sessionStorage.setItem("simulateCrash", "1"));
  await page.reload();

  await expect(page.getByText(/previous session .* ended unexpectedly/)).toBeVisible();
  // (A real kill has no unload events; this simulated one ends with them.)
  await expect(page.getByText(/Last event: \S/)).toBeVisible();

  await page.getByRole("button", { name: "See Settings → Diagnostics" }).click();
  await page.getByText("Show event log").click();
  const log = page.locator("pre");
  await expect(log).toContainText("ended unexpectedly");
  await expect(log).toContainText("next: Browser");
  await expect(log).toContainText("phase Speaking");

  await page.getByRole("button", { name: "Clear" }).click();
  await expect(log).toHaveText("");

  // Dismissable.
  await openTab(page, "Practice");
  await page.getByRole("button", { name: "✕" }).click();
  await expect(page.getByText("ended unexpectedly")).toHaveCount(0);
});
