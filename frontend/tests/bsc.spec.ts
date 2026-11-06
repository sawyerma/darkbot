import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";
import { readFileSync } from "node:fs";
import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import { BscLiveStateSchema, BscMonitoringStateSchema } from "../src/core/generated/darkbot/bsc/v1/status_pb";
import { BscQueryResponseSchema } from "../src/core/generated/darkbot/bsc/v1/query_pb";
import { Availability } from "../src/core/generated/darkbot/bsc/v1/common_pb";
import { AckStatus, ClientMessageSchema, ServerMessageSchema, StateQuality, WelcomeStatus } from "../src/core/generated/darkbot/control/v1/stream_pb";
import { CommandLookupResponseSchema, CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus, type CommandRequest } from "../src/core/generated/darkbot/control/v1/commands_pb";
import { ProjectId, TopicClass, TopicId } from "../src/core/generated/darkbot/control/v1/topics_pb";

// The BSC page (Graphite V8, FE-V8-01…18) in a production build (`npm run test:bsc`): its four areas,
// the Health window, the Settings and LIVE drawers, the one LIVE source and fail-closed controls. The
// static fixture server has no proxy (/api and /ws answer 503); the gateway is a stand-in in page
// routes serving the reporter's golden projections (protocol/golden/bsc). No request reaches a real
// gateway.
const golden = (name: string) => Buffer.from(readFileSync(new URL(`../../protocol/golden/bsc/${name}.hex`, import.meta.url), "utf8").trim(), "hex");
const schemaId = readFileSync(new URL("../../protocol/schema_id.txt", import.meta.url), "utf8").trim();
const AREAS = ["Overview", "Training", "Live", "Wallets"] as const;
const STATE_TOPICS: [TopicId, string, string, string][] = [
  [TopicId.BSC_STATUS, "bsc.status", "darkbot.bsc.v1.BscStatus", "status"],
  [TopicId.BSC_LAUNCH_STATE, "bsc.launch.state", "darkbot.bsc.v1.BscLaunchState", "launch"],
  [TopicId.BSC_MONITORING_STATE, "bsc.monitoring.state", "darkbot.bsc.v1.BscMonitoringState", "monitoring"],
  [TopicId.BSC_WALLETS_STATE, "bsc.wallets.state", "darkbot.bsc.v1.BscWalletsState", "wallets"],
  [TopicId.BSC_LIVE_STATE, "bsc.live.state", "darkbot.bsc.v1.BscLiveState", "live"],
];

/** The gateway stand-in: every BSC STATE topic with its golden payload (`live` replaced). Returns
 *  a push of the next LIVE state (a new full state, as the reporter republishes it). */
async function feed(page: Page, live: Uint8Array = golden("live"), monitoring?: Uint8Array): Promise<(next: Uint8Array) => void> {
  let socket: WebSocketRoute;
  let seq = 1n;
  const payloads = new Map<TopicId, Uint8Array>(STATE_TOPICS.map(([t, , , name]) => [t, t === TopicId.BSC_LIVE_STATE ? live
    : t === TopicId.BSC_MONITORING_STATE && monitoring ? monitoring : golden(name)]));
  const send = (msg: MessageInitShape<typeof ServerMessageSchema>["msg"]) =>
    socket.send(Buffer.from(toBinary(ServerMessageSchema, create(ServerMessageSchema, { msg }))));
  // The connection's session bootstrap comes first; without it no WebSocket is opened.
  await page.route("**/api/v1/session", (route) => route.fulfill({ status: 204, headers: { "x-csrf-token": "TEST_ONLY" } }));
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
  return (next: Uint8Array) => send({ case: "stateFull", value: { topic: TopicId.BSC_LIVE_STATE, generation: 1n, seq: ++seq, quality: StateQuality.OK, sourceId: "TEST_ONLY",
    sourceTimestampUnixNs: BigInt(Date.now()) * 1_000_000n, payload: next } });
}

const nav = (page: Page) => page.getByRole("navigation", { name: "BSC areas" });
async function area(page: Page, name: (typeof AREAS)[number]) {
  await nav(page).getByRole("button", { name, exact: true }).click();
  await expect(nav(page).getByRole("button", { name, exact: true })).toHaveAttribute("aria-current", "page");
}
const line = (page: Page, label: string) => page.locator(".g-line").filter({ has: page.locator(":scope > span:first-child", { hasText: new RegExp(`^${label}$`) }) }).first();
/** Opens one of the Live drawers (controls: sections 1/2/3/6, limits: 4/5, receipts: 7/8/9). */
async function liveDrawer(page: Page, which: "controls" | "limits" | "receipts") {
  await area(page, "Live");
  if (which === "receipts") await page.getByRole("button", { name: "Receipts", exact: true }).click();
  else await page.getByTestId(which === "controls" ? "bsc-live-controls-open" : "bsc-live-limits-open").click();
  await expect(page.getByRole("dialog")).toBeVisible();
}

