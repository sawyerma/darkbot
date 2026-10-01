export type RealtimeEnvelope<T = unknown> = {
  seq: number;
  ts_ns: number;
  type: string;
  project?: "bsc" | "project_0" | "solana";
  payload: T;
};

export type RealtimeStatus = "IDLE" | "CONNECTING" | "OPEN" | "CLOSED" | "ERROR";
