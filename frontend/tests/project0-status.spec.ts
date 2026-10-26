import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";
import { readFileSync } from "node:fs";
import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import { CapturePhase, CaptureStateSchema } from "../src/core/generated/darkbot/project0/v1/capture_pb";
import { EffectiveRecordSchema, SystemStatusSchema } from "../src/core/generated/darkbot/project0/v1/system_pb";
import { AnalyticsJobsSchema, BacktestJobCommandSchema } from "../src/core/generated/darkbot/project0/v1/analytics_pb";
import { SourceControlCommandSchema } from "../src/core/generated/darkbot/project0/v1/owner_pb";
import { AckStatus, ClientMessageSchema, ServerMessageSchema, StateQuality, WelcomeStatus } from "../src/core/generated/darkbot/control/v1/stream_pb";
import { ProjectId, TopicClass, TopicId } from "../src/core/generated/darkbot/control/v1/topics_pb";
import { CommandLookupResponseSchema, CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus, type CommandRequest } from "../src/core/generated/darkbot/control/v1/commands_pb";

const golden = (name: string) => Buffer.from(readFileSync(new URL(`../../protocol/golden/project0/${name}.hex`, import.meta.url), "utf8").trim(), "hex");
const schemaId = readFileSync(new URL("../../protocol/schema_id.txt", import.meta.url), "utf8").trim();

/** The owner and analytics topics with their golden frames (the job runner idle, so a job may start). */
function ownerTopics(): Map<TopicId, { wireName: string; payloadType: string; payload: Uint8Array }> {
  const analytics = fromBinary(AnalyticsJobsSchema, golden("analytics_jobs"));
  analytics.runnerBusy = false;
  return new Map([
    [TopicId.PROJECT0_LIQUIDATOR_STATE, { wireName: "project0.liquidator.state", payloadType: "darkbot.project0.v1.LiquidatorState", payload: golden("liquidator_state") }],
    [TopicId.PROJECT0_FINANCE_SUMMARY, { wireName: "project0.finance.summary", payloadType: "darkbot.project0.v1.FinanceSummary", payload: golden("finance_summary") }],
    [TopicId.PROJECT0_WALLETS_STATE, { wireName: "project0.wallets.state", payloadType: "darkbot.project0.v1.WalletsState", payload: golden("wallets_state") }],
    [TopicId.PROJECT0_ANALYTICS_JOBS, { wireName: "project0.analytics.jobs", payloadType: "darkbot.project0.v1.AnalyticsJobs", payload: toBinary(AnalyticsJobsSchema, analytics) }],
  ]);
}

