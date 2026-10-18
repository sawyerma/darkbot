import { expect, test, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../..", import.meta.url));
const row = (page: Page, title: string) => page.locator(".p0-health").getByText(title, { exact: true }).locator("..").locator(".p0-right");
const exit = (child: ChildProcess) => new Promise<void>((resolve) => {
  if (child.exitCode !== null || child.signalCode !== null) return resolve();
  child.once("exit", () => resolve());
});

test("real reporter process reaches browser Health through the real gateway", async ({ page }, info) => {
  const dir = mkdtempSync(path.join(tmpdir(), "p0-ui-"));
  const socket = `/tmp/p0-ui-${process.pid}.sock`;
  const port = 4187;
  const startReporter = () => spawn(path.join(root, "project_0/native/target/debug/p0-reporter-fixture"), [], {
    env: { ...process.env, P0_FIXTURE_SOCKET: socket, P0_FIXTURE_GENERATION_FILE: path.join(dir, "reporter.gen") },
    stdio: ["pipe", "ignore", "pipe"],
  });
  const gateway = spawn(path.join(root, "gateway/target/debug/darkbot-gateway"), [], {
    env: { ...process.env, DARKBOT_GATEWAY_BIND: `127.0.0.1:${port}`,
      DARKBOT_GATEWAY_ALLOWED_HOSTS: `127.0.0.1:${port}`,
      DARKBOT_GATEWAY_ALLOWED_ORIGINS: `http://127.0.0.1:${port}`,
      DARKBOT_GATEWAY_STATIC_DIR: path.join(root, "frontend/dist"),
      DARKBOT_GATEWAY_GENERATION_FILE: path.join(dir, "gateway.gen"),
      DARKBOT_GATEWAY_SOURCE_BACKOFF_MS: "50", DARKBOT_GATEWAY_FEEDS: `project0=${socket}` },
    stdio: ["ignore", "ignore", "pipe"],
  });
  let reporter = startReporter();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  let backendLog = "";
  gateway.stderr!.on("data", (data) => { backendLog += data.toString(); });
  reporter.stderr!.on("data", (data) => { backendLog += data.toString(); });
  try {
    await expect.poll(async () => {
      try { return (await page.request.get(`http://127.0.0.1:${port}/healthz`)).status(); } catch { return 0; }
    }).toBe(200);
    await page.goto(`http://127.0.0.1:${port}/project-0`);
    const health = page.getByRole("button", { name: /Health ·/ });
    await expect(health).toContainText("Health · Live");
    await health.click();
    await expect(row(page, "Run")).toContainText("run-TEST_ONLY");
    await expect(row(page, "Operational profile")).toContainText("71 parameters");
    await expect(row(page, "Gateway topics")).toContainText("project0.status LIVE · project0.capture.state LIVE");
    const before = await row(page, "Transactions · unique / success / failed / unknown").textContent();
    await expect.poll(() => row(page, "Transactions · unique / success / failed / unknown").textContent()).not.toBe(before);
    await expect(row(page, "Writer sample · engine clock / age at publication / frame")).toContainText("g1/");
    await page.screenshot({ path: info.outputPath("real-backend-health.png"), fullPage: true });
    reporter.stdin!.end();
    await exit(reporter);
    await expect(health).toContainText("Completed · Source detached");
    await expect(row(page, "Phase")).toContainText("CAPTURE_COMPLETE");
    reporter = startReporter();
    await expect(health).toContainText("Health · Live");
    await expect(row(page, "Writer sample · engine clock / age at publication / frame")).toContainText("g2/");
    expect(errors).toEqual([]);
  } finally {
    if (reporter.exitCode === null) reporter.kill("SIGTERM");
    gateway.kill("SIGTERM");
    await Promise.all([exit(reporter), exit(gateway)]);
    await info.attach("backend-log", { body: backendLog, contentType: "text/plain" });
    rmSync(dir, { recursive: true, force: true });
    rmSync(socket, { force: true });
  }
});
