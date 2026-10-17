import { GatewayConnection } from "./GatewayConnection";
import { TopicSubscriptions } from "./TopicSubscriptions";

// Ein Socket, ein Subscription-Registry für die ganze App. Verbindung wird NICHT hier
// aufgebaut; F3 ruft gateway.connect() in app/providers.tsx auf, wenn das Gateway existiert.
export const gateway = new GatewayConnection();
export const topics = new TopicSubscriptions(gateway);

export type { ConnectionInfo, ConnectionStatus, Limits } from "./GatewayConnection";
export type { EventMeta, StateMeta, TopicStatus } from "./TopicState";
export type { AcquireOptions, EventListener, StateListener } from "./TopicSubscriptions";
export { TOPIC_TABLE, bindingOf, wireNameOf } from "./topicTable";

export { ProjectId, TopicClass, TopicId } from "../generated/darkbot/control/v1/topics_pb";
export { StartPolicy } from "../generated/darkbot/control/v1/stream_pb";
