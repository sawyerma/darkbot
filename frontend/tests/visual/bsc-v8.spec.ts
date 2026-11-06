import { expect, test, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// FE-V8-01/18: the BSC page against the frozen V8 reference (frontend/design/reference/
// bsc_graphite_v8_approved.html). The reference shows demo values and the page native ones, so the
// comparison is the design itself: the computed style of every V8 building block, the four
// areas, the Health pill top right, at 1440 px and 390 px, and no horizontal page scroll on the
// phone. The page runs without a gateway here (fail-closed states).

const REFERENCE = new URL("../../design/reference/bsc_graphite_v8_approved.html", import.meta.url);
const REFERENCE_SHA256 = "b59f99ab20b985864ef70595b40ad7c74095c65856311804add149131d6df490";
const AREAS = ["Overview", "Training", "Live", "Wallets"] as const;
const PROPS = ["background-color", "color", "border-top-color", "border-top-width", "border-radius", "font-size", "font-weight", "line-height", "padding-top", "padding-left", "letter-spacing"] as const;
/** The V8 building blocks present in both on the Overview (selector, what it is). */
const BLOCKS: ReadonlyArray<[string, string]> = [
  [".sidebar", "sidebar"], [".brand strong", "brand name"], [".brandmark", "brand mark"], [".nav button.selected", "selected area"], [".nav button:not(.selected)", "area"],
  [".top", "top bar"], [".path strong", "breadcrumb"], [".health-pill", "Health pill"], [".top-right .round", "settings button"], [".wrap", "content"],
  [".heading h1", "page title"], [".heading p", "page subtitle"], [".card", "card"], [".topline h2", "card title"], [".system-metric .name", "metric label"],
  [".system-metric strong", "metric value"], [".dashboard-jump h2", "jump title"], [".btn.quiet", "quiet button"], [".badge", "badge"],
];

async function styles(page: Page): Promise<Record<string, Record<string, string>>> {
  return page.evaluate(([blocks, props]) => {
    const out: Record<string, Record<string, string>> = {};
    for (const [sel] of blocks as [string, string][]) {
      const el = document.querySelector(sel);
      if (!el) continue;
      const cs = getComputedStyle(el);
      // A border colour without a border is invisible (the app's base sets one on every element).
      out[sel] = Object.fromEntries((props as string[]).filter(p => p !== "border-top-color" || cs.borderTopWidth !== "0px").map(p => [p, cs.getPropertyValue(p)]));
    }
    return out;
  }, [BLOCKS, PROPS] as const);
}

test("the reference is the approved, unchanged file", () => {
  expect(createHash("sha256").update(readFileSync(REFERENCE)).digest("hex")).toBe(REFERENCE_SHA256);
});

for (const width of [1440, 390] as const) {
  test(`V8 design at ${width} px: the page's building blocks are the reference's`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 390 ? 844 : 1000 });
    await page.goto(`file://${fileURLToPath(REFERENCE)}`);
    const want = await styles(page);
    await page.route("**/ws/v1/gateway", r => r.abort());
    await page.goto("/bsc");
    await expect(page.getByTestId("bsc-v8")).toBeVisible();
    const got = await styles(page);
    for (const [sel, name] of BLOCKS) {
      expect(got[sel], `${name} (${sel}) missing in the page`).toBeDefined();
      if (!want[sel]) continue;
      // A badge's colours are its native state's (here UNKNOWN: amber), its shape the reference's.
      const strip = (o: Record<string, string> | undefined) => sel === ".badge" && o ? Object.fromEntries(Object.entries(o).filter(([k]) => k !== "color" && k !== "background-color")) : o;
      expect(strip(got[sel]), `${name} (${sel})`).toEqual(strip(want[sel]));
    }
    // The amber of a not-known state is the reference's .badge.warn (its tokens --v8-amber/--v8-amberbg).
    await expect(page.getByTestId("bsc-trading-state")).toHaveClass(/\bwarn\b/);
    await expect(page.getByTestId("bsc-trading-state")).toHaveCSS("color", "rgb(233, 195, 145)");
    await expect(page.getByTestId("bsc-trading-state")).toHaveCSS("background-color", "rgb(65, 53, 41)");
  });

  test(`V8 areas and Health at ${width} px, no horizontal scroll`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 390 ? 844 : 1000 });
    await page.route("**/ws/v1/gateway", r => r.abort());
    await page.goto("/bsc");
    const nav = page.getByRole("navigation", { name: "BSC areas" });
    await expect(nav.getByRole("button")).toHaveText([...AREAS]);
    for (const area of AREAS) {
      await nav.getByRole("button", { name: area, exact: true }).click();
      await expect(nav.getByRole("button", { name: area, exact: true })).toHaveAttribute("aria-current", "page");
      await expect(page.locator(".path strong")).toHaveText(area);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
      expect(overflow, `${area}: horizontal scroll`).toBeLessThanOrEqual(0);
    }
    // The Health pill sits top right and opens its own window; without a gateway nothing is green.
    const pill = page.getByTestId("bsc-health-pill");
    const box = await pill.boundingBox();
    expect(box && box.x + box.width).toBeGreaterThan(width * (width === 390 ? 0.55 : 0.75));
    await pill.click();
    await expect(page.getByTestId("bsc-health-window")).toBeVisible();
    await expect(page.locator("[data-testid^=bsc-health-][data-state=ok]")).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("bsc-health-window")).toHaveCount(0);
    if (width === 390) {
      // Phone: the four areas are the fixed bottom bar of the reference.
      const bar = await page.locator(".sidebar").evaluate(el => ({ position: getComputedStyle(el).position, bottom: el.getBoundingClientRect().bottom }));
      expect(bar.position).toBe("fixed");
      expect(Math.round(bar.bottom)).toBe(844);
    }
  });
}
