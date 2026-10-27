#!/bin/bash
# Paket 8: CPU and memory of the release bsc_runtime in SHADOW (public read-only WSS + C2 + EXEC + STRATEGY_C),
# sampled every 5 s for 180 s with ps; then an orderly stop. Nothing is sent.
set -uo pipefail
S=<scratch>
REPO=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot
R=$S/perf; rm -rf $R; mkdir -p $R/root
B=$R/bsc_runtime; cp $REPO/bsc/engine/target/release/bsc_runtime $B
echo "release build $(shasum -a 256 $B | cut -c1-64) at $(cd $REPO && git rev-parse --short HEAD); host $(sysctl -n machdep.cpu.brand_string) $(sysctl -n hw.ncpu) cpus"
(cd $REPO && git archive HEAD bsc/config bsc/engine/crates/trading/fixtures) | tar -x -C $R/root
C=$R/root/bsc/config/ingress.public.env
sed -i '' "s|^BSC_EXEC_PAPER_IDENTITY=.*|BSC_EXEC_PAPER_IDENTITY=$R/paper/paper_identity.key|" $C
$B paper-identity init --config $C >/dev/null
$B source init --state $R/sources >/dev/null
g=0; for id in BSC_INGRESS_PUBLICNODE BSC_STATE_C2 BSC_EXEC BSC_STRATEGY_C; do $B source request $id SHADOW --expect-generation $g --request-id perf-$g --state $R/sources >/dev/null || exit 1; g=$((g+1)); done
cd $R; $B run --sources $R/sources --rundir $R/run --config $C >> $R/runtime.log 2>&1 &
p=$!
echo "t_s rss_kb cpu_pct cpu_time"
for i in $(seq 1 36); do sleep 5; echo "$((i*5)) $(ps -o rss=,%cpu=,time= -p $p)"; done
echo stop > $R/run/bsc.runtime.stop; wait $p; echo "exit $?"