test("the four areas fail closed without a gateway: UNKNOWN, never 0, no control acts", async ({ page }) => {
  await page.routeWebSocket("**/ws/v1/gateway", (ws) => ws.close());
  await page.goto("/bsc");
  // The page is a lazy chunk: wait for its areas (a slow runner renders them later).
  await expect(nav(page).getByRole("button")).toHaveText([...AREAS]);
  for (const name of AREAS) {
    await area(page, name);
    // No direct BUY, SEND or SWEEP control: a BUY is only ever the native owner's, under its gate.
    expect(await page.locator("#bsc-content").getByRole("button", { name: /^(start )?buy$|^send$|sweep/i }).count()).toBe(0);
    // Unknown is "—" or UNKNOWN, never 0.
    for (const v of await page.locator(".system-metric strong, .stat b, .g-line .g-right").allTextContents()) expect(v.trim()).not.toMatch(/^0(\.0+)?( BNB| USD)?$/);
  }
  await area(page, "Overview");
  await expect(page.getByTestId("bsc-trading-state")).toHaveText("Trading UNKNOWN");
  await expect(page.getByTestId("bsc-metric-pnl")).toContainText("—");
  await expect(page.getByTestId("bsc-metric-balance")).toContainText("—");
  // Health: nothing green without a gateway.
  await page.getByTestId("bsc-health-pill").click();
  await expect(page.locator("[data-testid^=bsc-health-][data-state=ok]")).toHaveCount(0);
  await page.keyboard.press("Escape");
  // Training without its native owner: no module, no test starts.
  await area(page, "Training");
  await expect(page.getByTestId("bsc-training-new")).toBeDisabled();
  await page.getByRole("button", { name: "Open Strategy C module" }).click();
  await expect(page.getByTestId("bsc-training-start")).toBeDisabled();
  // Wallets: transfer and sweep stay visible, never executable.
  await area(page, "Wallets");
  await page.getByRole("button", { name: "Send funds", exact: true }).click();
  await expect(page.getByTestId("bsc-wallet-send").getByRole("button", { name: "Review transfer" })).toBeDisabled();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Auto-transfer", exact: true }).click();
  await expect(page.getByTestId("bsc-wallet-sweep").getByRole("button", { name: "Save rule" })).toBeDisabled();
  await page.keyboard.press("Escape");
  // Every owner request in the Settings drawer and the Live drawers is locked without owner state.
  let locked = 0;
  const lockedIn = async (scope: string) => {
    for (const button of await page.locator(scope).locator("button.g-secondary, button.g-primary").all()) { await expect(button, (await button.innerText()).trim()).toBeDisabled(); locked++; }
  };
  for (const wallet of ["launcher", "buyer"]) {
    await page.getByTestId(`bsc-wallet-${wallet}`).click();
    await lockedIn("#bsc-content");
  }
  await page.getByTestId("bsc-settings-open").click();
  await lockedIn("[data-testid=bsc-settings]");
  // Audit E 4.9 / review M1: no source, component or venue row without the owner's projection, and the empty lists say why.
  expect(await page.locator("[data-testid^=bsc-source-BSC_]").count()).toBe(0);
  for (const group of ["Sources", "Venues", "Components"]) await expect(line(page, group)).toContainText("BACKEND_UNAVAILABLE");
  await expect(line(page, "Economic switches").first()).toContainText("BACKEND_UNAVAILABLE");
  await page.keyboard.press("Escape");
  await liveDrawer(page, "controls");
  await lockedIn("[data-testid=bsc-live-controls]");
  // Audit E 1.6 / 4.9: no grantable source, strategy or adapter without the native projection.
  await expect(page.locator("[data-testid^=bsc-live-grant-source-]")).toHaveCount(0);
  await expect(page.getByTestId("bsc-live-grant-strategies").locator("option")).toHaveCount(0);
  await expect(page.locator("[data-testid^=bsc-live-grant-adapter-]")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await liveDrawer(page, "limits");
  await lockedIn("[data-testid=bsc-live-limits-drawer]");
  // REV.1.0 R2b: no campaign verdict is no evaluation, never a value.
  await expect(page.getByTestId("bsc-live-campaign-none")).toHaveText("keine Bewertung");
  await expect(page.getByTestId("bsc-live-attempts")).toHaveCount(0);
  await page.keyboard.press("Escape");
  // The Strategy C form saves nothing without its owner.
  await expect(page.getByTestId("bsc-live-strategy-save")).toBeDisabled();
  expect(locked).toBeGreaterThanOrEqual(20);
});

test("LIVE comes from bsc.live.state alone and the grant offers only its grantable sources", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const live = toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "CLEAR", pathState: "ARMED", generation: 5n, qualification: "VERIFIED", warnings: [],
    grantableSources: ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"],
    // Audit E 4.9: the owner's grant choices (TEST_ONLY values of the projection).
    grantVenues: ["BSC_VENUE_FLAP"], grantStrategies: ["C3", "STRATEGY_C", "C3+STRATEGY_C"],
    grantAdapters: ["CLUB48_BUILDER_V1", "BLOCKRAZOR_BUILDER_V1"],
  }));
  await feed(page, live);
  await page.goto("/bsc");
  // Audit E 4.10: ON is the owner's gate and the LIVE path, nothing else.
  await expect(page.getByTestId("bsc-trading-state")).toHaveText("Trading ON");
  await page.getByTestId("bsc-settings-open").click();
  await expect(page.getByTestId("bsc-guard")).toHaveCount(0);
  await expect(line(page, "Compiled guard")).toContainText("UNARMED");
  await expect(line(page, "LIVE")).toContainText("gate CLEAR · LIVE path ARMED");
  await page.keyboard.press("Escape");
  await area(page, "Live");
  await expect(page.getByTestId("bsc-live-strip")).toContainText("Trading ON");
  await liveDrawer(page, "controls");
  await expect(line(page, "LIVE gate")).toContainText("CLEAR");
  // Audit E 1.6: exactly the sources with an adapter endpoint; the first is chosen until the operator chooses.
  const sources = page.locator("[data-testid^=bsc-live-grant-source-]");
  await expect(sources).toHaveCount(2);
  expect(await sources.evaluateAll(els => els.map(e => e.getAttribute("data-testid")))).toEqual(
    ["bsc-live-grant-source-BSC_INGRESS_PUBLICNODE", "bsc-live-grant-source-BSC_INGRESS_DRPC"]);
  await expect(page.getByTestId("bsc-live-grant-source-BSC_INGRESS_PUBLICNODE")).toBeChecked();
  await expect(page.getByTestId("bsc-live-grant-source-BSC_INGRESS_DRPC")).not.toBeChecked();
  // Audit E 4.9: exactly the owner's projected strategies and adapters; its first ones are chosen.
  expect(await page.getByTestId("bsc-live-grant-strategies").locator("option").evaluateAll(els => els.map(e => e.textContent)))
    .toEqual(["C3", "STRATEGY_C", "C3+STRATEGY_C"]);
  const adapters = page.locator("[data-testid^=bsc-live-grant-adapter-]");
  expect(await adapters.evaluateAll(els => els.map(e => e.getAttribute("data-testid")))).toEqual(
    ["bsc-live-grant-adapter-CLUB48_BUILDER_V1", "bsc-live-grant-adapter-BLOCKRAZOR_BUILDER_V1"]);
  await expect(page.getByTestId("bsc-live-grant-adapter-CLUB48_BUILDER_V1")).toBeChecked();
  await page.getByTestId("bsc-live-grant-source-BSC_INGRESS_PUBLICNODE").uncheck();
  await expect(page.getByTestId("bsc-live-grant-error")).toContainText("at least one grantable source");
  await page.keyboard.press("Escape");
  await liveDrawer(page, "limits");
  await expect(page.getByTestId("bsc-live-campaign-none")).toHaveText("keine Bewertung");
  expect(errors).toEqual([]);
});

