import schemaIdFile from "../../../../protocol/schema_id.txt?raw";

// Single source: protocol/schema_id.txt, written by protocol/scripts/schema-id.sh.
// Reporter and gateway embed the same file at build time; no copy lives here.
export const PROTOCOL_MAJOR = 1;
export const PROTOCOL_MINOR = 0;
export const SCHEMA_ID = schemaIdFile.trim();
