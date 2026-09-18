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
from decimal import Decimal
from typing import Any, Callable

from darkbot.domain import NS_PER_MS, CostBasis, CostEvidence, FeeSnapshot, utc_iso
from darkbot.execution import priority_lamports
from darkbot.market import _Reader, b58decode

COMPUTE_BUDGET_PROGRAM = "ComputeBudget111111111111111111111111111111"
DUMMY_PAYER = bytes([1]) * 32  # fee probe only; getFeeForMessage does not validate signers, nothing is ever signed or sent
RpcCall = Callable[[str, list[Any]], Any]


class RpcError(RuntimeError):
    pass


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
        self.clock = clock
        self.run_id = run_id
        self.provider = provider_label
        self.cache = FeeCache()
        self._seq = 0
        self.errors: list[str] = []
        refresh = cfg.costs["snapshot_refresh_policy"]
        self.max_age = {"protocol": refresh["protocol_max_age_ms"] * NS_PER_MS, "network": refresh["network_fee_max_age_ms"] * NS_PER_MS, "rent": refresh["rent_max_age_ms"] * NS_PER_MS, "priority_policy": refresh["network_fee_max_age_ms"] * NS_PER_MS}
        self.interval_ms = {"protocol": refresh["protocol_refresh_interval_ms"], "network": refresh["network_fee_refresh_interval_ms"], "rent": refresh["rent_refresh_interval_ms"], "priority_policy": refresh["network_fee_refresh_interval_ms"]}

    def _snap(self, kind: str, source_type: str, method: str, accounts: tuple[str, ...], slot: int | None, raw: dict[str, Any], normalized: dict[str, Any], status: str, evidence: CostEvidence, applicability: str, rounding: str = "INTEGER_LAMPORTS") -> FeeSnapshot:
        self._seq += 1
        return FeeSnapshot(
            fee_snapshot_id=f"{self.run_id}_fee_{kind}_{self._seq}", source_type=source_type, source_provider=self.provider, source_method=method, source_account_addresses=accounts,
            source_slot=slot, observed_utc=utc_iso(self.clock.utc_ns()), received_mono_ns=self.clock.monotonic_ns(), commitment=self.cfg.raw["rpc_http"]["commitment"],
            protocol_program_id=self.cfg.protocol["program_id"], idl_version_or_hash=self.cfg.protocol["idl_version_or_hash"], raw_values=raw, normalized_fee_components=normalized,
            rounding_rules=rounding, applicability=applicability, freshness_policy_hash=self.cfg.freshness_policy_hash, validation_status=status, config_hash=self.cfg.cost_model_hash,
            max_age_ns=self.max_age[kind], evidence=evidence,
        )

    # -------------------------------------------------------------- individual resolvers (blocking RPC; run via to_thread)
    def resolve_protocol(self) -> FeeSnapshot:
        ga = self.cfg.protocol["global_account"]
        addr = ga["address"]
        if self.rpc is None:
            return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", (addr,), None, {}, {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None}, "UNAVAILABLE_NO_RPC", CostEvidence.UNKNOWN, "PUMP_BONDING_CURVE_SOL", self.cfg.protocol["fee_rounding"])
        try:
            result = self.rpc("getAccountInfo", [addr, {"encoding": "base64", "commitment": self.cfg.raw["rpc_http"]["commitment"]}])
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
            ok = status == "VALID"
            return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", (addr,), slot, raw,
                              {"protocol_fee_bps": fee_bps, "creator_fee_bps": creator_bps, "other_protocol_fee_bps": 0} if ok else {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None},
                              status, CostEvidence.MEASURED if ok else CostEvidence.UNKNOWN, "PUMP_BONDING_CURVE_SOL", self.cfg.protocol["fee_rounding"])
        except Exception as exc:  # noqa: BLE001 - recorded as UNKNOWN snapshot, never as zero fee
            self.errors.append(f"protocol: {type(exc).__name__}")
            return self._snap("protocol", "PROTOCOL_GLOBAL_ACCOUNT", "getAccountInfo", (addr,), None, {"error": type(exc).__name__}, {"protocol_fee_bps": None, "creator_fee_bps": None, "other_protocol_fee_bps": None}, "RPC_ERROR", CostEvidence.UNKNOWN, "PUMP_BONDING_CURVE_SOL", self.cfg.protocol["fee_rounding"])

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
            return self._snap("priority_policy", "PRIORITY_POLICY", "getRecentPrioritizationFees", (), None, {"error": type(exc).__name__}, {"compute_unit_price_micro_lamports": None}, "RPC_ERROR", CostEvidence.UNKNOWN, "COMPUTE_BUDGET_PRICE")

    def resolve_network(self, priority: FeeSnapshot | None) -> FeeSnapshot:
        ex, co = self.cfg.execution, self.cfg.costs
        sigs, cu_limit = ex["signature_count"], ex["priority_policy"]["compute_unit_limit"]
        base = sigs * co["lamports_per_signature"]
        price = None if priority is None else priority.normalized_fee_components.get("compute_unit_price_micro_lamports")
        template = message_template_hash(sigs, cu_limit, ex["message_format"])
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
        try:
            bh = self.rpc("getLatestBlockhash", [{"commitment": self.cfg.raw["rpc_http"]["commitment"]}])
            blockhash = b58decode(bh["value"]["blockhash"]).rjust(32, b"\0")
            msg = build_fee_probe_message(sigs, cu_limit, int(price), blockhash)
            fee = self.rpc("getFeeForMessage", [base64.b64encode(msg).decode(), {"commitment": self.cfg.raw["rpc_http"]["commitment"]}])
            rpc_fee = fee.get("value") if isinstance(fee, dict) else None
            raw.update({"rpc_fee_for_message_lamports": rpc_fee, "rpc_slot": fee.get("context", {}).get("slot") if isinstance(fee, dict) else None, "blockhash_slot": bh.get("context", {}).get("slot")})
            if rpc_fee is None:
                return self._snap("network", "NETWORK_FEE", "getFeeForMessage", (), raw["rpc_slot"], raw, {"base_fee_lamports": base, "priority_fee_lamports": None, "network_fee_total_lamports": None}, "RPC_RETURNED_NULL", CostEvidence.UNKNOWN, "PER_ATTEMPT_BUY_OR_SELL")
            status = "VALID" if int(rpc_fee) == base + prio else "VALID_FORMULA_MISMATCH_RPC"
            total = int(rpc_fee)
            return self._snap("network", "NETWORK_FEE", "getFeeForMessage", (), raw["rpc_slot"], raw, {"base_fee_lamports": base, "priority_fee_lamports": total - base, "network_fee_total_lamports": total}, status, CostEvidence.MEASURED, "PER_ATTEMPT_BUY_OR_SELL")
        except Exception as exc:  # noqa: BLE001
            self.errors.append(f"network: {type(exc).__name__}")
            return self._snap("network", "NETWORK_FEE", "getFeeForMessage", (), None, raw | {"error": type(exc).__name__}, {"base_fee_lamports": base, "priority_fee_lamports": None, "network_fee_total_lamports": None}, "RPC_ERROR", CostEvidence.UNKNOWN, "PER_ATTEMPT_BUY_OR_SELL")

    def resolve_rent(self) -> FeeSnapshot:
        size = self.cfg.execution["token_account_policy"]["spl_token_account_size_bytes"]
        configured = self.cfg.costs["account_rent_lamports"]
        if self.rpc is None:
            if configured is not None:  # explicit operator value: CONFIGURED evidence, never a silent default
                return self._snap("rent", "RENT_EXEMPTION", "CONFIG:costs.account_rent_lamports", (), None, {"account_size_bytes": size, "configured": configured}, {"token_account_rent_lamports": int(configured), "account_size_bytes": size}, "VALID", CostEvidence.CONFIGURED, "SPL_TOKEN_ACCOUNT_REFUNDABLE")
            return self._snap("rent", "RENT_EXEMPTION", "getMinimumBalanceForRentExemption", (), None, {"account_size_bytes": size}, {"token_account_rent_lamports": None, "account_size_bytes": size}, "UNAVAILABLE_NO_RPC", CostEvidence.UNKNOWN, "SPL_TOKEN_ACCOUNT_REFUNDABLE")
        try:
            lamports = self.rpc("getMinimumBalanceForRentExemption", [size])
            return self._snap("rent", "RENT_EXEMPTION", "getMinimumBalanceForRentExemption", (), None, {"account_size_bytes": size, "lamports": lamports}, {"token_account_rent_lamports": int(lamports), "account_size_bytes": size}, "VALID", CostEvidence.MEASURED, "SPL_TOKEN_ACCOUNT_REFUNDABLE")
        except Exception as exc:  # noqa: BLE001
            self.errors.append(f"rent: {type(exc).__name__}")
            return self._snap("rent", "RENT_EXEMPTION", "getMinimumBalanceForRentExemption", (), None, {"account_size_bytes": size, "error": type(exc).__name__}, {"token_account_rent_lamports": None, "account_size_bytes": size}, "RPC_ERROR", CostEvidence.UNKNOWN, "SPL_TOKEN_ACCOUNT_REFUNDABLE")

    def resolve_all(self) -> list[FeeSnapshot]:
        protocol = self.resolve_protocol()
        priority = self.resolve_priority()
        network = self.resolve_network(priority if priority.validation_status == "VALID" else None)
        rent = self.resolve_rent()
        self.cache = FeeCache(protocol, network, rent, priority)
        return [protocol, priority, network, rent]

    async def run_background(self, on_snapshots: Callable[[list[FeeSnapshot]], None], stop: asyncio.Event) -> None:
        """Refresh per configured intervals; snapshots are pushed to the caller (recorder + StateOwner) via callback."""
        def publish(batch: list[FeeSnapshot]) -> None:
            # A CostBasis is one causal knowledge point. Do not expose a new
            # component through the cache while the remaining refresh calls
            # are still in flight.
            current = {"protocol": self.cache.protocol, "network": self.cache.network,
                       "rent": self.cache.rent, "priority_policy": self.cache.priority_policy}
            kinds = {"PROTOCOL_GLOBAL_ACCOUNT": "protocol", "NETWORK_FEE": "network",
                     "RENT_EXEMPTION": "rent", "PRIORITY_POLICY": "priority_policy"}
            published_at = self.clock.monotonic_ns()
            batch = [replace(snap, received_mono_ns=max(snap.received_mono_ns, published_at)) for snap in batch]
            for snap in batch:
                current[kinds[snap.source_type]] = snap
            self.cache = FeeCache(current["protocol"], current["network"], current["rent"], current["priority_policy"])
            on_snapshots(batch)

        due = {k: 0.0 for k in self.interval_ms}
        loop = asyncio.get_running_loop()
        while not stop.is_set():
            now = loop.time()
            batch: list[FeeSnapshot] = []
            if now >= due["protocol"]:
                batch.append(await asyncio.to_thread(self.resolve_protocol))
                due["protocol"] = now + self.interval_ms["protocol"] / 1000
            if now >= due["priority_policy"]:
                prio = await asyncio.to_thread(self.resolve_priority)
                batch.append(prio)
                net = await asyncio.to_thread(self.resolve_network, prio if prio.validation_status == "VALID" else None)
                batch.append(net)
                due["priority_policy"] = due["network"] = now + self.interval_ms["network"] / 1000
            if now >= due["rent"]:
                batch.append(await asyncio.to_thread(self.resolve_rent))
                due["rent"] = now + self.interval_ms["rent"] / 1000
            if batch:
                publish(batch)
            try:
                await asyncio.wait_for(stop.wait(), timeout=max(self.cfg.costs["snapshot_refresh_policy"]["min_poll_interval_ms"] / 1000, min(due.values()) - loop.time()))
            except asyncio.TimeoutError:
                pass
