#!/usr/bin/env node
// wss_race.mjs — Wie früh sieht welche WebSocket-Quelle einen BSC-Launch? (read-only, Version 1.2)
// 1.2: Wartezeit zwischen Verbindungsversuchen wird erst nach erfolgreicher Pending-Subscription zurückgesetzt (vorher: bei
//      jedem Verbindungsaufbau -> bei Ratenlimit ~1 Versuch/s). Zusammenfassung sagt "Pending aktiv" statt "verbunden".
// 1.1: Modus FULL/HASH richtet sich nach den tatsächlich empfangenen Nachrichten, nicht nach der Annahme der Subscription.
//
// Misst auf DEINEM Rechner, mit EINER Uhr, für jeden Four.meme-Classic- und Flap-Launch:
//   - ob jede Quelle die Launch-Transaktion VOR dem Block als Pending gesehen hat,
//   - wie viele Millisekunden vor der ersten Block-Ankunft (über alle Quellen) das war,
//   - welche Quelle zuerst war,
//   - wie viele Launches KEINE Quelle vorher gesehen hat (= nicht über öffentliche Pending-Daten erkennbar).
// Sendet nichts, signiert nichts, braucht keinen Key. Keine Abhängigkeiten: Node >= 22 (eingebautes WebSocket).
//
// AUFRUF:
//   SOURCES="publicnode=wss://bsc-rpc.publicnode.com,drpc=wss://bsc.drpc.org" DURATION_S=1800 node wss_race.mjs
//   Optional eigener Node:  ...,mygeth=ws://127.0.0.1:8546
// AUSGABE: wss_race_launches.csv (eine Zeile je Launch), wss_race_summary.txt (Kennzahlen je Quelle)
//
// WAS DIE ZAHLEN BEDEUTEN — UND WAS NICHT:
//   lead_ms = (erste Ankunft des Block-Heads bei uns, über alle Quellen) − (Pending-Ankunft bei dieser Quelle).
//     Positiv = Quelle sah den Launch so viele ms, bevor wir den Block kannten. Das ist NICHT der Abstand zur
//     Blockproduktion beim Validator und NICHT die Zeit, die für einen Kauf im selben Block bleibt.
//   Status je Quelle: SEEN (vorher gesehen) · LATE (Pending kam erst nach dem Block) · NOT_SEEN · DOWN (Quelle war im
//     Fenster nicht verbunden; zählt nicht in die Abdeckung) · NO_PENDING (Quelle bietet keine Pending-Subscription).
//   pending_mode (aus den empfangenen Nachrichten): FULL (TX-Objekte), HASH (nur Hashes), MIXED, NONE (nichts empfangen). Im HASH-Modus ist die Ankunft des Hashes
//     gemessen, nicht die der vollständigen, handlungsfähigen Transaktion.
//   Ergebnisse gelten für diesen Rechner, diesen Standort und diesen Zeitraum. Ein anderer Standort misst anders.
//
// KONSTANTEN (Belege):
//   Four.meme TokenManager2 0x5c952063…762b, TokenCreate-Topic 0x396d5e90…ad20 (siehe br0_observe_launches.mjs;
//     keccak selbst berechnet, = kanonischer Receipt Log 301)
//   Flap Portal 0xe2ce6ab8…9de0 (bsc/engine/crates, 4 Fundstellen), TokenCreated(uint256,address,uint256,address,string,
//     string,string) = 0x504e7f36…9603 (keccak selbst berechnet = prestudy/…/2026-09-27-g01/keccak_comparison.json)

import fs from "node:fs";
import { pathToFileURL } from "node:url";

export const VENUES = Object.freeze({
  "0x5c952063c7fc8610ffdb798152d69f0b9550762b": { venue: "four_classic", topic: "0x396d5e902b675b032348d3d2e9517ee8f0c4a926603fbc075d3d282ff00cad20" },
  "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0": { venue: "flap", topic: "0x504e7f360b2e5fe33cbaaae4c593bc55305328341bf79009e43e0e3b7f699603" },
});
const lower = (s) => (s ?? "").toLowerCase();
const nowNs = () => process.hrtime.bigint();
const ms = (ns) => Number(ns) / 1e6;

export function parseSources(raw) {
  const out = [];
  for (const part of (raw ?? "").split(",").map((s) => s.trim()).filter(Boolean)) {
    const i = part.indexOf("=");
    const name = part.slice(0, i), url = part.slice(i + 1);
    if (i < 1 || !/^[a-z0-9_]+$/i.test(name) || !/^wss?:\/\//.test(url)) throw new Error(`SOURCES: ungültiger Eintrag "${part}" (Format name=wss://…)`);
    if (out.some((s) => s.name === name)) throw new Error(`SOURCES: Name ${name} doppelt`);
    out.push({ name, url });
  }
  if (!out.length) throw new Error("SOURCES fehlt");
  return out;
}

export function percentile(sorted, p) {
  if (!sorted.length) return null;
  return sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1))];
}

