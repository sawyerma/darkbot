import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright Configuration für Smoke Tests
 * https://playwright.dev/docs/test-configuration
 */
export default defineConfig({
  testDir: './tests',
  
  // Smoke Tests sollten schnell sein
  timeout: 10 * 1000, // 10s max per test
  
  // Parallel ausführen für Geschwindigkeit
  fullyParallel: true,
  
  // Keine Retries bei Smoke Tests (fail-fast!)
  retries: 0,
  
  // Reporter
  reporter: 'line',
  
  // Shared settings
  use: {
    // Base URL für Tests
    baseURL: 'http://localhost:8080',
    
    // Browser Context Options
    viewport: { width: 1280, height: 800 },
    
    // Collect trace nur bei Failure
    trace: 'retain-on-failure',
    
    // Screenshot nur bei Failure
    screenshot: 'only-on-failure',
  },
  
  // Configure projects für verschiedene Browser
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  
  // Dev Server starten (optional, wenn noch nicht läuft)
  // webServer: {
  //   command: 'npm run dev',
  //   url: 'http://localhost:8080',
  //   reuseExistingServer: true,
  //   timeout: 120 * 1000,
  // },
});
