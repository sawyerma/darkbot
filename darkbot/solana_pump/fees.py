"""Background acquisition of fee/protocol/account data and immutable snapshots + cache (spec: fees.py,
mandatory_fee_pipeline). Never called inside the StateOwner critical section; the owner only reads
the latest cached CostBasis. NULL means UNKNOWN, never zero.
"""

from __future__ import annotations

import asyncio
import base64
import json
import math
import struct
import urllib.request
from dataclasses import dataclass, replace
from collections import deque
import hashlib
from decimal import Decimal
from typing import Any, Callable

from darkbot.solana_pump.domain import NS_PER_MS, CostBasis, CostEvidence, FeeSnapshot, utc_iso
from darkbot.solana_pump.execution import priority_lamports
from darkbot.solana_pump.market import _Reader, b58decode
from darkbot.solana_pump.token_accounts import ASSOCIATED_TOKEN_PROGRAM, SYSTEM_PROGRAM, account_profiles, find_pda

COMPUTE_BUDGET_PROGRAM = "ComputeBudget111111111111111111111111111111"
DUMMY_PAYER = bytes([1]) * 32  # fee probe only; getFeeForMessage does not validate signers, nothing is ever signed or sent
RpcCall = Callable[[str, list[Any]], Any]


class RpcError(RuntimeError):
    pass


def _acquisition_error(exc: Exception) -> dict[str, Any]:
    # Parsing/layout failures also reach the resolver's broad exception boundary;
    # only transport failures or our exact local request-budget failure can retain evidence.
    transient = isinstance(exc, (OSError, TimeoutError, ConnectionError)) or (
        isinstance(exc, RpcError) and str(exc) == "configured free RPC request budget exhausted")
    return {
        "error": type(exc).__name__,
        # Selection semantics: fully transient failures may retain compatible, still-fresh prior evidence.
        "transient_acquisition_failure": transient,
        # Scheduling semantics are separate: partial observations can invalidate evidence but still deserve a fast retry.
        "retryable_acquisition_failure": transient,
    }


FEE_KINDS = {"PROTOCOL_GLOBAL_ACCOUNT": "protocol", "NETWORK_FEE": "network",
             "RENT_EXEMPTION": "rent", "PRIORITY_POLICY": "priority_policy"}


def select_cost_basis(previous: CostBasis, snapshots: list[FeeSnapshot]) -> CostBasis:
    """Apply one publication batch; retained evidence keeps its original identity and TTL.

    Raw failures remain observations. An authoritative invalidation replaces VALID evidence,
    so a later transient failure cannot resurrect it. Network/priority form a coherent pair.
    """
    if not snapshots:
        return previous
    now = max(s.received_mono_ns for s in snapshots)
    incoming = {FEE_KINDS[s.source_type]: s for s in snapshots}
    values = {k: getattr(previous, k) for k in FEE_KINDS.values()}

    def transient(s):
        return s is not None and s.raw_values.get("transient_acquisition_failure") is True

    def compatible(old, new):
        return old is not None and new is not None and old.is_fresh(now) and all(
            getattr(old, k) == getattr(new, k) for k in
            ("source_type", "source_provider", "config_hash", "protocol_program_id", "idl_version_or_hash", "freshness_policy_hash", "commitment")) and (
            old.source_account_addresses[:len(new.source_account_addresses)] == new.source_account_addresses)

    for kind in ("protocol", "rent"):
        new = incoming.get(kind)
        if new is not None and not (transient(new) and compatible(values[kind], new)):
            values[kind] = new
    if "network" in incoming or "priority_policy" in incoming:
        net = incoming.get("network", values["network"])
        priority = incoming.get("priority_policy", values["priority_policy"])
        priority_failure = transient(priority)
        net_failure = transient(net) or (net is not None and net.validation_status == "PRIORITY_UNKNOWN" and priority_failure)
        pair_failure = (net_failure or priority_failure) and (
            net_failure or (net is not None and net.validation_status == "VALID")) and (
            priority_failure or (priority is not None and priority.validation_status == "VALID"))
        if not (pair_failure and compatible(values["network"], net) and compatible(values["priority_policy"], priority)):
            values["network"], values["priority_policy"] = net, priority
            # A stand-alone priority update cannot silently relabel a previous fee probe.
            if net is not None and priority is not None:
                probed_price = net.raw_values.get("compute_unit_price_micro_lamports")
                if probed_price is not None and probed_price != priority.normalized_fee_components.get("compute_unit_price_micro_lamports"):
                    values["network"] = None
    return CostBasis(**values)


