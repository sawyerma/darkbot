import {
  WelcomeStatus,
  type Heartbeat,
  type ServerMessage,
  type Welcome,
} from "../generated/darkbot/control/v1/stream_pb";
import { DecodeError, decodeServer, encodeClient } from "./codec";
import { PROTOCOL_MAJOR, PROTOCOL_MINOR, SCHEMA_ID } from "./schemaId";

// Genau EINE WebSocket-Verbindung zum same-origin Gateway. Zuständig für Socket, Handshake,
// Heartbeat/Pong, Liveness (inbound-frame-age, visibilitychange) und Reconnect.
// Topic-Logik liegt in TopicSubscriptions; diese Klasse kennt keine Topics.

export type ConnectionStatus =
  | "IDLE"
  | "CONNECTING"
  | "HANDSHAKE"
  | "OPEN"
  | "STALE"      // Heartbeats bleiben aus; Reconnect läuft
  | "BLOCKED"    // Welcome REJECTED_* oder Envelope nicht decodierbar: fail-closed, kein Reconnect
  | "CLOSED";

export interface Limits {
  maxStateFullBytes: number;
  heartbeatIntervalMs: number;
  staleAfterMissedHeartbeats: number;
  unsubscribeGraceMs: number;
  maxCatchupRetries: number;
}

// Startkandidaten laut Wire V1.3 §5; das Gateway überschreibt sie im Welcome.
export const DEFAULT_LIMITS: Limits = {
  maxStateFullBytes: 262_144,
  heartbeatIntervalMs: 15_000,
  staleAfterMissedHeartbeats: 2,
  unsubscribeGraceMs: 2_000,
  maxCatchupRetries: 3,
};

export interface ConnectionInfo {
  status: ConnectionStatus;
  reason: string;
  connectionId: string;
  limits: Limits;
  lastInboundAtMs: number | null;
}

type FrameListener = (msg: ServerMessage) => void;
type StatusListener = (info: ConnectionInfo) => void;
type WelcomeListener = (welcome: Welcome) => void;

const HANDSHAKE_TIMEOUT_MS = 5_000;
const RECONNECT_BASE_MS = 500;
const RECONNECT_MAX_MS = 10_000;
const RECONNECT_JITTER_MS = 500;
// Marker of the one reload after a gateway rejected this bundle's schema or major version.
const SKEW_RELOAD_KEY = "darkbot.gateway.skewReloadFor";

/** One reload per server schema; a second rejection for the same server schema stays BLOCKED. */
export function shouldReloadForSkew(status: WelcomeStatus, serverSchemaId: string, marker: string | null): boolean {
  const skew = status === WelcomeStatus.REJECTED_SCHEMA || status === WelcomeStatus.REJECTED_MAJOR;
  return skew && marker !== (serverSchemaId || "unknown");
}

function clearSkewMarker(): void {
  try {
    if (typeof sessionStorage !== "undefined") sessionStorage.removeItem(SKEW_RELOAD_KEY);
  } catch {
    // storage unavailable: nothing to clear
  }
}

// Nur ein Pfad, nie ein fremder Host: same-origin ist Vertrag (Wire V1.3 §1/§7).
function resolveSameOriginUrl(path: string): string {
  const proto = window.location.protocol === "https:" ? "wss" : "ws";
  const p = path.startsWith("/") ? path : `/${path}`;
  return `${proto}://${window.location.host}${p}`;
}

export class GatewayConnection {
  private ws: WebSocket | null = null;
  private info: ConnectionInfo = {
    status: "IDLE",
    reason: "",
    connectionId: "",
    limits: DEFAULT_LIMITS,
    lastInboundAtMs: null,
  };
  private frameListeners = new Set<FrameListener>();
  private statusListeners = new Set<StatusListener>();
  private welcomeListeners = new Set<WelcomeListener>();

  private requestCounter = 0;
  private reconnectAttempt = 0;
  private reconnectTimer: number | null = null;
  private handshakeTimer: number | null = null;
  private livenessTimer: number | null = null;
  private manuallyClosed = false;
  private visibilityBound = false;

  constructor(private readonly path = "/ws/v1/gateway") {}

  // ---------------------------------------------------------------- public

  getInfo(): ConnectionInfo {
    return this.info;
  }

  get limits(): Limits {
    return this.info.limits;
  }

  get isOpen(): boolean {
    return this.info.status === "OPEN" && this.ws?.readyState === WebSocket.OPEN;
  }

  nextRequestId(): number {
    this.requestCounter = (this.requestCounter + 1) >>> 0;
    if (this.requestCounter === 0) this.requestCounter = 1;
    return this.requestCounter;
  }

  connect(): void {
    if (this.ws || this.info.status === "CONNECTING" || this.info.status === "BLOCKED") return;
    this.manuallyClosed = false;
    this.bindVisibility();
    this.setStatus("CONNECTING");

    void this.open();
  }