test("the campaign verdict and its attempts as recorded, evidence only (REV.1.0 R2b)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // TEST_ONLY values: they test the display, they are no campaign.
  const attempt = (ordinal: bigint, verdict: string) => ({
    ordinal, tx: `0x${ordinal.toString(16).padStart(64, "0")}`, kind: "C3_BUY", cell: "EXTERNAL_FULL_LAUNCH", state: "INCLUDED", verdict,
    acquisition: "PASS", canonical: "PASS", timing: "UNKNOWN", placement: "PASS", capture: "PASS", local: "PASS", economics: "PASS",
    reasons: ["TIMING_WITNESS_MISSING"], window: "COMPLETE", trades: 7n, buys: 5n, sells: 2n, priceVsFirstExternalPpm: 800000n,
    firstExternalPrice: "250000000000000000", ingressToSendNs: 900n, attempts: ordinal, acquired: ordinal, undecided: ordinal,
    adapters: [{ builder: "CLUB48_BUILDER_V1", outcome: "SENT", answer: "ACCEPTED", adapter: 0n, boundary: "WRITE", qualified: true }],
    attempt: `0x${"a0".repeat(32)}`, token: `0x${"ab".repeat(20)}`, launchHash: `0x${"1a".repeat(32)}`, trigger: "BSC_INGRESS_PUBLICNODE",
    blockNumber: 130000010n + ordinal, blockHash: `0x${"b1".repeat(32)}`, blockMs: 1793000000000n, tokens: "5000", ethWei: "1000", gasWei: "21",
    ownPrice: "200000000000000000", placed: "FIRST", windowMs: 15000n, evaluatedUnixMs: 1793000050000n,
  });
  const live = (attempts: ReturnType<typeof attempt>[]) => toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "BLOCKED", pathState: "NOT_CONFIGURED", generation: 5n, qualification: "NONE",
    campaign: { verdict: "CAMPAIGN_OPEN", releaseEvidence: "RELEASE_EVIDENCE_NOT_READY", reasons: ["SAMPLES_MISSING: EXTERNAL_FULL_LAUNCH 2/10"],
      planned: 10n, observed: 40n, selected: 30n, admitted: 2n, skipped: 38n, written: 2n, acquired: 2n, undecided: 2n, consecutiveMisses: 12n,
      coveredS: 3600n, uncoveredS: 400n, coveredPpm: 900000n, skipReasons: ["HASH_ONLY=7"], evaluatedUnixMs: 1793000050000n, recordSha256: "ab".repeat(32),
      cells: [{ cell: "EXTERNAL_FULL_LAUNCH", enabled: true, minAttempts: 10n, attempts: 2n, undecided: 2n, durationS: 3600n, verdict: "CAMPAIGN_OPEN" }] },
    attempts,
  }));
  await feed(page, live([attempt(1n, "UNKNOWN"), { ...attempt(2n, "UNKNOWN"), evaluatedUnixMs: 1793000060000n }]));
  await page.goto("/bsc");
  await liveDrawer(page, "limits");
  await expect(line(page, "Campaign verdict")).toContainText("CAMPAIGN_OPEN");
  await expect(line(page, "Campaign verdict")).toContainText("SAMPLES_MISSING: EXTERNAL_FULL_LAUNCH 2/10");
  await expect(line(page, "Release evidence")).toContainText("RELEASE_EVIDENCE_NOT_READY");
  await expect(line(page, "Release evidence")).toContainText("Kampagnen-Evidenz allein");
  const counts = page.getByTestId("bsc-live-campaign-counts");
  await expect(line(page, "Acquired")).toContainText("2/2");
  await expect(counts).toContainText("2/10 planned");
  await expect(counts).toContainText("38/40");
  await expect(page.getByTestId("bsc-live-campaign-cells")).toContainText("2 / min 10");
  await expect(page.getByText("T0→T_buy bleibt UNKNOWN ohne Producer-Witness", { exact: false })).toBeVisible();
  const rows = page.getByTestId("bsc-live-attempts").locator("tbody tr");
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText("timing UNKNOWN");
  await expect(rows.first()).toContainText("800000 ppm");
  // Review H1: the token, its block, what it acquired and cost, its own price.
  await expect(rows.first()).toContainText(`0x${"ab".repeat(20)}`);
  await expect(rows.first()).toContainText(`130000011 · 0x${"b1".repeat(32)}`);
  await expect(rows.first()).toContainText("tokens 5000 · input 1000 wei · gas 21 wei");
  await expect(rows.first()).toContainText("own 200000000000000000");
  await expect(rows.first()).toContainText("FIRST · launch index");
  // Review L8: counts of the C3 verdicts, not a share of the attempts.
  await expect(rows.last()).toContainText("acquired 2/2 LIVE BUYs · C3 verdicts so far: 0 passed · 0 failed · 2 undecided");
  // Review M4: a record of another evaluation than the verdict's is marked.
  await expect(rows.last()).toContainText("of another evaluation");
  await expect(rows.first()).not.toContainText("another evaluation");
  await expect(page.getByTestId("bsc-live-campaign-none")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("a campaign section beyond the state is INVALID, never no evaluation or no attempt (REV.1.0 R2b re-check F3)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // TEST_ONLY values: they test the display, they are no campaign.
  const beyond = "LIVE_CAMPAIGN_BEYOND_STATE 3 records, 2 warnings";
  await feed(page, toBinary(BscLiveStateSchema, create(BscLiveStateSchema, { gate: "BLOCKED", pathState: "NOT_CONFIGURED", generation: 5n, qualification: "NONE", warnings: [beyond] })));
  await page.goto("/bsc");
  await liveDrawer(page, "limits");
  await expect(page.getByTestId("bsc-live-campaign-invalid")).toHaveText("INVALID");
  await expect(line(page, "Campaign verdict")).toContainText(beyond);
  await expect(page.getByTestId("bsc-live-campaign-none")).toHaveCount(0);
  await expect(page.getByTestId("bsc-live-attempts-beyond")).toHaveText("INVALID");
  await expect(line(page, "Attempts")).toContainText(beyond);
  await expect(line(page, "Attempts")).not.toContainText("no attempt record");
  expect(errors).toEqual([]);
});

