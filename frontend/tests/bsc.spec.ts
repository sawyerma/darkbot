import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";
import { readFileSync } from "node:fs";
import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import { BscLiveStateSchema } from "../src/core/generated/darkbot/bsc/v1/status_pb";
import { AckStatus, ClientMessageSchema, ServerMessageSchema, StateQuality, WelcomeStatus } from "../src/core/generated/darkbot/control/v1/stream_pb";
import { ProjectId, TopicClass, TopicId } from "../src/core/generated/darkbot/control/v1/topics_pb";

// The BSC page in a production build (`npm run test:bsc`, CI workflow `bsc`): its six tabs, the one
// LIVE source and fail-closed controls. The gateway is a local stand-in serving the reporter's
// golden projections (protocol/golden/bsc); nothing leaves the browser.
const golden = (name: string) => Buffer.from(readFileSync(new URL(`../../protocol/golden/bsc/${name}.hex`, import.meta.url), "utf8").trim(), "hex");
const schemaId = readFileSync(new URL("../../protocol/schema_id.txt", import.meta.url), "utf8").trim();
const TABS = ["Overview", "Launch / BUY", "Monitoring", "Analytics", "Wallets", "LIVE"];
const STATE_TOPICS: [TopicId, string, string, string][] = [
  [TopicId.BSC_STATUS, "bsc.status", "darkbot.bsc.v1.BscStatus", "status"],
  [TopicId.BSC_LAUNCH_STATE, "bsc.launch.state", "darkbot.bsc.v1.BscLaunchState", "launch"],
  [TopicId.BSC_MONITORING_STATE, "bsc.monitoring.state", "darkbot.bsc.v1.BscMonitoringState", "monitoring"],
  [TopicId.BSC_WALLETS_STATE, "bsc.wallets.state", "darkbot.bsc.v1.BscWalletsState", "wallets"],
  [TopicId.BSC_LIVE_STATE, "bsc.live.state", "darkbot.bsc.v1.BscLiveState", "live"],
];

/** The gateway stand-in: every BSC STATE topic with its golden payload (`live` replaced). */
async function feed(page: Page, live: Uint8Array = golden("live")) {
  let socket: WebSocketRoute;
  const payloads = new Map<TopicId, Uint8Array>(STATE_TOPICS.map(([t, , , name]) => [t, t === TopicId.BSC_LIVE_STATE ? live : golden(name)]));
  const send = (msg: MessageInitShape<typeof ServerMessageSchema>["msg"]) =>
    socket.send(Buffer.from(toBinary(ServerMessageSchema, create(ServerMessageSchema, { msg }))));
  await page.routeWebSocket("**/ws/v1/gateway", (ws) => {
    socket = ws;
    ws.onMessage((data) => {
      const msg = fromBinary(ClientMessageSchema, typeof data === "string" ? Buffer.from(data) : data).msg;
      if (msg.case === "hello") {
        send({ case: "welcome", value: { status: WelcomeStatus.ACCEPTED, protocolMajor: 1, schemaId,
          topics: STATE_TOPICS.map(([topic, wireName, payloadType]) => ({ topic, wireName, class: TopicClass.STATE, project: ProjectId.BSC, payloadType, available: true })) } });
      } else if (msg.case === "subscribe") {
        send({ case: "controlAck", value: { requestId: msg.value.requestId, topic: msg.value.topic, status: AckStatus.ACCEPTED } });
        const payload = payloads.get(msg.value.topic);
        if (payload) send({ case: "stateFull", value: { topic: msg.value.topic, generation: 1n, seq: 1n, quality: StateQuality.OK, sourceId: "TEST_ONLY",
          sourceTimestampUnixNs: BigInt(Date.now()) * 1_000_000n, payload } });
      }
    });
  });
}

const tab = (page: Page, name: string) => page.getByRole("tablist", { name: "BSC areas" }).getByRole("tab", { name, exact: true });
const line = (page: Page, label: string) => page.locator(".g-line").filter({ has: page.locator(":scope > span:first-child", { hasText: new RegExp(`^${label}$`) }) }).first();
/** Buttons that only navigate between tabs; every other button is a request to a native owner. */
const NAVIGATION = new Set(["Launch / BUY", "Monitoring"]);

