# Sourced by darkbotctl; paths and lock are owned by that controller.
BSC_REPORTER_LABEL="${LABEL%.gateway}.bsc.reporter"
BSC_READINESS_LABEL="${LABEL%.gateway}.bsc.readiness"
BSC_RUNTIME_LABEL="${LABEL%.gateway}.bsc.runtime"
BSC_SOCKET="/tmp/darkbot/bsc.sock"
if [ -n "${DARKBOT_TEST_DIR:-}" ]; then BSC_SOCKET="$DARKBOT_TEST_DIR/b.sock"; fi
BSC_JOB_PLIST="$RUNDIR/$BSC_READINESS_LABEL.plist"
BSC_REPORTER_PLIST="$RUNDIR/$BSC_REPORTER_LABEL.plist"
BSC_RUNTIME_PLIST="$RUNDIR/$BSC_RUNTIME_LABEL.plist"
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
<key>RunAtLoad</key><true/><key>KeepAlive</key>$(if [ "$keep" = crash ]; then printf '<dict><key>SuccessfulExit</key><false/></dict>'; else printf '<%s/>' "$keep"; fi)
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
# The read-only BSC runtime (C1, order V2 §5): observes the sources the native
# owner made effective SHADOW; idle while all are OFF. Restarted by launchd only
# after a crash; a stop is a stop file it answers by committing its journal.
bsc_runtime_pid() { launchctl list "$BSC_RUNTIME_LABEL" 2>/dev/null | sed -n 's/.*"PID" = \([0-9][0-9]*\);/\1/p' || true; }
bsc_runtime_up() {
  [ -x "$APP_HOME/current/bsc_runtime" ] || return 0
  if launchctl list "$BSC_RUNTIME_LABEL" >/dev/null 2>&1; then return 0; fi
  rm -f "$RUNDIR/bsc.runtime.stop"
  local args
  args="$(for a in run --sources "$RUNDIR/bsc-sources" --rundir "$RUNDIR" --config "$BSC_ROOT/bsc/config/ingress.public.env"; do printf '<string>%s</string>' "$(printf '%s' "$a" | xml_escape)"; done)"
  bsc_plist "$BSC_RUNTIME_LABEL" "$APP_HOME/current/bsc_runtime" "$args" "$LOGDIR/bsc-runtime.log" crash > "$BSC_RUNTIME_PLIST"
  launchctl bootstrap "$DOMAIN" "$BSC_RUNTIME_PLIST"
}
# The runtime's own stop bound (validated by it against its derived stop path).
bsc_runtime_stop_timeout_ms() {
  local ms
  ms="$(sed -n 's/^BSC_RUNTIME_STOP_TIMEOUT_MS=\([0-9][0-9]*\)$/\1/p' "$BSC_ROOT/bsc/config/ingress.public.env" 2>/dev/null | tail -n 1)"
  [ -n "$ms" ] || die "BSC_RUNTIME_STOP_TIMEOUT_MS fehlt in ingress.public.env"
  printf '%s\n' "$ms"
}
bsc_runtime_down() {
  launchctl list "$BSC_RUNTIME_LABEL" >/dev/null 2>&1 || return 0
  printf 'stop\n' > "$RUNDIR/bsc.runtime.stop"
  # Wait the configured stop bound in 100-ms steps; then bootout (SIGTERM, which
  # the runtime also answers with its stop path; SIGKILL only after ExitTimeOut).
  local steps i=0
  steps=$(( ($(bsc_runtime_stop_timeout_ms) + 99) / 100 ))
  while [ -n "$(bsc_runtime_pid)" ] && [ "$i" -lt "$steps" ]; do sleep 0.1; i=$((i+1)); done
  launchctl bootout "$DOMAIN/$BSC_RUNTIME_LABEL" >/dev/null 2>&1 || true
  rm -f "$RUNDIR/bsc.runtime.stop"
}
# Plan 27.10.: a stop leaves every wallet and economic switch DISABLED (the
# owner's restart reset); a terminal runner never finds one ENABLED after it.
bsc_switches_reset() {
  [ -x "$APP_HOME/current/bsc_runtime" ] || return 0
  [ -d "$RUNDIR/bsc-sources" ] || return 0
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source init --state "$RUNDIR/bsc-sources" >/dev/null
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
  # Signal only once the job armed its stop handler (at most 5 s): a TERM
  # before would end it without a result (Paket 8 review L6).
  local i=0
  while [ "$(sed -n 's/^launch_id=//p' "$RUNDIR/bsc.readiness.armed" 2>/dev/null)" != "$launch" ] && [ "$i" -lt 50 ] && [ "$(bsc_pid)" = "$pid" ]; do sleep 0.1; i=$((i+1)); done
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
# Source, component and venue modes (plan §5a.2, v9 §2, order V2 §5) and the
# wallet and economic switches (plan 27.10.): the native owner applies or
# refuses one request with its generation as the CAS and prints the receipt;
# a requested LIVE is recorded and stays BLOCKED, an ENABLED switch is a
# permission only. Nothing here signs or sends. The same command the GUI uses.
BSC_REQUEST_USAGE="bsc source request <ID> OFF|SHADOW|PAPER|LIVE|DISABLED|ENABLED|<attempts per UTC day or in the campaign> <generation> <request_id>; bsc source show"
# The LIVE grant (plan 28.10.): the operator's one economic approval of a
# LIVE scope. `scope` prints the canonical scope text and its SHA-256, the
# value `grant` must confirm; `revoke` withdraws it; `status` names every
# condition LIVE still misses. A grant is a permission, never an order.
BSC_LIVE_USAGE="bsc live scope <venue> <strategies> <sources,...> <adapters,...> <max_input_wei> <max_spend_wei> <max_attempts> <expires_unix_ms>; bsc live grant <the same eight> <scope_sha256> <generation> <request_id>; bsc live revoke <generation> <request_id>; bsc live status"
bsc_live_flags() {
  for v in "$1" "$2" "$3" "$4"; do case "$v" in ''|*[!A-Z0-9_+,]*) die "unknown LIVE scope value" ;; esac; done
  for v in "$5" "$6" "$7" "$8"; do case "$v" in ''|*[!0-9]*) die "LIVE limits are unsigned integers" ;; esac; done
  LIVE_FLAGS=(--venue "$1" --strategies "$2" --sources "$3" --adapters "$4" --max-input-wei "$5" \
    --max-spend-wei "$6" --max-attempts "$7" --expires-unix-ms "$8")
}
bsc_live() {
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  local rt="$APP_HOME/current/bsc_runtime" cmd="${1:-}"
  [ "$#" -gt 0 ] && shift
  case "$cmd" in
    scope) [ "$#" -eq 8 ] || die "$BSC_LIVE_USAGE"; bsc_live_flags "$@"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live scope "${LIVE_FLAGS[@]}" ;;
    grant) [ "$#" -eq 11 ] || die "$BSC_LIVE_USAGE"; bsc_live_flags "$@"
      case "$9" in ''|*[!0-9a-f]*) die "scope_sha256 is 64 lowercase hex digits" ;; esac
      [ "${#9}" -eq 64 ] || die "scope_sha256 is 64 lowercase hex digits"
      case "${10}" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live grant "${LIVE_FLAGS[@]}" --scope-sha256 "$9" \
        --expect-generation "${10}" --request-id "${11}" --state "$RUNDIR/bsc-sources" ;;
    revoke) [ "$#" -eq 2 ] || die "$BSC_LIVE_USAGE"
      case "$1" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live revoke --expect-generation "$1" --request-id "$2" \
        --state "$RUNDIR/bsc-sources" ;;
    status) [ "$#" -eq 0 ] || die "$BSC_LIVE_USAGE"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live status --state "$RUNDIR/bsc-sources" --root "$BSC_ROOT" ;;
    *) die "$BSC_LIVE_USAGE" ;;
  esac
}
bsc_source_request() {
  [ "$#" -eq 4 ] || die "$BSC_REQUEST_USAGE"
  case "$2" in OFF|SHADOW|PAPER|LIVE|DISABLED|ENABLED) ;; ''|*[!0-9]*) die "unknown mode" ;; esac
  case "$3" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source request "$1" "$2" \
    --expect-generation "$3" --request-id "$4" --state "$RUNDIR/bsc-sources" --root "$BSC_ROOT"
}
cmd_bsc() {
  case "${1:-}" in
    source) case "${2:-}" in
      request) locked bsc_source_request "${@:3}" ;;
      show) [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
        /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source show --state "$RUNDIR/bsc-sources" ;;
      *) die "$BSC_REQUEST_USAGE" ;;
    esac ;;
    live) case "${2:-}" in
      grant|revoke) locked bsc_live "${@:2}" ;;
      *) bsc_live "${@:2}" ;;
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
    *) die "bsc readiness start | stop | status; bsc reporter; bsc candidate request; bsc source request | show; bsc live scope | grant | revoke | status" ;;
  esac
}

bsc_sync() {
  [ -z "$(bsc_pid)" ] || die "Readiness läuft; erst beenden, dann sync"
  /usr/bin/python3 "$ROOT/bsc/tools/status_sync.py" "$ROOT" "$APP_HOME" "$RUNDIR"
}

bsc_check_launch() {
  [ "$#" -eq 0 ] || [ "$1" = "$(sed -n 's/^launch_id=//p' "$RUNDIR/bsc.readiness.launch" 2>/dev/null || true)" ] || die "BSC launch changed; no action dispatched"
}