test("the LIVE receipts name the undecided and stranded lots, the receipt recovery and each transaction's source (REV.1.0 R1/R3b)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // TEST_ONLY values: they test the display, they are no LIVE state.
  const tx = (n: number) => `0x${n.toString(16).padStart(64, "0")}`;
  await feed(page, toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "BLOCKED", pathState: "ARMED", generation: 5n, qualification: "NONE",
    exitUndecided: ["0x" + "e1".repeat(20)], stranded: ["0x" + "e2".repeat(20) + " LIVE_POSITION_OFF_CURVE"], lotReadFaults: 3n, lotReadFault: "curve fault",
    recoveryPending: [`${tx(0xc1)} RECOVERY_NOT_FINAL`], recovered: 2n, recoveryFaults: 1n, recoveryFault: "recovery fault",
    transactions: [
      { txHash: tx(0xa1), intent: "BUY", token: "0x" + "aa".repeat(20), nonce: 1n, state: "INCLUDED", blockNumber: 90n, source: "RECEIPT" },
      { txHash: tx(0xb5), intent: "BUY", token: "0x" + "aa".repeat(20), nonce: 2n, state: "REPLACED", blockNumber: 91n, source: "" },
    ],
  })));
  await page.goto("/bsc");
  await liveDrawer(page, "receipts");
  const deciders = page.getByTestId("bsc-live-exit-deciders");
  await expect(line(page, "Exit undecided")).toContainText("0x" + "e1".repeat(20));
  await expect(line(page, "Stranded lots")).toContainText("LIVE_POSITION_OFF_CURVE");
  await expect(deciders).toContainText("curve fault");
  await expect(line(page, "Receipt recovery")).toContainText("2 resolved · 1 pending · 1 failed reads");
  await expect(line(page, "Receipt recovery")).toContainText("recovery fault");
  await expect(page.getByTestId("bsc-live-recovery-pending")).toContainText(`${tx(0xc1)}RECOVERY_NOT_FINAL`);
  const rows = page.getByTestId("bsc-live-transactions").locator("tr");
  await expect(rows.nth(0)).toContainText("RECEIPT");
  await expect(rows.nth(1)).not.toContainText("RECEIPT");
  // The positions are the V8 trade list, not a second table in the drawer.
  await expect(page.getByTestId("bsc-live-positions")).toHaveCount(0);
  expect(errors).toEqual([]);
});

/** REV.1.0 R3b (Audit A): a stand-in gateway that records each request and answers its receipt. */
async function commands(page: Page): Promise<CommandRequest[]> {
  const sent: CommandRequest[] = [];
  const proto = (bytes: Uint8Array) => ({ body: Buffer.from(bytes), contentType: "application/x-protobuf" });
  await page.route("**/api/v1/command**", async (r) => {
    if (r.request().method() === "POST") {
      const req = fromBinary(CommandRequestSchema, r.request().postDataBuffer()!);
      sent.push(req);
      return r.fulfill(proto(toBinary(CommandResponseSchema, create(CommandResponseSchema,
        { status: CommandStatus.ACCEPTED, commandId: req.requestId, requestId: req.requestId, phase: CommandPhase.COMPLETED }))));
    }
    const last = sent[sent.length - 1];
    return r.fulfill(proto(toBinary(CommandLookupResponseSchema, create(CommandLookupResponseSchema,
      { found: true, commandId: last?.requestId ?? "", phase: CommandPhase.COMPLETED, detail: `outcome=APPLIED ${last?.action ?? ""}` }))));
  });
  return sent;
}
// TEST_ONLY values: they test the display and the requests, they are no LIVE state.
const hash = (n: number) => `0x${n.toString(16).padStart(64, "0")}`;
const lotRow = (lot: string, state: string, decisions: string[]) => ({ lot, token: "0x" + "aa".repeat(20), component: "BSC_STRATEGY_C", state, tokens: "7", inputWei: "1000", proceedsWei: "0", gasWei: "1", decisions });
const txRow = (txHash: string, nonce: bigint, state: string, burnable: boolean, burnRequested = false) => ({ txHash, intent: "BUY", token: "0x" + "aa".repeat(20), nonce, state, source: "", burnable, burnRequested });
type LiveInit = MessageInitShape<typeof BscLiveStateSchema>;
const liveOf = (init: LiveInit) => toBinary(BscLiveStateSchema, create(BscLiveStateSchema, { gate: "BLOCKED", pathState: "ARMED", generation: 5n, qualification: "NONE", ...init }));
/** Opens the trade detail of a lot (the V8 trade list's row). */
async function tradeDetail(page: Page, lot: string) {
  await area(page, "Live");
  await page.locator(`[data-testid=bsc-trade-row][data-lot="${lot}"]`).click();
  await expect(page.getByTestId("bsc-trade-detail")).toBeVisible();
}

