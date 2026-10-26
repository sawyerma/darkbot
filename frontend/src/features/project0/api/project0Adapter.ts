import type { JsonObject } from "@bufbuild/protobuf";
import { TopicId, topics, type StateMeta } from "@/core/realtime";
import { project0Registration } from "@/app/projectRegistry";
import type { Project0Tab } from "../model/contracts";
import { useProject0Store, type OwnerTopic } from "../store/project0Store";
import { streamIdentity } from "../model/streamFreshness";
import {
  decodeFinanceSummary,
  decodeLiquidatorState,
  decodeWalletsState,
  toFinanceSnapshot,
  toLiquidatorView,
  toWalletsView,
} from "./ownerAdapter";
import { decodeAnalyticsJobs, toAnalyticsView } from "./analyticsAdapter";
import {
  captureJsonOf,
  decodeCaptureState,
  decodeSystemStatus,
  systemJsonOf,
  toCaptureView,
  toSystemView,
  type CaptureMessage,
  type SystemMessage,
} from "./project0ViewAdapter";

interface Update<T> {
  value: T;
  meta: StateMeta;
  receivedAtMs: number;
}

/** At most one store update per rendered frame; the newest state wins. */
function frameBatcher<T>(apply: (u: Update<T>) => void): (u: Update<T>) => void {
  let pending: Update<T> | null = null;
  let scheduled = false;
  const flush = () => {
    scheduled = false;
    const next = pending;
    pending = null;
    if (next) apply(next);
  };
  return (u) => {
    pending = u;
    if (scheduled) return;
    scheduled = true;
    if (typeof requestAnimationFrame === "function") requestAnimationFrame(flush);
    else setTimeout(flush, 0);
  };
}

let started = false;
let lastSystem: SystemMessage | null = null;
let lastCapture: CaptureMessage | null = null;

export function ensureProject0Adapter(): void {
  if (started) return;
  started = true;
  const store = useProject0Store.getState;
  const applySystem = frameBatcher<SystemMessage>((u) => {
    lastSystem = u.value;
    store().applySystem(toSystemView(u.value), u.meta, u.receivedAtMs);
  });
  const applyCapture = frameBatcher<CaptureMessage>((u) => {
    lastCapture = u.value;
    store().applyCapture(toCaptureView(u.value), u.meta, u.receivedAtMs);
  });
  topics.onTopicStatus(TopicId.PROJECT0_STATUS, (status, reason) => store().setSystemTopic(status, reason));
  topics.onTopicStatus(TopicId.PROJECT0_CAPTURE_STATE, (status, reason) => store().setCaptureTopic(status, reason));
  topics.onState(TopicId.PROJECT0_STATUS, (bytes: Uint8Array, meta: StateMeta) => {
    const message = decodeSystemStatus(bytes);
    applySystem({ value: message, meta, receivedAtMs: Date.now() });
  });
  topics.onState(TopicId.PROJECT0_CAPTURE_STATE, (bytes: Uint8Array, meta: StateMeta) => {
    const message = decodeCaptureState(bytes);
    const receivedAtMs = Date.now();
    // P0-012: the view is batched per frame (newest wins), the stream's progress never is — a
    // baseline and its successor within one frame are both observed.
    store().observeStream(message.transport?.applicationFrames ?? null,
      streamIdentity(message.run?.runId ?? "", meta.sourceId, meta.generation), receivedAtMs);
    applyCapture({ value: message, meta, receivedAtMs });
  });
  // P0-PROD-CTRL-04: the state owner's projection (Liquidator, Wallets, Overview finance) and
  // the analytics job store.
  const owner: ReadonlyArray<[OwnerTopic, TopicId, (bytes: Uint8Array) => Parameters<ReturnType<typeof store>["applyOwner"]>[1]]> = [
    ["liquidator", TopicId.PROJECT0_LIQUIDATOR_STATE, (b) => toLiquidatorView(decodeLiquidatorState(b))],
    ["wallets", TopicId.PROJECT0_WALLETS_STATE, (b) => toWalletsView(decodeWalletsState(b))],
    ["finance", TopicId.PROJECT0_FINANCE_SUMMARY, (b) => toFinanceSnapshot(decodeFinanceSummary(b))],
    // B03/U-9: the analytics job controller's store (Analytics, Capture run qualification).
    ["analytics", TopicId.PROJECT0_ANALYTICS_JOBS, (b) => toAnalyticsView(decodeAnalyticsJobs(b))],
  ];
  for (const [slice, topic, decode] of owner) {
    const apply = frameBatcher<ReturnType<typeof decode>>((u) => store().applyOwner(slice, u.value, u.meta, u.receivedAtMs));
    topics.onTopicStatus(topic, (status, reason) => store().setOwnerTopic(slice, status, reason));
    topics.onState(topic, (bytes: Uint8Array, meta: StateMeta) => {
      apply({ value: decode(bytes), meta, receivedAtMs: Date.now() });
    });
  }
}

/** Every value of the latest received states, generated from the contract. */
export function allValues(): { system: JsonObject | null; capture: JsonObject | null } {
  return {
    system: lastSystem ? systemJsonOf(lastSystem) : null,
    capture: lastCapture ? captureJsonOf(lastCapture) : null,
  };
}

/**
 * Holds the topics the visible tab renders (registry `panels`) until the returned release is
 * called: without an acquisition a topic is never subscribed, however many listeners exist.
 */
export function holdPanelTopics(
  tab: Project0Tab,
  acquire: (topic: TopicId) => () => void = (t) => topics.acquire(t),
): () => void {
  const release = project0Registration.topics.panels[tab].map((t) => acquire(t));
  return () => release.forEach((r) => r());
}
