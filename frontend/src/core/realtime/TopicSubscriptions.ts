import {
  StartPolicy,
  type ServerMessage,
  type Welcome,
} from "../generated/darkbot/control/v1/stream_pb";
import { TopicClass, type TopicId } from "../generated/darkbot/control/v1/topics_pb";
import type { GatewayConnection } from "./GatewayConnection";
import { TopicTracker, eventMetaOf, stateMetaOf, type EventMeta, type StateMeta, type TopicStatus } from "./TopicState";
import { bindingOf, checkDescriptor, wireNameOf } from "./topicTable";

// Refcount pro Topic, Unsubscribe-Grace, Re-Subscribe nach Reconnect, Routing der Frames
// in die TopicTracker, Retry-Cap mit Backoff+Jitter. Kennt keine Domänentypen:
// Adapter bekommen Payload-Bytes plus Meta und decodieren selbst.

export type StateListener = (payload: Uint8Array, meta: StateMeta) => void;
export type EventListener = (payload: Uint8Array, meta: EventMeta) => void;
export type TopicStatusListener = (status: TopicStatus, reason: string) => void;

export interface AcquireOptions {
  // Nur EVENT ohne bekannten Cursor. Default LIVE_FROM_BOUNDARY.
  startPolicy?: StartPolicy;
}

interface Entry {
  tracker: TopicTracker;
  refs: number;
  subscribed: boolean;           // Subscribe gesendet und nicht per Unsubscribe beendet
  graceTimer: number | null;
  retryTimer: number | null;
  startPolicy: StartPolicy;
  stateListeners: Set<StateListener>;
  eventListeners: Set<EventListener>;
  statusListeners: Set<TopicStatusListener>;
  descriptorOk: boolean | null;  // null = kein Welcome gesehen
  descriptorReason: string;
}

const RETRY_BASE_MS = 1_000;
const RETRY_JITTER_MS = 500;

export class TopicSubscriptions {
  private entries = new Map<TopicId, Entry>();
  private pending = new Map<number, { topic: TopicId; kind: "SUBSCRIBE" | "UNSUBSCRIBE" }>();

  constructor(private readonly conn: GatewayConnection) {
    conn.onFrame((m) => this.onFrame(m));
    conn.onWelcome((w) => this.onWelcome(w));
    conn.onStatus((info) => {
      if (info.status !== "OPEN") this.onConnectionDown();
    });
  }

  // ---------------------------------------------------------------- public

  acquire(topic: TopicId, opts: AcquireOptions = {}): () => void {
    const e = this.entry(topic);
    if (opts.startPolicy) e.startPolicy = opts.startPolicy;
    e.refs += 1;
    if (e.graceTimer !== null) {
      window.clearTimeout(e.graceTimer);
      e.graceTimer = null;
    }
    if (e.refs === 1) this.ensureSubscribed(e);

    let released = false;
    return () => {
      if (released) return;
      released = true;
      e.refs = Math.max(0, e.refs - 1);
      if (e.refs === 0) this.scheduleRelease(e);
    };
  }

  onState(topic: TopicId, fn: StateListener): () => void {
    const e = this.entry(topic);
    e.stateListeners.add(fn);
    return () => e.stateListeners.delete(fn);
  }

  onEvent(topic: TopicId, fn: EventListener): () => void {
    const e = this.entry(topic);
    e.eventListeners.add(fn);
    return () => e.eventListeners.delete(fn);
  }

  onTopicStatus(topic: TopicId, fn: TopicStatusListener): () => void {
    const e = this.entry(topic);
    e.statusListeners.add(fn);
    fn(e.tracker.status, e.tracker.statusReason);
    return () => e.statusListeners.delete(fn);
  }

  statusOf(topic: TopicId): TopicStatus {
    return this.entries.get(topic)?.tracker.status ?? "IDLE";
  }

  reasonOf(topic: TopicId): string {
    return this.entries.get(topic)?.tracker.statusReason ?? "";
  }

  // Manueller Retry nach ERROR/INCOMPLETE mit Retry-Cap.
  retry(topic: TopicId): void {
    const e = this.entries.get(topic);
    if (!e || e.refs === 0 || e.descriptorOk !== true) return;
    e.tracker.manualRetry();
    // Nach RESET_REQUIRED/epoch rollover zunächst bewusste StartPolicy/Source prüfen.
    // Nie automatisch über LIVE_FROM_BOUNDARY still ein History-Gap überspringen.
    if (e.tracker.statusReason.startsWith("RESET_REQUIRED") ||
        e.tracker.statusReason.startsWith("EPOCH_ROLLOVER")) return;
    this.resubscribe(e);
  }

  // ---------------------------------------------------------------- subscribe lifecycle

