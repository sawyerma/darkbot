import { defineConfig, devices } from "@playwright/test";

// The BSC page on a production build (`npm run build` first), served by the static test fixture
// (no proxy: no request can reach a real gateway; /api and /ws answer 503). Each test brings its
// own gateway stand-in through page routes.
export default defineConfig({
  testDir: "./tests",
  testMatch: "bsc.spec.ts",
  timeout: 20_000,
  retries: 0,
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://127.0.0.1:4188",
    viewport: { width: 1280, height: 900 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "node scripts/overview-static-server.mjs",
    url: "http://127.0.0.1:4188",
    reuseExistingServer: false,
    timeout: 15_000,
  },
});
