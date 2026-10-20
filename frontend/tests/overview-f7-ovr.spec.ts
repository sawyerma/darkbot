import { expect, test } from "@playwright/test";

const METRICS = ["Readiness", "Execution authorization", "Bots / jobs / captures", "RPC p95", "Realized net PnL"];

test.describe("F7-OVR overview", () => {
  test("three metric-rich cards with sibling controls, no nested buttons", async ({ page }) => {
    await page.setViewportSize({ width: 1152, height: 900 });
    await page.goto("/");
    for (const key of ["project0", "bsc", "solana"]) {
      const card = page.getByTestId(`project-card-${key}`);
      await expect(card.locator("dt")).toHaveText(METRICS);
      await expect(page.getByTestId(`project-process-${key}`)).toHaveText("Process · UNKNOWN");
      await expect(page.getByTestId(`project-startstop-${key}`)).toBeDisabled();
      if (key === "solana") await expect(page.getByTestId(`project-logs-${key}`)).toBeDisabled();
    else await expect(page.getByTestId(`project-logs-${key}`)).toBeEnabled();
      await expect(card.locator("a button, button button, button a")).toHaveCount(0);
    }
    await page.getByTestId("project-link-project0").click();
    await expect(page).toHaveURL(/\/project-0$/);
  });

  test("two equal tiles with exact names; stacked on narrow screens", async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1152, height: 900 });
    await page.goto("/");
    const sys = page.getByTestId("overview-system");
    const fin = page.getByTestId("overview-finance");
    await expect(sys.locator("h3")).toHaveText("System & Control Plane");
    await expect(fin.locator("h3")).toHaveText("Global Finance Aggregate");
    await expect(page.getByTestId("overview-stop-all")).toBeDisabled();
    await expect(page.getByTestId("overview-show-all-logs")).toBeEnabled();
    await expect(page.getByText(/Start Selected|Data Age|Valuation/)).toHaveCount(0);
    const a = await sys.boundingBox();
    const b = await fin.boundingBox();
    expect(Math.abs(a!.width - b!.width)).toBeLessThanOrEqual(1);
    expect(Math.abs(a!.height - b!.height)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: testInfo.outputPath("overview-1152.png"), fullPage: true });

    await page.setViewportSize({ width: 600, height: 1200 });
    const c = await sys.boundingBox();
    const d = await fin.boundingBox();
    expect(Math.abs(c!.x - d!.x)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: testInfo.outputPath("overview-600.png"), fullPage: true });
  });
});
