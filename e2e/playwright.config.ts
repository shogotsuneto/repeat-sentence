import { defineConfig, devices } from "@playwright/test";

const PORT = 4173;

export default defineConfig({
  testDir: "./tests",
  // Each test gets a fresh browser context (empty localStorage / IndexedDB).
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  timeout: 60_000,
  use: {
    baseURL: `http://127.0.0.1:${PORT}/`,
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        launchOptions: {
          // Let the AudioContext / <audio> run without a real user gesture.
          args: ["--autoplay-policy=no-user-gesture-required"],
        },
      },
    },
  ],
  // Builds the app (dev profile is enough) and serves dist/ statically, as
  // GitHub Pages would. CI builds once in this same command.
  webServer: {
    command: `cd .. && trunk build && node e2e/serve.mjs dist ${PORT}`,
    url: `http://127.0.0.1:${PORT}/`,
    reuseExistingServer: !process.env.CI,
    timeout: 600_000,
    stdout: "ignore",
    stderr: "pipe",
  },
});
