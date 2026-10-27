# Paket 8: CPU and memory of the release runtime (27.10.2026, read-only Mainnet, SHADOW)

- Build: `bsc_runtime` release at `73881d51` (sha256 `e8547e7c…`), Apple M4 Max, 16 CPUs (dev host, not the Linux target). Public WSS + C2 + EXEC + STRATEGY_C SHADOW, sampled with `ps` every 5 s for 180 s (`perf.sh`, `perf.txt`), then the stop file (exit 0).
- RSS 70 MB at 5 s, 138 MB at 60 s, 164 MB at 120 s, 174 MB at 180 s: still growing, more slowly (+27 MB, then +9 MB per minute); a plateau is not shown by this sample.
- CPU 20.7 % of one core on average (max 31.9 %), 36.9 s CPU time in 180 s.
