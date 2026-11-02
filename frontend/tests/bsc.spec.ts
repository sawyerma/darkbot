import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";
import { readFileSync } from "node:fs";
import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import { BscLiveStateSchema } from "../src/core/generated/darkbot/bsc/v1/status_pb";
import { AckStatus, ClientMessageSchema, ServerMessageSchema, StateQuality, WelcomeStatus } from "../src/core/generated/darkbot/control/v1/stream_pb";
import { CommandLookupResponseSchema, CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus, type CommandRequest } from "../src/core/generated/darkbot/control/v1/commands_pb";
import { ProjectId, TopicClass, TopicId } from "../src/core/generated/darkbot/control/v1/topics_pb";

// The BSC page in a production build (`npm run test:bsc`, CI workflow `bsc`): its six tabs, the one
// LIVE source and fail-closed controls. The static fixture server has no proxy (/api and /ws answer
// 503); the gateway is a stand-in in page routes serving the reporter's golden projections
// (protocol/golden/bsc). No request reaches a real gateway.
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

/** The gateway stand-in: every BSC STATE topic with its golden payload (`live` replaced). Returns
 *  a push of the next LIVE state (a new full state, as the reporter republishes it). */
async function feed(page: Page, live: Uint8Array = golden("live")): Promise<(next: Uint8Array) => void> {
  let socket: WebSocketRoute;
  let seq = 1n;
  const payloads = new Map<TopicId, Uint8Array>(STATE_TOPICS.map(([t, , , name]) => [t, t === TopicId.BSC_LIVE_STATE ? live : golden(name)]));
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
  // REV.1.0 R2b: no campaign verdict is no evaluation, never a value.
  await expect(page.getByTestId("bsc-live-campaign-none")).toHaveText("keine Bewertung");
  await expect(page.getByTestId("bsc-live-attempts")).toHaveCount(0);
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
  await tab(page, "LIVE").click();
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
  const live = toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "BLOCKED", pathState: "NOT_CONFIGURED", generation: 5n, qualification: "NONE", warnings: [beyond],
  }));
  await feed(page, live);
  await page.goto("/bsc");
  await tab(page, "LIVE").click();
  await expect(page.getByTestId("bsc-live-campaign-invalid")).toHaveText("INVALID");
  await expect(line(page, "Campaign verdict")).toContainText(beyond);
  await expect(page.getByTestId("bsc-live-campaign-none")).toHaveCount(0);
  await expect(page.getByTestId("bsc-live-attempts-beyond")).toHaveText("INVALID");
  await expect(line(page, "Attempts")).toContainText(beyond);
  await expect(line(page, "Attempts")).not.toContainText("no attempt record");
  expect(errors).toEqual([]);
});

test("the LIVE tab names the undecided and stranded lots, the receipt recovery and each transaction's source (REV.1.0 R1/R3b)", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  // TEST_ONLY values: they test the display, they are no LIVE state.
  const tx = (n: number) => `0x${n.toString(16).padStart(64, "0")}`;
  const live = toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "BLOCKED", pathState: "ARMED", generation: 5n, qualification: "NONE",
    exitUndecided: ["0x" + "e1".repeat(20)], stranded: ["0x" + "e2".repeat(20) + " LIVE_POSITION_OFF_CURVE"], lotReadFaults: 3n, lotReadFault: "curve fault",
    recoveryPending: [`${tx(0xc1)} RECOVERY_NOT_FINAL`], recovered: 2n, recoveryFaults: 1n, recoveryFault: "recovery fault",
    transactions: [
      { txHash: tx(0xa1), intent: "BUY", token: "0x" + "aa".repeat(20), nonce: 1n, state: "INCLUDED", blockNumber: 90n, source: "RECEIPT" },
      { txHash: tx(0xb5), intent: "BUY", token: "0x" + "aa".repeat(20), nonce: 2n, state: "REPLACED", blockNumber: 91n, source: "" },
    ],
  }));
  await feed(page, live);
  await page.goto("/bsc");
  await tab(page, "LIVE").click();
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

