import { useEffect, type ReactNode } from "react";
import { topics, type TopicId } from "@/core/realtime";

// Hält die Basis-Topics einer Route (Refcount + Grace). Vor gateway.connect() passiert
// nichts; nach dem Welcome werden referenzierte Topics automatisch abonniert.
export function RouteTopics({ topics: ids, children }: { topics: readonly TopicId[]; children: ReactNode }) {
  useEffect(() => {
    const release = ids.map((t) => topics.acquire(t));
    return () => release.forEach((r) => r());
  }, [ids]);
  return <>{children}</>;
}