def http_rpc(url: str, timeout_s: float) -> RpcCall:
    """Minimal JSON-RPC over urllib (standard library). Used only via asyncio.to_thread outside the hot path."""

    def call(method: str, params: list[Any]) -> Any:
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
        req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=timeout_s) as resp:  # noqa: S310 - operator-configured RPC endpoint
            payload = json.loads(resp.read().decode())
        if "error" in payload:
            raise RpcError(str(payload["error"]))
        return payload.get("result")

    return call


def account_discriminator(name: str) -> bytes:
    import hashlib as _h

    return _h.sha256(f"account:{name}".encode("ascii")).digest()[:8]


def compact_u16(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def build_fee_probe_message(signature_count: int, compute_unit_limit: int, cu_price_micro: int, blockhash: bytes, payer: bytes = DUMMY_PAYER) -> bytes:
    """Legacy message with only ComputeBudget instructions; template hash identifies the message shape."""
    keys = [payer] + [bytes([2]) * 32] * (signature_count - 1) + [b58decode(COMPUTE_BUDGET_PROGRAM).rjust(32, b"\0")]
    header = bytes([signature_count, 0, 1])
    program_index = len(keys) - 1
    ix_limit = bytes([program_index]) + compact_u16(0) + compact_u16(5) + bytes([2]) + struct.pack("<I", compute_unit_limit)
    ix_price = bytes([program_index]) + compact_u16(0) + compact_u16(9) + bytes([3]) + struct.pack("<Q", cu_price_micro)
    return header + compact_u16(len(keys)) + b"".join(keys) + blockhash + compact_u16(2) + ix_limit + ix_price


def build_trade_fee_message(cfg: Any, side: str, cu_price_micro: int, blockhash: bytes, token_program: str | None = None) -> bytes:
    """Unsigned configured Pump instruction/ATA/ComputeBudget message for getFeeForMessage.
    Dynamic addresses are deterministic placeholders: no signer secret and no broadcast.
    The instruction/account shape is the pinned IDL and is included in provenance.
    """
    template = cfg.execution["message_templates"][side]
    token_program = token_program or cfg.protocol["spl_token_program"]
    if token_program not in account_profiles(cfg):
        raise ValueError("unsupported token program")
    if len(blockhash) != 32:
        raise ValueError("blockhash must be 32 bytes")
    specs = {a["name"]: a for a in template["accounts"]}
    pump = b58decode(cfg.protocol["program_id"])
    fee_program = b58decode(cfg.protocol["fee_config"]["program_id"])
    ata_program = b58decode(ASSOCIATED_TOKEN_PROGRAM)
    base_program = b58decode(token_program)
    quote_program = b58decode(cfg.protocol["spl_token_program"])
    def placeholder(name: str) -> bytes:
        return hashlib.sha256(("PAPER_FEE_TEMPLATE:" + name).encode()).digest()
    mint, creator = placeholder("mint"), placeholder("creator")
    keys_by_name = {
        "user": DUMMY_PAYER, "base_mint": mint, "quote_mint": b58decode(cfg.protocol["wsol_mint"]),
        "base_token_program": base_program, "quote_token_program": quote_program,
        "associated_token_program": ata_program, "system_program": b58decode(SYSTEM_PROGRAM),
        "global": b58decode(cfg.protocol["global_account"]["address"]),
        "fee_config": b58decode(cfg.protocol["fee_config"]["address"]), "fee_program": fee_program,
        "program": pump, "event_authority": find_pda(pump, b"__event_authority"),
        "bonding_curve": find_pda(pump, b"bonding-curve", mint),
        "creator_vault": find_pda(pump, b"creator-vault", creator),
        "sharing_config": find_pda(fee_program, b"sharing-config", mint),
        "global_volume_accumulator": find_pda(pump, b"global_volume_accumulator"),
        "user_volume_accumulator": find_pda(pump, b"user_volume_accumulator", DUMMY_PAYER),
        "fee_recipient": placeholder("fee_recipient"), "buyback_fee_recipient": placeholder("buyback_fee_recipient"),
    }
    for account, owner in (("associated_base_user", "user"), ("associated_base_bonding_curve", "bonding_curve")):
        keys_by_name[account] = find_pda(ata_program, keys_by_name[owner], base_program, mint)
    for account, owner in (("associated_quote_user", "user"), ("associated_quote_bonding_curve", "bonding_curve"),
                           ("associated_quote_fee_recipient", "fee_recipient"), ("associated_quote_buyback_fee_recipient", "buyback_fee_recipient"),
                           ("associated_creator_vault", "creator_vault"), ("associated_user_volume_accumulator", "user_volume_accumulator")):
        keys_by_name[account] = find_pda(ata_program, keys_by_name[owner], quote_program, keys_by_name["quote_mint"])
    def key(name: str) -> bytes:
        address = specs.get(name, {}).get("address")
        value = keys_by_name[name]  # unknown account names fail closed, never invented
        if address and value != b58decode(address):
            raise ValueError("IDL account address mismatch: " + name)
        return value
    metas: dict[bytes, tuple[bool, bool]] = {DUMMY_PAYER: (True, True)}
    instructions: list[tuple[bytes, list[bytes], bytes]] = []
    def add(program: bytes, accounts: list[tuple[bytes, bool, bool]], data: bytes) -> None:
        metas.setdefault(program, (False, False))
        for k, signer, writable in accounts:
            old = metas.get(k, (False, False))
            metas[k] = (old[0] or signer, old[1] or writable)
        instructions.append((program, [a[0] for a in accounts], data))
    cb = b58decode(COMPUTE_BUDGET_PROGRAM)
    add(cb, [], bytes([2]) + struct.pack("<I", cfg.execution["priority_policy"]["compute_unit_limit"]))
    add(cb, [], bytes([3]) + struct.pack("<Q", cu_price_micro))
    if side == "buy" and cfg.execution["token_account_policy"]["assume_new_token_account_per_position"]:
        ata = b58decode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
        add(ata, [(key("user"), True, True), (key("associated_base_user"), False, True), (key("user"), True, True),
                  (key("base_mint"), False, False), (b58decode("11111111111111111111111111111111"), False, False),
                  (key("base_token_program"), False, False)], bytes([1]))
    # Fee computation depends on the configured message, not hypothetical swap amounts.
    if any(a["type"] != "u64" for a in template["args"]):
        raise ValueError("unsupported instruction argument type")
    data = bytes(template["discriminator"]) + b"".join(struct.pack("<Q", 1) for a in template["args"])
    add(b58decode(cfg.protocol["program_id"]), [(key(a["name"]), a.get("signer", False), a.get("writable", False)) for a in template["accounts"]], data)
    if side == "sell" and cfg.execution["token_account_policy"]["close_token_account_on_full_exit"]:
        add(key("base_token_program"), [(key("associated_base_user"), False, True), (key("user"), True, True), (key("user"), True, False)], bytes([9]))
    keys = sorted(metas, key=lambda k: (not metas[k][0], not metas[k][1]))
    signers = sum(metas[k][0] for k in keys)
    if signers != cfg.execution["signature_count"]:
        raise ValueError("configured signature count differs from Pump message template")
    readonly_signed = sum(s and not w for s, w in metas.values())
    readonly_unsigned = sum(not s and not w for s, w in metas.values())
    compiled = b"".join(bytes([keys.index(program)]) + compact_u16(len(accounts)) + bytes(keys.index(k) for k in accounts) + compact_u16(len(data)) + data for program, accounts, data in instructions)
    return bytes([signers, readonly_signed, readonly_unsigned]) + compact_u16(len(keys)) + b"".join(keys) + blockhash + compact_u16(len(instructions)) + compiled


def message_template_hash(signature_count: int, compute_unit_limit: int, message_format: str) -> str:
    import hashlib

    return hashlib.sha256(f"{message_format}|sigs={signature_count}|cu_limit={compute_unit_limit}|ix=SetComputeUnitLimit,SetComputeUnitPrice".encode()).hexdigest()[:16]


@dataclass
class FeeCache:
    protocol: FeeSnapshot | None = None
    network: FeeSnapshot | None = None
    rent: FeeSnapshot | None = None
    priority_policy: FeeSnapshot | None = None

    def basis(self) -> CostBasis:
        return CostBasis(self.protocol, self.network, self.rent, self.priority_policy)


class FeeResolver:
    """Builds immutable FeeSnapshots from RPC reads; each snapshot is handed to the recorder by the caller."""

    def __init__(self, cfg: Any, rpc: RpcCall | None, clock: Any, run_id: str, provider_label: str) -> None:
        self.cfg = cfg
        self.rpc = rpc
        self._rpc_times: deque[int] = deque()
        if rpc is not None:
            def budgeted_rpc(method: str, params: list[Any]) -> Any:
                now = self.clock.monotonic_ns()
                while self._rpc_times and self._rpc_times[0] <= now - 60_000_000_000:
                    self._rpc_times.popleft()
                if len(self._rpc_times) >= self.cfg.raw["rpc_http"]["max_requests_per_minute"]:
                    raise RpcError("configured free RPC request budget exhausted")
                self._rpc_times.append(now)
                return rpc(method, params)
            self.rpc = budgeted_rpc
        self.clock = clock
        self.run_id = run_id
        self.provider = provider_label
        self.cache = FeeCache()
        self._seq = 0
        self.errors: list[str] = []
        refresh = cfg.costs["snapshot_refresh_policy"]
        self.max_age = {"protocol": refresh["protocol_max_age_ms"] * NS_PER_MS, "network": refresh["network_fee_max_age_ms"] * NS_PER_MS, "rent": refresh["rent_max_age_ms"] * NS_PER_MS, "priority_policy": refresh["network_fee_max_age_ms"] * NS_PER_MS}
        self.interval_ms = {"protocol": refresh["protocol_refresh_interval_ms"], "network": refresh["network_fee_refresh_interval_ms"], "rent": refresh["rent_refresh_interval_ms"], "priority_policy": refresh["network_fee_refresh_interval_ms"]}
        # Fee acquisition reuses the configured transient connectivity backoff used by live feeds.
        # This keeps operative timing limits in config (S16); retry scheduling never changes evidence TTL.
        initial = cfg.connectivity["reconnect_initial_ms"]
        maximum = cfg.connectivity["reconnect_max_ms"]
        retry = []
        delay = initial
        while True:
            retry.append(delay)
            if delay >= maximum:
                break
            delay = min(delay * 2, maximum)
        self.retry_backoff_ms = tuple(retry)
        self.family_interval_ms = {
            "protocol": self.interval_ms["protocol"],
            "network_pair": self.interval_ms["network"],
            "rent": self.interval_ms["rent"],
        }
        self._retry_index = {family: 0 for family in self.family_interval_ms}

    def _snap(self, kind: str, source_type: str, method: str, accounts: tuple[str, ...], slot: int | None, raw: dict[str, Any], normalized: dict[str, Any], status: str, evidence: CostEvidence, applicability: str, rounding: str = "INTEGER_LAMPORTS") -> FeeSnapshot:
        self._seq += 1
        return FeeSnapshot(
            fee_snapshot_id=f"{self.run_id}_fee_{kind}_{self._seq}", source_type=source_type, source_provider=self.provider, source_method=method, source_account_addresses=accounts,
            source_slot=slot, observed_utc=utc_iso(self.clock.utc_ns()), received_mono_ns=self.clock.monotonic_ns(), commitment=self.cfg.raw["rpc_http"]["commitment"],
            protocol_program_id=self.cfg.protocol["program_id"], idl_version_or_hash=self.cfg.protocol["idl_version_or_hash"], raw_values=raw, normalized_fee_components=normalized,
            rounding_rules=rounding, applicability=applicability, freshness_policy_hash=self.cfg.freshness_policy_hash, validation_status=status, config_hash=self.cfg.cost_model_hash,
            max_age_ns=self.max_age[kind], evidence=evidence,
        )

    @staticmethod
    def _retryable_snapshot(snapshot: FeeSnapshot) -> bool:
        raw = snapshot.raw_values
        return bool(raw.get("retryable_acquisition_failure", raw.get("transient_acquisition_failure", False)))

    @staticmethod
    def _usable_snapshot(snapshot: FeeSnapshot) -> bool:
        return snapshot.validation_status == "VALID" and snapshot.evidence != CostEvidence.UNKNOWN

    def _next_refresh_delay_ms(self, family: str, snapshots: list[FeeSnapshot]) -> int:
        """Return next delay for one causal acquisition family.

        Retryable transport/request-budget failures use bounded backoff. Success resets the
        family. Non-retryable validation failures stay on the normal cadence because extra RPC
        traffic cannot repair a layout/profile/protocol mismatch.
        """
        if family not in self.family_interval_ms:
            raise ValueError(f"unknown fee refresh family: {family}")
        by_kind = {FEE_KINDS[s.source_type]: s for s in snapshots}
        required = ("priority_policy", "network") if family == "network_pair" else (family,)
        selected = [by_kind[k] for k in required if k in by_kind]
        usable = len(selected) == len(required) and all(self._usable_snapshot(s) for s in selected)
        retryable = any(self._retryable_snapshot(s) for s in selected)
        if usable or not retryable:
            self._retry_index[family] = 0
            return self.family_interval_ms[family]
        index = self._retry_index[family]
        delay = self.retry_backoff_ms[min(index, len(self.retry_backoff_ms) - 1)]
        self._retry_index[family] = min(index + 1, len(self.retry_backoff_ms) - 1)
        return delay

    # -------------------------------------------------------------- individual resolvers (blocking RPC; run via to_thread)
    def resolve_protocol(self) -> FeeSnapshot:
        ga = self.cfg.protocol["global_account"]
        addr = ga["address"]
        if self.rpc is None:
            return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", (addr,), None, {}, {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None}, "UNAVAILABLE_NO_RPC", CostEvidence.UNKNOWN, "PUMP_BONDING_CURVE_SOL", self.cfg.protocol["fee_rounding"])
        response_observed = False
        try:
            result = self.rpc("getAccountInfo", [addr, {"encoding": "base64", "commitment": self.cfg.raw["rpc_http"]["commitment"]}])
            response_observed = True
            value = result["value"]
            slot = result.get("context", {}).get("slot")
            owner = value["owner"]
            data = base64.b64decode(value["data"][0])
            if owner != self.cfg.protocol["program_id"]:
                return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", (addr,), slot, {"owner": owner, "data_len": len(data)}, {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None}, "INVALID_OWNER", CostEvidence.UNKNOWN, "PUMP_BONDING_CURVE_SOL", self.cfg.protocol["fee_rounding"])
            expected_disc = account_discriminator("Global")
            if data[:8] != expected_disc:
                return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", (addr,), slot, {"owner": owner, "data_len": len(data), "discriminator_hex": data[:8].hex(), "expected_discriminator_hex": expected_disc.hex()},
                                  {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None}, "INVALID_DISCRIMINATOR", CostEvidence.UNKNOWN, "PUMP_BONDING_CURVE_SOL", self.cfg.protocol["fee_rounding"])
            reader = _Reader(data[8:])
            fields = {name: reader.read(kind) for name, kind in (tuple(f) for f in ga["layout_after_discriminator"])}
            fee_bps, creator_bps = int(fields["fee_basis_points"]), int(fields["creator_fee_basis_points"])
            plausible = 0 <= fee_bps <= ga["max_reasonable_bps"] and 0 <= creator_bps <= ga["max_reasonable_bps"] and fields.get("initialized", True)
            verified = str(ga["verification_status"]).startswith("VERIFIED")
            raw = {k: (v if isinstance(v, (int, str, bool)) else str(v)) for k, v in fields.items()}
            raw.update({"data_len": len(data), "owner": owner, "layout_verification_status": ga["verification_status"], "candidate_protocol_fee_bps": fee_bps, "candidate_creator_fee_bps": creator_bps})
            if not plausible:
                status = "IMPLAUSIBLE_VALUES"
            elif not verified:
                status = "LAYOUT_UNVERIFIED"  # decoded candidates are reported for doctor/operator verification but never used economically
            else:
                status = "VALID"
            normalized = {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None}
            addresses = (addr,)
            if status == "VALID":
                if reader.remaining():
                    status = "GLOBAL_LAYOUT_LENGTH_MISMATCH"
                else:
                    fc = self.cfg.protocol["fee_config"]
                    result_fee = self.rpc("getAccountInfo", [fc["address"], {"encoding": "base64", "commitment": self.cfg.raw["rpc_http"]["commitment"]}])
                    account = result_fee["value"]
                    encoded = base64.b64decode(account["data"][0])
                    addresses = (addr, fc["address"])
                    raw["fee_config_slot"] = result_fee.get("context", {}).get("slot")
                    raw["fee_config_owner"] = account["owner"]
                    raw["fee_config_idl"] = fc["idl_version_or_hash"]
                    if account["owner"] != fc["program_id"]:
                        status = "INVALID_FEE_CONFIG_OWNER"
                    elif encoded[:8] != account_discriminator("FeeConfig"):
                        status = "INVALID_FEE_CONFIG_DISCRIMINATOR"
                    elif not fc["verification_status"].startswith("VERIFIED"):
                        status = "FEE_CONFIG_LAYOUT_UNVERIFIED"
                    else:
                        fr = _Reader(encoded[8:])
                        ff = {n: fr.read(k) for n, k in fc["layout_after_discriminator"]}
                        raw["fee_config"] = ff
                        raw["fee_config_allocated_bytes"] = len(encoded)
                        tail = fr.take(fr.remaining())
                        tiers = ff["fee_tiers"]
                        valid_tiers = bool(tiers) and all(0 <= int(v) <= ga["max_reasonable_bps"] for t in tiers for v in t["fees"].values())
                        valid_tiers = valid_tiers and [t["market_cap_lamports_threshold"] for t in tiers] == sorted({t["market_cap_lamports_threshold"] for t in tiers})
                        if any(tail) or not valid_tiers:
                            status = "INVALID_FEE_CONFIG_LAYOUT_OR_TIERS"
                        elif any(t["fees"]["lp_fee_bps"] for t in tiers):
                            status = "UNSUPPORTED_CURVE_LP_FEE"
                        else:
                            normalized = {"protocol_fee_bps": tiers[0]["fees"]["protocol_fee_bps"], "creator_fee_bps": tiers[0]["fees"]["creator_fee_bps"],
                                          "other_protocol_fee_bps": 0, "fee_tiers": tiers, "rule": fc["rule"],
                                          "creator_fee_configurable": fields["creator_fee_configurable"],
                                          "standard_supply_base_units": 1_000_000_000_000_000,
                                          "rule_source": "@pump-fun/pump-sdk@2.0.0/src/fees.ts"}
            return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo:Global+FeeConfig", addresses, slot, raw,
                              normalized, status, CostEvidence.KNOWN_RULE if status == "VALID" else CostEvidence.UNKNOWN,
                              "PUMP_BONDING_CURVE_STANDARD_SOL", self.cfg.protocol["fee_rounding"])
        except Exception as exc:  # noqa: BLE001 - recorded as UNKNOWN snapshot, never as zero fee
            self.errors.append(f"protocol: {type(exc).__name__}")
            error = _acquisition_error(exc)
            if response_observed:
                error["transient_acquisition_failure"] = False  # partial observations may contain changed fee rules
            return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", (addr,), None, error, {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None}, "RPC_ERROR", CostEvidence.UNKNOWN, "PUMP_BONDING_CURVE_SOL", self.cfg.protocol["fee_rounding"])

    def resolve_priority(self) -> FeeSnapshot:
        pp = self.cfg.execution["priority_policy"]
        if self.rpc is None:
            return self._snap("priority_policy", "PRIORITY_POLICY", "getRecentPrioritizationFees", (), None, {}, {"compute_unit_price_micro_lamports": None}, "UNAVAILABLE_NO_RPC", CostEvidence.UNKNOWN, "COMPUTE_BUDGET_PRICE")
        try:
            result = self.rpc("getRecentPrioritizationFees", [[]])
            fees = sorted(int(r["prioritizationFee"]) for r in result[-pp["lookback_slots"] :]) if result else []
            if not fees:
                price = pp["min_compute_unit_price_micro_lamports"]
                derivation = "NO_SAMPLES_MIN_POLICY"
            else:
                rank = max(1, math.ceil(Decimal(pp["percentile"]) * len(fees)))
                price = fees[rank - 1]
                derivation = f"nearest_rank_p{pp['percentile']}_n{len(fees)}"
            price = min(max(price, pp["min_compute_unit_price_micro_lamports"]), pp["max_compute_unit_price_micro_lamports"])
            return self._snap("priority_policy", "PRIORITY_POLICY", "getRecentPrioritizationFees", (), None, {"samples": fees, "derivation": derivation}, {"compute_unit_price_micro_lamports": price, "compute_unit_limit": pp["compute_unit_limit"]}, "VALID", CostEvidence.MEASURED, "COMPUTE_BUDGET_PRICE")
        except Exception as exc:  # noqa: BLE001
            self.errors.append(f"priority: {type(exc).__name__}")
            return self._snap("priority_policy", "PRIORITY_POLICY", "getRecentPrioritizationFees", (), None, _acquisition_error(exc), {"compute_unit_price_micro_lamports": None}, "RPC_ERROR", CostEvidence.UNKNOWN, "COMPUTE_BUDGET_PRICE")

    def resolve_network(self, priority: FeeSnapshot | None) -> FeeSnapshot:
        ex, co = self.cfg.execution, self.cfg.costs
        sigs, cu_limit = ex["signature_count"], ex["priority_policy"]["compute_unit_limit"]
        base = sigs * co["lamports_per_signature"]
        price = None if priority is None else priority.normalized_fee_components.get("compute_unit_price_micro_lamports")
        template = hashlib.sha256(json.dumps({"templates": ex["message_templates"], "format": ex["message_format"], "signature_count": sigs, "cu_limit": cu_limit, "token_account_policy": ex["token_account_policy"]}, sort_keys=True).encode()).hexdigest()
        if co["base_fee_lamports"] is not None:
            base = int(co["base_fee_lamports"])  # CONFIGURED override of the rule value (documented in raw values)
        if price is None and co["priority_fee_lamports"] is not None:  # configured priority lamports as CONFIGURED fallback when no measurement exists
            prio_cfg = int(co["priority_fee_lamports"])
            return self._snap("network", "NETWORK_FEE", "CONFIG:costs.priority_fee_lamports", (), None, {"message_template_hash": template, "configured_priority_fee_lamports": prio_cfg, "base_rule": co["base_fee_rule"]},
                              {"base_fee_lamports": base, "priority_fee_lamports": prio_cfg, "network_fee_total_lamports": base + prio_cfg}, "VALID", CostEvidence.CONFIGURED, "PER_ATTEMPT_BUY_OR_SELL")
        if price is None:
            return self._snap("network", "NETWORK_FEE", "getFeeForMessage", (), None, {"message_template_hash": template}, {"base_fee_lamports": base, "priority_fee_lamports": None, "network_fee_total_lamports": None}, "PRIORITY_UNKNOWN", CostEvidence.UNKNOWN, "PER_ATTEMPT_BUY_OR_SELL")
        prio = priority_lamports(cu_limit, int(price))
        raw: dict[str, Any] = {"message_template_hash": template, "signature_count": sigs, "compute_unit_limit": cu_limit, "compute_unit_price_micro_lamports": int(price), "formula_priority_lamports": prio, "base_rule": co["base_fee_rule"]}
        if self.rpc is None:
            return self._snap("network", "NETWORK_FEE", "getFeeForMessage", (), None, raw, {"base_fee_lamports": base, "priority_fee_lamports": prio, "network_fee_total_lamports": base + prio}, "VALID_FORMULA_UNVALIDATED_NO_RPC", CostEvidence.MODELED, "PER_ATTEMPT_BUY_OR_SELL")
        results: dict[str, Any] = {}
        try:
            bh = self.rpc("getLatestBlockhash", [{"commitment": self.cfg.raw["rpc_http"]["commitment"]}])
            blockhash = b58decode(bh["value"]["blockhash"]).rjust(32, b"\0")
            results: dict[str, Any] = {}
            profiles: dict[str, Any] = {}
            for program, profile in account_profiles(self.cfg).items():
                results[program], profiles[program] = {}, {}
                for side in ("buy", "sell"):
                    msg = build_trade_fee_message(self.cfg, side, int(price), blockhash, program)
                    # Stable shape hash excludes blockhash/priority price; these are recorded separately.
                    shape = build_trade_fee_message(self.cfg, side, 0, bytes(32), program)
                    shape_hash = hashlib.sha256(shape).hexdigest()
                    fee = self.rpc("getFeeForMessage", [base64.b64encode(msg).decode(), {"commitment": self.cfg.raw["rpc_http"]["commitment"]}])
                    total = fee.get("value") if isinstance(fee, dict) else None
                    slot = fee.get("context", {}).get("slot") if isinstance(fee, dict) else None
                    status = "VALID" if type(total) is int and total == base + prio else "FEE_MESSAGE_UNVERIFIED"
                    results[program][side] = {"fee_lamports": total, "rpc_slot": slot, "message_base64": base64.b64encode(msg).decode(),
                                             "message_hash": hashlib.sha256(msg).hexdigest(), "message_template_hash": shape_hash}
                    profiles[program][side.upper()] = {"validation_status": status, "profile_id": profile["profile_id"],
                        "instruction_name": side + "_v2", "message_template_hash": shape_hash,
                        "base_fee_lamports": base if status == "VALID" else None,
                        "priority_fee_lamports": prio if status == "VALID" else None,
                        "network_fee_total_lamports": total if status == "VALID" else None}
            raw["execution_profiles"] = results
            legacy = self.cfg.protocol["spl_token_program"]
            raw["messages"] = results[legacy]  # compatibility for older report consumers
            raw["blockhash_slot"] = bh.get("context", {}).get("slot")
            valid = all(v["validation_status"] == "VALID" for sides in profiles.values() for v in sides.values())
            normalized = dict(profiles[legacy]["BUY"], execution_profiles=profiles)
            return self._snap("network", "NETWORK_FEE", "getFeeForMessage", (), raw["blockhash_slot"], raw, normalized,
                              "VALID" if valid else ("RPC_RETURNED_NULL" if any(v["fee_lamports"] is None for sides in results.values() for v in sides.values()) else "FEE_MESSAGE_UNVERIFIED"), CostEvidence.MEASURED if valid else CostEvidence.UNKNOWN,
                              "PER_PROGRAM_PROFILE_AND_SIDE;FEE_PROBE_NOT_EXECUTION_VALIDATION")
        except Exception as exc:  # noqa: BLE001
            self.errors.append(f"network: {type(exc).__name__}")
            error = _acquisition_error(exc)
            if any(results.values()):
                error["transient_acquisition_failure"] = False
            return self._snap("network", "NETWORK_FEE", "getFeeForMessage", (), None, raw | error, {"base_fee_lamports": base, "priority_fee_lamports": None, "network_fee_total_lamports": None}, "RPC_ERROR", CostEvidence.UNKNOWN, "PER_ATTEMPT_BUY_OR_SELL")

    def resolve_rent(self) -> FeeSnapshot:
        profiles: dict[str, Any] = {}
        raw: dict[str, Any] = {}
        configured = self.cfg.costs["account_rent_lamports"]
        failures = []
        for program, profile in account_profiles(self.cfg).items():
            size = profile["account_size_bytes"]
            status, lamports = "VALID", None
            evidence = CostEvidence.MEASURED
            acquisition_error = None
            try:
                if self.rpc is not None:
                    lamports = self.rpc("getMinimumBalanceForRentExemption", [size])
                    if type(lamports) is not int or lamports < 0:
                        raise ValueError("invalid rent response")
                elif program == self.cfg.protocol["spl_token_program"] and configured is not None:
                    lamports, evidence = int(configured), CostEvidence.CONFIGURED
                else:
                    status, evidence = "UNAVAILABLE_NO_RPC", CostEvidence.UNKNOWN
            except Exception as exc:
                self.errors.append(f"rent: {type(exc).__name__}")
                status, evidence = "RPC_ERROR", CostEvidence.UNKNOWN
                acquisition_error = _acquisition_error(exc)
                failures.append(acquisition_error)
            profiles[program] = {"profile_id": profile["profile_id"], "account_size_bytes": size,
                                 "token_account_rent_lamports": lamports, "validation_status": status, "evidence": evidence.value}
            raw[program] = dict(profiles[program])
            if acquisition_error is not None:
                raw[program]["acquisition_error"] = acquisition_error
        legacy = profiles[self.cfg.protocol["spl_token_program"]]
        valid = all(v["validation_status"] == "VALID" for v in profiles.values())
        # Mixed profile results invalidate the combined rent snapshot, but a transport failure
        # in any profile still schedules a fast retry.
        raw["transient_acquisition_failure"] = bool(failures) and len(failures) == len(profiles) and all(f["transient_acquisition_failure"] for f in failures)
        raw["retryable_acquisition_failure"] = any(f["retryable_acquisition_failure"] for f in failures)
        evidence = CostEvidence.MEASURED if self.rpc is not None else CostEvidence.CONFIGURED
        return self._snap("rent", "RENT_EXEMPTION", "getMinimumBalanceForRentExemption" if self.rpc else "CONFIG:costs.account_rent_lamports",
                          (), None, raw, dict(legacy, account_profiles=profiles), "VALID" if valid else "ACCOUNT_RENT_UNVERIFIED",
                          evidence if valid else CostEvidence.UNKNOWN, "PER_PROGRAM_ACCOUNT_PROFILE_REFUNDABLE;NO_QUOTE_ATA_FOR_NATIVE_SOL")

    def resolve_all(self) -> list[FeeSnapshot]:
        protocol = self.resolve_protocol()
        priority = self.resolve_priority()
        network = self.resolve_network(priority if priority.validation_status == "VALID" else None)
        rent = self.resolve_rent()
        effective = select_cost_basis(self.cache.basis(), [protocol, priority, network, rent])
        self.cache = FeeCache(**{k: getattr(effective, k) for k in FEE_KINDS.values()})
        return [protocol, priority, network, rent]

    async def run_background(self, on_snapshots: Callable[[list[FeeSnapshot]], None], stop: asyncio.Event) -> None:
        """Refresh fee evidence outside the hot path with per-family transient retry backoff."""
        def publish(batch: list[FeeSnapshot]) -> None:
            published_at = self.clock.monotonic_ns()
            batch = [replace(snap, received_mono_ns=max(snap.received_mono_ns, published_at)) for snap in batch]
            effective = select_cost_basis(self.cache.basis(), batch)
            self.cache = FeeCache(**{k: getattr(effective, k) for k in FEE_KINDS.values()})
            on_snapshots(batch)

        due = {family: 0.0 for family in self.family_interval_ms}
        loop = asyncio.get_running_loop()
        while not stop.is_set():
            now = loop.time()
            batch: list[FeeSnapshot] = []

            if now >= due["protocol"]:
                protocol = await asyncio.to_thread(self.resolve_protocol)
                batch.append(protocol)
                due["protocol"] = loop.time() + self._next_refresh_delay_ms("protocol", [protocol]) / 1000

            if now >= due["network_pair"]:
                priority = await asyncio.to_thread(self.resolve_priority)
                network = await asyncio.to_thread(self.resolve_network, priority if priority.validation_status == "VALID" else None)
                pair = [priority, network]
                batch.extend(pair)
                due["network_pair"] = loop.time() + self._next_refresh_delay_ms("network_pair", pair) / 1000

            if now >= due["rent"]:
                rent = await asyncio.to_thread(self.resolve_rent)
                batch.append(rent)
                due["rent"] = loop.time() + self._next_refresh_delay_ms("rent", [rent]) / 1000

            if batch:
                publish(batch)

            wait_s = max(
                self.cfg.costs["snapshot_refresh_policy"]["min_poll_interval_ms"] / 1000,
                min(due.values()) - loop.time(),
            )
            try:
                await asyncio.wait_for(stop.wait(), timeout=wait_s)
            except asyncio.TimeoutError:
                pass
