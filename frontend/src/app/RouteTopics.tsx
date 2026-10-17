import { useEffect, type ReactNode } from "react";
import { gateway, topics, type TopicId } from "@/core/realtime";

export function RouteTopics({ topics: ids, children }: { topics: readonly TopicId[]; children: ReactNode }) {
  useEffect(() => {
    gateway.connect(); // idempotent; BLOCKED bleibt BLOCKED
    const release = ids.map((t) => topics.acquire(t));
    return () => release.forEach((r) => r());
  }, [ids]);
  return <>{children}</>;
}