test("the Live controls show the path's counters, the second source and the presence; a revoke names the exits it ends (REV.1.0 R10)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await feed(page, liveOf({
    nextNonce: 41n, armedGeneration: 6n, refusals: ["OWNER=2"], responses: ["ACCEPTED=3"], probes: 16n, included: 11n, finalized: 14n,
    latencySamples: 9n, latencyP50Ns: 100n, latencyP99Ns: 200n, latencyMaxNs: 300n, leaseAttempt: hash(0xab),
    divergenceQuarantine: true, confirmState: "DISAGREES", presenceAnchor: "0x" + "ab".repeat(20), presenceSigner: "",
    grant: { venue: "BSC_VENUE_FLAP", strategies: "C3+STRATEGY_C", requestId: "g1", setGeneration: 5n, expiresUnixMs: 1793000000000n, scopeSha256: "s", riskScopeSha256: "r1" },
    positions: [lotRow(hash(1), "OPEN", []), lotRow(hash(2), "CLOSED", [])],
  }));
  await page.goto("/bsc");
  await liveDrawer(page, "controls");
  await expect(line(page, "Next nonce")).toContainText("41");
  await expect(line(page, "Refusals by stage")).toContainText("OWNER=2");
  await expect(line(page, "Release latency")).toContainText("p50 100 ns · p99 200 ns · max 300 ns (9 samples)");
  await expect(line(page, "Second source")).toContainText("second source DISAGREES · quarantine ON");
  await expect(line(page, "Presence anchor")).toContainText("unsigned: BUYs refused (LIVE_PRESENCE_UNSIGNED)");
  await expect(line(page, "Grant risk scope SHA-256")).toContainText("r1");
  // A revoke ends the held position's exits; its dialog says so. Accessible: named, focus inside, cancelled by its button.
  await page.getByTestId("bsc-live-revoke-request").click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toHaveAccessibleName("LIVE-Freigabe widerrufen?");
  await expect(page.getByTestId("bsc-live-revoke-exits")).toHaveText("Ohne Grant laufen keine Exits: 1 gehaltene Position(en) bleiben offen, bis ein neuer Grant (auch EXIT-ONLY) sie abwickelt.");
  await expect(dialog.locator(":focus")).toHaveCount(1);
  await dialog.getByRole("button", { name: "Abbrechen", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page.keyboard.press("Escape");
  await liveDrawer(page, "limits");
  await expect(line(page, "Next C3 attempt")).toContainText(hash(0xab));
  await page.keyboard.press("Escape");
  await liveDrawer(page, "receipts");
  await expect(line(page, "Provider answers")).toContainText("ACCEPTED=3");
  await expect(line(page, "Chain evidence")).toContainText("11 included · 0 reverted · 0 replaced · 14 final · 0 orphaned");
  expect(errors).toEqual([]);
});

test("every BSC control has an accessible name and the areas follow the keyboard (REV.1.0 R10 a11y)", async ({ page }) => {
  await feed(page);
  await page.goto("/bsc");
  for (const name of AREAS) {
    await area(page, name);
    const controls = page.locator("[data-testid=bsc-v8]").locator("button:visible, input:visible, select:visible, textarea:visible");
    for (let i = 0; i < await controls.count(); i++) await expect(controls.nth(i)).toHaveAccessibleName(/\S/);
  }
  // The area buttons are reachable with Tab and act on Enter.
  await nav(page).getByRole("button", { name: "Overview", exact: true }).focus();
  await page.keyboard.press("Tab");
  await expect(nav(page).getByRole("button", { name: "Training", exact: true })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(nav(page).getByRole("button", { name: "Training", exact: true })).toHaveAttribute("aria-current", "page");
});

test("the LIVE operator decisions go at the state their dialog opened on (REV.1.0 R3b, Audit A)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const state = (ledgerVersion: bigint): LiveInit => ({
    ledgerVersion, operatorExit: false, nonceBurn: true, campaignSpentWei: "1000", campaignSpendMaxWei: "0",
    positions: [lotRow(hash(0xa1), "OPEN", ["RESUME", "WRITE_OFF", "EXIT_NOW"]), lotRow(hash(0xa2), "CLOSED", [])],
    transactions: [txRow(hash(0xa1), 1n, "INCLUDED", false), txRow(hash(0xb5), 4n, "UNKNOWN", true), txRow(hash(0xb6), 5n, "RELEASED", true, true)],
  });
  const sent = await commands(page);
  const push = await feed(page, liveOf(state(12n)));
  await page.goto("/bsc");
  // R4: the campaign's spend and its budget (0: no BUY), never made up.
  await liveDrawer(page, "limits");
  await expect(line(page, "Campaign spent")).toContainText(/1000 wei of .*· 0 wei/);
  await page.keyboard.press("Escape");
  // The ledger's decisions of the open lot, in its trade detail; exit-now locked (its switch OFF); a closed lot none.
  await tradeDetail(page, hash(0xa1));
  await expect(line(page, "Operator decisions")).toHaveCount(0);
  const exitNow = page.getByTestId("bsc-live-lot-EXIT_NOW");
  await expect(exitNow).toBeDisabled();
  await expect(exitNow).toHaveAttribute("title", "gesperrt: BSC_EXEC_LIVE_OPERATOR_EXIT ist OFF");
  await expect(page.getByTestId("bsc-live-lot-RESUME")).toBeEnabled();
  // Review H1: the dialog opened at 12 stays at 12 when a newer state arrives; it names it.
  await page.getByTestId("bsc-live-lot-WRITE_OFF").click();
  await expect(page.getByTestId("bsc-live-lot-dialog")).toContainText("Ledger-Version 12");
  push(liveOf(state(13n)));
  await expect(page.getByTestId("bsc-live-lot-moved")).toContainText("Version 13");
  await page.getByTestId("bsc-live-lot-confirm").click();
  await expect(page.getByTestId("bsc-live-lot-receipt")).toContainText("outcome=APPLIED bsc.live.lot");
  await page.keyboard.press("Escape");
  await tradeDetail(page, hash(0xa2));
  await expect(page.getByTestId("bsc-trade-detail").getByRole("button", { name: /^(Resume|Write off|Exit now)$/ })).toHaveCount(0);
  await page.keyboard.press("Escape");
  // The burnable transaction under Receipts; the one requested is named and offers nothing again; others none.
  await liveDrawer(page, "receipts");
  await expect(line(page, "Operator decisions")).toContainText("ledger version 13 · exit-now locked · nonce burn ON");
  const txs = page.getByTestId("bsc-live-transactions").locator("tr");
  await expect(txs.nth(0).getByRole("button")).toHaveCount(0);
  await expect(txs.nth(2).getByTestId("bsc-live-burn-request")).toBeDisabled();
  await expect(txs.nth(2)).toContainText("Burn requested");
  await txs.nth(1).getByTestId("bsc-live-burn-request").click();
  await expect(page.getByTestId("bsc-live-burn-dialog")).toContainText("Nonce 4 verbrennen?");
  await page.getByTestId("bsc-live-burn-confirm").click();
  await expect(page.getByTestId("bsc-live-burn-receipt")).toContainText("outcome=APPLIED bsc.live.nonce_burn");
  expect(sent.map(r => [r.action, r.resourceId, r.params.length, r.expected?.version])).toEqual([
    ["bsc.live.lot", `${hash(0xa1)}:WRITE_OFF`, 0, { case: "configVersion", value: 12n }],
    ["bsc.live.nonce_burn", hash(0xb5), 0, { case: "configVersion", value: 13n }],
  ]);
  expect(errors).toEqual([]);
});

