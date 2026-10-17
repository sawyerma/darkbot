import { expect, test } from "@playwright/test";

test("overview renders registry cards, no trading controls, unknown as dash", async ({ page }) => {
  await page.goto("/", { waitUntil: "networkidle" });

  await expect(page.getByTestId("project-card-project0")).toBeVisible();
  await expect(page.getByTestId("project-card-bsc")).toBeVisible();
  await expect(page.getByTestId("project-card-solana")).toBeVisible();

  const order = await page.locator(".ov-card h3").allTextContents();
  expect(order).toEqual(["Project 0", "BSC", "Solana"]);

  expect(await page.getByRole("button", { name: /start|buy|send|sweep/i }).count()).toBe(0);

  await expect(page.getByText("Aggregate unavailable")).toBeVisible();
  await expect(page.getByText("Execution authorization").first()).toBeVisible();
  for (const v of await page.locator(".ov-facts dd").allTextContents()) {
    expect(v.trim()).not.toMatch(/^0(\.00)?( USD)?$/);
  }

  await page.getByTestId("project-card-project0").click();
  await expect(page).toHaveURL(/\/project-0$/);
  await expect(page.locator(".p0-graphite")).toBeVisible();
});
