import { fromBinary, toJson, type JsonObject } from "@bufbuild/protobuf";
import {
  CapturePhase,
  CaptureStateSchema,
  type CaptureState,
} from "@/core/generated/darkbot/project0/v1/capture_pb";
import { SystemStatusSchema, type SystemStatus } from "@/core/generated/darkbot/project0/v1/system_pb";
import type { CapturePhaseName, CaptureView, SystemView } from "../model/status";

export type CaptureMessage = CaptureState;
export type SystemMessage = SystemStatus;

const JSON_OPTIONS = { useProtoFieldName: true, alwaysEmitImplicit: true } as const;

const PHASES: Readonly<Record<number, CapturePhaseName>> = {
  [CapturePhase.WAITING_FOR_STREAM]: "WAITING_FOR_STREAM",
  [CapturePhase.CAPTURE]: "CAPTURE",
  [CapturePhase.DRAIN]: "DRAIN",
  [CapturePhase.FINALIZE]: "FINALIZE",
  [CapturePhase.ENDED]: "ENDED",
};

/** Throws on invalid bytes, so the registry never acknowledges a broken state. */
export function decodeCaptureState(bytes: Uint8Array): CaptureMessage {
  return fromBinary(CaptureStateSchema, bytes);
}

/** Throws on invalid bytes, so the registry never acknowledges a broken state. */
export function decodeSystemStatus(bytes: Uint8Array): SystemMessage {
  return fromBinary(SystemStatusSchema, bytes);
}

export function toCaptureView(m: CaptureMessage): CaptureView {
  const { run, phase, transport, market, assessment, writer } = m;
  return {
    run: run
      ? {
          runId: run.runId,
          programId: run.programId,
          source: run.source,
          commitment: run.commitment,
          wsHost: run.wsHost,
          rpcHost: run.rpcHost ?? null,
          configuredDurationSeconds: run.configuredDurationSeconds,
          readyTimeoutSeconds: run.readyTimeoutSeconds,
          processStartedUnixS: run.processStartedUnixS,
          sourceCoverageVerified: run.sourceCoverageVerified,
        }
      : null,
    phase: phase
      ? {
          phase: PHASES[phase.phase] ?? "UNKNOWN",
          changedUnixMs: phase.changedUnixMs,
          streamReadyUnixS: phase.streamReadyUnixS ?? null,
          stopReason: phase.stopReason,
          drainComplete: phase.drainComplete ?? null,
          writeError: phase.writeError ?? null,
          endResult: phase.endResult,
        }
      : null,
    transport: transport
      ? {
          connected: transport.connected ?? null,
          lastFrameAgeNs: transport.lastFrameAgeNs ?? null,
          applicationFrames: transport.applicationFrames ?? null,
          lastSlot: transport.lastSlot ?? null,
          lastState: transport.lastState,
          lastCode: transport.lastCode,
          lastAttempt: transport.lastAttempt ?? null,
        }
      : null,
    market: market
      ? {
          observing: market.observing,
          streamReady: market.streamReady,
          counters: new Map(market.counters.map((c) => [c.name, c.value])),
        }
      : null,
    assessment: assessment
      ? {
          verdict: assessment.verdict,
          coverageComplete: assessment.coverageComplete,
          decodeComplete: assessment.decodeComplete,
          incompleteReasons: assessment.incompleteReasons,
          gapRecords: assessment.gapRecords,
          distinctReceivers: assessment.distinctReceivers,
          grossEdgeTotalUsd: assessment.grossEdgeTotalUsd,
          grossEdgeP50Usd: assessment.grossEdgeP50Usd ?? null,
          grossEdgeP90Usd: assessment.grossEdgeP90Usd ?? null,
          deploymentError: assessment.deploymentError ?? null,
        }
      : null,
    writer: writer
      ? {
          sampledUnixMs: writer.sampledUnixMs,
          queueBytes: writer.queueBytes,
          queuePeakBytes: writer.queuePeakBytes,
          admissionLimitBytes: writer.admissionLimitBytes,
          maxBytes: writer.maxBytes,
          freeBudgetBytes: writer.freeBudgetBytes,
          ingressBytesPerS: writer.ingressBytesPerS,
          writeBytesPerS: writer.writeBytesPerS,
          lagFrames: writer.lagFrames,
          budgetExhaustionS: writer.budgetExhaustionS ?? null,
          diskFreeBytes: writer.diskFreeBytes ?? null,
          diskReserveBytes: writer.diskReserveBytes,
          diskExhaustionS: writer.diskExhaustionS ?? null,
        }
      : null,
  };
}

export function toSystemView(m: SystemMessage): SystemView {
  const effective = m.operational?.effective;
  return {
    activated: effective !== undefined,
    profile: effective?.profile ?? null,
    parameterCount: m.operational?.parameters.length ?? 0,
    liveAuthorized: effective ? effective.liveAuthorized : null,
    submissionAuthorized: effective ? effective.submissionAuthorized : null,
  };
}

/** Every reported value with its contract field name (the names carry the units). */
export function captureJsonOf(m: CaptureMessage): JsonObject {
  return toJson(CaptureStateSchema, m, JSON_OPTIONS) as JsonObject;
}

export function systemJsonOf(m: SystemMessage): JsonObject {
  return toJson(SystemStatusSchema, m, JSON_OPTIONS) as JsonObject;
}