// ---------- Reine Auswertung (getestet) ----------
// state: { names, mode: {name: FULL|HASH|NONE}, heads: Map(block->Map(name->ns)),
//          launches: Map(tx->{venue, block, logNs: Map(name->ns), pendingNs: Map(name->ns)}),
//          down: {name: [[startNs, endNs|null]]}, windowMs }
export function analyze(state) {
  const rows = [];
  for (const [tx, L] of state.launches) {
    const headArrivals = [...(state.heads.get(L.block)?.values() ?? [])];
    const blockNs = headArrivals.length ? headArrivals.reduce((a, b) => (b < a ? b : a)) : [...L.logNs.values()].reduce((a, b) => (b < a ? b : a));
    const row = { tx, venue: L.venue, block: L.block, block_ref: headArrivals.length ? "HEAD" : "LOG", per: {} };
    let best = null;
    for (const n of state.names) {
      const winStart = blockNs - BigInt(state.windowMs) * 1000000n;
      const wasDown = (state.down[n] ?? []).some(([s, e]) => s <= blockNs && (e === null || e >= winStart));
      const p = L.pendingNs.get(n);
      let status, lead = null;
      if (state.mode[n] === "NONE") status = "NO_PENDING";
      else if (wasDown) status = "DOWN";
      else if (p === undefined) status = "NOT_SEEN";
      else { lead = ms(blockNs - p); status = lead > 0 ? "SEEN" : "LATE"; }
      row.per[n] = { status, lead };
      if (status === "SEEN" && (best === null || lead > row.per[best].lead)) best = n;
    }
    row.first = best ?? "";
    rows.push(row);
  }
  const summary = {};
  for (const n of state.names) {
    const usable = rows.filter((r) => ["SEEN", "LATE", "NOT_SEEN"].includes(r.per[n].status));
    const seen = usable.filter((r) => r.per[n].status === "SEEN");
    const leads = seen.map((r) => r.per[n].lead).sort((a, b) => a - b);
    const behind = seen.filter((r) => r.first && r.first !== n).map((r) => r.per[r.first].lead - r.per[n].lead).sort((a, b) => a - b);
    summary[n] = { mode: state.mode[n], usable: usable.length, seen: seen.length, late: usable.filter((r) => r.per[n].status === "LATE").length,
      down: rows.filter((r) => r.per[n].status === "DOWN").length, first: rows.filter((r) => r.first === n).length,
      lead_p50: percentile(leads, 50), lead_p90: percentile(leads, 90), lead_min: leads[0] ?? null, lead_max: leads.at(-1) ?? null,
      behind_best_p50: percentile(behind, 50) };
  }
  const anySeen = rows.filter((r) => state.names.some((n) => r.per[n].status === "SEEN")).length;
  return { rows, summary, total: rows.length, anySeen };
}

export function toCsv(result, names) {
  const head = ["launch_tx", "venue", "block", "block_ref", "first_source", ...names.flatMap((n) => [`${n}_status`, `${n}_lead_ms`])];
  const lines = [head.join(",")];
  for (const r of result.rows) {
    lines.push([r.tx, r.venue, r.block, r.block_ref, r.first, ...names.flatMap((n) => [r.per[n].status, r.per[n].lead === null ? "" : r.per[n].lead.toFixed(3)])].join(","));
  }
  return lines.join("\n") + "\n";
}

