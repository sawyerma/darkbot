#!/usr/bin/env node
// BR0 — Read-only-Beobachtung fremder Four.meme-Classic-Launches (Version 4; ersetzt v1–v3)
//
// STATUS: ENTWURF. Ergebnisse sind NICHT autoritativ, bis die ABNAHME unten vollständig erfüllt ist.
// Sendet nichts, signiert nichts, liest keinen Key. Keine Abhängigkeiten (Node >= 18).
//
// SCOPE (fest): Four.meme Classic, TokenManager2. NICHT abgedeckt: TokenManager V1, OpenFour, Flap.
//
// KONSTANTEN UND BELEGE (Stand 2026-10-22):
//   TokenManager2 0x5c952063c7fc8610ffdb798152d69f0b9550762b
//     bsc/docs/GATE0_G01_EVIDENCE_FREEZE_2026-09-27.md; kanonischer Receipt (Tx 0x8cc24c0c…7e5b, Block 124415333,
//     Index 43); four-meme-community/four-meme-ai @ c81f0eeb, references/contract-addresses.md
//   TokenCreate(address,address,uint256,string,string,uint256,uint256,uint256) = 0x396d5e90…ad20   (keccak selbst berechnet;
//     = 48Club-Doku = Log 301 des kanonischen Receipts; Layout laut archivierter ABI
//     prestudy/…/2026-09-27-g01/sources/TokenManager2.lite.json, sha256 1a71bdab…b6ef)
//   TokenPurchase(address,address,uint256,uint256,uint256,uint256,uint256,uint256) = 0x7db52723…2942 (keccak selbst
//     berechnet; Layout laut derselben ABI und four-meme-ai references/event-listening.md)
//   _tokenInfos(address) = 0xe684626b  -> (base, quote, template, totalSupply, maxOffers, maxRaising, launchTime,
//     offers, funds, lastPrice, K, T, status), 13 statische Wörter (archivierte ABI)
//   _tokenInfoEx1s(address) = 0x9f266331 -> (launchFee, pcFee, feeSetting, blockNumber, extraFee), 5 Wörter (dieselbe ABI)
//   decimals() = 0x313ce567
//   Modus-Regeln laut four-meme-ai references/errors.md: AntiSniperFeeMode = feeSetting > 0;
//     TaxToken = ((template >> 10) & 0x3F) == 5; X Mode = (template & 0x10000) > 0. Quote 0x0 = BNB, sonst BEP20
//     (references/execute-trade.md).
//
// WAS DIE SPALTEN BEDEUTEN — UND WAS NICHT:
//   Kauf          = TokenPurchase-Event von TokenManager2 für genau diesen Token in einer erfolgreichen TX. Ein Transfer
//                   allein ist kein Kauf. Käufer = Event-Feld `account` (kann ein Kontrakt sein).
//   fremd         = `account` und `tx.from` sind weder Launch-Absender noch `creator`. Verbundene Wallets des Entwicklers
//                   sind nicht erkennbar -> "fremd" ist eine Obergrenze.
//   *_quote_units = Event-Werte `cost`/`fee` in der kleinsten Einheit des Quote-Assets (BNB-Wei NUR wenn quote_asset=BNB).
//                   Beträge verschiedener Quote-Assets werden nie gemeinsam ausgewertet.
//   fee_completeness: ob das Event-Feld `fee` die gesamte Gebühr sein KANN. Bei AntiSniperFeeMode, TaxToken oder
//                   ungelesenem Modus ist das UNBEKANNT (ob `fee` die Anti-Sniper-Zusatzgebühr enthält, ist nicht belegt).
//   tx_value_wei  = mit der Kauf-TX gesendete native BNB; Erstattungen sind ohne Traces unsichtbar -> keine Kostenangabe.
//   adjacent_builder_transfer_wei = BNB-Transfers desselben Absenders an konfigurierte Builder-Adressen, die direkt auf den
//                   Kauf folgen. Die ZUORDNUNG ZUM KAUF IST UNBEWIESEN (Spalte tip_attribution = UNPROVEN); es ist weder
//                   ein belegter Tip noch eine Untergrenze eines Tips.
//   builder_hint_unverified = Absender/Empfänger der letzten Blocktransaktion, falls in BUILDER_ADDRS. Kein Nachweis.
//   mode_state_source = LAUNCH_BLOCK (Zustand am Launch-Block gelesen), LATEST (Fallback, Node ohne Archivzustand —
//                   Unveränderlichkeit von quote/feeSetting ist NICHT belegt) oder UNREAD.
//
// KETTENKONTINUITÄT — GENAU DAS WIRD GEPRÜFT:
//   (a) je Launch: Blocknummer, Blockhash, Receipt-Blockhash, Tx-Index, Log-Zugehörigkeit zum Launch-Receipt;
//   (b) je Launch: parentHash-Kette vom Launch-Block durch die durchsuchten Folgeblöcke;
//   (c) am Laufende: erneute Abfrage JEDES verwendeten Blocks; geänderter Hash = SNAPSHOT_CHANGED.
//   NICHT geprüft: eine durchgehende Kette über den GESAMTEN Bereich und Blöcke ohne Launch. Ein Reorg in solchen Blöcken
//   während des Laufs bliebe unbemerkt (Risiko durch CONFIRMATIONS gemindert, nicht ausgeschlossen).
//
// VOLLSTÄNDIGKEIT: Exit 0 NUR wenn: keine RPC-Lücke, JEDE Launch-Zeile row_status=OK, Nachprüfung (c) ohne Abweichung und
//   ohne Fehler. Jeder andere Zustand -> UNVOLLSTÄNDIG, Exit 3.
//
// UMGEBUNG: RPC_URL (Pflicht) · BUILDER_ADDRS "adresse:name,…" (optional, VORHER belegen)
//           FROM_BLOCK/TO_BLOCK oder N_BLOCKS (20000) · CONFIRMATIONS (50) · NEXT_BLOCKS (2) · LOG_CHUNK (1000)
// EXIT:     0 vollständig · 3 UNVOLLSTÄNDIG (nur informativ) · 1 Abbruch · 2 Konfiguration
//
// ABNAHME (erst danach dürfen Werte in Verträge, Gebote oder Architekturentscheidungen einfließen):
//   1. Lauf über festen Bereich mit Exit 0; Parameter, CSV, Zusammenfassung mit SHA-256 unter bsc/native/evidence/br0/.
//   2. >= 10 zufällig gezogene OK-Zeilen manuell im Explorer bestätigt (Position N, TX N+1, Käufer, Quote-Asset).
//   3. >= 5 Launches mit fee_completeness=UNKNOWN_ANTISNIPER geprüft: wird jeder Kauf als TokenPurchase emittiert, und was
//      enthält `fee`? Ergebnis dokumentieren; bis dahin bleibt die Spalte UNKNOWN.
//   4. BUILDER_ADDRS aus >= 3 Blöcken belegt, die der Builder selbst als seine ausweist.
//   5. Zeilen mit purchase_transfer_mismatch > 0 einzeln geprüft und erklärt.

