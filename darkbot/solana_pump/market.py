"""Versioned minimal decoder, atomic envelopes, canonical identity/dedup, market/venue state,
SOL scope and canonical coverage (spec: market.py). Mutation only via StateOwner calls.

Hot-path rule: only the JSON/Borsh decoding that is required; no re-serialisation, hashing,
DB rows or prints happen here.
"""

from __future__ import annotations

from darkbot.solana_pump.token_accounts import create_account_profiles

import base64
import binascii
import hashlib
import json
import struct
from dataclasses import dataclass, field
from typing import Any

from darkbot.solana_pump.domain import (
    Completeness, CoverageState, EventType, MarketState, NormalizedEnvelope, NormalizedEvent, QuoteScopeStatus, RawEnvelope, ValidationStatus, VenueState,
)

PROGRAM_DATA_PREFIX = "Program data: "
DISCRIMINATOR_BYTES = 8
PUBKEY_BYTES = 32
_B58 = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
# Known Pump event names that carry no curve state change for our purposes (technical definitions, versioned with decoder_version).
KNOWN_NON_STATE_EVENTS = ("SetCreatorEvent", "CollectCreatorFeeEvent", "ExtendAccountEvent", "SetParamsEvent", "UpdateGlobalAuthorityEvent",
                          "ClaimTokenIncentivesEvent", "SyncUserVolumeAccumulatorEvent", "InitUserVolumeAccumulatorEvent", "AdminSetCreatorEvent",
                          "SetMetaplexCreatorEvent", "AdminUpdateTokenIncentivesEvent", "AdminSetIdlAuthorityEvent", "CloseUserVolumeAccumulatorEvent")


def b58encode(raw: bytes) -> str:
    number = int.from_bytes(raw, "big")
    out = bytearray()
    while number > 0:
        number, rem = divmod(number, 58)
        out.append(_B58[rem])
    leading = len(raw) - len(raw.lstrip(b"\0"))
    return (b"1" * leading + bytes(reversed(out))).decode("ascii")


