import { expect, test } from "@playwright/test";

// Phasenplan §9.7: /project-0 lädt keinen BSC-Chunk und umgekehrt.
// Vite benennt Lazy-Chunks nach der Moduldatei (BscPage-<hash>.js, Project0Page-<hash>.js).

async function loadedScripts(page: import("@playwright/test").Page, path: string): Promise<string[]> {
  const urls: string[] = [];
  page.on("request", (req) => {
    if (req.resourceType() === "script") urls.push(req.url());
  });
  await page.goto(path, { waitUntil: "networkidle" });
  return urls;
}

test("project-0 route does not load the BSC chunk", async ({ page }) => {
  const scripts = await loadedScripts(page, "/project-0");
  expect(scripts.some((u) => /\/assets\/Project0Page-[^/]+\.js/.test(u))).toBe(true);
  expect(scripts.some((u) => /\/assets\/BscPage-[^/]+\.js/.test(u))).toBe(false);
});

test("bsc route does not load the Project 0 chunk", async ({ page }) => {
  const scripts = await loadedScripts(page, "/bsc");
  expect(scripts.some((u) => /\/assets\/BscPage-[^/]+\.js/.test(u))).toBe(true);
  expect(scripts.some((u) => /\/assets\/Project0Page-[^/]+\.js/.test(u))).toBe(false);
});

test("navigation order follows the registry", async ({ page }) => {
  await page.goto("/");
  const labels = await page.locator('nav[aria-label="Projects"] a').allTextContents();
  expect(labels).toEqual(["Overview", "Project 0", "BSC", "Solana"]);
});