import { pathToFileURL } from "node:url";

export const PROFILE = Object.freeze({
  name: "fourmeme_classic_tokenmanager2",
  tokenManager: "0x5c952063c7fc8610ffdb798152d69f0b9550762b",
  topicTokenCreate: "0x396d5e902b675b032348d3d2e9517ee8f0c4a926603fbc075d3d282ff00cad20",
  topicTokenPurchase: "0x7db52723a3b2cdd6164364b3b766e65e540d7be48ffa89582956d8eaebe62942",
  topicTransfer: "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
  selTokenInfos: "0xe684626b",
  selTokenInfoEx1s: "0x9f266331",
  selDecimals: "0x313ce567",
});

const ZERO_ADDR = "0x" + "0".repeat(40);
const lower = (a) => (a ?? "").toLowerCase();
const hex = (n) => "0x" + n.toString(16);
const isAddr = (s) => /^0x[0-9a-f]{40}$/.test(s);

// ---------- Dekodierung (rein, getestet) ----------
export function words(data) {
  const d = lower(data).replace(/^0x/, "");
  if (d.length % 64 !== 0) throw new Error("Datenlänge kein Vielfaches von 32 Byte");
  const w = [];
  for (let i = 0; i < d.length; i += 64) w.push(d.slice(i, i + 64));
  return w;
}
function asAddr(w) {
  if (!/^0{24}[0-9a-f]{40}$/.test(w)) throw new Error("Wort ist keine Adresse");
  return "0x" + w.slice(24);
}
const asUint = (w) => BigInt("0x" + w);
const topicAddr = (t) => asAddr(lower(t).replace(/^0x/, ""));