async function feed(page: Page, initiallyReported = true, owner = false, system: Uint8Array = golden("system_status")) {
  const extra = owner ? ownerTopics() : new Map();
  let socket: WebSocketRoute;
  let seq = 1n;
  const subscriptions = new Set<TopicId>();
  const c = fromBinary(CaptureStateSchema, golden("capture_state"));
  c.phase!.phase = CapturePhase.WAITING_FOR_STREAM;
  const send = (msg: MessageInitShape<typeof ServerMessageSchema>["msg"]) =>
    socket.send(Buffer.from(toBinary(ServerMessageSchema, create(ServerMessageSchema, { msg }))));
  const capture = (quality = StateQuality.OK, payload = toBinary(CaptureStateSchema, c), generation = 1n, nextSeq = ++seq) => {
    send({ case: "stateFull", value: { topic: TopicId.PROJECT0_CAPTURE_STATE, generation, seq: nextSeq, quality,
      sourceId: "TEST_ONLY", sourceTimestampUnixNs: BigInt(Date.now()) * 1_000_000n, payload } });
  };
  await page.route("**/api/v1/session", (route) => route.fulfill({ status: 204 }));
  await page.routeWebSocket("**/ws/v1/gateway", (ws) => {
    socket = ws;
    ws.onMessage((data) => {
      const msg = fromBinary(ClientMessageSchema, typeof data === "string" ? Buffer.from(data) : data).msg;
      if (msg.case === "hello") {
        expect(msg.value.schemaId).toBe(schemaId);
        send({ case: "welcome", value: { status: WelcomeStatus.ACCEPTED, protocolMajor: 1, schemaId,
          topics: [
            { topic: TopicId.PROJECT0_STATUS, wireName: "project0.status", class: TopicClass.STATE, project: ProjectId.PROJECT_0, payloadType: "darkbot.project0.v1.SystemStatus", available: true },
            { topic: TopicId.PROJECT0_CAPTURE_STATE, wireName: "project0.capture.state", class: TopicClass.STATE, project: ProjectId.PROJECT_0, payloadType: "darkbot.project0.v1.CaptureState", available: true },
            ...[...extra].map(([topic, x]) => ({ topic, wireName: x.wireName, class: TopicClass.STATE, project: ProjectId.PROJECT_0, payloadType: x.payloadType, available: true })),
          ] } });
      } else if (msg.case === "subscribe") {
        subscriptions.add(msg.value.topic);
        send({ case: "controlAck", value: { requestId: msg.value.requestId, topic: msg.value.topic, status: AckStatus.ACCEPTED } });
        if (!initiallyReported) return;
        const own = extra.get(msg.value.topic);
        if (msg.value.topic === TopicId.PROJECT0_STATUS) {
          send({ case: "stateFull", value: { topic: msg.value.topic, generation: 1n, seq: 1n, quality: StateQuality.OK, sourceId: "TEST_ONLY", payload: system } });
        } else if (own) {
          send({ case: "stateFull", value: { topic: msg.value.topic, generation: 1n, seq: 1n, quality: StateQuality.OK, sourceId: "TEST_ONLY",
            sourceTimestampUnixNs: BigInt(Date.now()) * 1_000_000n, payload: own.payload } });
        } else capture();
      }
    });
  });
  await page.goto("/project-0");
  // Base topics, plus the Overview's owner and analytics topics when they are announced.
  await expect.poll(() => subscriptions.size).toBe(owner ? 5 : 2);
  return { c, capture, subscriptions };
}

const badge = (page: Page) => page.getByRole("button", { name: /Health ·/ });
const tab = (page: Page, name: string) => page.getByRole("tablist", { name: "Project 0 areas" }).getByRole("tab", { name, exact: true }).click();

test("real binary contracts drive all panels and lifecycle states", async ({ page }, info) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const { c, capture } = await feed(page);
  await expect(badge(page)).toContainText("Provider rate limit · HTTP 429 ×7");
  c.phase!.phase = CapturePhase.CAPTURE;
  c.transport!.applicationFrames += 1n;
  c.transport!.lastState = "CAPTURING";
  capture();
  await expect(badge(page)).toContainText("Degraded");
  await expect(page.getByText("12 / 3", { exact: true })).toBeVisible();
  await badge(page).click();
  const parameterCount = fromBinary(SystemStatusSchema, golden("system_status")).operational!.parameters.length;
  await expect(page.getByText(`built-in defaults · ${parameterCount} parameters`, { exact: true })).toBeVisible();
  await page.getByText("All values reported by Project 0", { exact: true }).click();
  await expect(page.getByText("transport_http_429", { exact: true })).toBeVisible();
  await expect(page.getByText("rpc_rps", { exact: true })).toBeVisible();
  await page.getByText("All values reported by Project 0", { exact: true }).click();
  await page.screenshot({ path: info.outputPath("health-desktop.png"), fullPage: true });
  c.assessment!.coverageComplete = true;
  c.assessment!.decodeComplete = true;
  c.assessment!.incompleteReasons = [];
  capture();
  await expect(badge(page)).toContainText("Health · Live");
  capture(StateQuality.STALE);
  await expect(badge(page)).toContainText("Stale");
  capture(StateQuality.SOURCE_DETACHED);
  await expect(badge(page)).toContainText("Capture process gone");
  c.phase!.phase = CapturePhase.DRAIN;
  capture();
  await expect(badge(page)).toContainText("Finishing");
  c.phase!.phase = CapturePhase.ENDED;
  c.phase!.endResult = "CAPTURE_COMPLETE";
  capture(StateQuality.SOURCE_DETACHED);
  await expect(badge(page)).toContainText("Completed · no capture running");
  await tab(page, "Capture");
  await expect(page.getByText("CAPTURE_COMPLETE", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Until stopped · desktop control" })).toBeDisabled();
  for (const name of ["Liquidator", "Analytics", "Wallets", "Overview"]) await tab(page, name);
  expect(errors).toEqual([]);
});

test("missing and malformed data never become healthy or fabricated values", async ({ page }) => {
  const { capture } = await feed(page, false);
  await expect(badge(page)).toContainText("No capture data");
  await tab(page, "Capture");
  await expect(page.getByText("No capture reported by Project 0.")).toBeVisible();
  capture(StateQuality.OK, new Uint8Array([0xff]));
  await expect(badge(page)).toContainText("Unavailable");
});

test("mobile health and all-values view fit the viewport", async ({ page }, info) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await feed(page);
  await badge(page).click();
  await page.getByText("All values reported by Project 0", { exact: true }).click();
  await expect(page.getByText("rpc_rps", { exact: true })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: info.outputPath("health-mobile.png") });
  await page.getByText("rpc_rps", { exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: info.outputPath("values-mobile.png") });
  expect(overflow).toBe(false);
});


