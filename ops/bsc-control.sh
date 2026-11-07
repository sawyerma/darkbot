# Sourced by darkbotctl; paths and lock are owned by that controller.
BSC_REPORTER_LABEL="${LABEL%.gateway}.bsc.reporter"
BSC_READINESS_LABEL="${LABEL%.gateway}.bsc.readiness"
BSC_RUNTIME_LABEL="${LABEL%.gateway}.bsc.runtime"
BSC_TRAINING_LABEL="${LABEL%.gateway}.bsc.training"
# Audit E 3.5: the reporter's socket lies beside the controller's other feed sockets (the
# directory of $SOCK, the test directory in an isolated run). The reporter and the gateway feed
# take it, like the readiness label, only from here.
BSC_SOCKET="$(dirname "$SOCK")/bsc.sock"
# Audit E 3.6: the launchd timing of the BSC jobs, each named once (app.conf may set it): launchd
# restarts a crashed job no sooner than BSC_LAUNCHD_THROTTLE_S and kills a job still running
# BSC_STOP_BUDGET_S after its stop signal (default: the controller's own stop budget); a readiness
# stop waits that long, and at most BSC_READINESS_ARM_WAIT_S for the job's stop handler.
BSC_LAUNCHD_THROTTLE_S="$(app_conf BSC_LAUNCHD_THROTTLE_S 5)"
BSC_STOP_BUDGET_S="$(app_conf BSC_STOP_BUDGET_S "$STOP_BUDGET_S")"
BSC_READINESS_ARM_WAIT_S="$(app_conf BSC_READINESS_ARM_WAIT_S 5)"
# BSC-V8-02: how long a training start waits for its job to take the run up.
BSC_TRAINING_START_WAIT_S="$(app_conf BSC_TRAINING_START_WAIT_S 5)"
bsc_launchd_conf() {
  for v in "$BSC_LAUNCHD_THROTTLE_S" "$BSC_STOP_BUDGET_S" "$BSC_READINESS_ARM_WAIT_S" "$BSC_TRAINING_START_WAIT_S"; do
    case "$v" in ''|*[!0-9]*|0*) die "app.conf: BSC_LAUNCHD_THROTTLE_S, BSC_STOP_BUDGET_S, BSC_READINESS_ARM_WAIT_S and BSC_TRAINING_START_WAIT_S are positive whole seconds" ;; esac
  done
}
BSC_JOB_PLIST="$RUNDIR/$BSC_READINESS_LABEL.plist"
BSC_REPORTER_PLIST="$RUNDIR/$BSC_REPORTER_LABEL.plist"
BSC_RUNTIME_PLIST="$RUNDIR/$BSC_RUNTIME_LABEL.plist"
BSC_TRAINING_PLIST="$RUNDIR/$BSC_TRAINING_LABEL.plist"
BSC_ROOT="$APP_HOME/bsc-source"
# The runtime configuration of the snapshot: the runtime's, and the owner's bounds (Audit E 3.1/3.4).
BSC_CONFIG="$BSC_ROOT/bsc/config/ingress.public.env"

