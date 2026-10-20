import { expect, test } from "@playwright/test";

test("overview retains project navigation and renders two honest, responsive tiles", async ({ page }) => {
  await page.goto("/", { waitUntil: "networkidle" });

  await expect(page.getByTestId("project-card-project0")).toBeVisible();
  await expect(page.getByTestId("project-card-bsc")).toBeVisible();
  await expect(page.getByTestId("project-card-solana")).toBeVisible();

  const order = await page.locator(".ov-card h3").allTextContents();
  expect(order).toEqual(["Project 0", "BSC", "Solana"]);

  const system = page.getByTestId("overview-system");
  const finance = page.getByTestId("overview-finance");
  await expect(system).toBeVisible();
  await expect(finance).toBeVisible();
  await expect(system.getByRole("heading", { name: "System & Control Plane" })).toBeVisible();
  await expect(finance.getByRole("heading", { name: "Global Finance Aggregate" })).toBeVisible();
  await expect(system.locator("dt")).toHaveText([
    "Gateway", "Overview Feed", "Connected Projects", "Gateway RTT", "RPC Latency P95", "Last Overview Update",
  ]);
  await expect(finance.locator("dt")).toHaveText([
    "Realized Net PnL", "Unrealized PnL", "Total Equity", "Daily Net PnL", "PnL Coverage", "Reporting Currency",
  ]);
  await expect(finance.locator("dd").first()).toHaveText("— USD");
  await expect(finance.locator("dd").nth(2)).toHaveText("— USD");
  for (const key of ["project0", "bsc", "solana"]) {
    await expect(page.getByTestId(`project-startstop-${key}`)).toBeDisabled();
    if (key === "solana") await expect(page.getByTestId(`project-logs-${key}`)).toBeDisabled();
    else await expect(page.getByTestId(`project-logs-${key}`)).toBeEnabled();
  }
  await expect(page.getByTestId("overview-stop-all")).toBeDisabled();
  await expect(page.getByTestId("overview-show-all-logs")).toBeEnabled();
  await expect(page.getByText("Execution authorization").first()).toBeVisible();

  await page.setViewportSize({ width: 580, height: 800 });
  await expect.poll(async () => system.boundingBox()).not.toBeNull();
  const a = await system.boundingBox();
  const b = await finance.boundingBox();
  expect(a && b && b.y >= a.y + a.height).toBeTruthy();

  await page.getByTestId("project-link-project0").click();
  await expect(page).toHaveURL(/\/project-0$/);
  await expect(page.locator(".p0-graphite")).toBeVisible();
});