  private async open(): Promise<void> {
    try {
      const response = await fetch("/api/v1/session", { method: "GET", credentials: "same-origin", cache: "no-store" });
      if (!response.ok) throw new Error(`SESSION_HTTP_${response.status}`);
    } catch {
      if (!this.manuallyClosed && this.info.status === "CONNECTING") {
        this.setStatus("STALE", "SESSION_BOOTSTRAP_FAILED");
        this.scheduleReconnect();
      }
      return;
    }
    if (this.manuallyClosed || this.info.status !== "CONNECTING") return;
    const ws = new WebSocket(resolveSameOriginUrl(this.path));
    ws.binaryType = "arraybuffer";
    this.ws = ws;

    ws.onopen = () => {
      if (this.ws !== ws) return;
      this.setStatus("HANDSHAKE");
      this.sendHello();
      this.handshakeTimer = window.setTimeout(() => {
        if (this.ws !== ws) return;
        this.setStatus("STALE", "HANDSHAKE_TIMEOUT");
        ws.close();
      }, HANDSHAKE_TIMEOUT_MS);
    };

    ws.onmessage = (ev: MessageEvent) => {
      if (this.ws !== ws) return;
      if (!(ev.data instanceof ArrayBuffer)) {
        this.block("PROTOCOL_VIOLATION: non-binary frame");
        return;
      }
      let msg: ServerMessage;
      try {
        msg = decodeServer(ev.data);
      } catch (e) {
        // Envelope selbst nicht lesbar: Schemafamilie passt nicht. Stabil blockieren, kein Loop.
        this.block(`SCHEMA_INVALID: ${e instanceof DecodeError ? e.message : String(e)}`);
        return;
      }
      this.info = { ...this.info, lastInboundAtMs: nowMs() };
      this.dispatch(msg);
    };

    ws.onerror = () => {
      // onclose folgt immer; dort wird entschieden.
    };

    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.clearTimers();
      if (this.info.status !== "BLOCKED") {
        this.setStatus(this.manuallyClosed ? "CLOSED" : "STALE", this.manuallyClosed ? "" : "SOCKET_CLOSED");
        if (!this.manuallyClosed) this.scheduleReconnect();
      }
    };
  }

  close(): void {
    this.manuallyClosed = true;
    this.clearTimers();
    if (this.reconnectTimer !== null) window.clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    const ws = this.ws;
    this.ws = null;
    ws?.close();
    this.setStatus("CLOSED");
  }

  // Fataler Auth-/Protokollfehler: kein automatischer reconnect/fallback.
  blockProtocol(reason: string): void {
    this.block(reason);
  }

  // Nach BLOCKED nur durch bewussten Aufruf wieder versuchen (z. B. nach Deployment).
  resetBlock(): void {
    if (this.info.status !== "BLOCKED") return;
    this.setStatus("IDLE");
    this.reconnectAttempt = 0;
  }

  send(init: Parameters<typeof encodeClient>[0]): boolean {
    const ws = this.ws;
    if (!ws || ws.readyState !== WebSocket.OPEN) return false;
    ws.send(encodeClient(init));
    return true;
  }

  onFrame(fn: FrameListener): () => void {
    this.frameListeners.add(fn);
    return () => this.frameListeners.delete(fn);
  }

  onStatus(fn: StatusListener): () => void {
    this.statusListeners.add(fn);
    fn(this.info);
    return () => this.statusListeners.delete(fn);
  }

  onWelcome(fn: WelcomeListener): () => void {
    this.welcomeListeners.add(fn);
    return () => this.welcomeListeners.delete(fn);
  }

  // Wird von TopicSubscriptions bei Verbindungsfehlern aufgerufen (Error ohne Topic).
  forceReconnect(reason: string): void {
    if (this.info.status === "BLOCKED") return;
    this.setStatus("STALE", reason);
    const ws = this.ws;
    if (ws) ws.close();
    else this.scheduleReconnect();
  }

  // ---------------------------------------------------------------- handshake

  private sendHello(): void {
    this.send({
      msg: {
        case: "hello",
        value: {
          protocolMajor: PROTOCOL_MAJOR,
          protocolMinor: PROTOCOL_MINOR,
          schemaId: SCHEMA_ID,
          capabilities: { maxStateFullBytes: DEFAULT_LIMITS.maxStateFullBytes, acceptsStatePart: false },
        },
      },
    });
  }

  private dispatch(msg: ServerMessage): void {
    const m = msg.msg;
    if (this.info.status === "HANDSHAKE") {
      if (m.case !== "welcome") {
        this.block("PROTOCOL_VIOLATION: expected Welcome");
        return;
      }
      this.onWelcomeFrame(m.value);
      return;
    }
    if (!m.case || m.case === "welcome") {
      this.block("PROTOCOL_VIOLATION: unknown or duplicate server frame");
      return;
    }
    if (m.case === "heartbeat") this.onHeartbeat(m.value);
    try {
      for (const fn of this.frameListeners) fn(msg);
    } catch {
      this.block("PROTOCOL_VIOLATION: frame consumer threw");
    }
  }

  private onWelcomeFrame(w: Welcome): void {
    if (this.handshakeTimer !== null) window.clearTimeout(this.handshakeTimer);
    this.handshakeTimer = null;

    if (w.status !== WelcomeStatus.ACCEPTED) {
      const why = WelcomeStatus[w.status] ?? String(w.status);
      if (this.reloadForVersionSkew(w)) {
        this.block(`${why}: reloading the frontend that matches the gateway`);
        return;
      }
      this.block(`${why}${w.rejectReason ? `: ${w.rejectReason}` : ""}`);
      return;
    }
    if (w.protocolMajor !== PROTOCOL_MAJOR) {
      this.block(`MAJOR_MISMATCH: server ${w.protocolMajor}, client ${PROTOCOL_MAJOR}`);
      return;
    }
    const l = w.limits;
    const limits: Limits = l
      ? {
          maxStateFullBytes: l.maxStateFullBytes || DEFAULT_LIMITS.maxStateFullBytes,
          heartbeatIntervalMs: l.heartbeatIntervalMs || DEFAULT_LIMITS.heartbeatIntervalMs,
          staleAfterMissedHeartbeats: l.staleAfterMissedHeartbeats || DEFAULT_LIMITS.staleAfterMissedHeartbeats,
          unsubscribeGraceMs: l.unsubscribeGraceMs || DEFAULT_LIMITS.unsubscribeGraceMs,
          maxCatchupRetries: l.maxCatchupRetries || DEFAULT_LIMITS.maxCatchupRetries,
        }
      : DEFAULT_LIMITS;

    // F2-Major zwingend, minor/additive und schemaId als Diagnose:
    // SHA-Unterschied ALLEIN ist kein Inkompatibilitätsbeweis.
    if (w.protocolMinor < 0 || w.protocolMajor !== PROTOCOL_MAJOR) {
      this.block("INCOMPATIBLE_PROTOCOL");
      return;
    }
    this.reconnectAttempt = 0;
    clearSkewMarker();
    this.info = { ...this.info, connectionId: w.connectionId, limits };
    this.setStatus("OPEN");
    this.startLivenessWatch();
    for (const fn of this.welcomeListeners) fn(w);
  }

  // Version skew (gateway deployed after this page loaded) is not an attack: the page reloads once
  // from the same origin and receives the matching bundle. A real incompatibility stays BLOCKED.
  private reloadForVersionSkew(w: Welcome): boolean {
    if (typeof window === "undefined" || typeof sessionStorage === "undefined") return false;
    try {
      if (!shouldReloadForSkew(w.status, w.schemaId, sessionStorage.getItem(SKEW_RELOAD_KEY))) return false;
      sessionStorage.setItem(SKEW_RELOAD_KEY, w.schemaId || "unknown");
    } catch {
      return false;
    }
    window.location.reload();
    return true;
  }

  // ---------------------------------------------------------------- heartbeat / liveness

  private onHeartbeat(h: Heartbeat): void {
    this.send({ msg: { case: "pong", value: { heartbeatId: h.heartbeatId } } });
  }

  private startLivenessWatch(): void {
    if (this.livenessTimer !== null) window.clearInterval(this.livenessTimer);
    // Grober Timer; in Hintergrund-Tabs gedrosselt. Entscheidend ist die Prüfung bei
    // visibilitychange und bei jedem eingehenden Frame (lastInboundAtMs).
    this.livenessTimer = window.setInterval(() => this.checkLiveness(), this.info.limits.heartbeatIntervalMs);
  }

  private checkLiveness(): void {
    if (this.info.status !== "OPEN" || this.info.lastInboundAtMs === null) return;
    const { heartbeatIntervalMs, staleAfterMissedHeartbeats } = this.info.limits;
    const maxSilenceMs = heartbeatIntervalMs * (staleAfterMissedHeartbeats + 1);
    if (nowMs() - this.info.lastInboundAtMs > maxSilenceMs) {
      this.forceReconnect("HEARTBEAT_MISSED");
    }
  }

  private bindVisibility(): void {
    if (this.visibilityBound || typeof document === "undefined") return;
    this.visibilityBound = true;
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "visible") this.checkLiveness();
    });
  }

  // ---------------------------------------------------------------- reconnect / status

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null || this.manuallyClosed || this.info.status === "BLOCKED") return;
    const exp = Math.min(RECONNECT_MAX_MS, RECONNECT_BASE_MS * 2 ** this.reconnectAttempt);
    const delay = exp + Math.floor(Math.random() * RECONNECT_JITTER_MS);
    this.reconnectAttempt += 1;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }

  private block(reason: string): void {
    this.clearTimers();
    const ws = this.ws;
    this.ws = null;
    this.setStatus("BLOCKED", reason);
    ws?.close();
  }

  private clearTimers(): void {
    if (this.handshakeTimer !== null) window.clearTimeout(this.handshakeTimer);
    if (this.livenessTimer !== null) window.clearInterval(this.livenessTimer);
    this.handshakeTimer = null;
    this.livenessTimer = null;
  }

  private setStatus(status: ConnectionStatus, reason = ""): void {
    this.info = { ...this.info, status, reason };
    for (const fn of this.statusListeners) fn(this.info);
  }
}

function nowMs(): number {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}
