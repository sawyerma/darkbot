import {
  AckReason,
  AckStatus,
  CatchupQuality,
  CatchupReason,
  ErrorCode,
  EvidenceScope,
  StateQuality,
  type CatchupComplete,
  type ControlAck,
  type Event,
  type Error as WireError,
  type StateFull,
  type StateWatermark,
} from "../generated/darkbot/control/v1/stream_pb";
import { TopicClass, type TopicId } from "../generated/darkbot/control/v1/topics_pb";

// Reine Topic-Zustandsmaschine ohne Socket. Setzt die Apply-Regeln des Wire-Vertrags V1.3
// (§2 STATE, §3/§4 EVENT, §5 Heartbeat) um und ist ohne Browser unit-testbar.

export type TopicStatus =
  | "IDLE"          // nicht abonniert
  | "SUBSCRIBING"   // Subscribe gesendet, kein Ack/Frame
  | "LIVE"          // STATE: aktueller Wert; EVENT: Catch-up COMPLETE, Live-Stream läuft
  | "CATCHING_UP"   // EVENT: Archiv (C,H] läuft
  | "STALE"         // Verbindung weg oder Watermark > applied; Daten alt
  | "DETACHED"      // STATE: Gateway hat Engine verloren (SOURCE_DETACHED)
  | "INCOMPLETE"    // EVENT: Nachzug nicht beweisbar vollständig
  | "UNAVAILABLE"   // Topic existiert/darf nicht (stabil, kein Retry)
  | "ERROR";        // Retry-Cap erreicht oder fataler Topic-Fehler; manueller Retry

export interface StateMeta {
  generation: bigint;
  seq: bigint;
  quality: StateQuality;
  sourceTimestampUnixNs: bigint;
  sourceId: string;
}

export interface EventMeta {
  epoch: string;
  cursor: bigint;
  eventId: string;
  sourceId: string;
  committedAtUnixNs: bigint;
  evidenceRef: string;
}

export interface CursorPair {
  epoch: string;
  cursor: bigint;
}

export type StateDecision = "APPLY" | "DISCARD_OLD_GENERATION" | "DISCARD_OLD_SEQ";
export type EventDecision = "APPLY" | "DUPLICATE" | "EPOCH_MISMATCH" | "PROOF_CONFLICT";
// HOLD: topic value unavailable, subscription kept; the next valid STATE restores it.
export type ErrorDecision = "RETRY" | "GIVE_UP" | "FATAL" | "CONNECTION" | "HOLD";

export class TopicTracker {
  status: TopicStatus = "IDLE";
  statusReason = "";
  retries = 0;

  // STATE
  applied: { generation: bigint; seq: bigint } | null = null;
  quality: StateQuality = StateQuality.UNSPECIFIED;

  // EVENT
  epoch: string | null = null;
  lastAppliedCursor: bigint | null = null;
  lastEventId: string | null = null;
  private lastEventPayload: Uint8Array | null = null;
  private eventSourceId: string | null = null;
  catchup: { through: CursorPair; quality: CatchupQuality; reason: CatchupReason; evidenceScope: EvidenceScope } | null = null;

  lastFrameAtMs: number | null = null;

  constructor(readonly topic: TopicId, readonly cls: TopicClass) {}

  get isState(): boolean {
    return this.cls === TopicClass.STATE;
  }

  resumeCursor(): CursorPair | null {
    if (this.isState || this.epoch === null || this.lastAppliedCursor === null) return null;
    return { epoch: this.epoch, cursor: this.lastAppliedCursor };
  }

  // -------------------------------------------------------------- Lifecycle

  markSubscribing(): void {
    this.status = "SUBSCRIBING";
    this.statusReason = "";
  }

  markUnsubscribed(): void {
    // Cursor und applied bleiben erhalten: ein späteres Re-Subscribe resumiert daran.
    this.status = "IDLE";
    this.statusReason = "";
    this.retries = 0;
  }

  markConnectionLost(): void {
    if (this.status === "IDLE" || this.status === "UNAVAILABLE" || this.status === "ERROR") return;
    this.status = "STALE";
    this.statusReason = "CONNECTION_LOST";
  }

  markUnavailable(reason: string): void {
    this.status = "UNAVAILABLE";
    this.statusReason = reason;
  }

  /** FRAME_TOO_LARGE (Wire V1.3 §2): no current value, never LIVE; the subscription stays and the
   *  next valid state of the same publisher restores the topic. No retry, no resubscribe loop. */
  markTooLarge(detail = ""): void {
    this.status = "UNAVAILABLE";
    this.statusReason = `FRAME_TOO_LARGE${detail ? ` ${detail}` : ""}`;
  }