export function decodeTokenCreate(log) {
  if (lower(log.address) !== PROFILE.tokenManager || lower(log.topics?.[0]) !== PROFILE.topicTokenCreate || log.topics.length !== 1) {
    throw new Error("kein TokenCreate von TokenManager2");
  }
  const w = words(log.data);
  if (w.length < 8) throw new Error("TokenCreate zu kurz");
  return { creator: asAddr(w[0]), token: asAddr(w[1]), requestId: asUint(w[2]), totalSupply: asUint(w[5]), launchTime: asUint(w[6]), launchFee: asUint(w[7]) };
}

export function decodePurchase(log) {
  if (lower(log.address) !== PROFILE.tokenManager || lower(log.topics?.[0]) !== PROFILE.topicTokenPurchase || log.topics.length !== 1) {
    throw new Error("kein TokenPurchase von TokenManager2");
  }
  const w = words(log.data);
  if (w.length !== 8) throw new Error(`TokenPurchase hat ${w.length} statt 8 Wörter`);
  return { token: asAddr(w[0]), account: asAddr(w[1]), price: asUint(w[2]), amount: asUint(w[3]), cost: asUint(w[4]), fee: asUint(w[5]), offers: asUint(w[6]), funds: asUint(w[7]) };
}

// Rückgabe von _tokenInfos und _tokenInfoEx1s -> Modus. Wirft bei falscher Länge oder falschem Adressformat.
export function decodeMode(infosHex, ex1sHex) {
  const a = words(infosHex), b = words(ex1sHex);
  if (a.length !== 13) throw new Error(`_tokenInfos hat ${a.length} statt 13 Wörter`);
  if (b.length !== 5) throw new Error(`_tokenInfoEx1s hat ${b.length} statt 5 Wörter`);
  const quote = asAddr(a[1]), template = asUint(a[2]), feeSetting = asUint(b[2]);
  const antiSniper = feeSetting > 0n, taxToken = ((template >> 10n) & 0x3fn) === 5n, xMode = (template & 0x10000n) > 0n;
  return {
    quote, template, feeSetting, extraFee: asUint(b[4]), antiSniper, taxToken, xMode,
    feeCompleteness: antiSniper ? "UNKNOWN_ANTISNIPER" : taxToken ? "UNKNOWN_TAXTOKEN" : "EVENT_FEE_NO_KNOWN_EXTRA",
  };
}

function isTransfer(l) { return lower(l.topics?.[0]) === PROFILE.topicTransfer && l.topics.length === 3; }

export function purchasesOf(rc, token) {
  if (!rc || rc.status !== "0x1") return [];
  const out = [];
  for (const l of rc.logs ?? []) {
    if (lower(l.address) !== PROFILE.tokenManager || lower(l.topics?.[0]) !== PROFILE.topicTokenPurchase) continue;
    const p = decodePurchase(l);
    if (p.token !== token) continue;
    p.transferMatch = (rc.logs ?? []).some((t) => lower(t.address) === token && isTransfer(t)
      && topicAddr(t.topics[1]) === PROFILE.tokenManager && topicAddr(t.topics[2]) === p.account);
    out.push(p);
  }
  return out;
}

// Exit-Entscheidung als reine Funktion, damit sie eigenständig getestet wird.
export function isComplete({ gaps, byStatus, recheck }) {
  const nonOk = Object.entries(byStatus).filter(([k]) => k !== "OK").reduce((s, [, v]) => s + v, 0);
  return gaps === 0 && nonOk === 0 && recheck.changed === 0 && recheck.errors === 0;
}

// ---------- Konfiguration ----------
function loadConfig(env) {
  const die = (m) => { const e = new Error(m); e.code = 2; throw e; };
  const int = (name, def, min = 0) => {
    const raw = env[name];
    const v = raw === undefined || raw === "" ? def : Number(raw);
    if (!Number.isSafeInteger(v) || v < min) die(`${name} muss eine ganze Zahl >= ${min} sein`);
    return v;
  };
  if (!env.RPC_URL) die("RPC_URL fehlt");
  const builders = new Map();
  for (const p of (env.BUILDER_ADDRS ?? "").split(",").map((s) => s.trim()).filter(Boolean)) {
    const [a, name] = p.split(":");
    if (!isAddr(lower(a)) || !name) die(`BUILDER_ADDRS: ungültiger Eintrag ${p}`);
    builders.set(lower(a), name);
  }
  const cfg = {
    rpcUrl: env.RPC_URL, builders,
    nextBlocks: int("NEXT_BLOCKS", 2), confirmations: int("CONFIRMATIONS", 50),
    logChunk: int("LOG_CHUNK", 1000, 1), nBlocks: int("N_BLOCKS", 20000, 1),
    fromBlock: env.FROM_BLOCK ? int("FROM_BLOCK", 0) : null, toBlock: env.TO_BLOCK ? int("TO_BLOCK", 0) : null,
  };
  if (cfg.confirmations < cfg.nextBlocks) die("CONFIRMATIONS muss >= NEXT_BLOCKS sein");
  return cfg;
}

