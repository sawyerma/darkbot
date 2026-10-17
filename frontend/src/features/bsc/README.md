# BSC React UI – Part 5 (read-only)

BSC has a private types model, formatting helpers, Zustand state store, an adapter
seam, and five Graphite panels. No P0 imports and no BUY, SEND, ARM or SWEEP controls.

The BSC-specific protobuf schemas do not exist yet. Decoders throw rather than
applying fake/null LIVE frames; topicTable lists BSC topics as unavailable.
Unverified event metadata must not advance the durable cursor or become receipts.
Unknown/blocked fields show dash/status and no money actions are enabled.

zustand 5.0.8 is pinned in package.json and package-lock.json.
Build, TypeScript and browser tests remain NOT_RUN pending the F2 codegen and
full offline npm dependency availability. Project_0 frozen files are unchanged.
