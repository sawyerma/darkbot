// Tests für br0_observe_launches.mjs (Version 4). Ausführen im Paketordner: node --test test_br0.mjs
// Eigenständig: Die kanonischen Fixtures liegen in ./fixtures als unveränderte Kopien; ihre SHA-256-Werte und der
// Originalpfad im Repository stehen in ./fixtures/SHA256SUMS und werden vor jeder Nutzung geprüft.
// Alternativ FIXTURE_DIR=<Pfad im Repository> setzen, um direkt gegen die Originale zu testen.
//
// Echte Daten: kanonischer Launch (Tx 0x8cc24c0c…7e5b, Block 124415333, Index 43) und realer TokenPurchase-Log aus dem
// QuickNode-Guide. Synthetisch nach verifiziertem Layout: Käufe, Gegenfälle, Fehlerzustände, Modus-Antworten.

import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { decodePurchase, decodeTokenCreate, decodeMode, purchasesOf, isComplete, run, PROFILE, COLS } from "./br0_observe_launches.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = process.env.FIXTURE_DIR ?? path.join(HERE, "fixtures");
const SUMS = Object.fromEntries(fs.readFileSync(path.join(HERE, "fixtures", "SHA256SUMS"), "utf8").split("\n")
  .filter((l) => l && !l.startsWith("#")).map((l) => { const [h, f] = l.trim().split(/\s+/); return [f, h]; }));
const readFixture = (f) => {
  const buf = fs.readFileSync(path.join(FIXTURE_DIR, f));
  assert.equal(crypto.createHash("sha256").update(buf).digest("hex"), SUMS[f], `Fixture ${f} weicht vom Repository-Original ab`);
  return buf.toString("utf8");
};
const RECEIPT_RAW = JSON.parse(readFixture("four_create_receipt.json"));
const RECEIPT = RECEIPT_RAW.result ?? RECEIPT_RAW;
const TX = JSON.parse(readFixture("four_create_transaction.json")).tx;
const DECODED_TEXT = readFixture("four_token_create_decoded.json");
const DECODED = JSON.parse(DECODED_TEXT);
const DECODED_SUPPLY = BigInt(DECODED_TEXT.match(/"totalSupply":\s*(\d+)/)[1]); // JSON.parse verliert bei 10^27 Genauigkeit

const TM = PROFILE.tokenManager, T_CREATE = PROFILE.topicTokenCreate, T_BUY = PROFILE.topicTokenPurchase, T_XFER = PROFILE.topicTransfer;
const pad = (x) => x.replace(/^0x/, "").toLowerCase().padStart(64, "0");
const word = (v) => BigInt(v).toString(16).padStart(64, "0");
const topic = (a) => "0x" + pad(a);
const A = (c) => "0x" + c.repeat(40);
const ZERO = "0x" + "0".repeat(40);
const B48 = "0x4848489f0b2bedd788c696e2d79b6b69d7484848";
const TOKEN = DECODED.token.toLowerCase(), CREATOR = DECODED.creator.toLowerCase();
const encMode = ({ quote = ZERO, template = 0n, feeSetting = 0n }) => ({
  infos: "0x" + pad(TOKEN) + pad(quote) + word(template) + word(0).repeat(10),
  ex1s: "0x" + word(0) + word(0) + word(feeSetting) + word(0) + word(0),
});

// ---------- Reine Funktionen gegen echte Daten ----------
test("Fixtures sind byte-identisch mit den Repository-Originalen (SHA256SUMS)", () => {
  assert.equal(Object.keys(SUMS).length, 3);
});