test("the LIVE operator decisions stay locked without their switch ON or the ledger's version (REV.1.0 R3b review M2)", async ({ page }) => {
  const rows: LiveInit = {
    positions: [lotRow(hash(0xa1), "OPEN", ["RESUME", "WRITE_OFF", "EXIT_NOW"])],
    transactions: [txRow(hash(0xb5), 4n, "UNKNOWN", true)],
  };
  const push = await feed(page, liveOf({ ...rows, operatorExit: true, nonceBurn: true }));
  await page.goto("/bsc");
  // No version (the ledger unreadable): nothing goes.
  await tradeDetail(page, hash(0xa1));
  for (const d of ["RESUME", "WRITE_OFF", "EXIT_NOW"]) await expect(page.getByTestId(`bsc-live-lot-${d}`)).toBeDisabled();
  // At a version, exit-now ON: every decision.
  push(liveOf({ ...rows, ledgerVersion: 3n, operatorExit: true, nonceBurn: false }));
  for (const d of ["RESUME", "WRITE_OFF", "EXIT_NOW"]) await expect(page.getByTestId(`bsc-live-lot-${d}`)).toBeEnabled();
  push(liveOf({ ...rows, ledgerVersion: 3n }));
  await expect(page.getByTestId("bsc-live-lot-EXIT_NOW")).toBeDisabled();
  await page.keyboard.press("Escape");
  // The burn with its switch OFF, then unknown: locked; ON: it goes.
  push(liveOf({ ...rows, ledgerVersion: 3n, operatorExit: true, nonceBurn: false }));
  await liveDrawer(page, "receipts");
  const burn = page.getByTestId("bsc-live-transactions").locator("tr").nth(0).getByTestId("bsc-live-burn-request");
  await expect(burn).toBeDisabled();
  await expect(burn).toHaveAttribute("title", "gesperrt: BSC_EXEC_LIVE_NONCE_BURN ist OFF");
  push(liveOf({ ...rows, ledgerVersion: 3n }));
  await expect(burn).toHaveAttribute("title", /unbekannt/);
  push(liveOf({ ...rows, ledgerVersion: 3n, nonceBurn: true }));
  await expect(burn).toBeEnabled();
});

test("an unreadable LIVE ledger offers its lowest entry's quarantine at the owner generation, only with owner state (REV.1.0 R3b)", async ({ page }) => {
  const sent = await commands(page);
  const unreadable: LiveInit = { firstUnreadable: 9n, warnings: ["LIVE_LEDGER_INVALID 2 unreadable, first seq 9"] };
  const push = await feed(page, liveOf({ ...unreadable, gate: "UNKNOWN" }));
  await page.goto("/bsc");
  await liveDrawer(page, "controls");
  const request = page.getByTestId("bsc-live-quarantine-request");
  await expect(request).toBeDisabled();
  push(liveOf(unreadable));
  await expect(request).toBeEnabled();
  await request.click();
  await expect(page.getByTestId("bsc-live-quarantine-dialog")).toContainText("Generation 5");
  await page.getByTestId("bsc-live-quarantine-confirm").click();
  // Review L1: its receipt stays once the entry is gone.
  push(liveOf({ ledgerVersion: 10n }));
  await expect(request).toHaveCount(0);
  await expect(page.getByTestId("bsc-live-quarantine-receipt")).toContainText("outcome=APPLIED bsc.live.ledger.quarantine");
  expect(sent.map(r => [r.action, r.resourceId, r.expected?.version])).toEqual([
    ["bsc.live.ledger.quarantine", "9", { case: "configVersion", value: 5n }],
  ]);
});

