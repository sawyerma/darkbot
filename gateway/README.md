# DarkBot Rust Gateway – F3 / part 7.1 of 3

**INCOMPLETE / NOT BUILT:** This commit contains Rust Cargo metadata, protox/prost
codegen, topic registry, config, STATE watch hub, engine feed interface, source
supervisor and overview producer. HTTP/WS/session/main.rs are scheduled for
part 7.2; integration tests and frontend linkage for 7.3. Cargo build and test
cannot succeed before those files are present. Rust compiler unavailable in the
current tool environment; no PASS is claimed.

Critical corrections versus Fable's draft:
- Source feed cannot write gateway-owned overview or other projects' topics.
- Failed/rejected Engine State cannot mark the source CONNECTED.
- Heartbeat does not revive stale cached full states.
- Gateway publisher generation requires durable monotonic allocation, not epoch
  seconds. main.rs MUST handle gateway_generation() Result and fail closed on
  unavailable persistent storage or stale lock.
- ProjectSummary.live_authorized is optional: None means UNKNOWN, not an
  inferred false from missing evidence.
- Rust codegen takes absolute schema file paths; Buf generates only TypeScript.

Security/completeness: EngineFeed is an interface ONLY, not authenticated IPC.
Before F3 becomes VERIFIED, implement authenticated IPC, bounded source registry,
full reconnect and STATE tests, socket/session authorization, and source identity
fencing. Project_0/BSC are NOT attached. Trading and LIVE are NOT authorized.

Stage: frontend/f2-part2-review. Do not merge to main without full tests.