export function summaryText(result, names, sourceStats, meta) {
  const f = (v) => (v === null || v === undefined ? "—" : `${v.toFixed(1)} ms`);
  const pct = (a, b) => (b ? `${((100 * a) / b).toFixed(1)} %` : "—");
  const out = [];
  out.push(`wss_race — ${meta.startedIso} bis ${meta.endedIso}, Laufzeit ${meta.durationS.toFixed(0)} s, Knoten-Uhr: process.hrtime`);
  out.push(`Launches beobachtet (Log über mind. eine Quelle): ${result.total} · vorher von mind. einer Quelle gesehen: ${result.anySeen} (${pct(result.anySeen, result.total)})`);
  out.push(`Von KEINER Quelle vorher gesehen: ${result.total - result.anySeen} (${pct(result.total - result.anySeen, result.total)}) — mit diesen Quellen nicht vorab erkennbar`);
  out.push("");
  for (const n of names) {
    const s = result.summary[n], st = sourceStats[n];
    out.push(`[${n}] Modus ${s.mode} · Pending aktiv ${pct(st.connectedMs, meta.durationS * 1000)} der Zeit · Reconnects ${st.reconnects} · Fehler ${st.errors} (davon Ratenlimit ${st.rateLimited}) · Pending-Nachrichten ${st.pendingMsgs} (${(st.pendingMsgs / Math.max(1, meta.durationS)).toFixed(0)}/s)`);
    out.push(`    vorher gesehen: ${s.seen}/${s.usable} (${pct(s.seen, s.usable)}) · zu spät: ${s.late} · Quelle ausgefallen: ${s.down} · als Erste: ${s.first}`);
    out.push(`    Vorsprung vor Block-Ankunft: p50 ${f(s.lead_p50)} · p90 ${f(s.lead_p90)} · min ${f(s.lead_min)} · max ${f(s.lead_max)} · Rückstand zur schnellsten Quelle p50 ${f(s.behind_best_p50)}`);
  }
  out.push("");
  out.push("Hinweise: Vorsprung = vor ANKUNFT des Blocks bei uns, nicht vor Blockproduktion. HASH-Modus misst Hash-Ankunft, nicht die vollständige TX.");
  out.push("Ein Launch ohne Pending-Sichtung ging vermutlich privat an einen Builder — Vermutung, kein Nachweis.");
  return out.join("\n") + "\n";
}

// ---------- Live-Teil ----------
function startSource(src, state, stats, windowMs) {
  let ws = null, closed = false, attempt = 0, id = 0;
  const subs = new Map(); // subscription id -> kind
  const pend = new Map(); // request id -> kind
  stats[src.name] = { connectedMs: 0, reconnects: 0, errors: 0, rateLimited: 0, pendingMsgs: 0, pendingObj: 0, pendingStr: 0, upSince: null };
  state.pending[src.name] = new Map();
  state.down[src.name] = [[nowNs(), null]];
  const send = (kind, params) => { const i = ++id; pend.set(i, kind); ws.send(JSON.stringify({ jsonrpc: "2.0", id: i, method: "eth_subscribe", params })); };
  const markUp = () => { const d = state.down[src.name].at(-1); if (d && d[1] === null) d[1] = nowNs(); stats[src.name].upSince = nowNs(); };
  const markDown = () => {
    const st = stats[src.name];
    if (st.upSince !== null) { st.connectedMs += ms(nowNs() - st.upSince); st.upSince = null; }
    const d = state.down[src.name].at(-1); if (!d || d[1] !== null) state.down[src.name].push([nowNs(), null]);
  };
  const connect = () => {
    if (closed) return;
    ws = new WebSocket(src.url);
    ws.onopen = () => {
      send(state.mode[src.name] === "HASH" ? "pending_hash" : "pending_full", state.mode[src.name] === "HASH" ? ["newPendingTransactions"] : ["newPendingTransactions", true]);
      send("heads", ["newHeads"]);
      send("logs", ["logs", { address: Object.keys(VENUES), topics: [Object.values(VENUES).map((v) => v.topic)] }]);
    };
    ws.onmessage = (ev) => {
      const t = nowNs();
      let m; try { m = JSON.parse(typeof ev.data === "string" ? ev.data : Buffer.from(ev.data).toString()); } catch { stats[src.name].errors++; return; }
      if (m.id !== undefined) {
        const kind = pend.get(m.id); pend.delete(m.id);
        if (m.error) {
          stats[src.name].errors++;
          if (/limit|429|-32005|-32007|-32003/i.test(JSON.stringify(m.error))) stats[src.name].rateLimited++;
          if (kind === "pending_full") { state.mode[src.name] = "HASH"; send("pending_hash", ["newPendingTransactions"]); }
          else if (kind === "pending_hash") state.mode[src.name] = "NONE";
          return;
        }
        subs.set(m.result, kind);
        if (kind === "pending_full") { state.mode[src.name] = "FULL"; attempt = 0; markUp(); }
        if (kind === "pending_hash") { state.mode[src.name] = "HASH"; attempt = 0; markUp(); }
        return;
      }
      if (m.method !== "eth_subscription") return;
      const kind = subs.get(m.params?.subscription), r = m.params?.result;
      if (kind === "pending_full" || kind === "pending_hash") {
        const h = lower(typeof r === "string" ? r : r?.hash);
        if (!h) return;
        stats[src.name].pendingMsgs++;
        if (typeof r === "string") stats[src.name].pendingStr++; else stats[src.name].pendingObj++;
        const map = state.pending[src.name];
        if (!map.has(h)) map.set(h, t);
        const L = state.launches.get(h);
        if (L && !L.pendingNs.has(src.name)) L.pendingNs.set(src.name, map.get(h));
      } else if (kind === "heads") {
        const b = Number(r?.number); if (!Number.isFinite(b)) return;
        if (!state.heads.has(b)) state.heads.set(b, new Map());
        if (!state.heads.get(b).has(src.name)) state.heads.get(b).set(src.name, t);
      } else if (kind === "logs") {
        if (r?.removed) return;
        const v = VENUES[lower(r?.address)]; if (!v || lower(r?.topics?.[0]) !== v.topic) return;
        const tx = lower(r.transactionHash);
        if (!state.launches.has(tx)) {
          const L = { venue: v.venue, block: Number(r.blockNumber), logNs: new Map(), pendingNs: new Map() };
          for (const n of state.names) { const pt = state.pending[n]?.get(tx); if (pt !== undefined) L.pendingNs.set(n, pt); }
          state.launches.set(tx, L);
          state.launchBlocks.add(L.block);
        }
        const L = state.launches.get(tx); if (!L.logNs.has(src.name)) L.logNs.set(src.name, t);
      }
    };
    ws.onerror = () => { stats[src.name].errors++; };
    ws.onclose = () => {
      markDown(); subs.clear(); pend.clear();
      if (closed) return;
      stats[src.name].reconnects++;
      setTimeout(connect, Math.min(30000, 500 * 2 ** attempt++));
    };
  };
  connect();
  return () => { closed = true; markDown(); try { ws?.close(); } catch {} };
}