test("an unreadable campaign verdict is INVALID, an exact T0 leaves only the local-times notice (REV.1.0 R2b review L7)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // TEST_ONLY values: they test the display, they are no campaign.
  await feed(page, toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "BLOCKED", pathState: "NOT_CONFIGURED", generation: 5n, qualification: "NONE", warnings: ["LIVE_CAMPAIGN_VERDICT_INVALID"],
    attempts: [1n, 2n].map(ordinal => ({ ordinal, tx: `0x${ordinal.toString(16).padStart(64, "0")}`, kind: "C3_BUY", state: "INCLUDED", verdict: "UNKNOWN", window: "COMPLETE",
      t0Exact: true, windowMs: 15000n, evaluatedUnixMs: 1793000040000n + ordinal * 10000n })),
  })));
  await page.goto("/bsc");
  await liveDrawer(page, "limits");
  await expect(page.getByTestId("bsc-live-campaign-invalid")).toHaveText("INVALID");
  await expect(line(page, "Campaign verdict")).toContainText("LIVE_CAMPAIGN_VERDICT_INVALID");
  await expect(page.getByTestId("bsc-live-campaign-none")).toHaveCount(0);
  await expect(page.getByText("T0→T_buy bleibt UNKNOWN ohne Producer-Witness", { exact: false })).toHaveCount(0);
  // The local times are never T0→T_buy, whatever T0 is.
  await expect(page.getByText("Die lokalen Zeiten sind die Uhr der Runtime (Ingress→Send), nie T0→T_buy", { exact: false })).toBeVisible();
  // Without a verdict, a record of another evaluation than the newest attempt's is marked.
  const rows = page.getByTestId("bsc-live-attempts").locator("tbody tr");
  await expect(rows.first()).toContainText("of another evaluation");
  await expect(rows.last()).not.toContainText("of another evaluation");
  await expect(rows.first()).toContainText("T0 exact (recorded)");
  expect(errors).toEqual([]);
});

// TEST_ONLY records: the owner, its stored settings (generation 5) and a running strategy that started with generation 4.
const leaf = (name: string, value: string) => ({ name, value, kind: 7, unit: "" });
const record = (schema: string, fields: [string, string][]) => ({ schema, provenance: "NATIVE_CREATED", fields: fields.map(([n, v]) => leaf(n, v)) });
const strategyMonitoring = (extra: [string, string][] = []) => toBinary(BscMonitoringStateSchema, create(BscMonitoringStateSchema, { state: "NOT_EXECUTED", records: [
  record("darkbot.bsc.source-modes.v1", [["generation", "9"], ["sources[0].source_id", "BSC_INGRESS_GETH"], ["sources[0].requested", "SHADOW"], ["sources[0].effective", "SHADOW"],
    ["sources[1].source_id", "BSC_INGRESS_DRPC"], ["sources[1].requested", "OFF"], ["sources[1].effective", "OFF"]]),
  record("darkbot.bsc.strategy-settings.v1", [["generation", "5"], ["request_id", "r-5"], ["settings[0].id", "BSC_STRATEGY_C_MAX_OPEN"], ["settings[0].value", "4"]]),
  record("darkbot.bsc.runtime-status.v1", [["_provenance.observed_unix_ms", String(Date.now())], ["status_interval_ms", "60000"], ["status_stale_intervals", "3"], ["mode", "SHADOW"],
    ["strategy[0].state", "SHADOW"], ["strategy[0].settings_generation", "4"], ["strategy[0].settings[0].id", "BSC_STRATEGY_C_MAX_OPEN"], ["strategy[0].settings[0].value", "3"],
    ["strategy[0].settings[1].id", "BSC_STRATEGY_C_CREATOR_SELL_EXIT"], ["strategy[0].settings[1].value", "ENABLED"],
    ["strategy[0].gains", "2"], ["strategy[0].gain_wei", "30000000000000000"], ["strategy[0].losses", "1"], ["strategy[0].loss_wei", "10000000000000000"], ["strategy[0].launches", "37"],
    ["sources[0].source_id", "BSC_INGRESS_GETH"], ["sources[0].health", "HEALTHY"], ["sources[0].last_event_age_ms", "1200"], ["sources[0].capability", "FULL"], ...extra]),
] }));

