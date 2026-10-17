import { expect, test } from "@playwright/test";

test("BSC page shows five read-only panels and no BUY control", async ({ page }) => {
  await page.goto("/bsc", { waitUntil: "networkidle" });

  const tabs = await page.getByRole("tablist", { name: "BSC areas" }).getByRole("tab").allTextContents();
  expect(tabs).toEqual(["Overview", "Launch / BUY", "Monitoring", "Analytics", "Wallets"]);

  for (const label of tabs) {
    await page.getByRole("tab", { name: label }).click();
    await expect(page.getByRole("tab", { name: label })).toHaveAttribute("aria-selected", "true");
    // Kein ausführbares BUY/Send-Element in irgendeinem Panel
    expect(await page.getByRole("button", { name: /^(start )?buy$|send|sweep|arm/i }).count()).toBe(0);
    // Unbekannt ist "—", nie 0
    for (const v of await page.locator(".g-line .g-right").allTextContents()) {
      expect(v.trim()).not.toMatch(/^0(\.0+)?( BNB| USD)?$/);
    }
  }

  await expect(page.getByTestId("bsc-guard")).toHaveText(/Guard · (UNKNOWN|UNARMED)/);
});