// ---------- RPC ----------
function makeRpc(url) {
  let id = 0;
  const once = async (method, params) => {
    const res = await fetch(url, { method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: ++id, method, params }), signal: AbortSignal.timeout(30000) });
    if (!res.ok) throw new Error(`HTTP ${res.status} bei ${method}`);
    const body = await res.json();
    if (body.error) throw new Error(`${method}: ${body.error.message ?? JSON.stringify(body.error)}`);
    return body.result;
  };
  return async (method, params, tries = 3) => {
    let last;
    for (let t = 0; t < tries; t++) {
      try { return await once(method, params); } catch (e) { last = e; await new Promise((r) => setTimeout(r, 250 * 2 ** t)); }
    }
    throw last;
  };
}

class BindingError extends Error {}

function makeBlockSource(rpc, used) {
  const cache = new Map();
  return async (n) => {
    if (cache.has(n)) return cache.get(n);
    const block = await rpc("eth_getBlockByNumber", [hex(n), true]);
    if (!block) throw new BindingError(`Block ${n} nicht gefunden`);
    if (Number(block.number) !== n) throw new BindingError(`Block ${n}: falsche Blocknummer ${block.number}`);
    let receipts = null;
    try { receipts = await rpc("eth_getBlockReceipts", [hex(n)], 1); } catch { receipts = null; }
    if (!Array.isArray(receipts) || receipts.length !== block.transactions.length) {
      receipts = [];
      for (const tx of block.transactions) receipts.push(await rpc("eth_getTransactionReceipt", [tx.hash]));
    }
    for (let i = 0; i < receipts.length; i++) {
      const rc = receipts[i], tx = block.transactions[i];
      if (!rc || lower(rc.transactionHash) !== lower(tx.hash) || lower(rc.blockHash) !== lower(block.hash)
        || Number(rc.blockNumber) !== n || Number(rc.transactionIndex) !== i || Number(tx.transactionIndex) !== i) {
        throw new BindingError(`Block ${n}: Receipt/Transaktion ${i} nicht an Block ${block.hash} gebunden`);
      }
    }
    used.set(n, lower(block.hash));
    const d = { block, receipts };
    cache.set(n, d);
    if (cache.size > 64) cache.delete(cache.keys().next().value);
    return d;
  };
}

// Modus je Token: zuerst am Launch-Block, sonst latest (markiert), sonst UNREAD.
function makeModeSource(rpc) {
  const cache = new Map();
  const callAt = async (data, tag) => rpc("eth_call", [{ to: PROFILE.tokenManager, data }, tag], 1);
  const decimals = async (quote) => {
    try { const w = words(await rpc("eth_call", [{ to: quote, data: PROFILE.selDecimals }, "latest"], 1)); return w.length === 1 ? Number(asUint(w[0])) : null; }
    catch { return null; }
  };
  return async (token, bn) => {
    if (cache.has(token)) return cache.get(token);
    const arg = "0".repeat(24) + token.slice(2);
    let result = { source: "UNREAD", feeCompleteness: "UNKNOWN_MODE_UNREAD" };
    for (const tag of [hex(bn), "latest"]) {
      try {
        const m = decodeMode(await callAt(PROFILE.selTokenInfos + arg, tag), await callAt(PROFILE.selTokenInfoEx1s + arg, tag));
        result = { ...m, source: tag === "latest" ? "LATEST" : "LAUNCH_BLOCK" };
        break;
      } catch { /* nächster Versuch */ }
    }
    if (result.quote !== undefined) {
      result.quoteAsset = result.quote === ZERO_ADDR ? "BNB" : result.quote;
      result.quoteDecimals = result.quote === ZERO_ADDR ? 18 : await decimals(result.quote);
    }
    cache.set(token, result);
    return result;
  };
}

