import { defineConfig, devices } from "@playwright/test";

// The BSC page on a production build (`npm run build` first), with a gateway stand-in per test.
export default defineConfig({
  testDir: "./tests",
  testMatch: "bsc.spec.ts",
  timeout: 20_000,
  retries: 0,
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://127.0.0.1:4187",
    viewport: { width: 1280, height: 900 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npm run preview -- --host 127.0.0.1 --port 4187 --strictPort",
    url: "http://127.0.0.1:4187",
    reuseExistingServer: false,
  },
});