export async function runLive(env, writeFile = fs.writeFileSync, log = (s) => process.stderr.write(s + "\n")) {
  if (typeof WebSocket !== "function") throw new Error("Node >= 22 erforderlich (eingebautes WebSocket fehlt)");
  const sources = parseSources(env.SOURCES);
  const durationS = Number(env.DURATION_S ?? 600), ttlS = Number(env.PENDING_TTL_S ?? 300), windowMs = Number(env.DOWN_WINDOW_MS ?? 3000);
  const pruneMs = Number(env.PRUNE_INTERVAL_MS ?? 10000);
  if (!(durationS > 0) || !(ttlS > 0) || !(windowMs > 0) || !(pruneMs > 0)) throw new Error("DURATION_S, PENDING_TTL_S, DOWN_WINDOW_MS und PRUNE_INTERVAL_MS müssen > 0 sein");
  const names = sources.map((s) => s.name);
  const state = { names, mode: Object.fromEntries(names.map((n) => [n, "PENDING"])), pending: {}, heads: new Map(), launches: new Map(), launchBlocks: new Set(), down: {}, windowMs };
  const stats = {};
  const started = new Date(), t0 = nowNs();
  log(`wss_race: ${names.join(", ")} · ${durationS} s · nur lesend`);
  const stops = sources.map((s) => startSource(s, state, stats, windowMs));
  const prune = setInterval(() => {
    const limit = nowNs() - BigInt(ttlS) * 1000000000n;
    for (const n of names) for (const [h, t] of state.pending[n]) { if (t < limit) state.pending[n].delete(h); else break; }
    for (const [b, m] of state.heads) if (!state.launchBlocks.has(b) && [...m.values()].every((t) => t < limit)) state.heads.delete(b);
  }, pruneMs);
  await new Promise((resolve) => {
    const timer = setTimeout(resolve, durationS * 1000);
    process.once("SIGINT", () => { clearTimeout(timer); resolve(); });
  });
  clearInterval(prune);
  stops.forEach((s) => s());
  for (const n of names) {
    if (stats[n].pendingObj > 0) state.mode[n] = stats[n].pendingStr > 0 ? "MIXED" : "FULL";
    else if (stats[n].pendingStr > 0) state.mode[n] = "HASH";
    else if (state.mode[n] === "PENDING" || state.mode[n] === "FULL" || state.mode[n] === "HASH") state.mode[n] = "NONE";
  }
  const result = analyze(state);
  const meta = { startedIso: started.toISOString(), endedIso: new Date().toISOString(), durationS: ms(nowNs() - t0) / 1000 };
  const prefix = env.OUT_PREFIX ?? "wss_race";
  writeFile(`${prefix}_launches.csv`, toCsv(result, names));
  const text = summaryText(result, names, stats, meta);
  writeFile(`${prefix}_summary.txt`, text);
  log(text);
  return result;
}

const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) runLive(process.env).then(() => process.exit(0), (e) => { process.stderr.write(`FEHLER: ${e.message}\n`); process.exit(2); });