test("stream ages without new frames and resets on run and generation changes", async ({ page }) => {
  await page.clock.install();
  const { c, capture } = await feed(page);
  c.phase!.phase = CapturePhase.CAPTURE;
  c.transport!.lastState = "CAPTURING";
  c.transport!.applicationFrames += 1n;
  capture();
  await expect(badge(page)).toContainText("Degraded");
  await page.clock.fastForward(12_000);
  await expect(badge(page)).toContainText("Solana stream stale");
  c.run!.runId = "another-run";
  c.transport!.applicationFrames += 100n;
  capture();
  await expect(badge(page)).toContainText("Checking stream");
  c.transport!.applicationFrames += 1n;
  capture();
  await expect(badge(page)).toContainText("Degraded");
  c.transport!.applicationFrames += 100n;
  capture(StateQuality.OK, toBinary(CaptureStateSchema, c), 2n, 1n);
  await expect(badge(page)).toContainText("Checking stream");
});

test("schema rejection reloads once and stays blocked without subscribing", async ({ page }) => {
  let hellos = 0;
  let subscriptions = 0;
  await page.route("**/api/v1/session", route => route.fulfill({ status: 204 }));
  await page.routeWebSocket("**/ws/v1/gateway", ws => {
    ws.onMessage(data => {
      const msg = fromBinary(ClientMessageSchema, typeof data === "string" ? Buffer.from(data) : data).msg;
      if (msg.case === "subscribe") subscriptions++;
      if (msg.case === "hello") {
        hellos++;
        ws.send(Buffer.from(toBinary(ServerMessageSchema, create(ServerMessageSchema, { msg: {
          case: "welcome", value: { status: WelcomeStatus.REJECTED_SCHEMA, protocolMajor: 1,
            schemaId: "incompatible-test-server", rejectReason: "schema mismatch" },
        } }))));
      }
    });
  });
  await page.goto("/");
  await expect(page.getByText(/^BLOCKED/)).toBeVisible();
  await expect.poll(() => hellos).toBe(2);
  await page.waitForTimeout(500);
  expect(hellos).toBe(2);
  expect(subscriptions).toBe(0);
});