  private ensureSubscribed(e: Entry): void {
    if (e.subscribed || !this.conn.isOpen || e.tracker.status === "UNAVAILABLE" ||
        e.tracker.status === "ERROR") return;
    if (e.descriptorOk !== true) {
      e.tracker.markUnavailable(e.descriptorReason);
      this.emitStatus(e);
      return;
    }
    const t = e.tracker;
    const requestId = this.conn.nextRequestId();
    const cursor = t.resumeCursor();
    const isEvent = t.cls === TopicClass.EVENT;

    const sent = this.conn.send({
      msg: {
        case: "subscribe",
        value: {
          requestId,
          topic: t.topic,
          cursor: isEvent && cursor ? { journalEpoch: cursor.epoch, durableCursor: cursor.cursor } : undefined,
          startPolicy: isEvent && !cursor ? e.startPolicy : StartPolicy.UNSPECIFIED,
        },
      },
    });
    if (!sent) return;
    e.subscribed = true;
    this.pending.set(requestId, { topic: t.topic, kind: "SUBSCRIBE" });
    t.markSubscribing();
    this.emitStatus(e);
  }

  private scheduleRelease(e: Entry): void {
    if (e.graceTimer !== null) return;
    e.graceTimer = window.setTimeout(() => {
      e.graceTimer = null;
      if (e.refs > 0) return;
      this.sendUnsubscribe(e);
    }, this.conn.limits.unsubscribeGraceMs);
  }

  private sendUnsubscribe(e: Entry): void {
    if (e.retryTimer !== null) {
      window.clearTimeout(e.retryTimer);
      e.retryTimer = null;
    }
    if (e.subscribed && this.conn.isOpen) {
      const requestId = this.conn.nextRequestId();
      this.conn.send({ msg: { case: "unsubscribe", value: { requestId, topic: e.tracker.topic } } });
      this.pending.set(requestId, { topic: e.tracker.topic, kind: "UNSUBSCRIBE" });
    }
    e.subscribed = false;
    e.tracker.markUnsubscribed();
    this.emitStatus(e);
  }

  private resubscribe(e: Entry): void {
    if (e.subscribed && this.conn.isOpen) {
      const requestId = this.conn.nextRequestId();
      this.conn.send({ msg: { case: "unsubscribe", value: { requestId, topic: e.tracker.topic } } });
      this.pending.set(requestId, { topic: e.tracker.topic, kind: "UNSUBSCRIBE" });
    }
    e.subscribed = false;
    this.ensureSubscribed(e);
  }

  private scheduleRetry(e: Entry): void {
    if (e.retryTimer !== null || e.refs === 0 || e.descriptorOk !== true) return;
    const n = e.tracker.retries;
    const delay = RETRY_BASE_MS * 2 ** Math.max(0, n - 1) + Math.floor(Math.random() * RETRY_JITTER_MS);
    e.retryTimer = window.setTimeout(() => {
      e.retryTimer = null;
      if (e.refs > 0) this.resubscribe(e);
    }, delay);
  }

  private onConnectionDown(): void {
    this.pending.clear();
    for (const e of this.entries.values()) {
      e.subscribed = false;
      if (e.retryTimer !== null) {
        window.clearTimeout(e.retryTimer);
        e.retryTimer = null;
      }
      e.tracker.markConnectionLost();
      this.emitStatus(e);
    }
  }

  // Nach Welcome: Descriptoren prüfen, dann alle referenzierten Topics neu abonnieren.
  private onWelcome(w: Welcome): void {
    const seen = new Set<TopicId>();
    for (const d of w.topics) {
      seen.add(d.topic);
      const e = this.entry(d.topic);
      const r = checkDescriptor(d);
      e.descriptorOk = r.ok;
      e.descriptorReason = r.ok ? "" : `SCHEMA_INVALID:${r.reason}`;
    }
    this.pending.clear();
    for (const e of this.entries.values()) {
      if (!seen.has(e.tracker.topic)) {
        e.descriptorOk = false;
        e.descriptorReason = "SCHEMA_INVALID:NOT_OFFERED";
      }
      e.subscribed = false;
      if (e.refs > 0) this.ensureSubscribed(e);
    }
  }

  // ---------------------------------------------------------------- frame routing