function builderHint(block, builders) {
  const txs = block.transactions;
  if (!txs.length) return { lastFrom: "", lastTo: "", hint: "" };
  const last = txs[txs.length - 1];
  return { lastFrom: lower(last.from), lastTo: lower(last.to), hint: builders.get(lower(last.from)) ?? builders.get(lower(last.to)) ?? "" };
}
function adjacentBuilderTransfers(d, i, sender, builders) {
  let wei = 0n; const names = new Set();
  for (let j = i + 1; j < d.block.transactions.length; j++) {
    const t = d.block.transactions[j];
    if (lower(t.from) !== lower(sender)) break;
    const name = builders.get(lower(t.to));
    if (name && d.receipts[j].status === "0x1") { wei += BigInt(t.value); names.add(name); }
  }
  return { wei, to: [...names].join("|") };
}

export const COLS = ["row_status", "detail", "block", "block_hash", "launch_tx", "launch_index", "launch_tx_from", "launch_tx_to", "creator", "token",
  "quote_asset", "quote_decimals", "mode_state_source", "anti_sniper_fee_setting", "tax_token", "x_mode", "fee_completeness",
  "launch_internal_buys", "coinbase", "last_tx_from", "last_tx_to", "builder_hint_unverified",
  "n1_tx", "n1_from", "n1_to", "n1_status", "n1_is_buy", "n1_is_dev", "n1_via_other_contract", "n1_accounts",
  "dev_buys_in_launch_block", "foreign_buys_in_launch_block",
  "first_foreign_buy_block", "first_foreign_buy_index", "first_foreign_buy_tx", "first_foreign_buy_from", "first_foreign_buy_to",
  "first_foreign_buy_account", "first_foreign_buy_cost_quote_units", "first_foreign_buy_fee_quote_units", "first_foreign_buy_amount",
  "first_foreign_buy_tx_value_wei", "first_foreign_buy_effective_gas_price_wei", "first_foreign_buy_gas_used",
  "adjacent_builder_transfer_wei", "adjacent_builder_transfer_to", "tip_attribution",
  "purchase_transfer_mismatch"];