test("Check & start drives the one existing capture command; drafts are never sent as parameters", async ({ page }) => {
  await feed(page);
  let process = { state: "STOPPED", run_id: "", pid: null as number | null, reason: "", observed_unix_ms: Date.now(), controller_available: true };
  const sent: CommandRequest[] = [];
  const proto = (bytes: Uint8Array) => ({ body: Buffer.from(bytes), contentType: "application/x-protobuf" });
  await page.route("**/api/v1/session", (r) => r.fulfill({ status: 204, headers: { "x-csrf-token": "csrf" } }));
  await page.route("**/api/v1/project0/process", (r) => r.fulfill({ json: process }));
  await page.route("**/api/v1/command**", async (r) => {
    if (r.request().method() === "POST") {
      const req = fromBinary(CommandRequestSchema, r.request().postDataBuffer()!);
      sent.push(req);
      process = { ...process, state: req.action.endsWith(".start") ? "RUNNING" : "STOPPED", run_id: "run-1" };
      return r.fulfill(proto(toBinary(CommandResponseSchema, create(CommandResponseSchema,
        { status: CommandStatus.ACCEPTED, commandId: req.requestId, requestId: req.requestId, phase: CommandPhase.COMPLETED }))));
    }
    const id = new URL(r.request().url()).searchParams.get("request_id") ?? "";
    return r.fulfill(proto(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema,
      { found: true, commandId: id, phase: CommandPhase.COMPLETED, detail: "Managed process started" }))));
  });
  await tab(page, "Capture");
  await page.getByRole("button", { name: "10m", exact: true }).click();
  await page.getByLabel("Source").selectOption("transactions");
  const verify = page.getByRole("tablist", { name: "Verification mode" });
  await expect(verify.getByRole("tab", { name: "Index" })).toBeDisabled();
  await expect(verify.getByRole("tab", { name: "Crosscheck" })).toBeDisabled();
  const preview = page.locator(".p0-code");
  await expect(preview).toHaveText("p0ctl run start --duration 10m --network-audit <FILE> --mainnet-approved <TOKEN> --capture-source transactions --chain-verify off --report-markers all");
  await page.getByLabel("Observation").selectOption("market");
  await expect(preview).toContainText("no mode option");
  await page.getByLabel("Observation").selectOption("econ4h");
  const start = page.getByRole("button", { name: "Check & start" });
  await expect(start).toBeEnabled();
  await start.click();
  await expect(page.getByTestId("p0-capture-command-receipt")).toContainText("COMPLETED");
  expect(sent.map((r) => [r.action, r.resourceId, r.params.length, r.expected])).toEqual([["project0.capture.start", "", 0, undefined]]);
  // Stop asks in a separate control: a double-click on Stop never sends. An arm expires with its run.
  const stop = page.getByRole("button", { name: "Stop", exact: true });
  const confirm = page.getByRole("button", { name: "Confirm stop", exact: true });
  await expect(stop).toBeEnabled({ timeout: 6000 });
  await stop.dblclick();
  await expect(confirm).toBeVisible();
  await expect(page.getByText(/BSC readiness/)).toBeVisible();
  process = { ...process, run_id: "run-2", pid: 2 };
  await expect(confirm).toBeHidden({ timeout: 6000 });
  await stop.click();
  await confirm.click();
  await expect(page.getByTestId("p0-capture-command-receipt")).toContainText("STOPPED", { timeout: 6000 });
  expect(sent.map((r) => r.action)).toEqual(["project0.capture.start", "project0.capture.stop"]);
});

test("execution authorization and running liquidators are never fixed claims", async ({ page }) => {
  await feed(page);
  // The golden status runs on built-in defaults: no effective record, so authorization is unknown.
  await expect(page.getByText("Disabled · Authorization unknown", { exact: true })).toBeVisible();
  await tab(page, "Liquidator");
  // Without an owner topic the tab names its state, never a running liquidator.
  await expect(page.getByText(/^— · [A-Z_]+ · Authorization unknown$/)).toBeVisible();
  await expect(page.getByText("0 · LIVE not authorized")).toHaveCount(0);
  await page.getByRole("button", { name: "+ Add bot" }).click();
  await expect(page.locator(".p0-line", { hasText: "Execution status" })).toHaveText("Execution statusAuthorization unknown");
});