def b58decode(text: str) -> bytes:
    number = 0
    for ch in text.encode("ascii"):
        number = number * 58 + _B58.index(ch)
    raw = number.to_bytes((number.bit_length() + 7) // 8, "big") if number else b""
    leading = len(text) - len(text.lstrip("1"))
    return b"\0" * leading + raw


def event_discriminator(name: str) -> bytes:
    return hashlib.sha256(f"event:{name}".encode("ascii")).digest()[:DISCRIMINATOR_BYTES]


class _Reader:
    __slots__ = ("data", "pos")

    def __init__(self, data: bytes) -> None:
        self.data, self.pos = data, 0

    def remaining(self) -> int:
        return len(self.data) - self.pos

    def take(self, n: int) -> bytes:
        if self.remaining() < n:
            raise ValueError("borsh payload truncated")
        chunk = self.data[self.pos : self.pos + n]
        self.pos += n
        return chunk

    def read(self, kind: Any) -> Any:
        if isinstance(kind, dict):
            if "struct" in kind:
                return {n: self.read(t) for n, t in kind["struct"]}
            if "array" in kind:
                element, count = kind["array"]
            elif "vec" in kind:
                element, count = kind["vec"], struct.unpack("<I", self.take(4))[0]
            else:
                raise ValueError("unsupported Borsh container")
            if count > self.remaining():
                raise ValueError("Borsh vector length exceeds payload")
            return [self.read(element) for _ in range(count)]
        if kind == "u128":
            return int.from_bytes(self.take(16), "little")
        if kind == "u16":
            return int.from_bytes(self.take(2), "little")
        if kind == "u64":
            return struct.unpack("<Q", self.take(8))[0]
        if kind == "i64":
            return struct.unpack("<q", self.take(8))[0]
        if kind == "bool":
            value = self.take(1)[0]
            if value not in (0, 1):
                raise ValueError("invalid Borsh bool")
            return bool(value)
        if kind == "pubkey":
            return b58encode(self.take(PUBKEY_BYTES))
        if kind == "string":
            length = struct.unpack("<I", self.take(4))[0]
            return self.take(length).decode("utf-8", errors="replace")
        if kind == "u8":
            return self.take(1)[0]
        raise ValueError(f"unsupported borsh kind {kind}")


def encode_fields(fields: list[tuple[str, Any]], layout: list[list[str]]) -> bytes:
    """Test/fixture helper: Borsh-encode values by layout (inverse of _Reader)."""
    out = bytearray()
    values = dict(fields)
    for name, kind in layout:
        v = values[name]
        if kind == "u64":
            out += struct.pack("<Q", v)
        elif kind == "i64":
            out += struct.pack("<q", v)
        elif kind == "bool":
            out += b"\1" if v else b"\0"
        elif kind == "pubkey":
            raw = b58decode(v)
            out += raw.rjust(PUBKEY_BYTES, b"\0")
        elif kind == "string":
            data = v.encode("utf-8")
            out += struct.pack("<I", len(data)) + data
        elif kind == "u8":
            out += bytes([v])
    return bytes(out)


@dataclass(frozen=True, slots=True)
class DecodedEvent:
    kind: str  # create|trade|complete|migrate|ignored|unknown|undecodable
    log_index: int
    fields: dict[str, Any]
    tail_bytes: int
    tail_base64: str | None


class PumpDecoder:
    """Anchor event decoder with layouts pinned in config.protocol.events (decoder_version)."""

    def __init__(self, protocol: dict[str, Any]) -> None:
        self.program_id: str = protocol["program_id"]
        self.version: str = protocol["decoder_version"]
        events = protocol["events"]
        self.layouts = {name: [tuple(f) for f in spec["prefix_fields"]] for name, spec in events.items()}
        self.event_tail_layouts = {name: list(spec.get("tail_layouts", [])) for name, spec in events.items()}
        self.protocol = protocol
        self.spl_token_program = protocol["spl_token_program"]
        self.disc = {event_discriminator(name): name for name in events}
        self.ignored = {event_discriminator(name) for name in KNOWN_NON_STATE_EVENTS}
        self.tail_layouts = protocol["create_event_tail_layouts"]
        self.wsol = protocol["wsol_mint"]
        self.native_sentinel = protocol["native_sol_curve_sentinel"]
        self.layout_hash = hashlib.sha256(json.dumps({"events": events, "tails": self.tail_layouts}, sort_keys=True).encode()).hexdigest()[:16]

    def decode_logs(self, logs: list[str]) -> list[DecodedEvent]:
        out: list[DecodedEvent] = []
        stack: list[str] = []
        saw_ctx = False
        for index, line in enumerate(logs):
            if line.startswith("Program "):
                parts = line.split()
                if len(parts) >= 3 and parts[2] == "invoke":
                    saw_ctx = True
                    stack.append(parts[1])
                    continue
                if len(parts) >= 3 and parts[2] in ("success", "failed:"):
                    if stack and stack[-1] == parts[1]:
                        stack.pop()
                    elif parts[1] in stack:
                        del stack[len(stack) - stack[::-1].index(parts[1]) - 1 :]
                    continue
            if not line.startswith(PROGRAM_DATA_PREFIX):
                continue
            if saw_ctx and (not stack or stack[-1] != self.program_id):
                continue
            encoded = line[len(PROGRAM_DATA_PREFIX) :].strip()
            try:
                data = base64.b64decode(encoded, validate=True)
            except (binascii.Error, ValueError):
                out.append(DecodedEvent("undecodable", index, {}, 0, None))
                continue
            if len(data) < DISCRIMINATOR_BYTES:
                out.append(DecodedEvent("undecodable", index, {}, 0, None))
                continue
            disc, payload = data[:DISCRIMINATOR_BYTES], data[DISCRIMINATOR_BYTES:]
            name = self.disc.get(disc)
            if name is None:
                out.append(DecodedEvent("ignored" if disc in self.ignored else "unknown", index, {}, len(payload), None))
                continue
            try:
                reader = _Reader(payload)
                fields = {fname: reader.read(kind) for fname, kind in self.layouts[name]}
                tail = reader.remaining()
                tail_b64 = base64.b64encode(payload[reader.pos :]).decode() if tail else None
                kind = {"TradeEvent": "trade", "CreateEvent": "create", "CompleteEvent": "complete", "CompletePumpAmmMigrationEvent": "migrate"}[name]
                tail_bytes = payload[reader.pos :]
                if kind == "create":
                    self._resolve_create_quote(fields, tail_bytes)
                else:
                    self._decode_tail(name, fields, tail_bytes)
                fields["tail_len"] = tail
                fields["tail_prefix_hash"] = tail_bytes[:16].hex() if tail else None  # raw prefix evidence; hashing belongs to the writer
                out.append(DecodedEvent(kind, index, fields, tail, tail_b64))
            except ValueError:
                out.append(DecodedEvent("undecodable", index, {}, 0, None))
        return out

    def _decode_tail(self, name: str, fields: dict[str, Any], tail: bytes) -> None:
        """Pinned tail layouts for non-create events: decoded as evidence; economically used only when VERIFIED_*."""
        fields["tail_layout_id"] = None
        fields["tail_verified"] = False
        fields["tail_quote_mint_raw"] = None
        for layout in self.event_tail_layouts.get(name, []):
            try:
                reader = _Reader(tail)
                values = {n: reader.read(k) for n, k in (tuple(f) for f in layout["tail_fields"])}
            except ValueError:
                continue
            if reader.remaining() != 0:
                continue
            fields["tail_layout_id"] = layout["layout_id"]
            fields["tail_verified"] = str(layout["verification_status"]).startswith("VERIFIED")
            fields["tail_values"] = values
            if layout.get("quote_mint_field"):
                fields["tail_quote_mint_raw"] = values.get(layout["quote_mint_field"])
            return

    def _resolve_create_quote(self, fields: dict[str, Any], tail: bytes) -> None:
        """Field-sensitive SOL normalisation via pinned tail layouts only (quote_asset_contract)."""
        fields["quote_mint_raw"] = None
        fields["quote_field_origin"] = None
        fields["quote_layout_hash"] = None
        fields["tail_verified"] = False
        fields["normalized_quote_asset"] = None
        fields["quote_scope_status"] = QuoteScopeStatus.QUOTE_UNKNOWN_LAYOUT_UNVERIFIED
        fields["token_program"] = None
        for layout in self.tail_layouts:
            layout_fields = [tuple(f) for f in layout["tail_fields"]]
            try:
                reader = _Reader(tail)
                values = {n: reader.read(k) for n, k in layout_fields}
            except ValueError:
                continue
            if reader.remaining() != 0:
                continue
            fields["tail_values"] = values
            fields["quote_layout_hash"] = layout["layout_id"]
            fields["tail_verified"] = str(layout["verification_status"]).startswith("VERIFIED")
            fields["token_program"] = values.get("token_program", layout.get("token_program"))
            qfield = layout["quote_mint_field"]
            if qfield is None:
                fields["quote_field_origin"] = "IMPLIED_BY_LAYOUT"
                if layout.get("implied_quote_asset") == "SOL_WSOL" and layout["verification_status"].startswith("VERIFIED"):
                    fields["normalized_quote_asset"] = "SOL_WSOL"
                    fields["quote_scope_status"] = QuoteScopeStatus.SOL_WSOL
                else:
                    fields["quote_scope_status"] = QuoteScopeStatus.QUOTE_UNKNOWN_LAYOUT_UNVERIFIED
                return
            raw = values.get(qfield)
            fields["quote_mint_raw"] = raw
            fields["quote_field_origin"] = f"CreateEvent.{qfield}"
            if not layout["verification_status"].startswith("VERIFIED"):
                fields["quote_scope_status"] = QuoteScopeStatus.QUOTE_UNKNOWN_LAYOUT_UNVERIFIED
            elif raw in (self.wsol, self.native_sentinel):
                fields["normalized_quote_asset"] = "SOL_WSOL"
                fields["quote_scope_status"] = QuoteScopeStatus.SOL_WSOL
            elif isinstance(raw, str) and raw:
                fields["quote_scope_status"] = QuoteScopeStatus.UNSUPPORTED_QUOTE
            else:
                fields["quote_scope_status"] = QuoteScopeStatus.QUOTE_UNKNOWN
            return
        fields["tail_len_unpinned"] = len(tail)


# --------------------------------------------------------------------------- normalizer


class Normalizer:
    def __init__(self, decoder: PumpDecoder) -> None:
        self.decoder = decoder

    def normalize(self, raw: RawEnvelope, transport: str, decode_done_mono_ns: int) -> NormalizedEnvelope | None:
        """Return None for non-event frames (subscription ACKs, pings); never invent fields."""
        try:
            message = json.loads(raw.payload)
        except json.JSONDecodeError:
            return NormalizedEnvelope(None, None, None, (), Completeness.UNDECODABLE, raw.feed_id, decode_done_mono_ns, self.decoder.version, "JSON_DECODE_ERROR")
        if not isinstance(message, dict):
            return None
        if transport == "pumpportal":
            return self._pumpportal(message, raw, decode_done_mono_ns)
        if message.get("method") != "logsNotification":
            return None
        try:
            result = message["params"]["result"]
            value = result["value"]
            signature = str(value["signature"])
            slot = result.get("context", {}).get("slot")
            err = value.get("err")
            logs = list(value.get("logs") or [])
        except (KeyError, TypeError):
            return NormalizedEnvelope(None, None, None, (), Completeness.UNDECODABLE, raw.feed_id, decode_done_mono_ns, self.decoder.version, "MALFORMED_NOTIFICATION")
        if err is not None:
            return NormalizedEnvelope(signature, slot, False, (), Completeness.FAILED_TX, raw.feed_id, decode_done_mono_ns, self.decoder.version, None)
        decoded = self.decoder.decode_logs(logs)
        profiles = create_account_profiles(logs, self.decoder.protocol) if any(d.kind == "create" and d.fields.get("token_program") == self.decoder.protocol["token_2022_program"] for d in decoded) else {}
        events: list[NormalizedEvent] = []
        completeness = Completeness.COMPLETE
        error = None
        for d in decoded:
            # Already recognized administrative events are deliberately ignored by
            # the decoder. Their payload length is not an unverified trade tail.
            if d.tail_bytes and d.kind not in ("create", "ignored") and not bool(d.fields.get("tail_verified")):
                completeness = Completeness.PARTIAL
                error = "UNVERIFIED_EVENT_TAIL"
            if d.kind == "trade":
                f = d.fields
                events.append(NormalizedEvent(EventType.BUY if f["is_buy"] else EventType.SELL, f["mint"], d.log_index, actor=f["user"], sol_amount=f["sol_amount"], token_amount=f["token_amount"],
                                              virtual_sol_reserves=f["virtual_sol_reserves"], virtual_token_reserves=f["virtual_token_reserves"], real_sol_reserves=f["real_sol_reserves"], real_token_reserves=f["real_token_reserves"],
                                              quote_mint_raw=f.get("tail_quote_mint_raw"), quote_layout_hash=f.get("tail_layout_id"),
                                              extra={"tail_bytes": d.tail_bytes, "tail_prefix_hash": f.get("tail_prefix_hash"), "tail_verified": f.get("tail_verified"), "tail_values": f.get("tail_values")}))
            elif d.kind == "create":
                f = d.fields
                events.append(NormalizedEvent(EventType.CREATE, f["mint"], d.log_index, actor=f["user"], creator=f["creator"], virtual_sol_reserves=f["virtual_sol_reserves"], virtual_token_reserves=f["virtual_token_reserves"],
                                              real_token_reserves=f["real_token_reserves"], token_total_supply=f["token_total_supply"], token_program=f["token_program"], quote_mint_raw=f["quote_mint_raw"],
                                              quote_field_origin=f["quote_field_origin"], quote_layout_hash=f["quote_layout_hash"], normalized_quote_asset=f["normalized_quote_asset"], quote_scope_status=f["quote_scope_status"],
                                              extra={"account_profile_id": profiles.get(d.log_index), "tail_bytes": d.tail_bytes, "tail_base64": d.tail_base64, "tail_prefix_hash": f.get("tail_prefix_hash"), "tail_len_unpinned": f.get("tail_len_unpinned"), "tail_values": f.get("tail_values"), "tail_verified": f.get("tail_verified"), "bonding_curve": f["bonding_curve"], "name": f["name"], "symbol": f["symbol"]}))
            elif d.kind == "complete":
                events.append(NormalizedEvent(EventType.COMPLETE, d.fields["mint"], d.log_index, actor=d.fields["user"]))
            elif d.kind == "migrate":
                events.append(NormalizedEvent(EventType.MIGRATE, d.fields["mint"], d.log_index, actor=d.fields["user"]))
            elif d.kind == "unknown":
                completeness = Completeness.PARTIAL
                error = "UNKNOWN_PUMP_EVENT_DISCRIMINATOR"
            elif d.kind == "undecodable":
                completeness = Completeness.PARTIAL
                error = "UNDECODABLE_PUMP_FRAME"
        return NormalizedEnvelope(signature, slot, True, tuple(events), completeness, raw.feed_id, decode_done_mono_ns, self.decoder.version, error)

    def _pumpportal(self, message: dict[str, Any], raw: RawEnvelope, decode_done: int) -> NormalizedEnvelope | None:
        tx_type = message.get("txType")
        mint = message.get("mint") if isinstance(message.get("mint"), str) else None
        signature = message.get("signature") if isinstance(message.get("signature"), str) else None
        if tx_type == "create" and mint:
            ev = NormalizedEvent(EventType.HINT_CREATE, mint, 0, actor=message.get("traderPublicKey"), extra={"pool": message.get("pool")})
            return NormalizedEnvelope(signature, None, None, (ev,), Completeness.HINT, raw.feed_id, decode_done, "PUMPPORTAL_HINT_V1", None)
        if tx_type == "migrate" and mint:
            ev = NormalizedEvent(EventType.HINT_MIGRATION, mint, 0, extra={"pool": message.get("pool")})
            return NormalizedEnvelope(signature, None, None, (ev,), Completeness.HINT, raw.feed_id, decode_done, "PUMPPORTAL_HINT_V1", None)
        if "message" in message or "errors" in message:
            return NormalizedEnvelope(None, None, None, (), Completeness.HINT, raw.feed_id, decode_done, "PUMPPORTAL_HINT_V1", "PUMPPORTAL_SERVICE_MESSAGE")
        return None


# --------------------------------------------------------------------------- reducer


@dataclass(slots=True)
class EventApply:
    event: NormalizedEvent
    canonical_event_id: str | None
    validation: ValidationStatus
    is_new: bool
    applied_state: bool
    creator_sell: bool
    upgraded: bool = False  # PARTIAL copy upgraded to COMPLETE by this observation
    profile_upgraded: bool = False


def _fingerprint(ev: NormalizedEvent) -> tuple:
    return (ev.actor, ev.sol_amount, ev.token_amount, ev.virtual_sol_reserves, ev.virtual_token_reserves,
            ev.real_token_reserves, ev.real_sol_reserves, ev.creator, ev.token_program, ev.token_total_supply, ev.quote_scope_status, ev.quote_mint_raw, ev.quote_layout_hash,
            ev.extra.get("tail_prefix_hash"), ev.extra.get("tail_verified"), ev.extra.get("tail_values"))


@dataclass(slots=True)
class ApplyResult:
    envelope: NormalizedEnvelope
    raw: RawEnvelope
    conflicts: list[tuple[str, str, str]] = field(default_factory=list)
    decode_gaps: list[str] = field(default_factory=list)
    events: list[EventApply] = field(default_factory=list)
    new_creates: list[str] = field(default_factory=list)  # mints whose CREATE is new in this envelope
    creator_sells: list[str] = field(default_factory=list)
    venue_changes: list[tuple[str, VenueState, VenueState]] = field(default_factory=list)
    touched: list[str] = field(default_factory=list)


class MarketReducer:
    """Single-run RAM market state with canonical dedup, ordering, venue guard and coverage V2."""

    def __init__(self, run_id: str, history_per_mint: int, max_tracked_mints: int, trade_log_per_mint: int | None = None, event_points_max: int = 4096) -> None:
        self.run_id = run_id
        self.history_per_mint = history_per_mint
        self.trade_log_per_mint = trade_log_per_mint or history_per_mint * 4
        self.event_points_max = event_points_max
        self.signature_history_max = event_points_max
        self.dedup_keys_per_mint = event_points_max
        self.protected_mints: set[str] = set()
        self.max_tracked_mints = max_tracked_mints
        self.states: dict[str, MarketState] = {}
        self.gap_open_since: int | None = None
        self.gap_intervals: list[tuple[int, int | None]] = []
        self.hint_unmatched = 0
        self.wsol = ""
        self.native_sentinel = ""
        self.stale_events = 0
        self.duplicates = 0
        self.conflicts = 0
        self.applied_signatures: dict[str, set[str]] = {}  # signature -> all touched mints (bounded) for reorg detection
        self.reorg_conflicts = 0
        self.undecodable_points: list[int] = []  # receive times of frames that could not be attributed to a mint: any fill window containing one is UNKNOWN
        self.undecodable_lost_interval: tuple[int, int] | None = None
        self.decode_gaps_opened = 0
        self.first_hint_mono: dict[str, int] = {}

    def set_quote_constants(self, wsol: str, native_sentinel: str) -> None:
        self.wsol, self.native_sentinel = wsol, native_sentinel

    def canonical_id(self, signature: str, instruction_index: int, mint: str, event_type: EventType) -> str:
        return f"{self.run_id}|{signature}|{instruction_index}|{mint}|{event_type.value}"

    def state_for(self, mint: str) -> MarketState:
        st = self.states.get(mint)
        if st is None:
            st = MarketState(mint=mint)
            self.states[mint] = st
            if self.gap_open_since is not None:
                st.coverage_state = CoverageState.GAP
        return st

    # coverage -------------------------------------------------------------
    def open_gap(self, mono_ns: int) -> None:
        if self.gap_open_since is None:
            self.gap_open_since = mono_ns
            self.gap_intervals.append((mono_ns, None))
            for st in self.states.values():
                st.coverage_state = CoverageState.GAP

    def close_gap(self, mono_ns: int) -> None:
        if self.gap_open_since is not None:
            start = self.gap_open_since
            self.gap_open_since = None
            self.gap_intervals[-1] = (start, mono_ns)
            for st in self.states.values():
                st.coverage_state = CoverageState.RECOVERED_STATE_ONLY

    def gap_intersects(self, start_mono_ns: int, end_mono_ns: int | None) -> bool:
        for gs, ge in self.gap_intervals:
            if (ge is None or ge >= start_mono_ns) and (end_mono_ns is None or gs <= end_mono_ns):
                return True
        return False

    # apply ------------------------------------------------------------------
    def apply(self, envelope: NormalizedEnvelope, raw: RawEnvelope) -> ApplyResult:
        result = ApplyResult(envelope, raw)
        if envelope.completeness == Completeness.HINT:
            for ev in envelope.events:
                self._apply_hint(ev, raw, result)
            return result
        if envelope.completeness == Completeness.UNDECODABLE:
            self._remember_undecodable(raw.receive_mono_ns)
            return result
        if envelope.completeness == Completeness.FAILED_TX and envelope.signature in self.applied_signatures:
            for mint in set(self.applied_signatures[envelope.signature]):
                st = self.states.get(mint)
                if st is not None:  # processed-commitment reorg: invalidate every mint in the tx
                    st.quote_uncertain = True
                    st.quote_conflict_mono_ns = min(st.quote_conflict_mono_ns or raw.receive_mono_ns, raw.receive_mono_ns)
                    self.reorg_conflicts += 1
                    result.touched.append(mint)
                    result.conflicts.append((mint, envelope.signature, "FAILED_AFTER_APPLIED"))
            return result
        if not envelope.signature or not envelope.events:
            if envelope.completeness == Completeness.PARTIAL:  # unknown Pump event with no decodable sibling: unattributable
                self._remember_undecodable(raw.receive_mono_ns)
            return result
        sig = envelope.signature
        partial = envelope.completeness == Completeness.PARTIAL
        for ev in envelope.events:
            if ev.mint is None:
                result.events.append(EventApply(ev, None, ValidationStatus.UNMATCHED, False, False, False))
                continue
            cid = self.canonical_id(sig, ev.instruction_index, ev.mint, ev.event_type)
            st = self.state_for(ev.mint)
            fingerprint = _fingerprint(ev)
            if cid in st.seen_keys:
                seen_fp, seen_complete = st.seen_keys[cid]
                if seen_fp != fingerprint:
                    self.conflicts += 1
                    st.quote_uncertain = True
                    st.quote_conflict_mono_ns = min(st.quote_conflict_mono_ns or raw.receive_mono_ns, raw.receive_mono_ns)
                    result.conflicts.append((ev.mint, cid, "CONTENT_MISMATCH"))
                    if ev.mint not in result.touched:
                        result.touched.append(ev.mint)
                    result.events.append(EventApply(ev, cid, ValidationStatus.CONFLICT, False, False, False))
                    continue
                profile_upgrade = (seen_complete and ev.event_type == EventType.CREATE and st.account_profile_id is None
                                   and ev.extra.get("account_profile_id") is not None)
                if (seen_complete and not profile_upgrade) or envelope.completeness != Completeness.COMPLETE:
                    self.duplicates += 1
                    result.events.append(EventApply(ev, cid, ValidationStatus.DUPLICATE, False, False, False))
                    continue
                # first copy was PARTIAL, this copy is COMPLETE: upgrade (a partial copy must never poison actionability)
                st.seen_keys[cid] = (fingerprint, True)
                if ev.event_type == EventType.CREATE and st.create_event_id == cid:
                    st.quote_scope_status = ev.quote_scope_status or QuoteScopeStatus.QUOTE_UNKNOWN
                    st.create_completeness = Completeness.COMPLETE
                    result.new_creates.append(ev.mint)
                    if ev.mint not in result.touched:
                        result.touched.append(ev.mint)
                applied_up = False
                stale_up = st.last_state_slot is not None and envelope.slot is not None and envelope.slot < st.last_state_slot
                if not profile_upgrade and ev.virtual_sol_reserves and ev.virtual_token_reserves and st.quote_scope_status == QuoteScopeStatus.SOL_WSOL and not stale_up:
                    # every state-bearing event of the now-complete tx is applied in instruction order: the LAST one is the atomic post-tx state
                    if st.decode_gap_since is not None:
                        self._close_decode_gap(st, raw.receive_mono_ns)
                    self._set_state(st, ev, cid, raw, envelope.slot)
                    applied_up = True
                    if ev.mint not in result.touched:
                        result.touched.append(ev.mint)
                if ev.event_type == EventType.CREATE:
                    st.account_profile_id = ev.extra.get("account_profile_id")
                result.events.append(EventApply(ev, cid, ValidationStatus.VALID, True, applied_up, False, upgraded=True, profile_upgraded=profile_upgrade))
                continue
            st.seen_keys[cid] = (fingerprint, envelope.completeness == Completeness.COMPLETE)
            self.applied_signatures.setdefault(sig, set()).add(ev.mint)
            if len(st.seen_keys) > self.dedup_keys_per_mint:
                st.seen_keys.pop(next(iter(st.seen_keys)))
                st.quote_uncertain = True
                st.quote_conflict_mono_ns = min(st.quote_conflict_mono_ns or raw.receive_mono_ns, raw.receive_mono_ns)
                result.conflicts.append((ev.mint, cid, "DEDUP_HISTORY_TRUNCATED"))
            if len(self.applied_signatures) > self.signature_history_max:
                removed_sig = next((k for k, mints in self.applied_signatures.items() if not (mints & self.protected_mints)), next(iter(self.applied_signatures)))
                for affected in self.applied_signatures.pop(removed_sig):
                    affected_state = self.states.get(affected)
                    if affected_state is not None and affected in self.protected_mints:
                        affected_state.quote_uncertain = True
                        affected_state.quote_conflict_mono_ns = min(affected_state.quote_conflict_mono_ns or raw.receive_mono_ns, raw.receive_mono_ns)
                        if affected not in result.touched:
                            result.touched.append(affected)
                        result.conflicts.append((affected, removed_sig, "REORG_HISTORY_TRUNCATED"))
            if partial and st.decode_gap_since is None:  # an unknown Pump event in a tx touching this mint: the curve may have changed unknowably
                st.decode_gap_since = raw.receive_mono_ns
                st.decode_gap_intervals.append((raw.receive_mono_ns, None))
                self.decode_gaps_opened += 1
                result.decode_gaps.append(ev.mint)
            st.event_count += 1
            if ev.mint not in result.touched:
                result.touched.append(ev.mint)
            tail_values = ev.extra.get("tail_values") or {}
            unsupported = next((name for name, active in (("MAYHEM", tail_values.get("is_mayhem_mode") or tail_values.get("mayhem_mode")),
                                ("CASHBACK", tail_values.get("is_cashback_enabled") or tail_values.get("cashback_fee_basis_points")),
                                ("HOLDER_REWARD", tail_values.get("is_holder_reward") or tail_values.get("holder_rewards_bps")),
                                ("FEE_SHARING", tail_values.get("shareholders"))) if active), None)
            if unsupported:
                st.unsupported_fee_mode = unsupported
                st.quote_uncertain = True
                st.quote_conflict_mono_ns = min(st.quote_conflict_mono_ns or raw.receive_mono_ns, raw.receive_mono_ns)
                result.conflicts.append((ev.mint, cid, "UNSUPPORTED_FEE_MODE_" + unsupported))
            if ev.event_type == EventType.CREATE:
                st.creator_fee_bps = int(tail_values.get("creator_fee_bps", 0))
            validation = ValidationStatus.VALID
            applied = False
            creator_sell = False
            slot = envelope.slot
            stale = st.last_state_slot is not None and slot is not None and slot < st.last_state_slot
            if ev.event_type == EventType.CREATE:
                if st.create_event_id is None:
                    st.create_event_id = cid
                    st.create_receive_mono_ns = raw.receive_mono_ns
                    st.create_slot = slot
                    st.creator = ev.creator
                    st.token_program = ev.token_program
                    st.account_profile_id = ev.extra.get("account_profile_id")
                    st.quote_scope_status = ev.quote_scope_status or QuoteScopeStatus.QUOTE_UNKNOWN
                    st.create_completeness = envelope.completeness
                    st.token_total_supply = ev.token_total_supply
                    st.initial_virtual_token_reserves = ev.virtual_token_reserves
                    st.initial_virtual_sol_reserves = ev.virtual_sol_reserves
                    st.initial_real_token_reserves = ev.real_token_reserves
                    if st.venue_state == VenueState.VENUE_UNKNOWN:
                        st.venue_state = VenueState.BONDING_CURVE_ACTIVE
                    if st.coverage_state == CoverageState.UNKNOWN_WARMUP and self.gap_open_since is None:
                        st.coverage_state = CoverageState.OBSERVED
                    if envelope.completeness == Completeness.COMPLETE:
                        result.new_creates.append(ev.mint)
                    if ev.virtual_sol_reserves and ev.virtual_token_reserves and st.quote_scope_status == QuoteScopeStatus.SOL_WSOL:
                        self._set_state(st, ev, cid, raw, slot)  # non-SOL/unknown quote reserves never enter SOL formulas or SOL columns
                        applied = True
                else:
                    validation = ValidationStatus.CONFLICT
            elif ev.event_type in (EventType.BUY, EventType.SELL):
                unverified_tail = bool(ev.extra.get("tail_bytes")) and not bool(ev.extra.get("tail_verified"))
                foreign_quote = ev.quote_mint_raw is not None and ev.quote_mint_raw not in (self.wsol, self.native_sentinel)
                if unverified_tail:
                    st.quote_uncertain = True
                    st.quote_conflict_mono_ns = min(st.quote_conflict_mono_ns or raw.receive_mono_ns, raw.receive_mono_ns)
                    validation = ValidationStatus.CONFLICT
                    result.conflicts.append((ev.mint, cid, "UNVERIFIED_TRADE_TAIL"))
                elif foreign_quote:  # explicit foreign quote in a trade tail: lock the mint, never write these amounts into SOL state
                    st.quote_uncertain = True
                    st.quote_conflict_mono_ns = min(st.quote_conflict_mono_ns or raw.receive_mono_ns, raw.receive_mono_ns)
                    st.quote_scope_status = QuoteScopeStatus.UNSUPPORTED_QUOTE
                    result.conflicts.append((ev.mint, cid, "FOREIGN_QUOTE_IN_TRADE"))
                if stale:
                    validation = ValidationStatus.STALE
                    self.stale_events += 1
                elif foreign_quote:
                    validation = ValidationStatus.CONFLICT
                elif partial:
                    pass  # state from a partial tx is not trusted; gap stays open until a COMPLETE state
                elif st.quote_scope_status == QuoteScopeStatus.SOL_WSOL or st.create_event_id is None:
                    if st.decode_gap_since is not None:
                        self._close_decode_gap(st, raw.receive_mono_ns)
                    self._set_state(st, ev, cid, raw, slot)
                    applied = True
                    if ev.real_token_reserves == 0 and st.venue_state == VenueState.BONDING_CURVE_ACTIVE:
                        self._venue(st, VenueState.CURVE_COMPLETE, result, cid, raw)
                if ev.event_type == EventType.SELL and not unverified_tail and st.creator is not None and ev.actor == st.creator:
                    creator_sell = True
                    if st.creator_sell_event_id is None:
                        st.creator_sell_event_id = cid
                        st.creator_sell_receive_mono_ns = raw.receive_mono_ns
                        st.creator_sell_slot = slot
                        st.creator_sell_sequence = raw.arrival_sequence
                        result.creator_sells.append(ev.mint)
                in_create_tx = any(e.event_type == EventType.CREATE for e in envelope.events)
                if ev.event_type == EventType.BUY and st.create_event_id is not None and st.initial_buy_tokens is None and in_create_tx:
                    st.initial_buy_tokens = ev.token_amount
                if len(st.trade_log) < self.trade_log_per_mint:
                    st.trade_log.append((ev.event_type.value, raw.receive_mono_ns, ev.actor or "", ev.sol_amount or 0, ev.virtual_sol_reserves, ev.virtual_token_reserves, ev.real_token_reserves, in_create_tx, cid))
                else:
                    st.trade_log_truncated = True
                    if st.trade_log_truncated_from_mono_ns is None:
                        st.trade_log_truncated_from_mono_ns = raw.receive_mono_ns
            elif ev.event_type == EventType.COMPLETE:
                self._venue(st, VenueState.CURVE_COMPLETE, result, cid, raw)
            elif ev.event_type == EventType.MIGRATE:
                self._venue(st, VenueState.MIGRATED_UNSUPPORTED, result, cid, raw)
            result.events.append(EventApply(ev, cid, validation, True, applied, creator_sell))
        return result

    def _remember_undecodable(self, mono_ns: int) -> None:
        self.undecodable_points.append(mono_ns)
        if len(self.undecodable_points) > self.event_points_max:
            removed = self.undecodable_points.pop(0)
            start = self.undecodable_lost_interval[0] if self.undecodable_lost_interval else removed
            self.undecodable_lost_interval = (start, removed)

    def _close_decode_gap(self, st: MarketState, mono_ns: int) -> None:
        start = st.decode_gap_since
        st.decode_gap_since = None
        if st.decode_gap_intervals and st.decode_gap_intervals[-1][1] is None:
            st.decode_gap_intervals[-1] = (start, mono_ns)
        if st.coverage_state == CoverageState.OBSERVED:
            st.coverage_state = CoverageState.RECOVERED_STATE_ONLY

    def unknowable_between(self, st: MarketState, start_mono_ns: int, end_mono_ns: int) -> str | None:
        """Reason if the curve could have changed unobservably within (start, end]: mint decode gap, unattributable frame, or canonical gap."""
        for gs, ge in st.decode_gap_intervals:
            if gs <= end_mono_ns and (ge is None or ge > start_mono_ns):
                return "DECODE_GAP"
        lost = self.undecodable_lost_interval
        if lost and start_mono_ns < lost[1] and end_mono_ns >= lost[0]:
            return "UNDECODABLE_HISTORY_TRUNCATED"
        for t in self.undecodable_points:
            if start_mono_ns < t <= end_mono_ns:
                return "UNDECODABLE_FRAME_IN_WINDOW"
        if self.gap_intersects(start_mono_ns, end_mono_ns):
            return "CANONICAL_GAP"
        return None

    def _venue(self, st: MarketState, new: VenueState, result: ApplyResult, cid: str, raw: RawEnvelope) -> None:
        old = st.venue_state
        if old in (VenueState.MIGRATED_UNSUPPORTED,) and new != VenueState.MIGRATED_UNSUPPORTED:
            return
        if old != new:
            st.venue_state = new
            st.venue_change_mono_ns = raw.receive_mono_ns
            st.migration_evidence = {"source_event_id": cid, "receive_mono_ns": raw.receive_mono_ns, "feed_id": raw.feed_id, "evidence_status": "CHAIN_EVENT" if new != VenueState.MIGRATION_SUSPECTED else "PROVIDER_HINT_UNVERIFIED", "previous": old.value, "new": new.value}
            result.venue_changes.append((st.mint, old, new))

    def _set_state(self, st: MarketState, ev: NormalizedEvent, cid: str, raw: RawEnvelope, slot: int | None) -> None:
        st.virtual_sol_reserves = ev.virtual_sol_reserves
        st.virtual_token_reserves = ev.virtual_token_reserves
        st.real_sol_reserves = ev.real_sol_reserves
        st.real_token_reserves = ev.real_token_reserves
        st.last_state_event_id = cid
        st.last_state_slot = slot
        st.last_state_sequence = raw.arrival_sequence
        st.last_state_receive_mono_ns = raw.receive_mono_ns
        if st.coverage_state == CoverageState.RECOVERED_STATE_ONLY or (st.coverage_state == CoverageState.GAP and self.gap_open_since is None):
            st.coverage_state = CoverageState.RECOVERED_STATE_ONLY
        hist = st.state_history
        hist.append((raw.receive_mono_ns, raw.arrival_sequence, cid, slot, ev.virtual_sol_reserves, ev.virtual_token_reserves, ev.real_sol_reserves, ev.real_token_reserves))
        if len(hist) > self.history_per_mint:
            del hist[: len(hist) - self.history_per_mint]

    def _apply_hint(self, ev: NormalizedEvent, raw: RawEnvelope, result: ApplyResult) -> None:
        if ev.mint is None:
            return
        if ev.event_type == EventType.HINT_CREATE:
            self.first_hint_mono.setdefault(ev.mint, raw.receive_mono_ns)
            st = self.states.get(ev.mint)
            validation = ValidationStatus.UNMATCHED if st is None or st.create_event_id is None else ValidationStatus.VALID
            if validation == ValidationStatus.UNMATCHED:
                self.hint_unmatched += 1
            result.events.append(EventApply(ev, None, validation, True, False, False))
        elif ev.event_type == EventType.HINT_MIGRATION:
            st = self.states.get(ev.mint)
            if st is not None and st.venue_state == VenueState.BONDING_CURVE_ACTIVE:
                self._venue(st, VenueState.MIGRATION_SUSPECTED, result, f"{self.run_id}|hint|{raw.arrival_sequence}|{ev.mint}|HINT_MIGRATION", raw)
                if ev.mint not in result.touched:
                    result.touched.append(ev.mint)
            result.events.append(EventApply(ev, None, ValidationStatus.UNMATCHED if st is None else ValidationStatus.VALID, True, False, False))

    def proxy_state_at(self, st: MarketState, mono_ns: int) -> tuple[int, int, str, int | None, int, int, int | None, int | None] | None:
        """Last observed state with receive_mono_ns <= mono_ns (OBSERVED_STATE_PROXY selection)."""
        chosen = None
        for entry in st.state_history:
            if entry[0] <= mono_ns:
                chosen = entry
            else:
                break
        return chosen

    def next_event_after(self, st: MarketState, mono_ns: int) -> str | None:
        for entry in st.state_history:
            if entry[0] > mono_ns:
                return entry[2]
        return None

    def evict_inactive(self, now_mono_ns: int, retention_ns: int, protected: set[str]) -> int:
        removed = 0
        if len(self.states) <= self.max_tracked_mints:
            candidates = [m for m, s in self.states.items() if m not in protected and s.last_state_receive_mono_ns is not None and now_mono_ns - s.last_state_receive_mono_ns > retention_ns]
        else:
            ordered = sorted(((s.last_state_receive_mono_ns or 0, m) for m, s in self.states.items() if m not in protected))
            candidates = [m for _, m in ordered[: len(self.states) - self.max_tracked_mints]]
        for mint in candidates:
            for sig, mints in list(self.applied_signatures.items()):
                mints.discard(mint)
                if not mints:
                    self.applied_signatures.pop(sig, None)
            del self.states[mint]
            removed += 1
        return removed