test("the LIVE tab's operator decisions go at the state their dialog opened on (REV.1.0 R3b, Audit A)", async ({ page }) => {
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
  await tab(page, "LIVE").click();
  await expect(line(page, "Operator decisions")).toContainText("ledger version 12 · exit-now locked · nonce burn ON");
  // R4: the campaign's spend and its budget (0: no BUY), never made up.
  await expect(line(page, "Campaign spent")).toContainText(/1000 wei of .*· 0 wei/);
  // The ledger's decisions of the open lot; exit-now locked (its switch OFF); a closed lot none.
  const lots = page.getByTestId("bsc-live-positions").locator("tr");
  const exitNow = lots.nth(0).getByTestId("bsc-live-lot-EXIT_NOW");
  await expect(exitNow).toBeDisabled();
  await expect(exitNow).toHaveAttribute("title", "gesperrt: BSC_EXEC_LIVE_OPERATOR_EXIT ist OFF");
  await expect(lots.nth(0).getByTestId("bsc-live-lot-RESUME")).toBeEnabled();
  await expect(lots.nth(1).getByRole("button")).toHaveCount(0);
  // Review H1: the dialog opened at 12 stays at 12 when a newer state arrives; it names it.
  await lots.nth(0).getByTestId("bsc-live-lot-WRITE_OFF").click();
  await expect(page.getByTestId("bsc-live-lot-dialog")).toContainText("Ledger-Version 12");
  push(liveOf(state(13n)));
  await expect(page.getByTestId("bsc-live-lot-moved")).toContainText("Version 13");
  await page.getByTestId("bsc-live-lot-confirm").click();
  await expect(page.getByTestId("bsc-live-lot-receipt")).toContainText("outcome=APPLIED bsc.live.lot");
  // The burnable transaction; the one requested is named and offers nothing again; others none.
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

test("the LIVE tab's operator decisions stay locked without their switch ON or the ledger's version (REV.1.0 R3b review M2)", async ({ page }) => {
  const rows: LiveInit = {
    positions: [lotRow(hash(0xa1), "OPEN", ["RESUME", "WRITE_OFF", "EXIT_NOW"])],
    transactions: [txRow(hash(0xb5), 4n, "UNKNOWN", true)],
  };
  const push = await feed(page, liveOf({ ...rows, operatorExit: true, nonceBurn: true }));
  await page.goto("/bsc");
  await tab(page, "LIVE").click();
  // No version (the ledger unreadable): nothing goes.
  const lot = page.getByTestId("bsc-live-positions").locator("tr").nth(0);
  const burn = page.getByTestId("bsc-live-transactions").locator("tr").nth(0).getByTestId("bsc-live-burn-request");
  for (const d of ["RESUME", "WRITE_OFF", "EXIT_NOW"]) await expect(lot.getByTestId(`bsc-live-lot-${d}`)).toBeDisabled();
  await expect(burn).toBeDisabled();
  // At a version, exit-now ON: every decision; the burn with its switch OFF, then unknown: locked.
  push(liveOf({ ...rows, ledgerVersion: 3n, operatorExit: true, nonceBurn: false }));
  for (const d of ["RESUME", "WRITE_OFF", "EXIT_NOW"]) await expect(lot.getByTestId(`bsc-live-lot-${d}`)).toBeEnabled();
  await expect(burn).toBeDisabled();
  await expect(burn).toHaveAttribute("title", "gesperrt: BSC_EXEC_LIVE_NONCE_BURN ist OFF");
  push(liveOf({ ...rows, ledgerVersion: 3n }));
  await expect(lot.getByTestId("bsc-live-lot-EXIT_NOW")).toBeDisabled();
  await expect(burn).toBeDisabled();
  await expect(burn).toHaveAttribute("title", /unbekannt/);
  // ON: the burn goes.
  push(liveOf({ ...rows, ledgerVersion: 3n, nonceBurn: true }));
  await expect(burn).toBeEnabled();
});

test("an unreadable LIVE ledger offers its lowest entry's quarantine at the owner generation, only with owner state (REV.1.0 R3b)", async ({ page }) => {
  const sent = await commands(page);
  const unreadable: LiveInit = { firstUnreadable: 9n, warnings: ["LIVE_LEDGER_INVALID 2 unreadable, first seq 9"] };
  const push = await feed(page, liveOf({ ...unreadable, gate: "UNKNOWN" }));
  await page.goto("/bsc");
  await tab(page, "LIVE").click();
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
  const live = toBinary(BscLiveStateSchema, create(BscLiveStateSchema, {
    gate: "BLOCKED", pathState: "NOT_CONFIGURED", generation: 5n, qualification: "NONE", warnings: ["LIVE_CAMPAIGN_VERDICT_INVALID"],
    attempts: [1n, 2n].map(ordinal => ({ ordinal, tx: `0x${ordinal.toString(16).padStart(64, "0")}`, kind: "C3_BUY", state: "INCLUDED", verdict: "UNKNOWN", window: "COMPLETE",
      t0Exact: true, windowMs: 15000n, evaluatedUnixMs: 1793000040000n + ordinal * 10000n })),
  }));
  await feed(page, live);
  await page.goto("/bsc");
  await tab(page, "LIVE").click();
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