test("DEC-05: effective values reach the Liquidator tab with source, range and the provisional marker", async ({ page }) => {
  const m = fromBinary(SystemStatusSchema, golden("system_status"));
  m.operational!.effective = create(EffectiveRecordSchema, { profile: "performance", sources: { profile: "mode:econ4h", threshold_trigger: "environment" },
    thresholdTrigger: true, sourceGeyser: "off", sourceGeyserProvider: "publicnode" });
  await feed(page, true, false, toBinary(SystemStatusSchema, m));
  await badge(page).click();
  await expect(page.getByText(`performance (mode:econ4h) · ${m.operational!.parameters.length} parameters`, { exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await tab(page, "Liquidator");
  const row = (title: string) => page.locator(".p0-line", { hasText: title });
  await expect(row("workers").first()).toContainText("1 threads · builtin_default · default 1 · 1…4294967295 · restart · provisional, measurement open (B-2)");
  await expect(row("max_slippage_bps")).toContainText("0 bps · builtin_default · default 0 · 0…10000 · restart");
  await expect(row("threshold_trigger")).toContainText("true · environment · restart");
});

test("owner and analytics topics reach their tabs; job and source commands take the one command path", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await feed(page, true, true);
  const sent: CommandRequest[] = [];
  const proto = (bytes: Uint8Array) => ({ body: Buffer.from(bytes), contentType: "application/x-protobuf" });
  await page.route("**/api/v1/session", (r) => r.fulfill({ status: 204, headers: { "x-csrf-token": "csrf" } }));
  await page.route("**/api/v1/command**", async (r) => {
    if (r.request().method() === "POST") {
      const req = fromBinary(CommandRequestSchema, r.request().postDataBuffer()!);
      sent.push(req);
      return r.fulfill(proto(toBinary(CommandResponseSchema, create(CommandResponseSchema,
        { status: CommandStatus.ACCEPTED, commandId: req.requestId, requestId: req.requestId, phase: CommandPhase.COMPLETED }))));
    }
    const id = new URL(r.request().url()).searchParams.get("request_id") ?? "";
    return r.fulfill(proto(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema,
      { found: true, commandId: id, phase: CommandPhase.COMPLETED, detail: "Job recorded and its worker started" }))));
  });
  // Overview (§6): the ProgramData verdict is BLOCKED with both identities; the job summary is the controller's.
  await expect(page.getByText("Program pin · BLOCKED", { exact: true })).toBeVisible();
  await expect(page.getByText(/NOT_REFERENCE · upgrade latched .* qualified mrgn-0\.1\.11-rc1/)).toBeVisible();
  await expect(page.getByText(/^runner idle · \d+ running · \d+ queued/)).toBeVisible();
  // DEC-10: the Liquidator tab shows the FE-04 approval status the engine projected (no secrets).
  await tab(page, "Liquidator");
  await expect(page.locator(".p0-line", { hasText: "FE-04 approval" }))
    .toContainText("BINDING_MISMATCH (code_sha) · attempt pilot-1 · ZERO_PREFUND_SAME_TX");
  await expect(page.locator(".p0-line", { hasText: "FE-04 approval" })).toContainText("LIVE not authorized (FE-06)");
  // DEC-01: the early blockhash refusals (loss Build, cause BLOCKHASH_STALE) and their restarts.
  await expect(page.locator(".p0-line", { hasText: "BLOCKHASH_STALE before planning" })).toContainText("6 · 1 / 4 / 1 · 2");
  // Analytics (B03/U-9): a backtest is one typed command to the native job controller.
  await tab(page, "Analytics");
  await page.getByRole("tablist", { name: "Analysis mode" }).getByRole("tab", { name: "Backtest", exact: true }).click();
  await page.getByLabel("Minimum profit · USD").fill("1.5");
  await page.getByLabel("Capital scenarios · USD").fill("100,1000");
  await page.getByLabel("Budget · seconds").fill("60");
  await page.getByLabel("Budget · RAM MiB").fill("256");
  await page.getByRole("button", { name: "Run backtest" }).click();
  await expect(page.getByTestId("p0-analytics-command-receipt")).toContainText("COMPLETED");
  const backtest = sent.at(-1)!;
  expect(backtest.action).toBe("project0.analytics.backtest.start");
  const b = fromBinary(BacktestJobCommandSchema, backtest.params);
  expect([b.runId, b.minProfitUsd, b.capitalUsd, b.maxSeconds, b.maxRamBytes]).toEqual(["run-a", "1.5", [100n, 1000n], 60n, 256n * 1_048_576n]);
  // Capture (S1-02): a data-source mode change goes to the runtime owner through the same path.
  await tab(page, "Capture");
  await page.getByRole("combobox", { name: /^Unit/ }).selectOption("geyser:publicnode");
  await page.getByRole("combobox", { name: /^Mode/ }).selectOption("off");
  await page.getByRole("button", { name: "Request mode" }).click();
  await expect(page.getByTestId("p0-source-command-receipt")).toContainText("COMPLETED");
  const source = sent.at(-1)!;
  expect(source.action).toBe("project0.source.set");
  const s = fromBinary(SourceControlCommandSchema, source.params);
  expect([s.unit, s.mode]).toEqual(["geyser:publicnode", "off"]);
  // CTRL-03: the change states the configuration version the Liquidator projection showed.
  expect(source.expected?.version).toEqual({ case: "configVersion", value: 3n });
  expect(sent.map((r) => r.action)).toEqual(["project0.analytics.backtest.start", "project0.source.set"]);
  expect(errors).toEqual([]);
});
