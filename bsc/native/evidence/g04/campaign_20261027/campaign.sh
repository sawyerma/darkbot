#!/bin/bash
# G0.4/V10-8 raw-data campaign (order V2 §8, plan rule 5): read-only, FREE_ONLY,
# at most seven calendar days. Release bsc_runtime of the manifest-verified
# build, public WSS sources PUBLICNODE + DRPC, C2, EXEC and STRATEGY_C in
# SHADOW (PAPER ledger, unfunded identity outside the repository, nothing is
# signed for Mainnet or sent). Phases: a 6-minute probe run sealed by the stop
# file, the capacity preflight for the full duration, then the measured run
# until the deadline. A crashed runtime is started again (a new run, like the
# launchd restart); every gap stays counted in the journals.
set -uo pipefail
D=$1                       # durable campaign directory (outside /private/tmp)
REPO=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot
B=$D/bsc_runtime
C=$D/root/bsc/config/ingress.public.env
log() { echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $*" >> $D/campaign.log; }

caffeinate -i -w $$ &

if [ ! -f $D/started ]; then
  mkdir -p $D/root; mkdir -m 700 $D/paper
  cp $REPO/bsc/engine/target/release/bsc_runtime $B
  (cd $REPO && git archive HEAD bsc/config bsc/engine/crates/trading/fixtures) | tar -x -C $D/root
  sed -i '' "s|^BSC_EXEC_PAPER_IDENTITY=.*|BSC_EXEC_PAPER_IDENTITY=$D/paper/paper_identity.key|" $C
  sed -i '' "s|^BSC_RUNTIME_CAPTURE_PENDING=.*|BSC_RUNTIME_CAPTURE_PENDING=VENUES|" $C
  sed -i '' "s|^BSC_RUNTIME_CAPTURE_DURATION_S=.*|BSC_RUNTIME_CAPTURE_DURATION_S=604800|" $C
  log "head $(cd $REPO && git rev-parse HEAD) bsc_runtime $(shasum -a 256 $B | cut -c1-64) config $(shasum -a 256 $C | cut -c1-64)"
  $B paper-identity init --config $C >> $D/campaign.log 2>&1 || { log "paper identity FAILED"; exit 1; }
  $B source init --state $D/sources >/dev/null
  g=0
  for id in BSC_INGRESS_PUBLICNODE BSC_INGRESS_DRPC BSC_STATE_C2 BSC_EXEC BSC_STRATEGY_C; do
    $B source request $id SHADOW --expect-generation $g --request-id g04-$g --state $D/sources >> $D/campaign.log 2>&1 || { log "source request $id FAILED"; exit 1; }
    g=$((g+1))
  done
  date -u +%s > $D/started
  log "probe run 360 s"
  (cd $D/root && $B run --sources $D/sources --rundir $D/run --config $C >> $D/runtime.log 2>&1) &
  p=$!; sleep 360
  echo stop > $D/run/bsc.runtime.stop
  wait $p; log "probe run exit $?"
  (cd $D/root && $B capture-preflight --rundir $D/run --config $C > $D/preflight.txt 2>&1); rc=$?
  log "capture-preflight exit $rc: $(head -1 $D/preflight.txt | cut -c1-300)"
  [ $rc -eq 0 ] || { log "preflight not PASS: campaign not started"; exit 1; }
fi

deadline=$(( $(cat $D/started) + 7 * 86400 ))
echo $deadline > $D/deadline
log "measured run until $(date -u -r $deadline +%Y-%m-%dT%H:%M:%SZ)"
while [ $(date -u +%s) -lt $deadline ]; do
  rm -f $D/run/bsc.runtime.stop
  (cd $D/root && exec $B run --sources $D/sources --rundir $D/run --config $C >> $D/runtime.log 2>&1) &
  p=$!; echo $p > $D/runtime.pid
  while kill -0 $p 2>/dev/null; do
    [ $(date -u +%s) -ge $deadline ] && { echo stop > $D/run/bsc.runtime.stop; log "deadline: stop file written"; }
    [ -f $D/campaign.stop ] && { echo stop > $D/run/bsc.runtime.stop; log "campaign.stop: stop file written"; deadline=0; }
    sleep 5
  done
  wait $p; rc=$?
  log "runtime exit $rc"
  [ $(date -u +%s) -lt $deadline ] && { log "runtime ended before the deadline: start again in 30 s"; sleep 30; }
done
log "campaign ended"