async function evaluateLaunch(log, getBlock, getMode, cfg) {
  const key = lower(log.transactionHash);
  const bn = Number(log.blockNumber), N = Number(log.transactionIndex);
  const d = await getBlock(bn);
  const base = { block: bn, block_hash: lower(d.block.hash), launch_tx: key, launch_index: N, coinbase: lower(d.block.miner) };
  if (lower(log.blockHash) !== lower(d.block.hash)) return { ...base, row_status: "BINDING_FAILED", detail: "log.blockHash != block.hash" };
  const ltx = d.block.transactions[N], lrc = d.receipts[N];
  if (!ltx || lower(ltx.hash) !== key) return { ...base, row_status: "BINDING_FAILED", detail: "Index zeigt auf andere Transaktion" };
  const inReceipt = (lrc.logs ?? []).some((l) => Number(l.logIndex) === Number(log.logIndex) && lower(l.address) === lower(log.address)
    && lower(l.topics?.[0]) === lower(log.topics?.[0]) && lower(l.data) === lower(log.data));
  if (!inReceipt) return { ...base, row_status: "BINDING_FAILED", detail: "Log nicht im Launch-Receipt" };
  Object.assign(base, { launch_tx_from: lower(ltx.from), launch_tx_to: lower(ltx.to) });
  if (lrc.status !== "0x1") return { ...base, row_status: "LAUNCH_FAILED", detail: "Launch-Receipt status != 1 trotz Log" };

  let tc;
  try { tc = decodeTokenCreate(log); } catch (e) { return { ...base, row_status: "DECODE_FAILED", detail: e.message }; }
  const token = tc.token;
  Object.assign(base, { creator: tc.creator, token });
  const mints = (lrc.logs ?? []).filter((l) => isTransfer(l) && topicAddr(l.topics[1]) === ZERO_ADDR);
  if (!mints.length || mints.some((l) => lower(l.address) !== token)) {
    return { ...base, row_status: "TOKEN_MISMATCH", detail: "Mint-Transfer passt nicht zum TokenCreate-Token" };
  }
  const dev = new Set([lower(ltx.from), tc.creator]);
  const mismatch = new Set();
  const note = (txHash, p) => { if (!p.transferMatch) mismatch.add(lower(txHash) + ":" + p.account); };
  let internal;
  try { internal = purchasesOf(lrc, token); } catch (e) { return { ...base, row_status: "DECODE_FAILED", detail: `Kauf im Launch: ${e.message}` }; }
  for (const p of internal) note(ltx.hash, p);
  const bh = builderHint(d.block, cfg.builders);
  const isDevPurchase = (tx, p) => dev.has(lower(tx.from)) || dev.has(p.account);

  const n1tx = d.block.transactions[N + 1], n1rc = d.receipts[N + 1];
  let devBuys = 0, foreignBuys = 0, first = null, n1p = [];
  try {
    n1p = n1tx ? purchasesOf(n1rc, token) : [];
    const scan = (dd, startIdx) => {
      for (let i = startIdx; i < dd.block.transactions.length; i++) {
        const tx = dd.block.transactions[i];
        for (const p of purchasesOf(dd.receipts[i], token)) {
          note(tx.hash, p);
          if (dd === d) { if (isDevPurchase(tx, p)) devBuys++; else foreignBuys++; }
          if (!first && !isDevPurchase(tx, p)) first = { dd, i, p };
        }
        if (first && dd !== d) return;
      }
    };
    scan(d, N + 1);
    let prev = d;
    for (let k = 1; k <= cfg.nextBlocks && !first; k++) {
      const nd = await getBlock(bn + k);
      if (lower(nd.block.parentHash) !== lower(prev.block.hash)) {
        return { ...base, row_status: "CHAIN_BREAK", detail: `Block ${bn + k}.parentHash != Block ${bn + k - 1}.hash` };
      }
      scan(nd, 0);
      prev = nd;
    }
  } catch (e) {
    if (e instanceof BindingError) throw e;
    return { ...base, row_status: "DECODE_FAILED", detail: `Kauf-Event: ${e.message}` };
  }

  const mode = await getMode(token, bn);
  const row = { ...base, row_status: "OK",
    quote_asset: mode.quoteAsset ?? "UNKNOWN", quote_decimals: mode.quoteDecimals ?? "UNKNOWN", mode_state_source: mode.source,
    anti_sniper_fee_setting: mode.feeSetting !== undefined ? mode.feeSetting.toString() : "UNKNOWN",
    tax_token: mode.taxToken === undefined ? "UNKNOWN" : mode.taxToken ? 1 : 0,
    x_mode: mode.xMode === undefined ? "UNKNOWN" : mode.xMode ? 1 : 0, fee_completeness: mode.feeCompleteness,
    launch_internal_buys: internal.length,
    last_tx_from: bh.lastFrom, last_tx_to: bh.lastTo, builder_hint_unverified: bh.hint,
    n1_tx: n1tx ? lower(n1tx.hash) : "", n1_from: n1tx ? lower(n1tx.from) : "", n1_to: n1tx ? lower(n1tx.to) : "",
    n1_status: n1rc ? (n1rc.status === "0x1" ? 1 : 0) : "", n1_is_buy: n1p.length ? 1 : 0,
    n1_is_dev: n1p.length && n1p.some((p) => isDevPurchase(n1tx, p)) ? 1 : 0,
    n1_via_other_contract: n1p.length && lower(n1tx.to) !== PROFILE.tokenManager ? 1 : 0,
    n1_accounts: n1p.map((p) => p.account).join("|"),
    dev_buys_in_launch_block: devBuys, foreign_buys_in_launch_block: foreignBuys };
  if (first) {
    const ftx = first.dd.block.transactions[first.i], frc = first.dd.receipts[first.i];
    const adj = adjacentBuilderTransfers(first.dd, first.i, ftx.from, cfg.builders);
    Object.assign(row, {
      first_foreign_buy_block: Number(first.dd.block.number), first_foreign_buy_index: first.i, first_foreign_buy_tx: lower(ftx.hash),
      first_foreign_buy_from: lower(ftx.from), first_foreign_buy_to: lower(ftx.to), first_foreign_buy_account: first.p.account,
      first_foreign_buy_cost_quote_units: first.p.cost.toString(), first_foreign_buy_fee_quote_units: first.p.fee.toString(),
      first_foreign_buy_amount: first.p.amount.toString(), first_foreign_buy_tx_value_wei: BigInt(ftx.value ?? 0).toString(),
      first_foreign_buy_effective_gas_price_wei: BigInt(frc.effectiveGasPrice ?? ftx.gasPrice ?? 0).toString(),
      first_foreign_buy_gas_used: BigInt(frc.gasUsed ?? 0).toString(),
      adjacent_builder_transfer_wei: adj.wei.toString(), adjacent_builder_transfer_to: adj.to, tip_attribution: "UNPROVEN",
    });
  }
  row.purchase_transfer_mismatch = mismatch.size;
  return row;
}