test("TokenPurchase: realer Log aus dem QuickNode-Guide wird exakt wie dort veröffentlicht dekodiert", () => {
  const p = decodePurchase({ address: TM, topics: [T_BUY], data: "0x00000000000000000000000076138888158f7ce4bbe14c59e18e880d57ab44440000000000000000000000004262f7b70b81538258eb2c5ecad3947ba4e0c8b0000000000000000000000000000000000000000000000000000000029c13f53700000000000000000000000000000000000000000003a339d3d41d9ad2a4520000000000000000000000000000000000000000000000000000ae1e3ecafdee3e0000000000000000000000000000000000000000000000000001bdbe1163d1be000000000000000000000000000000000000000001991941ab911ad2ead4940000000000000000000000000000000000000000000000000021ff7f07b91232ea" });
  assert.equal(p.token, "0x76138888158f7ce4bbe14c59e18e880d57ab4444");
  assert.equal(p.account, "0x4262f7b70b81538258eb2c5ecad3947ba4e0c8b0");
  assert.equal(p.price, 11208488247n);
  assert.equal(p.amount, 4397589923821429000000000n);
  assert.equal(p.cost, 49009900990099006n);
  assert.equal(p.fee, 490099009900990n);
  assert.equal(p.offers, 494569930785511810000000000n);
  assert.equal(p.funds, 2449816393459315434n);
});

test("TokenCreate: kanonischer Launch-Receipt wird wie die Repo-Evidence dekodiert", () => {
  const log = RECEIPT.logs.find((l) => l.topics[0].toLowerCase() === T_CREATE);
  assert.equal(Number(log.logIndex), DECODED.logIndex);
  const tc = decodeTokenCreate(log);
  assert.equal(tc.token, TOKEN);
  assert.equal(tc.creator, CREATOR);
  assert.equal(tc.requestId, BigInt(DECODED.requestId));
  assert.equal(tc.totalSupply, DECODED_SUPPLY);
  assert.equal(tc.totalSupply, 10n ** 27n);
});

test("Kauf-Definition: Transfer ohne TokenPurchase ist kein Kauf; fehlgeschlagene TX und fremder Token zählen nicht", () => {
  assert.deepEqual(purchasesOf({ status: "0x1", logs: [{ address: TOKEN, topics: [T_XFER, topic(TM), topic(A("e"))], data: "0x" + word(1) }] }, TOKEN), []);
  const ev = { address: TM, topics: [T_BUY], data: "0x" + pad(TOKEN) + pad(A("e")) + word(1).repeat(6) };
  assert.deepEqual(purchasesOf({ status: "0x0", logs: [ev] }, TOKEN), []);
  assert.deepEqual(purchasesOf({ status: "0x1", logs: [ev] }, A("f")), []);
});

test("Falsch formatierte Events werden abgelehnt", () => {
  assert.throws(() => decodePurchase({ address: TM, topics: [T_BUY], data: "0x" + word(1).repeat(7) }), /8 Wörter/);
  assert.throws(() => decodePurchase({ address: A("1"), topics: [T_BUY], data: "0x" + word(1).repeat(8) }), /kein TokenPurchase/);
  assert.throws(() => decodePurchase({ address: TM, topics: [T_BUY], data: "0x" + "f".repeat(64) + word(1).repeat(7) }), /keine Adresse/);
});

test("Auflage 3 — Modus: Quote-Asset, AntiSniper, TaxToken, X Mode und Längenprüfung", () => {
  const bnb = encMode({}); const m1 = decodeMode(bnb.infos, bnb.ex1s);
  assert.equal(m1.quote, ZERO); assert.equal(m1.feeCompleteness, "EVENT_FEE_NO_KNOWN_EXTRA");
  const as = encMode({ quote: A("7"), feeSetting: 3n }); const m2 = decodeMode(as.infos, as.ex1s);
  assert.equal(m2.quote, A("7")); assert.equal(m2.antiSniper, true); assert.equal(m2.feeCompleteness, "UNKNOWN_ANTISNIPER");
  const tax = encMode({ template: 5n << 10n }); assert.equal(decodeMode(tax.infos, tax.ex1s).feeCompleteness, "UNKNOWN_TAXTOKEN");
  const x = encMode({ template: 0x10000n }); const m4 = decodeMode(x.infos, x.ex1s);
  assert.equal(m4.xMode, true); assert.equal(m4.taxToken, false);
  assert.throws(() => decodeMode("0x" + word(0).repeat(12), bnb.ex1s), /13 Wörter/);
  assert.throws(() => decodeMode(bnb.infos, "0x" + word(0).repeat(4)), /5 Wörter/);
});