  /** UNAVAILABLE only until the publisher sends a state that fits again. */
  get awaitsValidState(): boolean {
    return this.status === "UNAVAILABLE" && this.statusReason.startsWith("FRAME_TOO_LARGE");
  }

  manualRetry(): void {
    this.retries = 0;
    this.statusReason = "";
  }

  // -------------------------------------------------------------- ControlAck

  onAck(ack: ControlAck): "OK" | "REJECTED" | "RESET_REQUIRED" {
    if (ack.status === AckStatus.ACCEPTED) {
      if (this.isState) {
        // Status bleibt SUBSCRIBING bis der erste StateFull kommt.
      } else {
        this.status = "CATCHING_UP";
      }
      return "OK";
    }
    if (ack.status === AckStatus.RESET_REQUIRED) {
      // Fail closed: niemals automatisch auf LIVE_FROM_BOUNDARY zurückfallen.
      // Originalcursor und Epoche bleiben zur Diagnose erhalten. Ein neuer Start
      // benötigt eine bewusste Retry-/StartPolicy-Entscheidung nach Evidence-Prüfung.
      this.status = "INCOMPLETE";
      this.statusReason = `RESET_REQUIRED:${AckReason[ack.reason] ?? ack.reason}`;
      return "RESET_REQUIRED";
    }
    const reason = AckReason[ack.reason] ?? String(ack.reason);
    this.markUnavailable(`REJECTED:${reason}${ack.detail ? ` ${ack.detail}` : ""}`);
    return "REJECTED";
  }

  // -------------------------------------------------------------- STATE

  // Wire V1.3 §2: neue Generation akzeptieren (seq zurücksetzen), gleiche Generation bei
  // seq >= applied ('>=' wegen Quality-Overlay), ältere Generation verwerfen.
  onStateFull(f: StateFull): StateDecision {
    this.lastFrameAtMs = now();
    const a = this.applied;
    if (a !== null) {
      if (f.generation < a.generation) return "DISCARD_OLD_GENERATION";
      if (f.generation === a.generation && f.seq < a.seq) return "DISCARD_OLD_SEQ";
    }
    this.applied = { generation: f.generation, seq: f.seq };
    this.quality = f.quality;
    this.retries = 0;
    switch (f.quality) {
      case StateQuality.SOURCE_DETACHED:
        this.status = "DETACHED";
        this.statusReason = "SOURCE_DETACHED";
        break;
      case StateQuality.STALE:
        this.status = "STALE";
        this.statusReason = "SOURCE_STALE";
        break;
      case StateQuality.OK:
        this.status = "LIVE";
        this.statusReason = "";
        break;
      default:
        this.status = "STALE";
        this.statusReason = "SOURCE_UNVERIFIED";
    }
    return "APPLY";
  }

  // Heartbeat-Watermark ist die höchste an DIESEN Client gesendete (generation, seq).
  // Liegt sie vor dem lokal applizierten Stand, fehlt ein Frame lokal (Decode/Apply-Fehler):
  // STALE und gezieltes Re-Subscribe dieses Topics.
  onWatermark(w: StateWatermark): boolean {
    if (!this.isState || this.status === "IDLE" || this.status === "SUBSCRIBING") return false;
    const a = this.applied;
    if (a === null) return false;
    const behind =
      w.generation > a.generation || (w.generation === a.generation && w.highestSeqSent > a.seq);
    if (behind) {
      this.status = "STALE";
      this.statusReason = "WATERMARK_AHEAD";
    }
    return behind;
  }

  // -------------------------------------------------------------- EVENT

  // Ersten EVENT nur nach erfolgreichem Domain-Adapter/Listener-Apply committen.
  // Cursor-Identität ist (source_id, journal_epoch, durable_cursor), nie event_id
  // als zusätzlicher Ordnungsschlüssel.
  previewEvent(e: Event): EventDecision {
    const c = e.cursor;
    if (!c || !c.journalEpoch || !e.sourceId || !e.eventId) return "PROOF_CONFLICT";
    if (this.epoch !== null && c.journalEpoch !== this.epoch) return "EPOCH_MISMATCH";
    if (this.eventSourceId !== null && e.sourceId !== this.eventSourceId) return "PROOF_CONFLICT";
    if (this.lastAppliedCursor !== null && c.durableCursor <= this.lastAppliedCursor) {
      if (c.durableCursor === this.lastAppliedCursor) {
        if (e.eventId !== this.lastEventId || !sameBytes(e.payload, this.lastEventPayload))
          return "PROOF_CONFLICT";
      }
      return "DUPLICATE";
    }
    return "APPLY";
  }