test("the six tabs fail closed without a gateway: UNKNOWN, never 0, no control acts", async ({ page }) => {
  await page.routeWebSocket("**/ws/v1/gateway", (ws) => ws.close());
  await page.goto("/bsc");
  // The page is a lazy chunk: wait for its tabs (a slow runner renders them later).
  await expect(page.getByRole("tablist", { name: "BSC areas" }).getByRole("tab")).toHaveText(TABS);
  let locked = 0;
  for (const name of TABS) {
    await tab(page, name).click();
    await expect(tab(page, name)).toHaveAttribute("aria-selected", "true");
    // No direct BUY, SEND or SWEEP control: a BUY is only ever the native owner's, under its gate.
    expect(await page.getByRole("button", { name: /^(start )?buy$|^send$|sweep/i }).count()).toBe(0);
    // Without the native owner's state every request control is locked.
    for (const button of await page.locator(".bsc-page button:not([role=tab])").all()) {
      const label = (await button.innerText()).trim();
      if (!NAVIGATION.has(label)) { await expect(button, `${name}: ${label}`).toBeDisabled(); locked++; }
    }
    // Unknown is "—" or UNKNOWN, never 0.
    for (const v of await page.locator(".g-line .g-right").allTextContents()) expect(v.trim()).not.toMatch(/^0(\.0+)?( BNB| USD)?$/);
  }
  // Every tab's requests were checked: readiness, candidate, switches, modes, limits, grant, revoke.
  expect(locked).toBeGreaterThan(20);
  await tab(page, "Overview").click();
  await expect(page.getByTestId("bsc-guard")).toHaveText("Guard · UNKNOWN");
  await expect(line(page, "LIVE")).toContainText("UNKNOWN");
  // Audit E 1.6: no grantable source without the native projection, so none is offered.
  await tab(page, "LIVE").click();
  await expect(page.locator("[data-testid^=bsc-live-grant-source-]")).toHaveCount(0);
});

test("LIVE comes from bsc.live.state alone and the grant offers only its grantable sources", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const live = toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "CLEAR", pathState: "ARMED", generation: 5n, qualification: "VERIFIED", warnings: [],
    grantableSources: ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"],
  }));
  await feed(page, live);
  await page.goto("/bsc");
  await expect(page.getByTestId("bsc-guard")).toHaveText("Guard · UNARMED");
  // Audit E 4.10: the Overview's LIVE line is the owner's gate and the LIVE path, nothing else.
  await expect(line(page, "LIVE")).toContainText("LIVE");
  await expect(line(page, "LIVE")).toContainText("gate CLEAR · LIVE path ARMED");
  for (const name of TABS) {
    await tab(page, name).click();
    await expect(tab(page, name)).toHaveAttribute("aria-selected", "true");
  }
  await expect(line(page, "LIVE gate")).toContainText("CLEAR");
  // Audit E 1.6: exactly the sources with an adapter endpoint; the first is chosen until the operator chooses.
  const sources = page.locator("[data-testid^=bsc-live-grant-source-]");
  await expect(sources).toHaveCount(2);
  expect(await sources.evaluateAll(els => els.map(e => e.getAttribute("data-testid")))).toEqual(
    ["bsc-live-grant-source-BSC_INGRESS_PUBLICNODE", "bsc-live-grant-source-BSC_INGRESS_DRPC"]);
  await expect(page.getByTestId("bsc-live-grant-source-BSC_INGRESS_PUBLICNODE")).toBeChecked();
  await expect(page.getByTestId("bsc-live-grant-source-BSC_INGRESS_DRPC")).not.toBeChecked();
  await page.getByTestId("bsc-live-grant-source-BSC_INGRESS_PUBLICNODE").uncheck();
  await expect(page.getByTestId("bsc-live-grant-error")).toContainText("at least one grantable source");
  expect(errors).toEqual([]);
});
