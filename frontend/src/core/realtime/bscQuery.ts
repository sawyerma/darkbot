import { create as message, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import { Availability } from "../generated/darkbot/bsc/v1/common_pb";
import { BscQueryRequestSchema, BscQueryResponseSchema, QueryRefusal, type BscQueryResponse } from "../generated/darkbot/bsc/v1/query_pb";
import { READ_TIMEOUT_MS, session } from "./bscControl";

// V8 (BSC-V8-10): the one bounded, read-only BSC query (protocol/schemas/darkbot/bsc/v1/
// query.proto): one POST per operator view or page step, never polling, never a STATE topic. The
// gateway route is registered (session, origin and CSRF like a command; the deployed reporter's
// `--query` answers): strategy_settings, training_profiles, training_runs and replay_sources by
// their native owners, every other query NOT_IMPLEMENTED with its missing BSC-V8 owner. A failed answer is UNAVAILABLE with its reason (503: no reporter
// answer within its bound; a non-protobuf answer). A refused query (a stale or foreign cursor, a
// limit above the owner's cap, a filter it does not accept) is UNAVAILABLE with the refusal, never
// another page. Nothing here substitutes, caches or computes a value.

export const BSC_QUERY_PATH = "/api/v1/bsc/query";

export type BscQuery = NonNullable<MessageInitShape<typeof BscQueryRequestSchema>["query"]>;
export type BscQueryResult = BscQueryResponse["result"];
/** The answer of one query: the owner's result, or why there is none. */
export type BscQueryOutcome =
  | { kind: "ANSWER"; result: BscQueryResult; requestId: string }
  | { kind: "NOT_IMPLEMENTED" | "UNAVAILABLE"; reason: string };

const PROTOBUF = "application/x-protobuf";

/** Classifies an answer of the query route; exported for the transport tests. */
export function classify(status: number, contentType: string | null, body: Uint8Array | null): BscQueryOutcome {
  // A gateway without the route (an older release) answers through its static SPA fallback: 405.
  if (status === 404 || status === 405) return { kind: "NOT_IMPLEMENTED", reason: `QUERY_ROUTE_NOT_REGISTERED (HTTP ${status}, BSC-V8-10)` };
  if (status < 200 || status > 299) return { kind: "UNAVAILABLE", reason: `HTTP ${status}` };
  if (!contentType?.toLowerCase().startsWith(PROTOBUF) || !body) return { kind: "UNAVAILABLE", reason: "QUERY_ANSWER_NOT_PROTOBUF" };
  let r: BscQueryResponse;
  try { r = fromBinary(BscQueryResponseSchema, body); } catch { return { kind: "UNAVAILABLE", reason: "QUERY_ANSWER_INVALID" }; }
  if (r.availability === Availability.NOT_IMPLEMENTED) return { kind: "NOT_IMPLEMENTED", reason: r.unavailableReason || "NOT_IMPLEMENTED" };
  if (r.refusal !== QueryRefusal.UNSPECIFIED) return { kind: "UNAVAILABLE", reason: `QUERY_REFUSED ${QueryRefusal[r.refusal] ?? r.refusal}${r.refusalDetail ? ` (${r.refusalDetail})` : ""}` };
  if (r.availability !== Availability.AVAILABLE || r.result.case === undefined) return { kind: "UNAVAILABLE", reason: r.unavailableReason || "OWNER_UNAVAILABLE" };
  return { kind: "ANSWER", result: r.result, requestId: r.requestId };
}

/** One bounded query, posted once; a failure is an answer (UNAVAILABLE), never a retry. */
export async function bscQuery(query: BscQuery): Promise<BscQueryOutcome> {
  try {
    const csrf = await session();
    const body = toBinary(BscQueryRequestSchema, message(BscQueryRequestSchema, { requestId: crypto.randomUUID(), query }));
    const r = await fetch(BSC_QUERY_PATH, { credentials: "same-origin", cache: "no-store", method: "POST",
      headers: { "Content-Type": PROTOBUF, "X-CSRF-Token": csrf }, body, signal: AbortSignal.timeout(READ_TIMEOUT_MS) });
    const bytes = r.ok ? new Uint8Array(await r.arrayBuffer()) : null;
    return classify(r.status, r.headers.get("content-type"), bytes);
  } catch (error) {
    return { kind: "UNAVAILABLE", reason: error instanceof Error ? error.message : String(error) };
  }
}
