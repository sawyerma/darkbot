# Paket 8: CPU and memory of the release runtime over 60 minutes (27.10.2026, read-only Mainnet, SHADOW)

- Build: `bsc_runtime` release from `ops/darkbotctl build` at `73881d51` (sha256 `e8547e7c…`), Apple M4 Max, 16 CPUs (dev host, not the Linux target). Public WSS + C2 + EXEC + STRATEGY_C SHADOW, `ps` every 5 s for 3600 s (`perf.sh`, `perf.txt`, 720 samples), then the stop file (exit 0).
- RSS: 148 MB at 1 min, 269 MB at 10 min, 363 MB at 20, 429 MB at 30, 437 MB at 40, 440 MB at 50, 451 MB at 60 min. The growth flattens after about 30 min (+8, +3, +11 MB per 10 min); bounded by the configured retention (C2 `BSC_STATE_MAX_TXS` with eviction, journal and dedup bounds). A fully flat line is not shown within 60 min.
- CPU: 16.3 % of one core on average, 10:01 CPU time in 60 min.
