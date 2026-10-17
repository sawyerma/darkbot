import { defineConfig, devices } from "@playwright/test";

// Pixeldiff 0 ist nur mit fixierter Browser-/Font-/Viewport-Basis sinnvoll. Baselines werden
// in derselben Umgebung erzeugt, in der CI läuft (gleiches Playwright-Chromium, gleiche Fonts).
// Lokale Baselines von macOS (SF Pro) sind NICHT mit Linux-CI vergleichbar.
export default defineConfig({
  testDir: "./tests/visual",
  fullyParallel: false,
  retries: 0,
  reporter: [["list"], ["html", { open: "never", outputFolder: "playwright-report-visual" }]],
  snapshotPathTemplate: "{testDir}/__baseline__/{testFilePath}/{arg}{ext}",
  expect: {
    toHaveScreenshot: { maxDiffPixels: 0, threshold: 0, animations: "disabled", caret: "hide", scale: "css" },
  },
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1280, height: 900 },
    deviceScaleFactor: 1,
    colorScheme: "dark",
    locale: "en-US",
    timezoneId: "UTC",
    reducedMotion: "reduce",
  },
  webServer: {
    command: "npm run preview -- --port 4173 --host 127.0.0.1 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
    timeout: 180_000,
  },
});
