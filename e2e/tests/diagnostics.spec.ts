import { test, expect, openTab } from "./fixtures";

const SESSION_KEY = "repeat-sentence.diag.session.v1";

test("a normal reload is not reported as a crash", async ({ page }) => {
  await page.goto("/");
  await page.reload();
  await expect(page.getByText("ended unexpectedly")).toHaveCount(0);
});

// A process kill skips `pagehide`. Simulate one: right before the next
// load's scripts run, mark the previous session as never having ended, and
// as on screen (`hidden: false`) or in the background (`hidden: true`).
async function simulateKillOnReload(page: import("@playwright/test").Page, hidden: boolean) {
  await page.addInitScript(
    ({ key, hidden }) => {
      if (!sessionStorage.getItem("simulateKill")) return;
      sessionStorage.removeItem("simulateKill");
      const s = JSON.parse(localStorage.getItem(key) ?? "null");
      if (s) localStorage.setItem(key, JSON.stringify({ ...s, ended: false, hidden }));
    },
    { key: SESSION_KEY, hidden },
  );
}

test("a page discarded in the background gets a calm notice, not a crash warning", async ({ page }) => {
  await simulateKillOnReload(page, true);
  await page.goto("/");
  await page.evaluate(() => sessionStorage.setItem("simulateKill", "1"));
  await page.reload();

  await expect(page.getByText(/closed while it was in the background/)).toBeVisible();
  await expect(page.getByText("ended unexpectedly")).toHaveCount(0);
  await page.getByRole("button", { name: "History", exact: true }).last().click();
  await expect(page.getByText("No attempts yet.", { exact: false })).toBeVisible();

  await openTab(page, "Settings");
  await page.getByText("Show event log").click();
  await expect(page.locator("pre")).toContainText("was closed while in the background");
});

test("a page killed while on screen is reported, with the event log", async ({ page }) => {
  await simulateKillOnReload(page, false);

  await page.goto("/");
  await page.getByRole("button", { name: "Start" }).click(); // leave some events behind
  await expect(page.locator("#practice .tabular-nums").first()).toHaveText("Listen");
  await page.evaluate(() => sessionStorage.setItem("simulateKill", "1"));
  await page.reload();

  await expect(page.getByText(/previous session .* ended unexpectedly .* on screen/)).toBeVisible();
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