// ---------- Hauptlauf ----------
export async function run(env, write = (s) => process.stdout.write(s + "\n"), info = (s) => process.stderr.write(s + "\n")) {
  const cfg = loadConfig(env);
  const rpc = makeRpc(cfg.rpcUrl);
  const used = new Map();
  const getBlock = makeBlockSource(rpc, used);
  const getMode = makeModeSource(rpc);
  const chain = await rpc("eth_chainId", []);
  if (lower(chain) !== "0x38") { const e = new Error(`chainId ${chain} ist nicht BSC Mainnet (0x38)`); e.code = 2; throw e; }
  const head = Number(await rpc("eth_blockNumber", []));
  const toBlock = cfg.toBlock ?? head - cfg.confirmations;
  const fromBlock = cfg.fromBlock ?? toBlock - cfg.nBlocks + 1;
  if (toBlock > head - cfg.confirmations) { const e = new Error(`TO_BLOCK ${toBlock} liegt näher als ${cfg.confirmations} Blöcke am Head ${head}`); e.code = 2; throw e; }
  if (fromBlock < 0 || fromBlock > toBlock) { const e = new Error(`ungültiger Bereich ${fromBlock}..${toBlock}`); e.code = 2; throw e; }
  info(`Profil ${PROFILE.name} · Bereich ${fromBlock}..${toBlock} (Head ${head})`);
  write(COLS.join(","));

  const gaps = [];
  const getLogs = async (a, b) => {
    try {
      return await rpc("eth_getLogs", [{ fromBlock: hex(a), toBlock: hex(b), address: PROFILE.tokenManager, topics: [PROFILE.topicTokenCreate] }]);
    } catch (e) {
      if (a === b) { gaps.push({ block: a, error: String(e.message ?? e) }); return []; }
      const m = Math.floor((a + b) / 2);
      return [...(await getLogs(a, m)), ...(await getLogs(m + 1, b))];
    }
  };

  const st = { byStatus: {}, ok: 0, n1Buy: 0, n1Foreign: 0, n1Senders: {}, firstSameBlock: 0, firstNone: 0, builder: {},
    adj: 0, internal: 0, mismatchRows: 0, quote: {}, feeComp: {}, modeSource: {} };
  const seen = new Set();
  for (let a = fromBlock; a <= toBlock; a += cfg.logChunk) {
    const b = Math.min(toBlock, a + cfg.logChunk - 1);
    const logs = (await getLogs(a, b)).sort((x, y) => Number(x.blockNumber) - Number(y.blockNumber) || Number(x.logIndex) - Number(y.logIndex));
    for (const log of logs) {
      const key = lower(log.transactionHash) + ":" + Number(log.logIndex);
      if (log.removed || seen.has(key)) continue;
      seen.add(key);
      let row;
      try { row = await evaluateLaunch(log, getBlock, getMode, cfg); }
      catch (e) {
        if (!(e instanceof BindingError)) throw e;
        row = { row_status: "BINDING_FAILED", detail: e.message, block: Number(log.blockNumber), launch_tx: lower(log.transactionHash) };
      }
      st.byStatus[row.row_status] = (st.byStatus[row.row_status] ?? 0) + 1;
      if (row.row_status === "OK") {
        st.ok++;
        const inc = (o, k) => { o[k] = (o[k] ?? 0) + 1; };
        inc(st.builder, row.builder_hint_unverified || "unbekannt");
        inc(st.quote, row.quote_asset); inc(st.feeComp, row.fee_completeness); inc(st.modeSource, row.mode_state_source);
        if (row.launch_internal_buys > 0) st.internal++;
        if (row.n1_is_buy) st.n1Buy++;
        if (row.n1_is_buy && !row.n1_is_dev) { st.n1Foreign++; inc(st.n1Senders, row.n1_from); }
        if (row.first_foreign_buy_tx) {
          if (row.first_foreign_buy_block === row.block) st.firstSameBlock++;
          if (BigInt(row.adjacent_builder_transfer_wei) > 0n) st.adj++;
        } else st.firstNone++;
        if (row.purchase_transfer_mismatch > 0) st.mismatchRows++;
      }
      write(COLS.map((c) => String(row[c] ?? "").replaceAll(",", ";")).join(","));
    }
  }

  // Nachprüfung (c): jeden verwendeten Block erneut abfragen
  const recheck = { checked: 0, changed: 0, errors: 0, detail: [] };
  for (const [n, h] of [...used.entries()].sort((x, y) => x[0] - y[0])) {
    try {
      const again = await rpc("eth_getBlockByNumber", [hex(n), false]);
      recheck.checked++;
      if (!again || lower(again.hash) !== h) { recheck.changed++; recheck.detail.push(`Block ${n}: ${h} -> ${again ? lower(again.hash) : "fehlt"}`); }
    } catch (e) { recheck.errors++; recheck.detail.push(`Block ${n}: ${e.message}`); }
  }

  const complete = isComplete({ gaps: gaps.length, byStatus: st.byStatus, recheck });
  const pct = (x) => (st.ok ? ((100 * x) / st.ok).toFixed(1) : "0.0");
  const top = Object.entries(st.n1Senders).sort((x, y) => y[1] - x[1]).slice(0, 10);
  const total = Object.values(st.byStatus).reduce((s, v) => s + v, 0);
  info(`\nERGEBNIS: ${complete ? "vollständig (Abnahme laut Dateikopf noch erforderlich)" : "UNVOLLSTÄNDIG — nur informativ, keine Statistik als Ergebnis verwenden"}`);
  info(`Scope: nur ${PROFILE.name}; TokenManager V1, OpenFour und Flap sind nicht enthalten.`);
  info(`Launch-Zeilen: ${total}  Status: ${JSON.stringify(st.byStatus)}`);
  info(`Auswertbar (OK): ${st.ok} · mit Kauf im Launch selbst: ${st.internal}`);
  info(`N+1 ist TokenPurchase: ${st.n1Buy} (${pct(st.n1Buy)} %) · davon fremd: ${st.n1Foreign} (${pct(st.n1Foreign)} %)  [fremd = Obergrenze]`);
  info(`Erster fremder Kauf im Launch-Block: ${st.firstSameBlock} (${pct(st.firstSameBlock)} %) · keiner bis +${cfg.nextBlocks} Blöcke: ${st.firstNone}`);
  info(`Quote-Assets: ${JSON.stringify(st.quote)} · Gebühren-Vollständigkeit: ${JSON.stringify(st.feeComp)} · Modus gelesen: ${JSON.stringify(st.modeSource)}`);
  info(`Kosten: keine Statistik. Event-Beträge stehen je Zeile in Quote-Einheiten; nur gleiche Quote-Assets vergleichen.`);
  info(`Benachbarte Builder-Transfers nach erstem fremden Kauf: ${st.adj} Fälle — Zuordnung zum Kauf UNBEWIESEN, nicht als Tip oder Gebotsbasis verwenden.`);
  info(`Builder-HINWEIS (kein Nachweis): ${JSON.stringify(st.builder)}`);
  info(`Häufigste fremde N+1-Absender: ${top.map(([a, n]) => `${a}:${n}`).join("  ") || "—"}`);
  info(`Zeilen mit TokenPurchase ohne passenden Transfer: ${st.mismatchRows} (einzeln prüfen)`);
  info(`Nachprüfung verwendeter Blöcke: ${recheck.checked} geprüft, ${recheck.changed} geändert, ${recheck.errors} Fehler. Blöcke ohne Launch werden nicht geprüft.`);
  for (const x of recheck.detail) info(`  ${x}`);
  if (gaps.length) { info(`LÜCKEN (${gaps.length} Blöcke ohne Logs):`); for (const g of gaps) info(`  Block ${g.block}: ${g.error}`); }
  return complete ? 0 : 3;
}

const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  run(process.env).then((code) => process.exit(code), (e) => {
    process.stderr.write(`${e.code === 2 ? "FEHLER" : "ABBRUCH"}: ${e.code === 2 ? e.message : e.stack ?? e}\n`);
    process.exit(e.code === 2 ? 2 : 1);
  });
}
