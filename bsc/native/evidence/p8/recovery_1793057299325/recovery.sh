#!/bin/bash
# Paket 8: a real release runtime start, a crash (SIGKILL), the restart, an
# orderly stop; then every journal check. Public read-only WSS only; PAPER
# identity fresh and unfunded; nothing is sent.
set -uo pipefail
S=<scratch>
REPO=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot
R=$S/rec; rm -rf $R; mkdir -p $R/root
B=$R/bsc_runtime; cp $REPO/bsc/engine/target/release/bsc_runtime $B
echo "release build $(shasum -a 256 $B | cut -c1-64) at $(cd $REPO && git rev-parse --short HEAD)"
(cd $REPO && git archive HEAD bsc/config bsc/engine/crates/trading/fixtures) | tar -x -C $R/root
C=$R/root/bsc/config/ingress.public.env
sed -i '' "s|^BSC_EXEC_PAPER_IDENTITY=.*|BSC_EXEC_PAPER_IDENTITY=$R/paper/paper_identity.key|" $C
$B paper-identity init --config $C
$B source init --state $R/sources >/dev/null
g=0
for id in BSC_INGRESS_PUBLICNODE BSC_STATE_C2 BSC_EXEC BSC_STRATEGY_C; do
  $B source request $id SHADOW --expect-generation $g --request-id rec-$g --state $R/sources | tail -1 || exit 1
  g=$((g+1))
done
cd $R
$B run --sources $R/sources --rundir $R/run --config $C >> $R/runtime.log 2>&1 &
p1=$!; echo "run 1 pid $p1"; sleep 150
kill -9 $p1; wait $p1 2>/dev/null; echo "run 1 killed (SIGKILL)"; sleep 2
$B run --sources $R/sources --rundir $R/run --config $C >> $R/runtime.log 2>&1 &
p2=$!; echo "run 2 pid $p2 (restart after the crash)"; sleep 150
cp $R/run/bsc-runtime/status.dbrec $R/status-run2.dbrec
echo stop > $R/run/bsc.runtime.stop
for i in $(seq 1 300); do kill -0 $p2 2>/dev/null || break; sleep 0.1; done
wait $p2; echo "run 2 exit $?"
for run in $(ls $R/run/bsc-runtime/journal); do
  for check in replay-proof capture-quality strategy-replay; do
    (cd $R/root && $B $check --rundir $R/run --config bsc/config/ingress.public.env --run $run 2>&1 | head -2 | cut -c1-400)
  done
done
(cd $R/root && $B census --rundir $R/run --config bsc/config/ingress.public.env 2>&1 | head -5 | cut -c1-400)
$B source show --state $R/sources
/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/bsc/engine/target/debug/dbrec_show $R/status-run2.dbrec | grep -E "^(canonical|state|exec|strategy)\[0\]\.(state|reason|checkpoint_restore|restored_blocks|coherent|resume|backfill|head|sender_paths|refusals)|sources\[1\]" | head -30
echo RECOVERY-DONE