test("Auflage 1 — Vollständigkeit: jeder Nicht-OK-Status, jede Lücke und jede Snapshot-Abweichung verhindern Exit 0", () => {
  const ok = { checked: 1, changed: 0, errors: 0 };
  assert.equal(isComplete({ gaps: 0, byStatus: { OK: 5 }, recheck: ok }), true);
  for (const s of ["DECODE_FAILED", "TOKEN_MISMATCH", "LAUNCH_FAILED", "BINDING_FAILED", "CHAIN_BREAK", "IRGENDEIN_NEUER_STATUS"]) {
    assert.equal(isComplete({ gaps: 0, byStatus: { OK: 5, [s]: 1 }, recheck: ok }), false, s);
  }
  assert.equal(isComplete({ gaps: 1, byStatus: { OK: 5 }, recheck: ok }), false);
  assert.equal(isComplete({ gaps: 0, byStatus: { OK: 5 }, recheck: { ...ok, changed: 1 } }), false);
  assert.equal(isComplete({ gaps: 0, byStatus: { OK: 5 }, recheck: { ...ok, errors: 1 } }), false);
});

// ---------- End-to-End gegen Mock-RPC ----------
let h = 0;
const H = () => "0x" + (++h).toString(16).padStart(64, "f");
const purchase = (account, { token = TOKEN, transfer = true, words7 = false } = {}) => {
  const data = "0x" + pad(token) + pad(account) + word(10) + word(500) + word(1000) + word(10) + word(1) + (words7 ? "" : word(1));
  const logs = [{ address: TM, topics: [T_BUY], data }];
  if (transfer) logs.push({ address: token, topics: [T_XFER, topic(TM), topic(account)], data: "0x" + word(500) });
  return logs;
};
const tx = (from, to, value, logs, status = "0x1") => ({ hash: H(), from, to, value: "0x" + BigInt(value).toString(16), gasPrice: "0x3b9aca00", logs, status });
const createLog = (creator, token, extra = "") => ({ address: TM, topics: [T_CREATE],
  data: "0x" + pad(creator) + pad(token) + word(7) + word(256) + word(320) + word(1000) + word(0) + word(0) + word(1) + "41".padEnd(64, "0") + word(1) + "42".padEnd(64, "0") + extra });
const mint = (token) => ({ address: token, topics: [T_XFER, topic(ZERO), topic(A("b"))], data: "0x" + word(1) });
const synthLaunch = (creator, token, { mintToken = token, extra = "", status = "0x1" } = {}) =>
  tx(creator, TM, 0, [mint(mintToken), createLog(creator, token, extra)], status);

const L = 124415333;
const T2 = A("a"), QUOTE = A("7"), C2 = A("2"), F1 = A("3"), F2 = A("4"), F3 = A("5"), F4 = A("6"), ROUTER = A("d"), BOTC = A("c");
const TOK_TAX = "0x" + "1a".repeat(20), TOK_UNREAD = "0x" + "1b".repeat(20), TOK_BADBUY = "0x" + "1c".repeat(20);

