import { expect, test, type Page } from "@playwright/test";

// Phasenplan §9.2: fünf P0-Reiter, Wallets Pixeldiff 0. Fotografiert wird das Element
// .p0-graphite, nicht die Seite, damit Host-Navigation und Theme die Baseline nicht berühren.
// Volatile Stellen (Uhrzeiten, Verbindungsstatus) bekommen data-volatile und werden maskiert.

const TABS = ["Overview", "Liquidator", "Capture", "Analytics", "Wallets"] as const;
const WALLET_SUBTABS = ["Balances", "Send", "Auto-Sweep"] as const;

async function openProject0(page: Page) {
  await page.goto("/project-0", { waitUntil: "networkidle" });
  const shell = page.locator(".p0-graphite");
  await expect(shell).toBeVisible();
  return shell;
}

async function selectTab(page: Page, label: string) {
  const tabs = page.getByRole("tablist", { name: "Project 0 areas" });
  await tabs.getByRole("tab", { name: label, exact: true }).click();
  await expect(tabs.getByRole("tab", { name: label, exact: true })).toHaveAttribute("aria-selected", "true");
}

for (const tab of TABS) {
  test(`P0 tab ${tab} matches baseline`, async ({ page }) => {
    const shell = await openProject0(page);
    await selectTab(page, tab);
    await expect(shell).toHaveScreenshot(`p0-${tab.toLowerCase()}.png`);
  });
}

for (const sub of WALLET_SUBTABS) {
  test(`P0 Wallets/${sub} matches baseline (pixeldiff 0)`, async ({ page }) => {
    const shell = await openProject0(page);
    await selectTab(page, "Wallets");
    const walletTabs = page.getByRole("tablist", { name: "Wallet views" });
    await walletTabs.getByRole("tab", { name: sub, exact: true }).click();
    await expect(walletTabs.getByRole("tab", { name: sub, exact: true })).toHaveAttribute("aria-selected", "true");
    await expect(shell).toHaveScreenshot(`p0-wallets-${sub.toLowerCase().replace(/[^a-z]/g, "")}.png`);
  });
}

test("Wallets balance rows keep the frozen order", async ({ page }) => {
  await openProject0(page);
  await selectTab(page, "Wallets");
  await page.getByRole("tablist", { name: "Wallet views" }).getByRole("tab", { name: "Balances" }).click();
  const labels = await page.locator(".p0-graphite .p0-line > span:first-child").allTextContents();
  const expected = [
    "Total capital",
    "Realized net PnL",
    "On-chain balance",
    "Available capital",
    "Reserved capital",
    "Withdrawable profit",
    "Transferred to Treasury",
  ];
  const found = labels.map((l) => l.trim()).filter((l) => expected.includes(l));
  expect(found).toEqual(expected);
});

test("Unknown values render as dash, never zero", async ({ page }) => {
  await openProject0(page);
  await selectTab(page, "Wallets");
  const values = await page.locator(".p0-line .p0-right").allTextContents();
  for (const v of values) expect(v.trim()).not.toMatch(/^0(\.0+)?\s*USD$/);
});
