import { useGatewayStatus } from "./hooks";
import type { ConnectionStatus } from "./GatewayConnection";
// Status-only: RouteTopics owns the idempotent WebSocket connection.
export function useRealtimeStatus(): ConnectionStatus {
  return useGatewayStatus();
}