function buildChain() {
  const blocks = new Map();
  const put = (n, txs) => blocks.set(n, { n, hash: n === L ? RECEIPT.blockHash : H(), txs });
  const pre = Array.from({ length: 43 }, () => tx(A("8"), A("7"), 0, []));
  const launch = { hash: TX.hash, from: TX.from, to: TX.to, value: TX.value, gasPrice: TX.gasPrice, logs: RECEIPT.logs, status: RECEIPT.status };
  put(L, [...pre, launch,
    tx(F1, ROUTER, 10, purchase(F1)),                                   // 44: N+1 fremder Kauf über Router
    tx(F1, B48, 1000, []),                                              // 45: benachbarter Transfer an Builder (Zuordnung unbewiesen)
    tx(F2, TM, 10, [{ address: TOKEN, topics: [T_XFER, topic(TM), topic(F2)], data: "0x" + word(5) }]), // 46: nur Transfer
    tx(CREATOR, TM, 10, purchase(CREATOR)),                             // 47: Dev-Kauf
    tx(F3, BOTC, 10, purchase(BOTC)),                                   // 48: Käufer ist Kontrakt
    tx(F4, TM, 10, purchase(F4, { transfer: false })),                  // 49: Kauf ohne Transfer
    tx(B48, A("9"), 1, []),                                             // letzte TX: Builder-Zahlung
  ]);
  for (let n = L + 1; n <= L + 120; n++) if (!blocks.has(n)) put(n, [tx(A("8"), A("7"), 0, [])]);
  put(L + 10, [synthLaunch(C2, T2)]);
  put(L + 11, [tx(F2, TM, 10, purchase(F2, { token: T2 }))]);
  put(L + 20, [synthLaunch(C2, A("1"))]);                                  // Kettenbruch im Folgeblock
  put(L + 30, [synthLaunch(C2, A("e"))]);                                  // Receipt an falschen Block gebunden
  put(L + 40, [synthLaunch(C2, "0x" + "40".repeat(20), { extra: "00" })]); // beschädigtes TokenCreate (Länge)
  put(L + 50, [synthLaunch(C2, "0x" + "50".repeat(20), { mintToken: "0x" + "51".repeat(20) })]); // Token-Mismatch
  put(L + 60, [synthLaunch(C2, "0x" + "60".repeat(20), { status: "0x0" })]); // Launch fehlgeschlagen trotz Log
  put(L + 70, [synthLaunch(C2, TOK_BADBUY), tx(F1, TM, 10, purchase(F1, { token: TOK_BADBUY, words7: true }))]); // kaputtes Kauf-Event
  put(L + 80, [synthLaunch(C2, TOK_TAX), tx(F1, TM, 10, purchase(F1, { token: TOK_TAX }))]);
  put(L + 90, [synthLaunch(C2, TOK_UNREAD), tx(F1, TM, 10, purchase(F1, { token: TOK_UNREAD }))]);
  const sorted = [...blocks.keys()].sort((a, b) => a - b);
  for (let i = 1; i < sorted.length; i++) blocks.get(sorted[i]).parent = blocks.get(sorted[i - 1]).hash;
  blocks.get(L + 21).parent = "0x" + "ab".repeat(32);
  blocks.get(L + 30).badReceiptHash = true;
  const modes = {
    [TOKEN]: { ...encMode({}) },
    [T2]: { ...encMode({ quote: QUOTE, feeSetting: 3n }), failAtBlock: true },
    [TOK_TAX]: { ...encMode({ template: 5n << 10n }) },
    [TOK_UNREAD]: { failAll: true },
  };
  return { blocks, modes };
}

function serve(chain, { failLogsAt = null, changeOnRecheck = null } = {}) {
  const hx = (n) => "0x" + n.toString(16);
  const blockObj = (b, hydrated) => ({ number: hx(b.n), hash: !hydrated && b.n === changeOnRecheck ? "0x" + "99".repeat(32) : b.hash,
    parentHash: b.parent ?? "0x" + "0".repeat(64), miner: A("9"),
    transactions: b.txs.map((t, i) => ({ hash: t.hash, from: t.from, to: t.to, value: t.value, gasPrice: t.gasPrice, transactionIndex: hx(i) })) });
  const rcs = (b) => b.txs.map((t, i) => ({ transactionHash: t.hash, status: t.status, logs: t.logs.map((l, li) => ({ ...l, logIndex: l.logIndex ?? hx(li) })),
    blockHash: b.badReceiptHash ? "0x" + "cd".repeat(32) : b.hash, blockNumber: hx(b.n), transactionIndex: hx(i), effectiveGasPrice: t.gasPrice, gasUsed: "0x5208" }));
  const server = http.createServer((req, res) => {
    let body = ""; req.on("data", (c) => (body += c)); req.on("end", () => {
      const { id, method, params } = JSON.parse(body); let result, error;
      if (method === "eth_chainId") result = "0x38";
      else if (method === "eth_blockNumber") result = hx(L + 200);
      else if (method === "eth_getBlockByNumber") { const b = chain.blocks.get(Number(params[0])); result = b ? blockObj(b, params[1]) : null; }
      else if (method === "eth_getBlockReceipts") { const b = chain.blocks.get(Number(params[0])); result = b ? rcs(b) : null; }
      else if (method === "eth_call") {
        const { to, data } = params[0], tag = params[1];
        if (to === QUOTE && data === PROFILE.selDecimals) result = "0x" + word(18);
        else if (to === TM) {
          const m = chain.modes["0x" + data.slice(-40)];
          if (!m || m.failAll || (m.failAtBlock && tag !== "latest")) error = { code: -32000, message: "missing trie node" };
          else result = data.startsWith(PROFILE.selTokenInfos) ? m.infos : data.startsWith(PROFILE.selTokenInfoEx1s) ? m.ex1s : null;
        } else error = { code: -32000, message: "execution reverted" };
      } else if (method === "eth_getLogs") {
        const f = Number(params[0].fromBlock), t = Number(params[0].toBlock);
        if (failLogsAt !== null && f <= failLogsAt && failLogsAt <= t) error = { code: -32000, message: "simulierter Fehler" };
        else {
          result = [];
          const want = [].concat(params[0].topics[0]).map((x) => x.toLowerCase());
          for (const b of chain.blocks.values()) if (b.n >= f && b.n <= t) rcs(b).forEach((rc, i) => rc.logs.forEach((l) => {
            if (l.address.toLowerCase() === params[0].address && want.includes(l.topics[0].toLowerCase()))
              result.push({ ...l, blockNumber: hx(b.n), blockHash: b.hash, transactionIndex: hx(i), transactionHash: rc.transactionHash, removed: false });
          }));
        }
      } else error = { code: -32601, message: method };
      res.setHeader("content-type", "application/json");
      res.end(JSON.stringify(error ? { jsonrpc: "2.0", id, error } : { jsonrpc: "2.0", id, result }));
    });
  });
  return new Promise((r) => server.listen(0, "127.0.0.1", () => r(server)));
}