bsc_pid() { launchctl list "$BSC_READINESS_LABEL" 2>/dev/null | sed -n 's/.*"PID" = \([0-9][0-9]*\);/\1/p' || true; }
# Audit E 4.13: the readiness job's one native owner is the deployed reporter: it runs the job
# (launchd program), writes the controller's typed marks and reads them; no key=value file here.
bsc_reporter() { /usr/bin/env -i PATH=/usr/bin:/bin DARKBOT_BSC_ROOT="$BSC_ROOT" DARKBOT_RUNDIR="$RUNDIR" "$APP_HOME/current/bsc-status-reporter" "$@"; }
bsc_readiness_launch() { bsc_reporter --readiness-launch 2>/dev/null || true; }
bsc_readiness_result() { bsc_reporter --readiness-result 2>/dev/null || true; }
bsc_plist() {
  local label="$1" program="$2" extra="$3" log="$4" keep="$5"
  bsc_launchd_conf
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
<key>ThrottleInterval</key><integer>$BSC_LAUNCHD_THROTTLE_S</integer>
<key>ExitTimeOut</key><integer>$BSC_STOP_BUDGET_S</integer>
<key>StandardOutPath</key><string>$(printf '%s' "$log" | xml_escape)</string>
<key>StandardErrorPath</key><string>$(printf '%s' "$log" | xml_escape)</string>
</dict></plist>
EOF
}
bsc_reporter_up() {
  [ -x "$APP_HOME/current/bsc-status-reporter" ] || return 0
  # The source owner's state is on file before the reporter projects it
  # (`source init` only creates it: a running LIVE path keeps its grant).
  if [ -x "$APP_HOME/current/bsc_runtime" ]; then
    /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source init --state "$RUNDIR/bsc-sources" >/dev/null
  fi
  if launchctl list "$BSC_REPORTER_LABEL" >/dev/null 2>&1; then return 0; fi
  bsc_plist "$BSC_REPORTER_LABEL" "$APP_HOME/current/bsc-status-reporter" "" "$LOGDIR/bsc-reporter.log" true > "$BSC_REPORTER_PLIST"
  launchctl bootstrap "$DOMAIN" "$BSC_REPORTER_PLIST"
}
# The BSC runtime (C1, order V2 §5, plan 28.10.): observes the sources the native
# owner made effective and runs the PAPER paths; its LIVE path signs and sends only
# while the owner's LIVE gate holds for the granted scope. Idle while all are OFF.
# Restarted by launchd only after a crash; a stop is a stop file it answers by
# committing its journal.
bsc_runtime_pid() { launchctl list "$BSC_RUNTIME_LABEL" 2>/dev/null | sed -n 's/.*"PID" = \([0-9][0-9]*\);/\1/p' || true; }
# REV.1.0 R10: the configuration as the runtime `$1` takes it (and the LIVE path's files, unread):
# a refused start is never handed to launchd, which would restart it every BSC_LAUNCHD_THROTTLE_S.
bsc_config_check() {
  /usr/bin/env -i PATH=/usr/bin:/bin "$1" config-check --config "${2:-$BSC_CONFIG}"
}
bsc_runtime_up() {
  [ -x "$APP_HOME/current/bsc_runtime" ] || return 0
  if launchctl list "$BSC_RUNTIME_LABEL" >/dev/null 2>&1; then return 0; fi
  bsc_config_check "$APP_HOME/current/bsc_runtime" >/dev/null || {
    say "BSC Runtime nicht gestartet: Konfiguration verweigert (darkbotctl bsc config-check)"; return 0; }
  rm -f "$RUNDIR/bsc.runtime.stop"
  local args
  args="$(for a in run --sources "$RUNDIR/bsc-sources" --rundir "$RUNDIR" --config "$BSC_CONFIG"; do printf '<string>%s</string>' "$(printf '%s' "$a" | xml_escape)"; done)"
  bsc_plist "$BSC_RUNTIME_LABEL" "$APP_HOME/current/bsc_runtime" "$args" "$LOGDIR/bsc-runtime.log" crash > "$BSC_RUNTIME_PLIST"
  launchctl bootstrap "$DOMAIN" "$BSC_RUNTIME_PLIST"
}
# The runtime's own stop bound (validated by it against its derived stop path).
bsc_runtime_stop_timeout_ms() {
  local ms
  ms="$(sed -n 's/^BSC_RUNTIME_STOP_TIMEOUT_MS=\([0-9][0-9]*\)$/\1/p' "$BSC_CONFIG" 2>/dev/null | tail -n 1)"
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
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source reset --state "$RUNDIR/bsc-sources" >/dev/null
}
# 01.11. (R1): nothing LIVE holds (no grant, no position held, nothing
# unresolved): a runtime restart ends nothing. Asked with the build about to
# be deployed (`$1`; it knows `live idle` even over an older running one).
# Without owner state there is nothing to end.
bsc_live_idle() {
  [ -d "$RUNDIR/bsc-sources" ] || return 0
  /usr/bin/env -i PATH=/usr/bin:/bin "$1" live idle --state "$RUNDIR/bsc-sources"
}
bsc_readiness_start() {
  assert_not_down
  bsc_check_launch "$@"
  [ -z "$(bsc_pid)" ] || { say "BSC readiness läuft bereits"; return 0; }
  [ -x "$APP_HOME/current/g07_pre_live_readiness" ] || die "BSC readiness nicht deployed"
  [ -x "$APP_HOME/current/bsc-status-reporter" ] || die "BSC Reporter nicht deployed"
  launchctl bootout "$DOMAIN/$BSC_READINESS_LABEL" >/dev/null 2>&1 || true
  BSC_LAUNCH="$(/usr/bin/uuidgen | tr '[:upper:]' '[:lower:]')"
  local log="$LOGDIR/bsc-readiness-$BSC_LAUNCH.log"
  bsc_reporter --readiness-mark launch "$BSC_LAUNCH" || die "BSC readiness: Start nicht vermerkt"
  bsc_plist "$BSC_READINESS_LABEL" "$APP_HOME/current/bsc-status-reporter" "<string>--readiness-job</string>" "$log" false > "$BSC_JOB_PLIST"
  ln -sfn "$log" "$LOGDIR/bsc-readiness-latest.log"
  launchctl bootstrap "$DOMAIN" "$BSC_JOB_PLIST"
  say "BSC readiness gestartet launch=$BSC_LAUNCH (read-only)"
}
bsc_readiness_stop() {
  bsc_check_launch "$@"
  local pid; pid="$(bsc_pid)"
  [ -n "$pid" ] || { say "kein BSC readiness Job aktiv"; return 0; }
  local launch; launch="$(bsc_readiness_launch)"
  # The mark only shows STOPPING; the stop goes without it (review L3).
  bsc_reporter --readiness-mark stopping "$launch" || say "BSC readiness: Stopp nicht vermerkt (keine STOPPING-Anzeige); Signal folgt"
  # Signal only once the job armed its stop handler (at most
  # BSC_READINESS_ARM_WAIT_S): a TERM before would end it without a result
  # (Paket 8 review L6).
  bsc_launchd_conf
  local i=0
  while ! bsc_reporter --readiness-armed "$launch" && [ "$i" -lt $((BSC_READINESS_ARM_WAIT_S * 10)) ] && [ "$(bsc_pid)" = "$pid" ]; do sleep 0.1; i=$((i+1)); done
  launchctl kill SIGTERM "$DOMAIN/$BSC_READINESS_LABEL" || { [ "$(bsc_pid)" != "$pid" ] || die "BSC readiness konnte nicht signalisiert werden"; }
  say "BSC readiness: geordneter Stopp angefordert"
}
bsc_wait_stopped() {
  local pid i=0; pid="$(bsc_pid)"
  [ -n "$pid" ] || return 0
  bsc_launchd_conf
  while [ "$(bsc_pid)" = "$pid" ]; do
    sleep 1; i=$((i+1)); [ "$i" -lt "$BSC_STOP_BUDGET_S" ] || die "BSC readiness stoppt noch; kein SIGKILL, Gateway bleibt aktiv"
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
# a requested LIVE is effective only while the owner's LIVE gate holds (plan
# 28.10.), an ENABLED switch is a permission only. This controller signs and
# sends nothing itself. The same command the GUI uses.
BSC_REQUEST_USAGE="bsc source request <ID> OFF|SHADOW|PAPER|LIVE|DISABLED|ENABLED|<attempts per UTC day or in the campaign> <generation> <request_id>; bsc source show"
# The LIVE grant (plan 28.10.): the operator's one economic approval of a
# LIVE scope. `scope` prints the canonical scope text and its SHA-256, the
# value `grant` must confirm; `revoke` withdraws it; `status` names every
# condition LIVE still misses. A grant is a permission, never an order.
BSC_LIVE_USAGE="bsc live status | positions | idle | ledger-check | attempts | campaign-precommit | qualify <--venue|--source|--adapter|--acquisition|--risk-by|--risk-statement|--risk-scope-sha256|--risk-evidence value>... | key-init <absolute key file> | loopback-identity init|show <test owner state> | adopt-g07 <g08 result> <capture> <request_id>; bsc live scope <venue> <strategies> <sources,...> <adapters,...> <max_input_wei> <max_spend_wei> <max_attempts> <expires_unix_ms> <ALL|EXIT_ONLY> <max_buy_slippage_bps> <max_exit_slippage_bps> <max_gas_price_wei> <max_buy_gas_limit> <max_approve_gas_limit> <max_sell_gas_limit> <max_open_positions> <max_realized_loss_wei> <max_daily_realized_loss_wei> <min_buy_interval_ms> <OUTLIER_ACK|NO_OUTLIER_ACK>; bsc live grant <the same twenty> <scope_sha256> <generation> <request_id>; bsc live revoke <generation> <request_id>; bsc live status; bsc live lot <0x lot> RESUME|WRITE_OFF|EXIT_NOW <ledger_version> <request_id>; bsc live nonce-burn <0x tx> <ledger_version> <request_id>; bsc live ledger-quarantine <seq> <generation> <request_id>"
bsc_live_flags() {
  for v in "$1" "$2" "$3" "$4"; do case "$v" in ''|*[!A-Z0-9_+,]*) die "unknown LIVE scope value" ;; esac; done
  for v in "$5" "$6" "$7" "$8"; do case "$v" in ''|*[!0-9]*) die "LIVE limits are unsigned integers" ;; esac; done
  LIVE_FLAGS=(--venue "$1" --strategies "$2" --sources "$3" --adapters "$4" --max-input-wei "$5" \
    --max-spend-wei "$6" --max-attempts "$7" --expires-unix-ms "$8")
  # BSC3-058: the exits of open positions alone, a separate authorization.
  case "$9" in ALL) ;; EXIT_ONLY) LIVE_FLAGS+=(--exit-only) ;; *) die "the grant's buys are ALL or EXIT_ONLY" ;; esac
  # BSC3-069: the execution risk the operator grants, every bound.
  for v in "${10}" "${11}" "${12}" "${13}" "${14}" "${15}"; do
    case "$v" in ''|*[!0-9]*) die "the LIVE risk bounds are unsigned integers" ;; esac
  done
  LIVE_FLAGS+=(--max-buy-slippage-bps "${10}" --max-exit-slippage-bps "${11}" --max-gas-price-wei "${12}" \
    --max-buy-gas-limit "${13}" --max-approve-gas-limit "${14}" --max-sell-gas-limit "${15}")
  # 31.10. order: the positions and losses the operator grants, every bound.
  for v in "${16}" "${17}" "${18}" "${19}"; do
    case "$v" in ''|*[!0-9]*) die "the LIVE limits are unsigned integers" ;; esac
  done
  LIVE_FLAGS+=(--max-open-positions "${16}" --max-realized-loss-wei "${17}" \
    --max-daily-realized-loss-wei "${18}" --min-buy-interval-ms "${19}")
  # R4 (BSC3-069): the operator confirms a risk above the runtime's soft bounds as an outlier.
  case "${20}" in OUTLIER_ACK) LIVE_FLAGS+=(--risk-outlier-ack) ;; NO_OUTLIER_ACK) ;;
    *) die "the grant's risk is OUTLIER_ACK or NO_OUTLIER_ACK" ;; esac
}
# REV.1.0 R10 (Audit D, terminal parity): `live qualify` writes the qualification of the deployed
# runtime into this checkout, replacing the one of an earlier build (`bsc sync` carries it into the
# snapshot); its evidence flags pass as the runtime takes them, nothing else. `key-init` draws the
# LIVE buyer's key once into a new private file outside the checkout and prints its address
# (BSC_LIVE_KEY and BSC_LIVE_BUYER in ingress.public.env are the operator's).
BSC_QUALIFY_FLAGS=' --venue --source --adapter --acquisition --risk-by --risk-statement --risk-scope-sha256 --risk-evidence '
bsc_live_qualify() {
  [ "${DARKBOT_STAGED:-}" != 1 ] || die "bsc live qualify im Terminal aus dem Original-Repository ausführen"
  local rt="$1" args=(); shift
  [ "$#" -gt 0 ] || die "$BSC_LIVE_USAGE"
  while [ "$#" -gt 0 ]; do
    case "$BSC_QUALIFY_FLAGS" in *" $1 "*) ;; *) die "bsc live qualify: unknown flag $1" ;; esac
    [ "$#" -ge 2 ] || die "bsc live qualify: $1 needs a value"
    case "$2" in ''|-*) die "bsc live qualify: $1 needs a value" ;; esac
    args+=("$1" "$2"); shift 2
  done
  /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live qualify --root "$ROOT" --runtime-binary "$rt" "${args[@]}" --replace
  say "weiter: darkbotctl bsc sync"
}
# REV.1.0 R3b (Audit A): the operator's decisions of the LIVE ledger, once per request, each with
# the state it was decided on as the CAS: a lot's RESUME, WRITE_OFF or EXIT_NOW and an unresolved
# transaction's nonce burn at the ledger's version, the quarantine of an entry that does not
# decode at the owner generation. Exit-now and the burn stay locked while their switch in
# ingress.public.env is OFF. The decider is the account this runs as (the gateway's for the GUI).
# The same command the GUI uses.
# Character lists, never ranges: a range follows the locale's collation (en_US.UTF-8: [a-f] holds
# A-F too).
BSC_HEX='0123456789abcdef'
BSC_ID_CHARS='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-'
bsc_live_hash() {
  case "$1" in 0x*) ;; *) die "a lot or transaction is 0x and 64 lowercase hex digits" ;; esac
  case "${1#0x}" in ''|*[!$BSC_HEX]*) die "a lot or transaction is 0x and 64 lowercase hex digits" ;; esac
  [ "${#1}" -eq 66 ] || die "a lot or transaction is 0x and 64 lowercase hex digits"
}
bsc_live_number() {
  case "$1" in ''|*[!0-9]*) die "$2 must be an unsigned integer" ;; esac
}
bsc_live_ledger() {
  local rt="$1" id="$2"; shift 2
  # R3b review L3: a request ID as the native owner takes it, never a flag.
  case "$id" in ''|-*|*[!$BSC_ID_CHARS]*) die "the request id is letters, digits and hyphens" ;; esac
  [ "${#id}" -le 64 ] || die "the request id is letters, digits and hyphens"
  /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live "$@" --request-id "$id" --decided-by "$(/usr/bin/id -un)" \
    --state "$RUNDIR/bsc-sources"
}
bsc_live() {
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  local rt="$APP_HOME/current/bsc_runtime" cmd="${1:-}"
  [ "$#" -gt 0 ] && shift
  case "$cmd" in
    scope) [ "$#" -eq 20 ] || die "$BSC_LIVE_USAGE"; bsc_live_flags "$@"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live scope "${LIVE_FLAGS[@]}" --config "$BSC_CONFIG" ;;
    grant) [ "$#" -eq 23 ] || die "$BSC_LIVE_USAGE"; bsc_live_flags "$@"
      case "${21}" in ''|*[!$BSC_HEX]*) die "scope_sha256 is 64 lowercase hex digits" ;; esac
      [ "${#21}" -eq 64 ] || die "scope_sha256 is 64 lowercase hex digits"
      case "${22}" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live grant "${LIVE_FLAGS[@]}" --scope-sha256 "${21}" \
        --expect-generation "${22}" --request-id "${23}" --state "$RUNDIR/bsc-sources" --config "$BSC_CONFIG" ;;
    revoke) [ "$#" -eq 2 ] || die "$BSC_LIVE_USAGE"
      case "$1" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live revoke --expect-generation "$1" --request-id "$2" \
        --state "$RUNDIR/bsc-sources" ;;
    status) [ "$#" -eq 0 ] || die "$BSC_LIVE_USAGE"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live status --state "$RUNDIR/bsc-sources" --root "$BSC_ROOT" \
        --rundir "$RUNDIR" --config "$BSC_CONFIG" ;;
    positions|idle) [ "$#" -eq 0 ] || die "$BSC_LIVE_USAGE"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live "$cmd" --state "$RUNDIR/bsc-sources" ;;
    ledger-check) [ "$#" -eq 0 ] || die "$BSC_LIVE_USAGE"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live ledger check --state "$RUNDIR/bsc-sources" ;;
    attempts) [ "$#" -eq 0 ] || die "$BSC_LIVE_USAGE"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live attempts --rundir "$RUNDIR" --state "$RUNDIR/bsc-sources" \
        --config "$BSC_CONFIG" --manifest "$BSC_ROOT/bsc/config/live_campaign.env" ;;
    campaign-precommit) [ "$#" -eq 0 ] || die "$BSC_LIVE_USAGE"
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live campaign-precommit --rundir "$RUNDIR" \
        --manifest "$BSC_ROOT/bsc/config/live_campaign.env" --decided-by "$(/usr/bin/id -un)" ;;
    qualify) bsc_live_qualify "$rt" "$@" ;;
    key-init) [ "$#" -eq 1 ] || die "$BSC_LIVE_USAGE"
      case "$1" in /*) ;; *) die "the key file is an absolute path" ;; esac
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live key init --key "$1" --root "$ROOT" ;;
    loopback-identity) [ "$#" -eq 2 ] || die "$BSC_LIVE_USAGE"
      case "$1" in init|show) ;; *) die "$BSC_LIVE_USAGE" ;; esac
      /usr/bin/env -i PATH=/usr/bin:/bin "$rt" live loopback-identity "$1" --state "$2" --root "$ROOT" ;;
    adopt-g07) [ "$#" -eq 3 ] || die "$BSC_LIVE_USAGE"
      bsc_live_ledger "$rt" "$3" adopt-g07 --g08 "$1" --capture "$2" \
        --config "$BSC_CONFIG" ;;
    lot) [ "$#" -eq 4 ] || die "$BSC_LIVE_USAGE"; bsc_live_hash "$1"
      local what
      case "$2" in RESUME) what=resume ;; WRITE_OFF) what=write-off ;; EXIT_NOW) what=exit-now ;;
        *) die "a lot's decision is RESUME, WRITE_OFF or EXIT_NOW" ;; esac
      bsc_live_number "$3" "the ledger version"
      bsc_live_ledger "$rt" "$4" lot "$what" --lot "$1" --expect-version "$3" \
        --config "$BSC_CONFIG" ;;
    nonce-burn) [ "$#" -eq 3 ] || die "$BSC_LIVE_USAGE"; bsc_live_hash "$1"
      bsc_live_number "$2" "the ledger version"
      bsc_live_ledger "$rt" "$3" nonce-burn --tx "$1" --expect-version "$2" \
        --config "$BSC_CONFIG" ;;
    ledger-quarantine) [ "$#" -eq 3 ] || die "$BSC_LIVE_USAGE"
      bsc_live_number "$1" "a ledger entry's sequence number"
      bsc_live_number "$2" "generation"
      bsc_live_ledger "$rt" "$3" ledger quarantine --seq "$1" --expect-generation "$2" ;;
    *) die "$BSC_LIVE_USAGE" ;;
  esac
}
bsc_source_request() {
  [ "$#" -eq 4 ] || die "$BSC_REQUEST_USAGE"
  case "$2" in OFF|SHADOW|PAPER|LIVE|DISABLED|ENABLED) ;; ''|*[!0-9]*) die "unknown mode" ;; esac
  case "$3" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source request "$1" "$2" \
    --expect-generation "$3" --request-id "$4" --state "$RUNDIR/bsc-sources" --root "$BSC_ROOT" \
    --rundir "$RUNDIR" --config "$BSC_CONFIG"
}
# Frontend package (06.11.): the operator's Strategy C settings (the keys the
# LIVE tab sets): the native owner checks a value with the runtime
# configuration's own parser on the snapshot's configuration, applies or
# refuses it with its generation as the CAS and prints the receipt. Effective
# at the strategy's next start (its switch on, a runtime restart).
BSC_STRATEGY_USAGE="bsc strategy set <BSC_STRATEGY_C_KEY> <value> <generation> <request_id>; bsc strategy set-all <generation> <BSC_STRATEGY_C_KEY>=<value>... <request_id>; bsc strategy show"
bsc_strategy_set() {
  [ "$#" -eq 4 ] || die "$BSC_STRATEGY_USAGE"
  case "$1" in BSC_STRATEGY_C_*) ;; *) die "unknown Strategy C setting" ;; esac
  case "$3" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
  case "$4" in ''|-*|*[!$BSC_ID_CHARS]*) die "the request id is letters, digits and hyphens" ;; esac
  [ "${#4}" -le 64 ] || die "the request id is letters, digits and hyphens"
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" strategy set "$1" "$2" \
    --expect-generation "$3" --request-id "$4" --state "$RUNDIR/bsc-sources" --config "$BSC_CONFIG"
}
# BSC-V8-04: every given Strategy C setting in one generation with one receipt (all or none).
bsc_strategy_set_all() {
  [ "$#" -ge 3 ] || die "$BSC_STRATEGY_USAGE"
  local gen="$1" req="${@: -1}" kv
  case "$gen" in ''|*[!0-9]*) die "generation must be an unsigned integer" ;; esac
  case "$req" in ''|-*|*[!$BSC_ID_CHARS]*) die "the request id is letters, digits and hyphens" ;; esac
  [ "${#req}" -le 64 ] || die "the request id is letters, digits and hyphens"
  set -- "${@:2:$#-2}"
  for kv in "$@"; do case "$kv" in BSC_STRATEGY_C_*=*) ;; *) die "unknown Strategy C setting" ;; esac; done
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" strategy set-all "$@" \
    --expect-generation "$gen" --request-id "$req" --state "$RUNDIR/bsc-sources" --config "$BSC_CONFIG"
}
# BSC-V8-01: the Strategy C profiles at their native owner (training runs, the LIVE
# selection): a profile carries every Strategy C value, checked by the runtime
# configuration's own parser; create and clone take the catalog version as CAS,
# update, archive and delete the profile's revision. The receipt is printed.
BSC_PROFILE_USAGE="bsc profile create <catalog_version> <name> <BSC_VENUE_*,...> <BSC_STRATEGY_C_KEY>=<value>... <request_id>; bsc profile update <profile_id> <revision> <name> <BSC_VENUE_*,...> <BSC_STRATEGY_C_KEY>=<value>... <request_id>; bsc profile clone <source_profile_id> <source_revision> <catalog_version> <name> <request_id>; bsc profile archive|delete <profile_id> <revision> <request_id>; bsc profile show"
bsc_profile() {
  local what="${1:-}" kv
  [ "$#" -ge 1 ] && shift
  local req="${@: -1}"
  [ "$#" -ge 3 ] || die "$BSC_PROFILE_USAGE"
  case "$req" in ''|-*|*[!$BSC_ID_CHARS]*) die "the request id is letters, digits and hyphens" ;; esac
  [ "${#req}" -le 64 ] || die "the request id is letters, digits and hyphens"
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  local owner=(--request-id "$req" --state "$RUNDIR/bsc-sources" --config "$BSC_CONFIG")
  local number id
  case "$what" in
    create)
      [ "$#" -ge 5 ] || die "$BSC_PROFILE_USAGE"
      number="$1"; local name="$2" venues="$3"; set -- "${@:4:$#-4}" ;;
    update)
      [ "$#" -ge 6 ] || die "$BSC_PROFILE_USAGE"
      id="$1"; number="$2"; local name="$3" venues="$4"; set -- "${@:5:$#-5}" ;;
    clone)
      [ "$#" -eq 5 ] || die "$BSC_PROFILE_USAGE"
      id="$1"; number="$3"
      case "$2" in ''|*[!0-9]*) die "the source revision must be an unsigned integer" ;; esac ;;
    archive|delete)
      [ "$#" -eq 3 ] || die "$BSC_PROFILE_USAGE"
      id="$1"; number="$2" ;;
    *) die "$BSC_PROFILE_USAGE" ;;
  esac
  case "$number" in ''|*[!0-9]*) die "the version must be an unsigned integer" ;; esac
  if [ -n "${id:-}" ]; then
    case "$id" in ''|-*|*[!$BSC_ID_CHARS]*) die "the profile id is letters, digits and hyphens" ;; esac
  fi
  local rt=(/usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" profile "$what")
  case "$what" in
    create|update)
      for kv in "$@"; do case "$kv" in BSC_STRATEGY_C_*=*) ;; *) die "unknown Strategy C setting" ;; esac; done
      [ "$what" = create ] || set -- "$@" --profile "$id"
      "${rt[@]}" "$@" --name "$name" --venues "$venues" --expect "$number" "${owner[@]}" ;;
    clone) "${rt[@]}" --source "$id" --source-revision "$2" --name "$4" --expect "$number" "${owner[@]}" ;;
    *) "${rt[@]}" --profile "$id" --expect "$number" "${owner[@]}" ;;
  esac
}
# BSC-V8-02/03: a training run (REPLAY of a recorded run) at its native owner; its job is its
# own launchd program (away from the runtime's hot path). A start whose run the owner applied
# launches the job unless one runs; a run whose job is gone ends first (PENDING FAILED, RUNNING
# INTERRUPTED). A stop is the owner's request, then the job's SIGTERM.
BSC_TRAINING_USAGE="bsc training start <profile_id> <revision> REPLAY <BSC_VENUE_*,...> <DURATION|LAUNCH_COUNT|UNTIL_STOPPED> <bound|-> <replay_source> <request_id>; bsc training stop <run_id> <reason|-> <request_id>; bsc training show"
# The run the loaded training job executes (its program's --run argument).
bsc_training_job_run() { launchctl list "$BSC_TRAINING_LABEL" 2>/dev/null | sed -n '/"--run";/{n;s/[^t0-9]//g;p;}' || true; }
bsc_training_pid() { launchctl list "$BSC_TRAINING_LABEL" 2>/dev/null | sed -n 's/.*"PID" = \([0-9][0-9]*\);/\1/p' || true; }
bsc_training_rt() {
  [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
  /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" training "$@" \
    --state "$RUNDIR/bsc-sources" --rundir "$RUNDIR" --config "$BSC_CONFIG"
}
bsc_training_id() {
  case "$1" in ''|-*|*[!$BSC_ID_CHARS]*) die "$2 is letters, digits and hyphens" ;; esac
  [ "${#1}" -le 64 ] || die "$2 is letters, digits and hyphens"
}
bsc_training_start() {
  [ "$#" -eq 8 ] || die "$BSC_TRAINING_USAGE"
  bsc_training_id "$1" "the profile id"; bsc_training_id "$7" "the replay source"; bsc_training_id "$8" "the request id"
  case "$2" in ''|*[!0-9]*) die "the revision must be an unsigned integer" ;; esac
  case "$3" in REPLAY|SHADOW|PAPER) ;; *) die "unknown training mode" ;; esac
  case "$4" in ''|*[!ABCDEFGHIJKLMNOPQRSTUVWXYZ_,]*) die "the venues are BSC_VENUE_* names" ;; esac
  case "$5" in DURATION|LAUNCH_COUNT|UNTIL_STOPPED) ;; *) die "unknown training bound" ;; esac
  local bound=()
  case "$6" in -) ;; ''|*[!0-9]*) die "the bound must be an unsigned integer or -" ;; *) bound=(--bound "$6") ;; esac
  bsc_launchd_conf
  # The owner ends an active run of another request whose job is gone (review M2: never its own).
  local job=NONE; [ -z "$(bsc_training_pid)" ] || job=RUNNING
  local out rc=0
  out="$(bsc_training_rt start --profile "$1" --expect "$2" --mode "$3" --venues "$4" --stop-kind "$5" ${bound[@]+"${bound[@]}"} \
    --replay-source "$7" --job "$job" --request-id "$8")" || rc=$?
  printf '%s\n' "$out"
  [ "$rc" -eq 0 ] || return "$rc"
  local run status
  run="$(printf '%s\n' "$out" | sed -n 's/^run=\(t[0-9][0-9]*\) status=[A-Z]*$/\1/p')"
  status="$(printf '%s\n' "$out" | sed -n 's/^run=t[0-9][0-9]* status=\([A-Z]*\)$/\1/p')"
  [ -n "$run" ] || die "BSC training: kein Lauf im Beleg"
  # Only a PENDING run needs its job (the same request again gets its run back).
  [ "$status" = PENDING ] || { say "BSC training run=$run status=$status"; return 0; }
  # A job that still runs: this run's (nothing to do) or an older run's end (wait for it).
  local i=0
  while [ -n "$(bsc_training_pid)" ]; do
    [ "$(bsc_training_job_run)" != "$run" ] || { say "BSC training run=$run läuft bereits"; return 0; }
    sleep 1; i=$((i+1)); [ "$i" -lt "$BSC_STOP_BUDGET_S" ] || die "BSC training: der Job eines früheren Laufs endet noch"
  done
  launchctl bootout "$DOMAIN/$BSC_TRAINING_LABEL" >/dev/null 2>&1 || true
  local args="<string>training</string><string>execute</string><string>--run</string><string>$run</string>"
  args="$args<string>--state</string><string>$(printf '%s' "$RUNDIR/bsc-sources" | xml_escape)</string>"
  args="$args<string>--rundir</string><string>$(printf '%s' "$RUNDIR" | xml_escape)</string>"
  args="$args<string>--config</string><string>$(printf '%s' "$BSC_CONFIG" | xml_escape)</string>"
  bsc_plist "$BSC_TRAINING_LABEL" "$APP_HOME/current/bsc_runtime" "$args" "$LOGDIR/bsc-training-$run.log" false > "$BSC_TRAINING_PLIST"
  launchctl bootstrap "$DOMAIN" "$BSC_TRAINING_PLIST" || die "BSC training: Job nicht gestartet (derselbe Auftrag startet ihn erneut)"
  i=0
  while [ -z "$(bsc_training_pid)" ] && [ "$i" -lt $((BSC_TRAINING_START_WAIT_S * 10)) ]; do sleep 0.1; i=$((i+1)); done
  say "BSC training gestartet run=$run (REPLAY, read-only)"
}
bsc_training_stop() {
  [ "$#" -eq 3 ] || die "$BSC_TRAINING_USAGE"
  bsc_training_id "$1" "the run id"; bsc_training_id "$3" "the request id"
  local reason="$2"; [ "$reason" != - ] || reason=""
  local rc=0
  bsc_training_rt stop --run "$1" --reason "$reason" --request-id "$3" || rc=$?
  [ "$rc" -eq 0 ] || return "$rc"
  if [ -n "$(bsc_training_pid)" ]; then
    launchctl kill SIGTERM "$DOMAIN/$BSC_TRAINING_LABEL" || say "BSC training: Signal nicht zugestellt (der Lauf endet mit seinem Job)"
  else
    # No job: a run it left RUNNING ends now (INTERRUPTED).
    bsc_training_rt end-orphans --reason "stopped while its job was gone" >/dev/null || die "BSC training: Laufstand nicht lesbar"
  fi
}
cmd_bsc() {
  case "${1:-}" in
    training) case "${2:-}" in
      start) locked bsc_training_start "${@:3}" ;;
      stop) locked bsc_training_stop "${@:3}" ;;
      show) [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
        /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" training show --state "$RUNDIR/bsc-sources" ;;
      *) die "$BSC_TRAINING_USAGE" ;;
    esac ;;
    profile) case "${2:-}" in
      create|update|clone|archive|delete) locked bsc_profile "${@:2}" ;;
      show) [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
        /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" profile show --state "$RUNDIR/bsc-sources" ;;
      *) die "$BSC_PROFILE_USAGE" ;;
    esac ;;
    strategy) case "${2:-}" in
      set) locked bsc_strategy_set "${@:3}" ;;
      set-all) locked bsc_strategy_set_all "${@:3}" ;;
      show) [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
        /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" strategy show --state "$RUNDIR/bsc-sources" --config "$BSC_CONFIG" ;;
      *) die "$BSC_STRATEGY_USAGE" ;;
    esac ;;
    source) case "${2:-}" in
      request) locked bsc_source_request "${@:3}" ;;
      show) [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
        /usr/bin/env -i PATH=/usr/bin:/bin "$APP_HOME/current/bsc_runtime" source show --state "$RUNDIR/bsc-sources" --config "$BSC_CONFIG" ;;
      *) die "$BSC_REQUEST_USAGE" ;;
    esac ;;
    live) case "${2:-}" in
      grant|revoke|lot|nonce-burn|ledger-quarantine|qualify|key-init|loopback-identity|adopt-g07|attempts|campaign-precommit) locked bsc_live "${@:2}" ;;
      *) bsc_live "${@:2}" ;;
    esac ;;
    candidate) case "${2:-}" in
      request) locked bsc_candidate_request "${@:3}" ;;
      *) die "bsc candidate request SELF_A|SELF_B <generation> <request_id>" ;;
    esac ;;
    readiness) case "${2:-status}" in
      start) locked bsc_readiness_start "${@:3}" ;;
      stop) locked bsc_readiness_stop "${@:3}" ;;
      status) say "BSC readiness pid=$(bsc_pid) result=$(bsc_readiness_result)" ;;
      *) die "bsc readiness start | stop | status" ;;
    esac ;;
    sync) [ "${DARKBOT_STAGED:-}" != 1 ] || die "bsc sync im Terminal aus dem Original-Repository ausführen"
      locked bsc_sync ;;
    reporter) locked bsc_reporter_up ;;
    config-check) [ -x "$APP_HOME/current/bsc_runtime" ] || die "BSC runtime nicht deployed"
      bsc_config_check "$APP_HOME/current/bsc_runtime" ;;
    status) bsc_status ;;
    *) die "bsc readiness start | stop | status; bsc reporter; bsc status; bsc config-check; bsc candidate request; bsc source request | show; $BSC_STRATEGY_USAGE; $BSC_LIVE_USAGE" ;;
  esac
}

# REV.1.0 R10 (Audit D): the BSC part of `darkbotctl status`: the runtime and reporter jobs, the
# readiness job, the snapshot's source, whether anything LIVE holds and the configuration check.
bsc_status() {
  local rt="$APP_HOME/current/bsc_runtime" pid
  pid="$(bsc_runtime_pid)"
  if [ -n "$pid" ]; then say "bsc runtime: läuft pid $pid"
  elif launchctl list "$BSC_RUNTIME_LABEL" >/dev/null 2>&1; then say "bsc runtime: geladen, kein Prozess"
  else say "bsc runtime: gestoppt"; fi
  if [ -S "$BSC_SOCKET" ]; then say "bsc reporter-socket: vorhanden"; else say "bsc reporter-socket: fehlt"; fi
  say "bsc readiness: pid=$(bsc_pid) result=$(bsc_readiness_result)"
  if [ -f "$BSC_ROOT/COMMIT" ]; then say "bsc snapshot: $(cat "$BSC_ROOT/COMMIT")"; else say "bsc snapshot: fehlt"; fi
  [ -x "$rt" ] || { say "bsc runtime: nicht deployed"; return 0; }
  local held
  if held="$(bsc_live_idle "$rt" 2>&1)"; then say "bsc live: nichts offen"; else say "bsc live: ${held##*$'\n'}"; fi
  say "bsc config: $(bsc_config_check "$rt" 2>&1 | tail -n 1 || true)"
}

bsc_sync() {
  [ -z "$(bsc_pid)" ] || die "Readiness läuft; erst beenden, dann sync"
  /usr/bin/python3 "$ROOT/bsc/tools/status_sync.py" "$ROOT" "$APP_HOME" "$RUNDIR"
}

bsc_check_launch() {
  [ "$#" -eq 0 ] || [ "$1" = "$(bsc_readiness_launch)" ] || die "BSC launch changed; no action dispatched"
}
