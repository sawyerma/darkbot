"""Integrated counterexamples from the independent R5 audit, against the shared engine."""
import base64
import json
import unittest
from dataclasses import replace
from decimal import Decimal

from darkbot.solana_pump.backtest import Variant, apply_stress_gate, select_candidate, variant_config
from darkbot.solana_pump.domain import ExecutionOutcome, PositionState, RawEnvelope, utc_iso
from darkbot.solana_pump.market import event_discriminator
from tests.solana_pump.helpers import (BUYER, CREATOR, MINT_A, T0, Harness, attempts, base_config,
                           create_frame_logs, create_payload, logs_notification, open_position,
                           pump_frame, trade_frame_logs)


class IntegratedR5Regressions(unittest.TestCase):
    def harness(self, **overrides):
        h = Harness(base_config(**overrides))
        self.addCleanup(h.conn.close)
        return h

    def test_stale_complete_create_upgrade_preserves_newer_state(self):
        h = self.harness()
        logs = create_frame_logs(MINT_A, CREATOR)
        logs.insert(3, 'Program data: ' + base64.b64encode(event_discriminator('FutureEvent') + b'X' * 40).decode())
        h.feed('existing_rpc', logs_notification('s', 100, logs), T0 + 1_000_000)
        h.feed('helius_standard', pump_frame(h, MINT_A, 5_000_000_000, 'new', 101), T0 + 2_000_000)
        h.feed('helius_standard', create_payload(sig='s', slot=100), T0 + 3_000_000)
        st = h.owner.reducer.states[MINT_A]
        self.assertEqual((st.last_state_slot, st.virtual_sol_reserves), (101, 35_000_000_000))

    def test_future_truncation_does_not_change_zero_ms_features(self):
        h = self.harness(storage={'trade_log_per_mint': 2})
        open_position(h)
        for i in range(4):
            h.feed('helius_standard', pump_frame(h, MINT_A, 10, f'p{i}', 101+i), T0 + 300_000_000 + i*10_000_000)
        h.advance(T0 + 2_000_000_000)
        rows = h.rows("SELECT features_json, quality_status FROM feature_snapshots WHERE cutoff_mono_ns=?", (T0+1_000_000,))
        self.assertTrue(rows)
        for r in rows:
            self.assertEqual(json.loads(r['features_json'])['buy_count'], 0)
            self.assertEqual(r['quality_status'], 'OK')

    def test_lost_unknown_history_remains_unknown_for_delayed_callback(self):
        h = self.harness(storage={'fee_history_max': 4, 'undecodable_points_max': 4}, latency_model={'entry_delay_ms': 10})
        h.feed('helius_standard', create_payload(), T0+1_000_000)
        for i, ms in enumerate((5,20,21,22,23,24,25)):
            t = T0 + ms*1_000_000
            h.clock.advance_to(t)
            h.owner.on_raw(RawEnvelope('helius_standard', 'c', t, utc_iso(h.clock.utc_ns()), i+2, '{invalid'))
        h.clock.advance_to(T0+100_000_000)
        h.scheduler.fire_due(T0+100_000_000, T0+100_000_000)
        self.assertEqual(attempts(h)[0]['status'], 'UNKNOWN')
        self.assertIsNone(attempts(h)[0]['fees_charged_lamports'])

    def test_later_conflict_does_not_rewrite_earlier_modeled_fill(self):
        h = self.harness(latency_model={'entry_delay_ms': 10})
        h.feed('helius_standard', create_payload(), T0+1_000_000)
        handle = next(iter(h.adapter.pending.values()))
        h.clock.advance_to(T0+20_000_000)
        h.owner.on_raw(RawEnvelope('existing_rpc', 'c', T0+20_000_000, utc_iso(h.clock.utc_ns()), 1, create_payload(vsol=31_000_000_000)))
        h.owner.on_outcome(ExecutionOutcome(handle.attempt_id, handle.intent, handle.submit_mono_ns, handle.scheduled_fill_mono_ns, T0+30_000_000))
        self.assertEqual(attempts(h)[0]['status'], 'FILLED')
        self.assertEqual(h.position().state, PositionState.UNRESOLVED)

    def test_rejected_fill_records_current_network_fee(self):
        h = self.harness(latency_model={'entry_delay_ms': 50})
        h.feed('helius_standard', create_payload(), T0+1_000_000)
        h.fees.network = replace(h.fees.network, fee_snapshot_id='new_network', received_mono_ns=T0+5_000_000,
                                normalized_fee_components={'base_fee_lamports': 9000, 'priority_fee_lamports': 2000, 'network_fee_total_lamports':11000})
        h.recorder.try_append(('fee_snapshot', (h.fees.network, h.run_id)))
        h.owner.on_fee_snapshots([h.fees.network])
        h.feed('helius_standard', pump_frame(h, MINT_A, 20_000_000_000, 'big', 101), T0+10_000_000)
        h.advance(T0+60_000_000)
        row = attempts(h)[0]
        self.assertEqual(row['status'], 'SLIPPAGE_REJECTED')
        self.assertEqual(row['fees_charged_lamports'], 11000)
        self.assertEqual(row['network_fee_total_lamports'], 11000)
        self.assertEqual(row['network_fee_snapshot_id'], 'new_network')
        self.assertEqual(row['protocol_fee_lamports'], 0)
        self.assertEqual(h.position().net_pnl_lamports, -11000)

    def test_unknown_trade_tail_never_changes_curve_or_sells(self):
        h = self.harness()
        pos = open_position(h)
        st = h.owner.reducer.states[MINT_A]
        before = st.virtual_sol_reserves
        logs = trade_frame_logs(MINT_A, 1, 1, True, BUYER, 60_000_000_000, st.virtual_token_reserves, st.real_token_reserves, tail=b'new-layout')
        h.feed('helius_standard', logs_notification('unknown', 101, logs), T0+2_000_000)
        self.assertEqual(st.virtual_sol_reserves, before)
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual([r['side'] for r in attempts(h)], ['BUY'])

    def test_research_stress_has_distinct_model_provenance(self):
        cfg = base_config()
        a = variant_config(cfg, Variant('a','1.5',5,5,'1.0',0,1,False), 1, 123)
        b = variant_config(cfg, Variant('b','1.5',5,5,'2.0',100,1,False), 1, 123)
        c = variant_config(cfg, Variant('c','1.5',5,5,'2.0',100,1,False), 1, 124)
        for attr in ('config_hash','cost_model_hash','quote_model_hash'):
            self.assertNotEqual(getattr(a,attr),getattr(b,attr))
            self.assertNotEqual(getattr(b,attr),getattr(c,attr))

    def test_insufficient_validation_cannot_freeze(self):
        row = {'split':'validation','status':'INSUFFICIENT_DATA','target':'1.5','fee_multiplier':'1.0','adverse_bps':0,'bootstrap':{'ci_low':99}}
        self.assertFalse(select_candidate([row], '1.5', Decimal('.9')).get('positive',False))

    def test_stress_gate_is_order_independent(self):
        base = {'split':'forward','target':'1.5','entry_delay_ms':5,'exit_delay_ms':5,'portfolio_limit':1,'source_run_id':'r',
                'status':'POSITIVE_UNDER_MODEL_PENDING_STRESS','bootstrap':{'ci_low':100},'unresolved':0,'reject_rate':'0','adverse_bps':0}
        a = [dict(base,fee_multiplier='1.0'),dict(base,fee_multiplier='2.0')]
        b = list(reversed([dict(x) for x in a]))
        apply_stress_gate(a,'1.5',1)
        apply_stress_gate(b,'1.5',1)
        self.assertEqual([x['status'] for x in a],['POSITIVE_UNDER_MODEL']*2)
        self.assertEqual([x['status'] for x in b],['POSITIVE_UNDER_MODEL']*2)