  onEvent(e: Event): EventDecision {
    const decision = this.previewEvent(e);
    if (decision !== "APPLY") return decision;
    const c = e.cursor!;
    this.lastFrameAtMs = now();
    this.epoch = c.journalEpoch;
    this.eventSourceId = e.sourceId;
    this.lastAppliedCursor = c.durableCursor;
    this.lastEventId = e.eventId;
    this.lastEventPayload = new Uint8Array(e.payload);
    if (this.status === "SUBSCRIBING") this.status = "CATCHING_UP";
    return "APPLY";
  }

  markProofConflict(reason: string): void {
    this.status = "INCOMPLETE";
    this.statusReason = reason;
  }

  onCatchupComplete(c: CatchupComplete): void {
    this.lastFrameAtMs = now();
    const through = c.through ?? null;
    if (c.quality === CatchupQuality.COMPLETE && (!through || !through.journalEpoch ||
        (this.epoch !== null && this.epoch !== through.journalEpoch) ||
        (this.lastAppliedCursor !== null && through.durableCursor < this.lastAppliedCursor))) {
      this.markProofConflict("CATCHUP_CURSOR_CONFLICT");
      return;
    }
    if (through && c.quality === CatchupQuality.COMPLETE) {
      if (this.epoch === null) this.epoch = through.journalEpoch;
      // Nur mit verifiziertem COMPLETE darf die archivierte H-Grenze Resume-Cursor werden.
      if (this.lastAppliedCursor === null || through.durableCursor > this.lastAppliedCursor)
        this.lastAppliedCursor = through.durableCursor;
    }
    this.catchup = {
      through: { epoch: through?.journalEpoch ?? this.epoch ?? "", cursor: through?.durableCursor ?? 0n },
      quality: c.quality,
      reason: c.reason,
      evidenceScope: c.evidenceScope,
    };
    if (c.quality === CatchupQuality.COMPLETE) {
      this.status = "LIVE";
      this.statusReason = "";
      this.retries = 0;
    } else {
      this.status = "INCOMPLETE";
      this.statusReason = `CATCHUP:${CatchupReason[c.reason] ?? c.reason}`;
      if (c.lastProven) {
        this.epoch = c.lastProven.journalEpoch;
        this.lastAppliedCursor = c.lastProven.durableCursor;
      }
    }
  }

  // Epoch-Wechsel im Live-Stream: Cursor verwerfen, INCOMPLETE, Re-Subscribe per StartPolicy.
  onEpochMismatch(newEpoch: string): void {
    this.status = "INCOMPLETE";
    this.statusReason = `EPOCH_ROLLOVER:${this.epoch}->${newEpoch}`;
    // Epoch/Resume-Cursor als Evidence für manuellen Retry bewahren.
  }

  // -------------------------------------------------------------- Error

  onError(err: WireError, maxRetries: number): ErrorDecision {
    const code = ErrorCode[err.code] ?? String(err.code);
    switch (err.code) {
      case ErrorCode.PROTOCOL_VIOLATION:
      case ErrorCode.AUTH_REQUIRED:
      case ErrorCode.SESSION_EXPIRED:
      case ErrorCode.INTERNAL:
        return "CONNECTION";
      case ErrorCode.FRAME_TOO_LARGE:
        this.markTooLarge(err.message);
        return "HOLD";
      case ErrorCode.SCHEMA_INVALID:
      case ErrorCode.TOPIC_UNAVAILABLE:
        this.markUnavailable(`${code}${err.message ? ` ${err.message}` : ""}`);
        return "FATAL";
      default:
        break;
    }
    // retryable: EVENT_OVERFLOW, CATCHUP_LIMIT, SOURCE_INCOMPLETE, SOURCE_DETACHED
    this.retries += 1;
    if (this.retries > maxRetries) {
      this.status = this.isState ? "ERROR" : "INCOMPLETE";
      this.statusReason = `${code}:RETRY_CAP`;
      return "GIVE_UP";
    }
    this.status = this.isState ? "STALE" : "INCOMPLETE";
    this.statusReason = `${code}:RETRY_${this.retries}`;
    return "RETRY";
  }
}

export function stateMetaOf(f: StateFull): StateMeta {
  return {
    generation: f.generation,
    seq: f.seq,
    quality: f.quality,
    sourceTimestampUnixNs: f.sourceTimestampUnixNs,
    sourceId: f.sourceId,
  };
}

export function eventMetaOf(e: Event): EventMeta {
  return {
    epoch: e.cursor?.journalEpoch ?? "",
    cursor: e.cursor?.durableCursor ?? 0n,
    eventId: e.eventId,
    sourceId: e.sourceId,
    committedAtUnixNs: e.committedAtUnixNs,
    evidenceRef: e.evidenceRef,
  };
}

function sameBytes(a: Uint8Array, b: Uint8Array | null): boolean {
  if (!b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) if (a[i] !== b[i]) return false;
  return true;
}

function now(): number {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}