async function runRange(from, to, env = {}, opts = {}) {
  const chain = buildChain();
  const server = await serve(chain, opts);
  const lines = [], info = [];
  try {
    const code = await run({ RPC_URL: `http://127.0.0.1:${server.address().port}`, BUILDER_ADDRS: `${B48}:48Club`, LOG_CHUNK: "16",
      FROM_BLOCK: String(from), TO_BLOCK: String(to), ...env }, (s) => lines.push(s), (s) => info.push(s));
    const [head, ...rows] = lines;
    assert.equal(head, COLS.join(","));
    const parsed = rows.map((r) => Object.fromEntries(r.split(",").map((v, i) => [COLS[i], v])));
    return { code, rows: parsed, byBlock: Object.fromEntries(parsed.map((r) => [Number(r.block), r])), info: info.join("\n") };
  } finally { server.close(); }
}

test("E2E: echter Launch — N+1, Dev-/Fremdkäufe, Router, Kontrakt-Käufer, unzugeordneter Builder-Transfer, Quote BNB", async () => {
  const { code, byBlock } = await runRange(L, L);
  assert.equal(code, 0);
  const r = byBlock[L];
  assert.equal(r.row_status, "OK");
  assert.equal(r.launch_tx, TX.hash.toLowerCase());
  assert.equal(r.launch_index, "43");
  assert.equal(r.token, TOKEN);
  assert.equal(r.creator, CREATOR);
  assert.equal(r.n1_is_buy, "1"); assert.equal(r.n1_is_dev, "0"); assert.equal(r.n1_via_other_contract, "1"); assert.equal(r.n1_accounts, F1);
  assert.equal(r.dev_buys_in_launch_block, "1");
  assert.equal(r.foreign_buys_in_launch_block, "3", "Index 44, 48, 49 — nicht 46 (nur Transfer)");
  assert.equal(r.first_foreign_buy_index, "44");
  assert.equal(r.first_foreign_buy_account, F1);
  assert.equal(r.first_foreign_buy_cost_quote_units, "1000");
  assert.equal(r.first_foreign_buy_tx_value_wei, "10");
  assert.equal(r.quote_asset, "BNB"); assert.equal(r.quote_decimals, "18"); assert.equal(r.mode_state_source, "LAUNCH_BLOCK");
  assert.equal(r.fee_completeness, "EVENT_FEE_NO_KNOWN_EXTRA");
  assert.equal(r.adjacent_builder_transfer_wei, "1000");
  assert.equal(r.adjacent_builder_transfer_to, "48Club");
  assert.equal(r.tip_attribution, "UNPROVEN", "Auflage 2: Zuordnung unbewiesen");
  assert.equal(r.builder_hint_unverified, "48Club");
  assert.equal(r.purchase_transfer_mismatch, "1", "nur Index 49");
});

