import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";
import { readFileSync } from "node:fs";
import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import { CapturePhase, CaptureStateSchema } from "../src/core/generated/darkbot/project0/v1/capture_pb";
import { SystemStatusSchema } from "../src/core/generated/darkbot/project0/v1/system_pb";
import { AckStatus, ClientMessageSchema, ServerMessageSchema, StateQuality, WelcomeStatus } from "../src/core/generated/darkbot/control/v1/stream_pb";
import { ProjectId, TopicClass, TopicId } from "../src/core/generated/darkbot/control/v1/topics_pb";

const golden = (name: string) => Buffer.from(readFileSync(new URL(`../../protocol/golden/project0/${name}.hex`, import.meta.url), "utf8").trim(), "hex");
const schemaId = readFileSync(new URL("../../protocol/schema_id.txt", import.meta.url), "utf8").trim();

async function feed(page: Page, initiallyReported = true) {
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
          ] } });
      } else if (msg.case === "subscribe") {
        subscriptions.add(msg.value.topic);
        send({ case: "controlAck", value: { requestId: msg.value.requestId, topic: msg.value.topic, status: AckStatus.ACCEPTED } });
        if (!initiallyReported) return;
        if (msg.value.topic === TopicId.PROJECT0_STATUS) {
          send({ case: "stateFull", value: { topic: msg.value.topic, generation: 1n, seq: 1n, quality: StateQuality.OK, sourceId: "TEST_ONLY", payload: golden("system_status") } });
        } else capture();
      }
    });
  });
  await page.goto("/project-0");
  await expect.poll(() => subscriptions.size).toBe(2);
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
