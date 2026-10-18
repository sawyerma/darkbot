import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import {
  ClientMessageSchema,
  ServerMessageSchema,
  type ClientMessage,
  type ServerMessage,
} from "../generated/darkbot/control/v1/stream_pb";

// Einziger Ort, an dem Bytes <-> Envelope übersetzt werden. Kein handgeschriebener Codec.

export class DecodeError extends Error {
  constructor(message: string, readonly cause?: unknown) {
    super(message);
    this.name = "DecodeError";
  }
}

export function encodeClient(init: MessageInitShape<typeof ClientMessageSchema>): Uint8Array {
  return toBinary(ClientMessageSchema, create(ClientMessageSchema, init));
}

export function decodeServer(data: ArrayBuffer | Uint8Array): ServerMessage {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  try {
    return fromBinary(ServerMessageSchema, bytes);
  } catch (e) {
    throw new DecodeError("ServerMessage decode failed", e);
  }
}

export type { ClientMessage, ServerMessage };