test("Auflage 3 — E2E: BEP20-Quote mit AntiSniper (Fallback latest), TaxToken, ungelesener Modus", async () => {
  const a = await runRange(L + 10, L + 10);
  const r2 = a.byBlock[L + 10];
  assert.equal(r2.quote_asset, QUOTE); assert.equal(r2.quote_decimals, "18");
  assert.equal(r2.mode_state_source, "LATEST"); assert.equal(r2.anti_sniper_fee_setting, "3");
  assert.equal(r2.fee_completeness, "UNKNOWN_ANTISNIPER");
  assert.equal(r2.first_foreign_buy_block, String(L + 11));
  assert.equal(a.byBlock[L + 10].row_status, "OK");
  assert.equal((await runRange(L + 80, L + 80)).byBlock[L + 80].fee_completeness, "UNKNOWN_TAXTOKEN");
  const u = (await runRange(L + 90, L + 90)).byBlock[L + 90];
  assert.equal(u.mode_state_source, "UNREAD"); assert.equal(u.quote_asset, "UNKNOWN"); assert.equal(u.fee_completeness, "UNKNOWN_MODE_UNREAD");
  assert.match(a.info, /Kosten: keine Statistik/);
});

test("Gegenprüfung, reproduzierte Fälle: gültig -> OK/Exit 0; beschädigtes TokenCreate -> DECODE_FAILED/Exit 3; Mismatch -> TOKEN_MISMATCH/Exit 3", async () => {
  const valid = await runRange(L, L);
  assert.equal(valid.byBlock[L].row_status, "OK"); assert.equal(valid.code, 0);
  const broken = await runRange(L + 40, L + 40);
  assert.equal(broken.byBlock[L + 40].row_status, "DECODE_FAILED"); assert.equal(broken.code, 3);
  assert.match(broken.info, /UNVOLLSTÄNDIG/);
  const mism = await runRange(L + 50, L + 50);
  assert.equal(mism.byBlock[L + 50].row_status, "TOKEN_MISMATCH"); assert.equal(mism.code, 3);
});

test("Auflage 1 — weitere Fehlerzustände: LAUNCH_FAILED, kaputtes Kauf-Event, Kettenbruch, Bindungsfehler -> jeweils Exit 3", async () => {
  for (const [n, status] of [[L + 60, "LAUNCH_FAILED"], [L + 70, "DECODE_FAILED"], [L + 20, "CHAIN_BREAK"], [L + 30, "BINDING_FAILED"]]) {
    const res = await runRange(n, n);
    assert.equal(res.byBlock[n].row_status, status, `Block ${n}`);
    assert.equal(res.code, 3, `Block ${n}`);
  }
});

test("Auflage 4 — Snapshot-Nachprüfung: geänderter Hash eines verwendeten Blocks -> Exit 3", async () => {
  const res = await runRange(L, L + 15, {}, { changeOnRecheck: L + 11 });
  assert.equal(res.code, 3);
  assert.match(res.info, new RegExp(`Block ${L + 11}: .* -> 0x9999`));
  assert.match(res.info, /1 geändert/);
});

test("Sauberer Bereich -> Exit 0; NEXT_BLOCKS=0 sucht nicht in Folgeblöcken", async () => {
  const a = await runRange(L, L + 15);
  assert.equal(a.code, 0); assert.equal(a.rows.length, 2);
  const b = await runRange(L, L + 15, { NEXT_BLOCKS: "0" });
  assert.equal(b.byBlock[L + 10].first_foreign_buy_tx, "");
});

test("RPC-Lücke wird gemeldet, nicht verschwiegen", async () => {
  const res = await runRange(L, L + 15, {}, { failLogsAt: L + 5 });
  assert.equal(res.code, 3);
  assert.match(res.info, new RegExp(`Block ${L + 5}: .*simulierter Fehler`));
});

test("Konfiguration: fehlende RPC_URL und zu nahe am Head werden abgelehnt", async () => {
  await assert.rejects(run({}), /RPC_URL fehlt/);
  const server = await serve(buildChain());
  try {
    await assert.rejects(run({ RPC_URL: `http://127.0.0.1:${server.address().port}`, TO_BLOCK: String(L + 190) }, () => {}, () => {}), /näher als 50 Blöcke/);
  } finally { server.close(); }
});
