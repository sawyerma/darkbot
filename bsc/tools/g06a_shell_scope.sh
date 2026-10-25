# Sourced only by the frozen DRY harness and its offline regressions.
# Remove inherited/source-file export attributes before any external command.
clear_exports() {
  local name
  while IFS= read -r name; do export -n "$name"; done < <(compgen -e)
}
allowed_environment() {
  local name
  for name in PATH HOME TMPDIR CARGO_HOME RUSTUP_HOME \
    BSC_MAINNET_RPC_HTTPS DARKBOT_G06A_ARCHIVE_RPC_HTTPS BSC_LAUNCHER_ADDRESS BSC_BUYER_ADDRESS \
    DARKBOT_G06A_LAUNCHER_ADDRESS DARKBOT_G06A_BUYER_ADDRESS DARKBOT_G06A_TAX_V3_IMPL \
    DARKBOT_G06A_GAS_PRICE_WEI DARKBOT_G06A_LAUNCH_GAS_LIMIT DARKBOT_G06A_BUY_GAS_LIMIT \
    DARKBOT_G06A_LAUNCH_VALUE_WEI DARKBOT_G06A_BUY_VALUE_WEI DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI \
    DARKBOT_G06A_LAUNCHER_NONCE DARKBOT_G06A_BUYER_NONCE DARKBOT_G06A_SEND_HEAD DARKBOT_G06A_SEND_HEAD_HASH \
    DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256; do
    if [[ ${!name+x} ]]; then export "$name"; fi
  done
}
# No salt and no key ever reaches an ordinary child (v9.1 D1).
run() ( clear_exports; allowed_environment; exec "$@"; )
with_keys() (
  clear_exports; allowed_environment
  export DARKBOT_G06A_LAUNCHER_PRIVATE_KEY="${BSC_LAUNCHER_PRIVATE_KEY:?launcher key required}"
  export DARKBOT_G06A_BUYER_PRIVATE_KEY="${BSC_BUYER_PRIVATE_KEY:?buyer key required}"
  exec "$@"
)
# Einheit 0 (fehlerliste BSC Gate A): the key file is read only right before
# the signing steps, in a subshell that hands back one key and nothing else
# of the file; no other variable of it ever reaches this shell.
key_from_file() (
  set +a
  # shellcheck disable=SC1090
  source "$1" >/dev/null 2>&1 || exit 1
  [[ -n "${!2:-}" ]] || exit 1
  printf '%s' "${!2}"
)
load_keys() {
  BSC_LAUNCHER_PRIVATE_KEY="$(key_from_file "$1" BSC_LAUNCHER_PRIVATE_KEY)" || return 1
  BSC_BUYER_PRIVATE_KEY="$(key_from_file "$1" BSC_BUYER_PRIVATE_KEY)" || return 1
}
# BSC-007: after the last signer no shell copy of a key remains.
drop_keys() { unset BSC_LAUNCHER_PRIVATE_KEY BSC_BUYER_PRIVATE_KEY; }
# A's public frozen salt, for exactly one local consumer. B's secret salt
# never enters this shell: `g06a_dry_handoff exec-salt` gives it to its
# allowlisted consumer only.
with_public_salt() (
  local salt="${1:?salt required}"; shift
  clear_exports; allowed_environment
  export DARKBOT_G06A_SALT="$salt"
  exec "$@"
)
with_keys_public_salt() (
  local salt="${1:?salt required}"; shift
  clear_exports; allowed_environment
  export DARKBOT_G06A_LAUNCHER_PRIVATE_KEY="${BSC_LAUNCHER_PRIVATE_KEY:?launcher key required}"
  export DARKBOT_G06A_BUYER_PRIVATE_KEY="${BSC_BUYER_PRIVATE_KEY:?buyer key required}"
  export DARKBOT_G06A_SALT="$salt"
  exec "$@"
)
get_required() {
  local value
  value="$(get "$1" "$2")" || return 1
  [[ -n "$value" && "$value" != *$'\n'* ]] || return 1
  case "$1" in
    *nonce|*block|latest_block_number|headers_verified|bloom_candidate_blocks|expected_output|simulated_received_amount|output_difference|*floor_wei|transactions.min_output)
      [[ "$value" =~ ^(0|[1-9][0-9]*)$ ]] || return 1 ;;
    salt|transactions.*tx_hash|latest_block_hash) [[ "$value" =~ ^0x[[:xdigit:]]{64}$ ]] || return 1 ;;
    launcher|buyer|portal_implementation|transactions.token) [[ "$value" =~ ^0x[[:xdigit:]]{40}$ ]] || return 1 ;;
    wallet_qualification.*|launch_success|buy_success|dry_pass.*) [[ "$value" == true || "$value" == false ]] || return 1 ;;
  esac
  printf '%s' "$value"
}
required_sha256() {
  local value
  value="$(run shasum -a 256 "$1" | run awk '{print $1}')" || return 1
  [[ "$value" =~ ^[[:xdigit:]]{64}$ ]] || return 1
  printf '%s' "$value"
}
