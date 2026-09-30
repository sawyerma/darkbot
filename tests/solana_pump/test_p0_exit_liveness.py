"""P0: creator liquidation survives missing costs/state without invented fills."""
from __future__ import annotations

import base64
import json
import unittest
from dataclasses import replace

from darkbot.solana_pump.domain import ExitReason, PositionState
from tests.helpers import Harness, MINT_A, T0, attempts, base_config, creator_sell_frame, open_position


def partial_copy(payload: str) -> str:
    message = json.loads(payload)
    message['params']['result']['value']['logs'].insert(-1, 'Program data: ' + base64.b64encode(b'UNKNOWN!').decode())
    return json.dumps(message)


class ExitLiveness(unittest.TestCase):
    def test_A_immediate_creator_sell(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'cs', 101), T0 + 2_000_000)
        self.assertEqual(pos.state, PositionState.EXIT_PENDING)
        self.assertEqual(pos.origin_trigger, ExitReason.CREATOR_SELL)
        self.assertEqual(attempts(h)[-1]['submit_mono_ns'], T0 + 2_000_000)

    def test_B_C_F_fee_recovery_without_trade_and_no_duplicate(self):
        h = Harness(base_config(latency_model={'exit_delay_ms': 10}))
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        good = h.fees.network
        h.fees.network = replace(good, validation_status='UNAVAILABLE')
        h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'cs', 101), T0 + 2_000_000)
        self.assertEqual(pos.origin_trigger, ExitReason.CREATOR_SELL)
        self.assertEqual(pos.exit_attempts, 0)
        h.clock.advance_to(T0 + 3_000_000)
        h.fees.network = good
        h.owner.on_fee_snapshots([good])
        self.assertEqual(pos.state, PositionState.EXIT_PENDING)
        for _ in range(5):
            h.owner.on_fee_snapshots([good])
        h.advance(T0 + 20_000_000)
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertEqual(len([a for a in attempts(h) if a['side'] == 'SELL']), 1)
        self.assertEqual(h.fk_check(), [])

    def test_complete_copy_recovers_gap_without_new_trade(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        complete = creator_sell_frame(h, MINT_A, 'cs', 101)
        h.feed('helius_standard', partial_copy(complete), T0 + 2_000_000)
        self.assertIsNotNone(h.owner.reducer.states[MINT_A].decode_gap_since)
        self.assertEqual(pos.exit_attempts, 0)
        h.feed('existing_rpc', complete, T0 + 3_000_000)
        self.assertIsNone(h.owner.reducer.states[MINT_A].decode_gap_since)
        self.assertEqual(pos.state, PositionState.EXIT_PENDING)
        h.advance(T0 + 4_000_000)
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertEqual(h.fk_check(), [])

    def test_D_true_gap_deadline_is_honest_and_later_state_recovers(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        complete = creator_sell_frame(h, MINT_A, 'cs', 101)
        held = (pos.token_balance_base_units, pos.locked_rent_lamports, pos.reserved_cash_lamports)
        h.feed('helius_standard', partial_copy(complete), T0 + 2_000_000)
        since = pos.exit_quote_blocked_since_mono_ns
        deadline = since + h.cfg.connectivity['position_state_stale_ms'] * 1_000_000
        h.advance(deadline)
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(pos.unresolved_reason, 'EXIT_QUOTE_UNAVAILABLE')
        self.assertEqual(pos.quality_status, 'EXIT_LATCHED_DECODE_GAP')
        self.assertTrue(pos.exit_quote_recoverable)
        self.assertEqual(pos.origin_trigger, ExitReason.CREATOR_SELL)
        self.assertEqual(pos.exit_attempts, 0)
        self.assertIsNone(pos.net_pnl_lamports)
        self.assertEqual(held, (pos.token_balance_base_units, pos.locked_rent_lamports, pos.reserved_cash_lamports))
        h.owner.on_fee_snapshots([h.fees.network])
        h.owner._on_exit_quote_deadline(pos.position_id, since, deadline)
        self.assertEqual(pos.exit_attempts, 0)
        h.feed('existing_rpc', complete, deadline + 1_000_000)
        h.advance(deadline + 2_000_000)
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertFalse(pos.exit_quote_recoverable)
        self.assertIsNone(pos.unresolved_reason)
        self.assertEqual(h.fk_check(), [])

    def test_fee_recovery_never_uses_stale_curve(self):
        h = Harness()
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        good = h.fees.network
        h.fees.network = replace(good, validation_status='UNAVAILABLE')
        h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'cs', 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000_000)
        h.fees.network = good
        h.owner.on_fee_snapshots([good])
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(pos.exit_attempts, 0)
        self.assertEqual(pos.quality_status, 'EXIT_LATCHED_PROXY_STATE_STALE')

    def test_E_creator_priority_same_tick_and_pending_audit(self):
        from unittest.mock import patch
        h = Harness()
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        with patch.object(h.owner.engine, 'net_liquidation_value', return_value=pos.entry_economic_cost_lamports * 2):
            h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'cs', 101), T0 + 2_000_000)
        self.assertEqual(pos.origin_trigger, ExitReason.CREATOR_SELL)
        self.assertEqual({reason for reason, _ in pos.latched_triggers}, {ExitReason.CREATOR_SELL, ExitReason.TARGET})

        h2 = Harness(base_config(latency_model={'exit_delay_ms': 10}))
        self.addCleanup(h2.conn.close)
        pos2 = open_position(h2)
        pos2.latched_triggers.append((ExitReason.TARGET, T0 + 1_000_000))
        h2.owner._maybe_exit(pos2, h2.owner.reducer.states[MINT_A], h2.clock.monotonic_ns())
        h2.feed('helius_standard', creator_sell_frame(h2, MINT_A, 'cs', 101), T0 + 2_000_000)
        self.assertEqual(pos2.origin_trigger, ExitReason.TARGET)
        self.assertIn(ExitReason.CREATOR_SELL, [reason for reason, _ in pos2.latched_triggers])
        self.assertEqual(pos2.exit_attempts, 1)

    def test_replay_fee_receipt_without_market_or_timer_at_receipt(self):
        from darkbot.solana_pump.replay import load_replay_input, run_replay
        h = Harness()
        self.addCleanup(h.conn.close)
        open_position(h)
        h.clock.advance_to(T0 + 2_000_000)
        good = h.fees.network
        bad = replace(good, fee_snapshot_id='bad', received_mono_ns=T0 + 2_000_000, validation_status='UNAVAILABLE')
        h.fees.network = bad
        h.recorder.try_append(('fee_snapshot', (bad, h.run_id)))
        h.owner.on_fee_snapshots([bad])
        h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'cs', 101), T0 + 3_000_000)
        h.clock.advance_to(T0 + 7_000_000)
        valid = replace(good, fee_snapshot_id='recovered', received_mono_ns=T0 + 7_000_000)
        h.fees.network = valid
        h.recorder.try_append(('fee_snapshot', (valid, h.run_id)))
        h.owner.on_fee_snapshots([valid])
        h.finish(T0 + 20_000_000)
        replay = run_replay(h.conn, load_replay_input(h.conn, h.run_id, h.cfg), h.cfg, 'replay', 'camp', persist=False)
        self.assertEqual(replay.digest['positions'][0]['state'], 'CLOSED')
        self.assertEqual(replay.digest['attempts'][-1]['submit'], T0 + 7_000_000)
        self.assertEqual(replay.digest['positions'][0]['net_pnl'], h.position().net_pnl_lamports)

    def test_G_recorded_mainnet_frames_and_fees_replay(self):
        import hashlib
        from darkbot.solana_pump.config import build_runtime_config
        from darkbot.solana_pump.domain import Completeness, CostEvidence, FeeSnapshot, RawEnvelope
        from darkbot.solana_pump.market import Normalizer, PumpDecoder
        from darkbot.solana_pump.replay import ReplayInput, run_replay
        from tests.helpers import ROOT
        fixture = json.loads((ROOT / 'tests/fixtures/edge_probe_01_p2.json').read_text())
        cfg = build_runtime_config(ROOT / 'config/phase1.json', json.loads(fixture['config_snapshot']['canonical_json']))
        normalizer = Normalizer(PumpDecoder(cfg.protocol))
        observations = []
        for row in fixture['observations']:
            self.assertEqual(hashlib.sha256(row['raw_payload'].encode()).hexdigest(), row['raw_payload_hash'])
            t = row['receive_mono_ns']
            raw = RawEnvelope(row['feed_id'], row['connection_id'], t, row['receive_utc'], row['arrival_sequence'], row['raw_payload'])
            normalized = normalizer.normalize(raw, 'logsSubscribe', t)
            self.assertEqual(normalized.completeness, Completeness.COMPLETE)
            observations.append((t, row['arrival_sequence'], row['feed_id'], row['connection_id'], row['receive_utc'], row['raw_payload']))
        creator_sell = observations[-1]
        self.assertEqual(creator_sell[0], 1657260603098791)
        self.assertEqual(fixture['decisions'][-1]['reason'], 'EXIT_LATCHED_DECODE_GAP')
        refresh = cfg.costs['snapshot_refresh_policy']
        ages = {'PROTOCOL_GLOBAL_ACCOUNT': refresh['protocol_max_age_ms'], 'NETWORK_FEE': refresh['network_fee_max_age_ms'], 'PRIORITY_POLICY': refresh['network_fee_max_age_ms'], 'RENT_EXEMPTION': refresh['rent_max_age_ms']}
        fees = []
        for row in fixture['fee_snapshots']:
            values = {k: row[k] for k in FeeSnapshot.__slots__ if k in row}
            for key in ('source_account_addresses', 'raw_values', 'normalized_fee_components'):
                values[key] = json.loads(values[key])
            values['source_account_addresses'] = tuple(values['source_account_addresses'])
            values['evidence'] = CostEvidence(values['evidence'])
            values['max_age_ns'] = ages[row['source_type']] * 1_000_000
            fees.append(FeeSnapshot(**values))
        data = ReplayInput(fixture['run']['run_id'], 'fixture', observations[0][0], 0, observations, fees, [], {}, fixture['source_backup_sha256'], creator_sell[0] + 1_000_000)
        h = Harness(cfg)
        self.addCleanup(h.conn.close)
        replay = run_replay(h.conn, data, cfg, 'real_p2', 'fixture', persist=False)
        pos = replay.digest['positions'][0]
        self.assertEqual(pos['state'], 'CLOSED')
        self.assertEqual(pos['origin_trigger'], 'CREATOR_SELL')
        self.assertEqual(replay.digest['attempts'][-1]['submit'], creator_sell[0])
        self.assertEqual(replay.owner.reducer.decode_gaps_opened, 0)
        self.assertEqual(replay.owner.counters.internal_errors, 0)
        replay2 = run_replay(h.conn, data, cfg, 'real_p2_repeat', 'fixture', persist=False)
        self.assertEqual(replay.digest, replay2.digest)

    def test_fee_recovery_after_diagnostic_deadline_requires_fresh_state(self):
        # This fixture explicitly allows 5s state freshness so recovery at 2s
        # exercises fees alone. Production's 250ms limit is tested separately.
        h = Harness(base_config(execution={'quote_max_age_ms': 5000}))
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        good = h.fees.network
        h.fees.network = replace(good, validation_status='UNAVAILABLE')
        h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'cs', 101), T0 + 2_000_000)
        h.advance(T0 + 2_003_000_000)
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertTrue(pos.exit_quote_recoverable)
        h.fees.network = good
        h.owner.on_fee_snapshots([good])
        self.assertEqual(pos.state, PositionState.EXIT_PENDING)
        h.advance(T0 + 2_004_000_000)
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertEqual(pos.exit_attempts, 1)

    def test_unknown_submission_never_recovers_from_fees(self):
        h = Harness(base_config(latency_model={'exit_delay_ms': 10}))
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'cs', 101), T0 + 2_000_000)
        h.feed('existing_rpc', partial_copy(creator_sell_frame(h, MINT_A, 'partial', 102)), T0 + 3_000_000)
        h.advance(T0 + 15_000_000)
        self.assertEqual(pos.unresolved_reason, 'ATTEMPT_STATUS_UNKNOWN')
        self.assertFalse(pos.exit_quote_recoverable)
        for _ in range(3):
            h.owner.on_fee_snapshots([h.fees.network])
        h.feed('existing_rpc', creator_sell_frame(h, MINT_A, 'recovered', 103), T0 + 16_000_000)
        self.assertEqual(pos.exit_attempts, 1)
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(pos.unresolved_reason, 'ATTEMPT_STATUS_UNKNOWN')

    def test_unknown_sibling_never_uses_pre_gap_curve_for_target(self):
        from unittest.mock import patch
        h = Harness()
        self.addCleanup(h.conn.close)
        pos = open_position(h)
        with patch.object(h.owner.engine, 'net_liquidation_value', side_effect=AssertionError('untrusted target curve')):
            h.feed('helius_standard', partial_copy(creator_sell_frame(h, MINT_A, 'cs', 101)), T0 + 2_000_000)
        self.assertEqual(h.owner.counters.internal_errors, 0)
        self.assertEqual(pos.latched_triggers, [(ExitReason.CREATOR_SELL, T0 + 2_000_000)])
        self.assertEqual(pos.exit_attempts, 0)
