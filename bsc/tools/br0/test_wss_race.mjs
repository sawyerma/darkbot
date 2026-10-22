// Tests für wss_race.mjs. Ausführen: npm i ws@8.21.3 && node --test test_wss_race.mjs
// `ws` wird NUR für die Test-Server gebraucht; das Messwerkzeug selbst hat keine Abhängigkeiten.

import test from "node:test";
import assert from "node:assert/strict";
import { WebSocketServer } from "ws";
import { analyze, parseSources, percentile, toCsv, runLive, VENUES } from "./wss_race.mjs";

const MS = (x) => BigInt(Math.round(x * 1e6));
const FOUR = "0x5c952063c7fc8610ffdb798152d69f0b9550762b";
const FOUR_TOPIC = VENUES[FOUR].topic;

test("Auswertung: Vorsprung, Erstsichtung, privater Launch, Ausfall, zu spät, keine Pending-Subscription", () => {
  const launch = (block, pend, log) => ({ venue: "four_classic", block, logNs: new Map(Object.entries(log)), pendingNs: new Map(Object.entries(pend)) });
  const state = {
    names: ["a", "b", "c"], mode: { a: "FULL", b: "HASH", c: "NONE" }, windowMs: 3000,
    heads: new Map([[10, new Map([["a", MS(400)], ["b", MS(420)]])], [20, new Map([["b", MS(5400)]])], [30, new Map([["a", MS(9000)]])]]),
    launches: new Map([
      ["0x01", launch(10, { a: MS(100), b: MS(150) }, { a: MS(410) })],   // a 300 ms, b 250 ms vor dem Block
      ["0x02", launch(20, {}, { b: MS(5410) })],                          // privat: niemand
      ["0x03", launch(30, { b: MS(9100) }, { a: MS(9010) })],             // a ausgefallen, b zu spät
    ]),
    down: { a: [[MS(8000), MS(9500)]], b: [], c: [] },
  };
  const res = analyze(state);
  const row = (tx) => res.rows.find((r) => r.tx === tx);
  assert.equal(row("0x01").per.a.status, "SEEN"); assert.equal(row("0x01").per.a.lead, 300);
  assert.equal(row("0x01").per.b.status, "SEEN"); assert.equal(row("0x01").per.b.lead, 250);
  assert.equal(row("0x01").per.c.status, "NO_PENDING");
  assert.equal(row("0x01").first, "a");
  assert.equal(row("0x02").per.a.status, "NOT_SEEN"); assert.equal(row("0x02").first, "");
  assert.equal(row("0x03").per.a.status, "DOWN");
  assert.equal(row("0x03").per.b.status, "LATE"); assert.equal(row("0x03").per.b.lead, -100);
  assert.equal(res.total, 3); assert.equal(res.anySeen, 1);
  assert.equal(res.summary.a.usable, 2, "DOWN zählt nicht in die Abdeckung");
  assert.equal(res.summary.a.seen, 1);
  assert.equal(res.summary.b.usable, 3); assert.equal(res.summary.b.late, 1);
  assert.equal(res.summary.b.behind_best_p50, 50);
  assert.equal(res.summary.c.usable, 0);
  const csv = toCsv(res, state.names).split("\n");
  assert.equal(csv[0], "launch_tx,venue,block,block_ref,first_source,a_status,a_lead_ms,b_status,b_lead_ms,c_status,c_lead_ms");
  assert.equal(csv[1], "0x01,four_classic,10,HEAD,a,SEEN,300.000,SEEN,250.000,NO_PENDING,");
});

test("Auswertung: ohne Head dient die früheste Log-Ankunft als Blockreferenz", () => {
  const res = analyze({ names: ["a"], mode: { a: "FULL" }, windowMs: 3000, heads: new Map(), down: { a: [] },
    launches: new Map([["0x09", { venue: "flap", block: 5, logNs: new Map([["a", MS(700)]]), pendingNs: new Map([["a", MS(200)]]) }]]) });
  assert.equal(res.rows[0].block_ref, "LOG"); assert.equal(res.rows[0].per.a.lead, 500);
});

test("Konfiguration und Hilfsfunktionen", () => {
  assert.deepEqual(parseSources("x=wss://a.example,y=ws://127.0.0.1:1"), [{ name: "x", url: "wss://a.example" }, { name: "y", url: "ws://127.0.0.1:1" }]);
  assert.throws(() => parseSources(""), /SOURCES fehlt/);
  assert.throws(() => parseSources("x=https://a"), /ungültiger Eintrag/);
  assert.throws(() => parseSources("x=wss://a,x=wss://b"), /doppelt/);
  assert.equal(percentile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 50), 5);
  assert.equal(percentile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90), 9);
  assert.equal(percentile([], 50), null);
});

