# Sourced by darkbotctl; paths and lock are owned by that controller.
BSC_REPORTER_LABEL="${LABEL%.gateway}.bsc.reporter"
BSC_READINESS_LABEL="${LABEL%.gateway}.bsc.readiness"
BSC_SOCKET="/tmp/darkbot/bsc.sock"
if [ -n "${DARKBOT_TEST_DIR:-}" ]; then BSC_SOCKET="$DARKBOT_TEST_DIR/b.sock"; fi
BSC_JOB_PLIST="$RUNDIR/$BSC_READINESS_LABEL.plist"
BSC_REPORTER_PLIST="$RUNDIR/$BSC_REPORTER_LABEL.plist"
BSC_ROOT="$APP_HOME/bsc-source"

bsc_pid() { launchctl list "$BSC_READINESS_LABEL" 2>/dev/null | sed -n 's/.*"PID" = \([0-9][0-9]*\);/\1/p' || true; }
bsc_result_field() { sed -n "s/^$1=//p" "$RUNDIR/bsc.readiness.result" 2>/dev/null || true; }
bsc_plist() {
  local label="$1" program="$2" extra="$3" log="$4" keep="$5"
  cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$label</string>
<key>ProgramArguments</key><array><string>$(printf '%s' "$program" | xml_escape)</string>$extra</array>
<key>WorkingDirectory</key><string>$(printf '%s' "$BSC_ROOT" | xml_escape)</string>
<key>EnvironmentVariables</key><dict>
$(plist_env DARKBOT_BSC_ROOT "$BSC_ROOT")
$(plist_env DARKBOT_RUNDIR "$RUNDIR")
$(plist_env DARKBOT_BSC_SOCKET "$BSC_SOCKET")
$(plist_env DARKBOT_BSC_READINESS_LABEL "$BSC_READINESS_LABEL")
$(plist_env DARKBOT_BSC_LAUNCH_ID "${BSC_LAUNCH:-}")
$(plist_env DARKBOT_BSC_READINESS_BIN "$APP_HOME/current/g07_pre_live_readiness")
</dict>
<key>RunAtLoad</key><true/><key>KeepAlive</key><$keep/>
<key>ThrottleInterval</key><integer>5</integer>
<key>ExitTimeOut</key><integer>330</integer>
<key>StandardOutPath</key><string>$(printf '%s' "$log" | xml_escape)</string>
<key>StandardErrorPath</key><string>$(printf '%s' "$log" | xml_escape)</string>
</dict></plist>
EOF
}
bsc_reporter_up() {
  [ -x "$APP_HOME/current/bsc-status-reporter" ] || return 0
  # The source owner's state is on file before the reporter projects it.
  if [ -x "$APP_HOME/current/bsc_runtime" ]; then
    /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source init --state "$RUNDIR/bsc-sources" >/dev/null
  fi
  if launchctl list "$BSC_REPORTER_LABEL" >/dev/null 2>&1; then return 0; fi
  bsc_plist "$BSC_REPORTER_LABEL" "$APP_HOME/current/bsc-status-reporter" "" "$LOGDIR/bsc-reporter.log" true > "$BSC_REPORTER_PLIST"
  launchctl bootstrap "$DOMAIN" "$BSC_REPORTER_PLIST"
}
bsc_readiness_start() {
  assert_not_down
  bsc_check_launch "$@"
  [ -z "$(bsc_pid)" ] || { say "BSC readiness läuft bereits"; return 0; }
  [ -x "$APP_HOME/current/g07_pre_live_readiness" ] || die "BSC readiness nicht deployed"
  launchctl bootout "$DOMAIN/$BSC_READINESS_LABEL" >/dev/null 2>&1 || true
  BSC_LAUNCH="$(/usr/bin/uuidgen | tr '[:upper:]' '[:lower:]')"
  local log="$LOGDIR/bsc-readiness-$BSC_LAUNCH.log" tmp="$RUNDIR/bsc.readiness.launch.$$"
  printf 'launch_id=%s\nstarted_unix_s=%s\n' "$BSC_LAUNCH" "$(date +%s)" > "$tmp"
  mv -f "$tmp" "$RUNDIR/bsc.readiness.launch"
  bsc_plist "$BSC_READINESS_LABEL" /usr/bin/python3 "<string>$(printf '%s' "$BSC_ROOT/bsc/tools/readiness_job.py" | xml_escape)</string>" "$log" false > "$BSC_JOB_PLIST"
  ln -sfn "$log" "$LOGDIR/bsc-readiness-latest.log"
  launchctl bootstrap "$DOMAIN" "$BSC_JOB_PLIST"
  say "BSC readiness gestartet launch=$BSC_LAUNCH (read-only)"
}
bsc_readiness_stop() {
  bsc_check_launch "$@"
  local pid; pid="$(bsc_pid)"
  [ -n "$pid" ] || { say "kein BSC readiness Job aktiv"; return 0; }
  local launch; launch="$(sed -n 's/^launch_id=//p' "$RUNDIR/bsc.readiness.launch")"
  printf 'launch_id=%s\n' "$launch" > "$RUNDIR/bsc.readiness.stopping"
  launchctl kill SIGTERM "$DOMAIN/$BSC_READINESS_LABEL" || { [ "$(bsc_pid)" != "$pid" ] || die "BSC readiness konnte nicht signalisiert werden"; }
  say "BSC readiness: geordneter Stopp angefordert"
}
bsc_wait_stopped() {
  local pid i=0; pid="$(bsc_pid)"
  [ -n "$pid" ] || return 0
  while [ "$(bsc_pid)" = "$pid" ]; do
    sleep 1; i=$((i+1)); [ "$i" -lt 330 ] || die "BSC readiness stoppt noch; kein SIGKILL, Gateway bleibt aktiv"
  done
}
# GUI A/B selection, offline only: records a typed native request against the
# public snapshot mirror. The desktop `bsc sync` lets the native store apply
# or reject it; nothing here arms, signs or sends.
bsc_candidate_request() {
  [ "$#" -eq 3 ] || die "bsc candidate request SELF_A|SELF_B <generation> <request_id>"
  case "$1" in SELF_A|SELF_B) ;; *) die "unknown candidate profile" ;; esac
  case "$2" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
  [ -x "$APP_HOME/current/g07_candidate" ] || die "BSC candidate manager nicht deployed"
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/g07_candidate" request "$1" \
    --expect-generation "$2" --request-id "$3" --rundir "$RUNDIR" --snapshot "$BSC_ROOT"
}
# Source modes (plan §5a.2, order V2 §5): the native owner applies or refuses
# one request with its generation as the CAS and prints the receipt; a
# requested LIVE is recorded and stays BLOCKED. Nothing here signs or sends.
bsc_source_request() {
  [ "$#" -eq 4 ] || die "bsc source request <SOURCE_ID> OFF|SHADOW|LIVE <generation> <request_id>"
  case "$2" in OFF|SHADOW|LIVE) ;; *) die "unknown source mode" ;; esac
  case "$3" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source request "$1" "$2" \
    --expect-generation "$3" --request-id "$4" --state "$RUNDIR/bsc-sources"
}
cmd_bsc() {
  case "${1:-}" in
    source) case "${2:-}" in
      request) locked bsc_source_request "${@:3}" ;;
      *) die "bsc source request <SOURCE_ID> OFF|SHADOW|LIVE <generation> <request_id>" ;;
    esac ;;
    candidate) case "${2:-}" in
      request) locked bsc_candidate_request "${@:3}" ;;
      *) die "bsc candidate request SELF_A|SELF_B <generation> <request_id>" ;;
    esac ;;
    readiness) case "${2:-status}" in
      start) locked bsc_readiness_start "${@:3}" ;;
      stop) locked bsc_readiness_stop "${@:3}" ;;
      status) say "BSC readiness pid=$(bsc_pid) result=$(bsc_result_field result_code)" ;;
      *) die "bsc readiness start | stop | status" ;;
    esac ;;
    sync) [ "${DARKBOT_STAGED:-}" != 1 ] || die "bsc sync im Terminal aus dem Original-Repository ausführen"
      locked bsc_sync ;;
    reporter) locked bsc_reporter_up ;;
    *) die "bsc readiness start | stop | status; bsc reporter; bsc candidate request; bsc source request" ;;
  esac
}

bsc_sync() {
  [ -z "$(bsc_pid)" ] || die "Readiness läuft; erst beenden, dann sync"
  /usr/bin/python3 "$ROOT/bsc/tools/status_sync.py" "$ROOT" "$APP_HOME" "$RUNDIR"
}

bsc_check_launch() {
  [ "$#" -eq 0 ] || [ "$1" = "$(sed -n 's/^launch_id=//p' "$RUNDIR/bsc.readiness.launch" 2>/dev/null || true)" ] || die "BSC launch changed; no action dispatched"
}
