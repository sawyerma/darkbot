import type { RealtimeEnvelope, RealtimeStatus } from "./protocol";

type MessageListener = (message: RealtimeEnvelope) => void;
type StatusListener = (status: RealtimeStatus) => void;
type GapListener = (expected: number, received: number) => void;

const configuredUrl = import.meta.env.VITE_WS_URL as string | undefined;

function resolveUrl(): string {
  if (configuredUrl?.trim()) {
    if (/^wss?:\/\//.test(configuredUrl)) return configuredUrl;
    const proto = window.location.protocol === "https:" ? "wss" : "ws";
    return `${proto}://${window.location.host}${configuredUrl.startsWith("/") ? "" : "/"}${configuredUrl}`;
  }
  const proto = window.location.protocol === "https:" ? "wss" : "ws";
  return `${proto}://${window.location.host}/ws/v1/events`;
}

export class RealtimeClient {
  private ws: WebSocket | null = null;
  private status: RealtimeStatus = "IDLE";
  private listeners = new Set<MessageListener>();
  private statusListeners = new Set<StatusListener>();
  private gapListeners = new Set<GapListener>();
  private reconnectAttempt = 0;
  private reconnectTimer: number | null = null;
  private lastSeq: number | null = null;
  private manuallyClosed = false;

  connect() {
    if (this.ws || this.status === "CONNECTING") return;
    this.manuallyClosed = false;
    this.setStatus("CONNECTING");
    const ws = new WebSocket(resolveUrl());
    this.ws = ws;

    ws.onopen = () => {
      if (this.ws !== ws) return;
      this.reconnectAttempt = 0;
      this.setStatus("OPEN");
    };

    ws.onmessage = (event) => {
      if (this.ws !== ws || typeof event.data !== "string") return;
      let parsed: RealtimeEnvelope;
      try { parsed = JSON.parse(event.data) as RealtimeEnvelope; } catch { return; }
      if (!Number.isFinite(parsed.seq) || typeof parsed.type !== "string") return;
      if (this.lastSeq !== null && parsed.seq !== this.lastSeq + 1) {
        for (const listener of this.gapListeners) listener(this.lastSeq + 1, parsed.seq);
      }
      this.lastSeq = parsed.seq;
      for (const listener of this.listeners) listener(parsed);
    };

    ws.onerror = () => {
      if (this.ws !== ws) return;
      this.setStatus("ERROR");
    };

    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.setStatus("CLOSED");
      if (!this.manuallyClosed) this.scheduleReconnect();
    };
  }

  close() {
    this.manuallyClosed = true;
    if (this.reconnectTimer !== null) window.clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    const ws = this.ws;
    this.ws = null;
    ws?.close();
    this.setStatus("CLOSED");
  }

  subscribe(listener: MessageListener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  onStatus(listener: StatusListener) {
    this.statusListeners.add(listener);
    listener(this.status);
    return () => this.statusListeners.delete(listener);
  }

  onGap(listener: GapListener) {
    this.gapListeners.add(listener);
    return () => this.gapListeners.delete(listener);
  }

  private setStatus(status: RealtimeStatus) {
    this.status = status;
    for (const listener of this.statusListeners) listener(status);
  }

  private scheduleReconnect() {
    if (this.reconnectTimer !== null || this.manuallyClosed) return;
    this.reconnectAttempt += 1;
    const delay = Math.min(5000, 250 * 2 ** (this.reconnectAttempt - 1));
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }
}

export const realtimeClient = new RealtimeClient();
