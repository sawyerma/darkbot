import { defineConfig, devices } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  testMatch: "project0-backend.spec.ts",
  timeout: 30_000,
  workers: 1,
  retries: 0,
  use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 900 }, trace: "retain-on-failure" },
});