  private onFrame(m: ServerMessage): void {
    const v = m.msg;
    switch (v.case) {
      case "controlAck": {
        const p = this.pending.get(v.value.requestId);
        this.pending.delete(v.value.requestId);
        const e = this.entries.get(v.value.topic);
        if (!e || !p || p.topic !== v.value.topic || p.kind !== "SUBSCRIBE") return;
        const r = e.tracker.onAck(v.value);
        if (r === "RESET_REQUIRED") {
          // Keine automatische Re-Subscribe-Policy, die Journalhistorie überspringt.
          e.subscribed = false;
        } else if (r === "REJECTED") {
          e.subscribed = false;
        }
        this.emitStatus(e);
        return;
      }
      case "stateFull": {
        const f = v.value;
        const e = this.entries.get(f.topic);
        if (!e || e.refs === 0 || !e.subscribed || e.descriptorOk !== true ||
            e.tracker.cls !== TopicClass.STATE || e.tracker.status === "UNAVAILABLE") return;
        if (f.payload.byteLength > this.conn.limits.maxStateFullBytes) {
          e.tracker.markUnavailable("FRAME_TOO_LARGE");
          this.emitStatus(e);
          return;
        }
        // Adapter muss vollständig und synchron decodieren, BEVOR wir seq committen.
        try {
          if (e.stateListeners.size === 0) {
            e.tracker.markUnavailable("STATE_ADAPTER_MISSING");
            this.emitStatus(e);
            return;
          }
          const a = e.tracker.applied;
          if (a !== null && (f.generation < a.generation ||
              (f.generation === a.generation && f.seq < a.seq))) return;
          const meta = stateMetaOf(f);
          for (const fn of e.stateListeners) fn(f.payload, meta);
          e.tracker.onStateFull(f);
        } catch {
          e.tracker.markUnavailable("STATE_SCHEMA_INVALID_OR_ADAPTER_FAILED");
        }
        this.emitStatus(e);
        return;
      }
      case "event": {
        const ev = v.value;
        const e = this.entries.get(ev.topic);
        if (!e || e.refs === 0 || !e.subscribed || e.descriptorOk !== true ||
            e.tracker.cls !== TopicClass.EVENT || e.tracker.status === "UNAVAILABLE") return;
        const d = e.tracker.previewEvent(ev);
        if (d === "APPLY") {
          try {
            if (e.eventListeners.size === 0) throw Error("EVENT_ADAPTER_MISSING");
            const meta = eventMetaOf(ev);
            for (const fn of e.eventListeners) fn(ev.payload, meta);
            e.tracker.onEvent(ev);
          } catch {
            e.tracker.markProofConflict("EVENT_APPLY_FAILED");
          }
        } else if (d === "EPOCH_MISMATCH") {
          e.tracker.onEpochMismatch(ev.cursor?.journalEpoch ?? "");
        } else if (d === "PROOF_CONFLICT") {
          e.tracker.markProofConflict("PROOF_CONFLICT");
        }
        this.emitStatus(e);
        return;
      }
      case "catchupComplete": {
        const e = this.entries.get(v.value.topic);
        if (!e || e.refs === 0 || !e.subscribed || e.tracker.cls !== TopicClass.EVENT) return;
        e.tracker.onCatchupComplete(v.value);
        this.emitStatus(e);
        return;
      }
      case "heartbeat": {
        for (const w of v.value.stateWatermarks) {
          const e = this.entries.get(w.topic);
          if (!e) continue;
          if (e.tracker.onWatermark(w)) {
            // Lokal fehlt ein bereits gesendeter Frame: einmal gezielt neu abonnieren.
            e.tracker.retries += 1;
            if (e.tracker.retries > this.conn.limits.maxCatchupRetries) {
              e.tracker.status = "ERROR";
              e.tracker.statusReason = "WATERMARK_AHEAD:RETRY_CAP";
            } else {
              this.scheduleRetry(e);
            }
            this.emitStatus(e);
          }
        }
        return;
      }
      case "error": {
        const err = v.value;
        const e = err.topic ? this.entries.get(err.topic) : undefined;
        if (!e) {
          // Fataler Auth-/Schemafehler darf nicht zu einer Reconnect-Schleife werden.
          if (!err.retryable) this.conn.blockProtocol(`ERROR:${err.message || err.code}`);
          else this.conn.forceReconnect(`ERROR:${err.message || err.code}`);
          return;
        }
        const d = e.tracker.onError(err, this.conn.limits.maxCatchupRetries);
        if (d === "CONNECTION") {
          this.conn.blockProtocol(`ERROR:${err.message || err.code}`);
          return;
        }
        e.subscribed = false;
        if (d === "RETRY") this.scheduleRetry(e);
        this.emitStatus(e);
        return;
      }
      default:
        return;
    }
  }

  // ---------------------------------------------------------------- helpers

  private entry(topic: TopicId): Entry {
    let e = this.entries.get(topic);
    if (e) return e;
    const binding = bindingOf(topic);
    const cls = binding?.cls ?? TopicClass.STATE;
    e = {
      tracker: new TopicTracker(topic, cls),
      refs: 0,
      subscribed: false,
      graceTimer: null,
      retryTimer: null,
      startPolicy: StartPolicy.LIVE_FROM_BOUNDARY,
      stateListeners: new Set(),
      eventListeners: new Set(),
      statusListeners: new Set(),
      descriptorOk: null,
      descriptorReason: "",
    };
    if (!binding) {
      e.descriptorOk = false;
      e.descriptorReason = `SCHEMA_INVALID:UNKNOWN_TOPIC ${wireNameOf(topic)}`;
    }
    this.entries.set(topic, e);
    return e;
  }

  private emitStatus(e: Entry): void {
    for (const fn of e.statusListeners) fn(e.tracker.status, e.tracker.statusReason);
  }
}
