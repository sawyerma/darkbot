import { defineConfig, devices } from "@playwright/test";

// Requires an existing production build; tests are isolated from the dev-server suite.
export default defineConfig({
  testDir: "./tests",
  testMatch: "code-split.spec.ts",
  timeout: 20_000,
  retries: 0,
  use: { baseURL: "http://127.0.0.1:4173", viewport: { width: 1280, height: 800 } },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "npm run preview -- --host 127.0.0.1 --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
