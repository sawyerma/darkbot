import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  testMatch: ["overview.spec.ts", "overview-f7-ovr.spec.ts"],
  timeout: 15000,
  retries: 0,
  use: { baseURL: "http://127.0.0.1:4188", browserName: "chromium" },
  webServer: {
    command: "node scripts/overview-static-server.mjs",
    url: "http://127.0.0.1:4188",
    reuseExistingServer: false,
    timeout: 15000,
  },
});