// ---------- End-to-End gegen zwei lokale WebSocket-Server ----------
function mockSource({ fullAllowed }) {
  const wss = new WebSocketServer({ port: 0, host: "127.0.0.1" });
  const clients = new Set();
  wss.on("connection", (ws) => {
    const subs = {};
    ws.on("message", (raw) => {
      const m = JSON.parse(raw.toString());
      const [kind, arg] = m.params;
      if (kind === "newPendingTransactions" && arg === true && !fullAllowed) {
        ws.send(JSON.stringify({ jsonrpc: "2.0", id: m.id, error: { code: -32602, message: "full tx not supported" } })); return;
      }
      const sid = "0x" + Math.random().toString(16).slice(2);
      subs[kind === "newPendingTransactions" ? "pending" : kind === "newHeads" ? "heads" : "logs"] = sid;
      ws.send(JSON.stringify({ jsonrpc: "2.0", id: m.id, result: sid }));
    });
    clients.add({ ws, subs });
  });
  const emit = (kind, result) => {
    for (const c of clients) if (c.subs[kind]) c.ws.send(JSON.stringify({ jsonrpc: "2.0", method: "eth_subscription", params: { subscription: c.subs[kind], result } }));
  };
  return { wss, emit, url: () => `ws://127.0.0.1:${wss.address().port}`, close: () => new Promise((r) => { for (const c of clients) c.ws.terminate(); wss.close(r); }) };
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

test("E2E: echte WebSocket-Verbindungen — Voll- vs. Hash-Modus, Vorsprung, privater Launch, Schutz vor Löschung", async () => {
  const A = mockSource({ fullAllowed: true }), B = mockSource({ fullAllowed: false });
  await Promise.all([new Promise((r) => A.wss.on("listening", r)), new Promise((r) => B.wss.on("listening", r))]);
  const files = {};
  const done = runLive({ SOURCES: `a=${A.url()},b=${B.url()}`, DURATION_S: "3", PENDING_TTL_S: "1", PRUNE_INTERVAL_MS: "100", DOWN_WINDOW_MS: "200", OUT_PREFIX: "t" },
    (f, c) => { files[f] = c; }, () => {});
  await sleep(400); // Verbindungen und Subscriptions stehen
  const TX1 = "0x" + "11".repeat(32), TX2 = "0x" + "22".repeat(32);
  A.emit("pending", { hash: TX1, to: FOUR });       // A zuerst, volles Objekt
  await sleep(80);
  B.emit("pending", TX1);                           // B 80 ms später, nur Hash
  await sleep(250);
  A.emit("heads", { number: "0x64" }); B.emit("heads", { number: "0x64" });
  const log = (tx, block) => ({ address: FOUR, topics: [FOUR_TOPIC], transactionHash: tx, blockNumber: block, removed: false });
  A.emit("logs", log(TX1, "0x64")); B.emit("logs", log(TX1, "0x64"));
  await sleep(100);
  A.emit("heads", { number: "0x65" }); A.emit("logs", log(TX2, "0x65"));  // privater Launch: nie Pending
  await done; // läuft bis 3 s; Pending-TTL 1 s mit Löschung alle 100 ms
  await A.close(); await B.close();
  const rows = files["t_launches.csv"].trim().split("\n").map((l) => l.split(","));
  const [head, ...data] = rows;
  const get = (tx, col) => data.find((r) => r[0] === tx)[head.indexOf(col)];
  assert.equal(get(TX1, "a_status"), "SEEN", "trotz Löschung nach 1 s noch vorhanden");
  assert.equal(get(TX1, "b_status"), "SEEN");
  const la = Number(get(TX1, "a_lead_ms")), lb = Number(get(TX1, "b_lead_ms"));
  assert.ok(la > lb && lb > 0, `A vor B, beide vor dem Block (a=${la}, b=${lb})`);
  assert.ok(la - lb > 50 && la - lb < 200, `Abstand ≈ 80 ms (gemessen ${(la - lb).toFixed(1)})`);
  assert.equal(get(TX1, "first_source"), "a");
  assert.equal(get(TX2, "a_status"), "NOT_SEEN"); assert.equal(get(TX2, "first_source"), "");
  const sum = files["t_summary.txt"];
  assert.match(sum, /\[a\] Modus FULL · Pending aktiv/); assert.match(sum, /\[b\] Modus HASH/);
  assert.match(sum, /Von KEINER Quelle vorher gesehen: 1 \(50\.0 %\)/);
});

test("E2E: Anbieter nimmt den Vollmodus an, liefert aber nur Hashes -> Modus HASH (wie PublicNode am 2026-10-22)", async () => {
  const C = mockSource({ fullAllowed: true });
  await new Promise((r) => C.wss.on("listening", r));
  const files = {};
  const done = runLive({ SOURCES: `c=${C.url()}`, DURATION_S: "1.5", DOWN_WINDOW_MS: "200", OUT_PREFIX: "h" }, (f, c) => { files[f] = c; }, () => {});
  await sleep(400);
  C.emit("pending", "0x" + "33".repeat(32));
  C.emit("pending", "0x" + "44".repeat(32));
  await done; await C.close();
  assert.match(files["h_summary.txt"], /\[c\] Modus HASH/);
  assert.match(files["h_summary.txt"], /Pending-Nachrichten 2/);
});

test("E2E: Ratenlimit mit Verbindungsabbruch -> Wartezeit wächst, kein Dauerfeuer (Befund dRPC 2026-10-22: 94 Versuche/120 s)", async () => {
  const wss = new WebSocketServer({ port: 0, host: "127.0.0.1" });
  await new Promise((r) => wss.on("listening", r));
  let connections = 0;
  wss.on("connection", (ws) => {
    connections++;
    ws.on("message", (raw) => {
      const m = JSON.parse(raw.toString());
      ws.send(JSON.stringify({ jsonrpc: "2.0", id: m.id, error: { code: 15, message: "You reached Public endpoint rate limit" } }));
      setTimeout(() => ws.close(), 10);
    });
  });
  const files = {};
  await runLive({ SOURCES: `d=ws://127.0.0.1:${wss.address().port}`, DURATION_S: "4", OUT_PREFIX: "r" }, (f, c) => { files[f] = c; }, () => {});
  for (const c of wss.clients) c.terminate();
  await new Promise((r) => wss.close(r));
  assert.ok(connections <= 4, `höchstens 4 Verbindungen in 4 s mit wachsender Wartezeit (gemessen ${connections})`);
  assert.match(files["r_summary.txt"], /Ratenlimit [1-9]/);
});
