export interface Project0StatusView {
  runId: string;
  lastObservedSlot: bigint | null;
  lastFrameAgeNs: bigint | null;
  gapCount: bigint | null;
  reconnectCount: bigint | null;
  liquidationEvents: bigint | null;
  queueBytes: bigint | null;
  lagFrames: bigint | null;
  marketVerdict: string;
}