test("Live sets one Strategy C setting at the stored generation; saved vs running stays visible (frontend package 06.11., FE-V8-06)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const sent = await commands(page);
  await feed(page, liveOf({}), strategyMonitoring());
  await page.goto("/bsc");
  await area(page, "Live");
  await expect(page.getByTestId("bsc-live-strategy-generation")).toContainText("Saved generation 5 · 4 in effect");
  await expect(page.getByTestId("bsc-live-strategy-maxOpen")).toHaveValue("4");
  await expect(page.getByTestId("bsc-live-strategy-maxOpen-running")).toHaveText("Saved · running 3 until the next start");
  await expect(page.getByLabel("Exit when the creator’s team sells")).toHaveValue("Not implemented");
  // Nothing changed: nothing to save; two changes: one at a time (no half set, BSC-V8-04).
  const save = page.getByTestId("bsc-live-strategy-save");
  await expect(save).toBeDisabled();
  await page.getByTestId("bsc-live-strategy-creatorSellExit").selectOption("DISABLED");
  await page.getByTestId("bsc-live-strategy-maxOpen").fill("5");
  await expect(save).toBeDisabled();
  await expect(page.getByTestId("bsc-live-strategy-generation")).toContainText("save one change at a time");
  await page.getByTestId("bsc-live-strategy-maxOpen").fill("4");
  await save.click();
  await expect(page.getByTestId("bsc-live-strategy-dialog")).toContainText("nächsten Start von Strategy C");
  await expect(page.getByTestId("bsc-live-strategy-request")).toHaveText("BSC_STRATEGY_C_CREATOR_SELL_EXIT=DISABLED");
  await page.getByTestId("bsc-live-strategy-confirm").click();
  await expect(page.getByTestId("bsc-live-strategy-receipt")).toContainText("outcome=APPLIED bsc.strategy.set");
  // A filter: its toggle and bound make the native rule string.
  await page.getByTestId("bsc-live-strategy-filter-buy_count").getByLabel("Buy count filter").check();
  await page.getByTestId("bsc-live-strategy-filter-buy_count").getByLabel("Buy count minimum").fill("3");
  await save.click();
  await expect(page.getByTestId("bsc-live-strategy-request")).toHaveText("BSC_STRATEGY_C_FILTERS=buy_count:3:");
  await page.getByTestId("bsc-live-strategy-confirm").click();
  await expect(page.getByTestId("bsc-live-strategy-receipt")).toContainText("outcome=APPLIED bsc.strategy.set");
  expect(sent.map(r => [r.action, r.resourceId, r.params.length, r.expected?.version])).toEqual([
    ["bsc.strategy.set", "BSC_STRATEGY_C_CREATOR_SELL_EXIT:DISABLED", 0, { case: "profileVersion", value: 5n }],
    ["bsc.strategy.set", "BSC_STRATEGY_C_FILTERS:buy_count:3:", 0, { case: "profileVersion", value: 5n }],
  ]);
  // Training shows the running strategy's own PAPER result, never a computed net.
  await area(page, "Training");
  await expect(page.getByTestId("bsc-training-native")).toContainText("+0.030");
  await expect(page.getByTestId("bsc-training-native")).toContainText("−0.010");
  // R14: the LIVE tab fits a phone's width (390 px), the settings form included.
  await area(page, "Live");
  await page.setViewportSize({ width: 390, height: 844 });
  const [scroll, width] = await page.evaluate(() => [document.documentElement.scrollWidth, window.innerWidth]);
  expect(scroll).toBeLessThanOrEqual(width + 1);
  expect(errors).toEqual([]);
});

test("the trade list joins only the lot's own attempt, shows 17 chosen columns and asks the history once per request", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const queries: string[] = [];
  await page.route("**/api/v1/bsc/query", async (r) => {
    queries.push(r.request().method());
    return r.fulfill({ status: 200, contentType: "application/x-protobuf",
      body: Buffer.from(toBinary(BscQueryResponseSchema, create(BscQueryResponseSchema, { availability: Availability.NOT_IMPLEMENTED, unavailableReason: "BSC-V8-10" }))) });
  });
  await feed(page, liveOf({
    positions: [{ ...lotRow(hash(0xa1), "CLOSED", []), pnlWei: "0" }, lotRow(hash(0xa2), "OPEN", []), { ...lotRow(hash(0xa3), "CLOSED", []), pnlWei: "-5", proceedsUnknown: true }],
    attempts: [{ ordinal: 1n, tx: hash(0xa2), trigger: "BSC_INGRESS_GETH", ingressToSendNs: 420_000n, sendToAckNs: 610_000n, placed: "FIRST", blockMs: 1793000000000n }],
  }));
  await page.goto("/bsc");
  await area(page, "Live");
  const rows = page.getByTestId("bsc-trade-row");
  await expect(rows).toHaveCount(3);
  // Active first; a lower-bound loss (proceeds unknown) is no LOSS and no WIN; 0 PnL is no WIN.
  await expect(rows.nth(0)).toContainText("RUN");
  await expect(rows.nth(0)).toContainText("Geth (local)");
  await expect(rows.nth(0)).toContainText("0.42 ms");
  await expect(rows.nth(1)).toContainText("UNKN");
  await expect(rows.nth(2)).toContainText("DONE");
  await expect(rows.nth(2)).not.toContainText("Geth");
  // Columns: 17 to choose, token always; the choice is a browser preference.
  await page.getByTestId("bsc-trade-columns").click();
  await expect(page.locator("[data-testid^=bsc-col-]")).toHaveCount(17);
  await expect(page.getByTestId("bsc-col-token")).toBeDisabled();
  await page.getByTestId("bsc-col-ack").check();
  await page.getByTestId("bsc-trade-columns-apply").click();
  await expect(page.locator(".trade-flow-table thead th").filter({ hasText: /^Ack$/ })).toHaveCount(1);
  await expect(rows.nth(0)).toContainText("0.61 ms");
  // The history: no query until asked, then one; the owner's NOT_IMPLEMENTED is shown as such.
  expect(queries).toEqual([]);
  await page.getByTestId("bsc-live-trades").getByPlaceholder("Find token or transaction").fill("0xa");
  expect(queries).toEqual([]);
  await page.getByTestId("bsc-trade-history-more").click();
  await expect(page.getByTestId("bsc-live-trades")).toContainText("Not implemented yet");
  expect(queries).toEqual(["POST"]);
  expect(errors).toEqual([]);
});

test("Health shows each native state, a disabled source grey and never a missing one green", async ({ page }) => {
  await feed(page, liveOf({ grantAdapters: ["CLUB48_BUILDER_V1"] }), strategyMonitoring([["canonical[0].state", "STALLED"], ["canonical[0].head", "100"], ["canonical[0].head_age_ms", "9000"]]));
  await page.goto("/bsc");
  await page.getByTestId("bsc-health-pill").click();
  await expect(page.getByTestId("bsc-health-BSC_INGRESS_GETH")).toHaveAttribute("data-state", "ok");
  await expect(page.getByTestId("bsc-health-BSC_INGRESS_DRPC")).toHaveAttribute("data-state", "off");
  await expect(page.getByTestId("bsc-health-chain")).toHaveAttribute("data-state", "warn");
  await expect(page.getByTestId("bsc-health-confirm")).toHaveAttribute("data-state", "off");
  // The golden builder evidence is unqualified: never green.
  await expect(page.getByTestId("bsc-health-CLUB48_BUILDER_V1")).not.toHaveAttribute("data-state", "ok");
  await expect(page.getByTestId("bsc-health-pill")).toContainText("alert");
});
