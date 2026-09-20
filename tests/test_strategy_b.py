"""B01–B18 and causal/evidence edge cases; same StateOwner for PAPER and replay."""
import json
import unittest
from dataclasses import replace
from decimal import Decimal
from pathlib import Path
from darkbot.config import ConfigError, build_runtime_config, load_runtime_config
from darkbot.domain import ExitReason, PositionState, RawEnvelope, NS_PER_MS
from darkbot.fx import FxSnapshot
from darkbot.replay import load_replay_input, run_replay, digest
from tests.helpers import *
from tests.helpers import _pubkey

START = T0 + NS_PER_MS
END = START + 1000 * NS_PER_MS


def a_creator_config(**overrides):
    cfg = load_runtime_config(ROOT / 'config/strategy_a_creator_exit_usd_slots.json')
    return cfg.with_overrides(**overrides) if overrides else cfg


def bconfig(**overrides):
    cfg = load_runtime_config(ROOT / 'config/strategy_b.json')
    return cfg.with_overrides(**overrides) if overrides else cfg


def make_b(**overrides):
    h = Harness(bconfig(**overrides))
    h.owner.on_fx_snapshot(FxSnapshot.from_payload('{"data":{"base":"SOL","currency":"USD","amount":"100"}}',
                                                   h.cfg['fx']['source_url'], h.clock.utc_iso(), T0))
    return h


def buys(h, actors, mint=MINT_A, start=START, prefix='b'):
    for n, actor in enumerate(actors):
        h.feed('helius_standard', pump_frame(h, mint, 10000, prefix + str(n), 101+n, actor), start+(100+int(800*n/max(1,len(actors)-1)))*NS_PER_MS)


def qualified(h, mint=MINT_A, start=START, prefix='b'):
    h.feed('helius_standard', create_payload(mint, sig='create'+prefix), start)
    buys(h, [_pubkey(str(n)) for n in range(5)], mint, start, prefix)
    h.advance(start+1000*NS_PER_MS)


class StrategyBTests(unittest.TestCase):
    def filter_case(self, actors, expected):
        h=make_b(); h.feed('helius_standard', create_payload(), START); buys(h, actors)
        self.assertEqual(h.owner.counters.entry_intents, 0)
        h.advance(END)
        self.assertEqual(h.owner.counters.entry_intents, expected)
        rows=h.rows('select * from strategy_observations')
        self.assertEqual((rows[0]['buy_count'],rows[0]['unique_buyers'],rows[0]['qualified']), (len(actors),len(set(actors)),expected))
        self.assertFalse(h.disk_errors); self.assertFalse(h.fk_check())
        return h

    def test_B01_four_four_skip(self):
        self.filter_case([_pubkey(str(n)) for n in range(4)],0)

    def test_B02_five_four_skip(self):
        self.filter_case([BUYER,BUYER2,CREATOR,_pubkey('x'),BUYER],0)

    def test_B03_five_five_entry(self):
        h=self.filter_case([_pubkey(str(n)) for n in range(5)],1)
        self.assertEqual(h.position().state,PositionState.OPEN)
        row=h.rows('select * from entry_sizing')[0]
        self.assertEqual(row['swap_budget_lamports'],100_000_000)
        self.assertLess(row['curve_input_lamports'],100_000_000)
        buy=h.rows("select * from execution_attempts where side='BUY'")[0]
        self.assertLessEqual(buy['amount_in_base_units'],100_000_000)
        self.assertEqual(row['max_swap_debit_lamports'],buy['amount_in_base_units']*10300//10000)
        self.assertGreater(h.position().entry_cash_debit_lamports,100_000_000)

    def test_B04_ten_one_skip(self):
        self.filter_case([BUYER]*10,0)

    def test_B05_all_create_transaction_buys_excluded(self):
        h=make_b(); h.feed('helius_standard',create_payload(initial_buy=(10000,10000), extra_trades=[{'sol':10000,'tok':10000,'is_buy':True,'user':BUYER,'vsol':V_SOL0+20000,'vtok':V_TOK0-20000,'rtok':R_TOK0-20000}]),START)
        buys(h,[_pubkey(str(n)) for n in range(4)]); h.advance(END)
        row=h.rows('select * from strategy_observations')[0]
        self.assertEqual((row['buy_count'],row['unique_buyers'],row['qualified']),(4,4,0))

    def test_B06_cross_feed_dedup(self):
        h=make_b();h.feed('helius_standard',create_payload(),START)
        for n in range(4):
            frame=pump_frame(h,MINT_A,10000,'b'+str(n),101+n,_pubkey(str(n)))
            h.feed('helius_standard',frame,START+(100+200*n)*NS_PER_MS)
            h.feed('existing_rpc',frame,START+(101+200*n)*NS_PER_MS)
        h.advance(END); self.assertEqual(h.rows('select buy_count from strategy_observations')[0][0],4)
        self.assertEqual(h.owner.counters.entry_intents,0)

    def test_B07_deadline_and_late_buys_do_not_change_decision(self):
        h=make_b();h.feed('helius_standard',create_payload(),START);buys(h,[_pubkey(str(n)) for n in range(4)])
        h.feed('helius_standard',pump_frame(h,MINT_A,10000,'boundary',110,_pubkey('fifth')),END)
        h.advance(END)
        self.assertEqual(h.owner.counters.entry_intents,1)
        row=h.rows('select buy_count,unique_buyers,qualified from strategy_observations')[0]
        self.assertEqual(tuple(row),(5,5,1))
        h.feed('helius_standard',pump_frame(h,MINT_A,10000,'late',111,_pubkey('sixth')),END+1)
        row=h.rows('select buy_count,unique_buyers,qualified from strategy_observations')[0]
        self.assertEqual(tuple(row),(5,5,1))

    def test_B07_replay_keeps_inclusive_deadline(self):
        h=make_b();h.feed('helius_standard',create_payload(),START);buys(h,[_pubkey(str(n)) for n in range(4)])
        h.feed('helius_standard',pump_frame(h,MINT_A,10000,'boundary_replay',110,_pubkey('fifth')),END)
        h.finish(END+NS_PER_MS)
        ri=load_replay_input(h.conn,'run',h.cfg)
        replay=run_replay(h.conn,ri,h.cfg,'replayed_boundary','camp',persist=True,persist_observations=True)
        self.assertEqual(digest(h.owner),replay.digest)
        rows=h.rows("select run_id,buy_count,unique_buyers,qualified from strategy_observations where mint=? order by run_id",(MINT_A,))
        self.assertEqual([(r['buy_count'],r['unique_buyers'],r['qualified']) for r in rows],[(5,5,1),(5,5,1)])

    def test_B08_paper_replay_decision_and_fx_parity(self):
        h=make_b();qualified(h);h.finish(END+NS_PER_MS)
        ri=load_replay_input(h.conn,'run',h.cfg)
        replay=run_replay(h.conn,ri,h.cfg,'replayed','camp',persist=True,persist_observations=True)
        self.assertEqual(digest(h.owner),replay.digest)
        self.assertEqual(h.rows('select count(*) from entry_sizing')[0][0],2)
        self.assertFalse(h.fk_check())

    def test_B09_below_net_target_holds(self):
        h=make_b();qualified(h)
        self.assertFalse(h.owner.strategy.target_reached(1049,1000,Decimal('1.05')))

    def test_B10_net_target_exits(self):
        h=make_b();qualified(h)
        self.assertTrue(h.owner.strategy.target_reached(1050,1000,Decimal('1.05')))
        h.owner.engine.net_liquidation_value=lambda *args: h.position().entry_economic_cost_lamports*2
        h.owner._position_tick(MINT_A,END)
        h.advance(END)
        self.assertEqual(h.position().exit_reason,ExitReason.TARGET)

    def test_B11_no_stop_even_zero_negative(self):
        h=make_b();qualified(h)
        for value in [Decimal('.90'),Decimal('.50'),Decimal('.10'),Decimal('0'),Decimal('-1')]:
            h.owner.engine.net_liquidation_value=lambda *args: int(h.position().entry_economic_cost_lamports*value)
            h.owner._position_tick(MINT_A,END)
            self.assertEqual(h.position().state,PositionState.OPEN)
            self.assertFalse(h.position().latched_triggers)

    def test_B12_creator_sell_no_exit_or_entry_rejection(self):
        h=make_b();h.feed('helius_standard',create_payload(),START)
        h.feed('helius_standard',creator_sell_frame(h,MINT_A,'sell',101,tok=10000),START+NS_PER_MS)
        buys(h,[_pubkey(str(n)) for n in range(5)])
        h.advance(END);self.assertEqual(h.position().state,PositionState.OPEN)
        h.feed('helius_standard',creator_sell_frame(h,MINT_A,'sell2',110,tok=10000),END+NS_PER_MS)
        self.assertEqual(h.position().state,PositionState.OPEN)
        self.assertFalse(h.position().latched_triggers)

    def test_B13_timeout_from_successful_fill(self):
        h=make_b(latency_model={'entry_delay_ms':100,'exit_delay_ms':0});qualified(h)
        h.advance(END+100*NS_PER_MS);p=h.position()
        self.assertEqual(p.timeout_due_mono_ns,p.entry_fill_mono_ns+60_000*NS_PER_MS)
        due=p.timeout_due_mono_ns
        h.feed('helius_standard',pump_frame(h,MINT_A,10000,'fresh',150),due-NS_PER_MS)
        h.advance(due-1);self.assertEqual(p.state,PositionState.OPEN)
        h.advance(due);self.assertEqual(p.exit_reason,ExitReason.TIMEOUT)

    def test_B14_simultaneous_observations(self):
        h=make_b();h.feed('helius_standard',create_payload(),START);h.feed('helius_standard',create_payload(MINT_B,sig='c2'),START)
        self.assertEqual(len(h.owner.strategy.observations),2)
        h.advance(END);self.assertEqual(len(h.rows('select * from strategy_observations')),2)

    def test_B15_open_position_does_not_block_observation(self):
        h=make_b();qualified(h);h.feed('helius_standard',create_payload(MINT_B,sig='c2'),END+NS_PER_MS)
        self.assertIn(MINT_B,h.owner.strategy.observations)

    def test_B16_parallel_positions_shared_portfolio(self):
        for limit,expected in [(1,1),(10,2)]:
            h=make_b(portfolio={'max_open_positions':limit});qualified(h)
            qualified(h,MINT_B,END+NS_PER_MS,'second')
            self.assertEqual(h.owner.counters.entry_fills,expected)

    def test_B17_a_regression_immediate_entry_creator_exit(self):
        h=Harness();p=open_position(h)
        self.assertEqual(p.entry_fill_mono_ns,START)
        h.feed('helius_standard',creator_sell_frame(h,MINT_A,'sell',102),START+NS_PER_MS);h.advance(START+NS_PER_MS)
        self.assertEqual(p.exit_reason,ExitReason.CREATOR_SELL)

    def test_A_slot_capital_scales_exactly_with_slot_count(self):
        for slots, expected in [(1,100_000_000),(2,200_000_000),(10,1_000_000_000)]:
            h=Harness(a_creator_config(portfolio={'max_open_positions':slots}))
            self.assertTrue(h.owner.capital_initialized)
            self.assertEqual(h.owner.initial_capital_lamports,expected)
            self.assertEqual(h.owner.portfolio.cash_lamports,expected)

    def test_A_one_trade_never_reserves_more_than_10_usd_slot_capital(self):
        h=Harness(a_creator_config())
        h.feed('helius_standard',create_payload(),START)
        p=h.position()
        self.assertEqual(p.state,PositionState.ENTRY_PENDING)
        self.assertLessEqual(p.reserved_cash_lamports,100_000_000)
        row=h.rows('select trade_size_usd,swap_budget_lamports,sizing_basis from entry_sizing')[0]
        self.assertEqual(row['trade_size_usd'],'10')
        self.assertEqual(row['swap_budget_lamports'],100_000_000)
        self.assertEqual(row['sizing_basis'],'USD_SWAP_BUDGET')

    def test_A_target_disabled_but_creator_sell_still_exits(self):
        h=Harness(a_creator_config());p=open_position(h)
        self.assertFalse(h.owner.strategy.target_reached(2500,1000,Decimal('1.5')))
        h.owner.engine.net_liquidation_value=lambda *args: p.entry_economic_cost_lamports*3
        h.owner._position_tick(MINT_A,START)
        self.assertEqual(p.state,PositionState.OPEN)
        self.assertFalse(any(r == ExitReason.TARGET for r, _ in p.latched_triggers))
        h.feed('helius_standard',creator_sell_frame(h,MINT_A,'creator_exit_after_disabled_target',103),START+NS_PER_MS)
        h.advance(START+NS_PER_MS)
        self.assertEqual(p.exit_reason,ExitReason.CREATOR_SELL)

    def test_B18_strategy_selection_hash_and_run_identity(self):
        a,b=Harness(),make_b()
        self.assertNotEqual(a.cfg.strategy_id,b.cfg.strategy_id)
        self.assertNotEqual(a.cfg.strategy_config_hash,b.cfg.strategy_config_hash)
        for h in (a,b):
            row=h.rows('select strategy_id,strategy_config_hash from runs')[0]
            self.assertEqual(tuple(row),(h.cfg.strategy_id,h.cfg.strategy_config_hash))

    def test_missing_stale_future_invalid_fx_fail_closed(self):
        for variant in ['missing','stale','future','invalid']:
            h=make_b();snap=h.owner.fx_snapshot
            h.owner.fx_snapshot={'missing':None,'stale':replace(snap,received_mono_ns=T0-31_000*NS_PER_MS),'future':replace(snap,received_mono_ns=END+1),'invalid':replace(snap,validation_status='INVALID')}[variant]
            qualified(h)
            self.assertEqual(h.owner.counters.entry_intents,0,variant)
            self.assertIn('SOL_USD_EVIDENCE_MISSING_OR_STALE',h.owner.counters.ineligible)

    def test_fx_validation_and_floor(self):
        for amount in ['NaN','Infinity','0','-1','bad']:
            s=FxSnapshot.from_payload(json.dumps({'data':{'amount':amount,'base':'SOL','currency':'USD'}}),'fixture','now',T0)
            self.assertIsNone(s.budget('10',T0,30000))
        s=FxSnapshot.from_payload('{"data":{"amount":"3","base":"SOL","currency":"USD"}}','fixture','now',T0)
        self.assertEqual(s.budget('10',T0,30000),3_333_333_333)

    def test_policy_version_is_frozen(self):
        for k,v in [('target_net_multiple','1.06'),('observation_ms',999),('min_buy_count',4),('stop_net_multiple','0.9'),('creator_sell_exit_enabled',True),('strategy_timeout_ms',590000),('trade_size_usd','11')]:
            with self.assertRaises(ConfigError):bconfig(strategy={k:v})

    def test_late_timer_does_not_extend_window(self):
        h=make_b();h.feed('helius_standard',create_payload(),START);buys(h,[_pubkey(str(n)) for n in range(4)])
        h.clock.advance_to(END+10*NS_PER_MS)
        h.owner.on_raw(RawEnvelope('helius_standard','c',END+NS_PER_MS,'',100,pump_frame(h,MINT_A,10000,'late',120,_pubkey('extra'))))
        h.scheduler.fire_due(END,END+10*NS_PER_MS)
        row=h.rows('select * from strategy_observations')[0]
        self.assertEqual(row['observation_end'],END);self.assertEqual(row['buy_count'],4)
        self.assertEqual(row['decision_actual_mono_ns'],END+10*NS_PER_MS)

    def test_fx_tampering_rejected(self):
        h=make_b();qualified(h);h.finish(END)
        h.conn.execute("update fx_snapshots set amount='1'")
        with self.assertRaisesRegex(ValueError,'INTEGRITY'):
            load_replay_input(h.conn,'run',h.cfg)

    def test_observation_censored_at_recording_end(self):
        h=make_b();h.feed('helius_standard',create_payload(),START);h.finish(START+500*NS_PER_MS)
        row=h.rows('select * from strategy_observations')[0]
        self.assertEqual(row['status'],'CENSORED_RUN_END');self.assertIsNone(row['qualified'])

    def test_same_capture_ab_matrix_and_read_only_source(self):
        import tempfile
        import hashlib
        from darkbot.ab_replay import compare_capture
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source.sqlite'
            h=Harness(bconfig(),db_path=str(source));h.owner.on_fx_snapshot(FxSnapshot.from_payload('{"data":{"amount":"100","base":"SOL","currency":"USD"}}','fixture',h.clock.utc_iso(),T0))
            qualified(h);h.finish(END+NS_PER_MS);h.conn.close()
            before=hashlib.sha256(source.read_bytes()).hexdigest()
            summary=compare_capture(source,'run',root/'out.sqlite',root/'reports',base_config(),bconfig(),ROOT/'config/research_ab.json')
            self.assertEqual(len(summary['results']),24)
            self.assertEqual({r['portfolio_limit'] for r in summary['results']},{1,10})
            self.assertEqual(len({r['dataset_hash'] for r in summary['results']}),1)
            self.assertEqual({r['strategy_id'] for r in summary['results']},{base_config().strategy_id,bconfig().strategy_id})
            self.assertEqual(summary['measured_distribution']['status'],'UNAVAILABLE_NO_MEASUREMENTS_SUPPLIED')
            self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(),before)
            report=json.loads(next((root/'reports').glob('ab_B_L1_S5_*/report.json')).read_text())
            self.assertEqual(report['qualified_creates'],1)
            self.assertEqual(report['strategy_observations'][0]['buy_count'],5)
            self.assertEqual(report['entry_sizing'][0]['swap_budget_lamports'],100_000_000)
            self.assertEqual(report['economic']['entry_fills'],1)

    def test_measured_distribution_is_explicit_and_reproducible(self):
        import tempfile
        from darkbot.ab_replay import compare_capture
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source.sqlite'
            h=Harness(bconfig(),db_path=str(source));h.owner.on_fx_snapshot(FxSnapshot.from_payload('{"data":{"amount":"100","base":"SOL","currency":"USD"}}','fixture',h.clock.utc_iso(),T0));qualified(h);h.finish(END+100*NS_PER_MS);h.conn.close()
            measured=root/'measured.json';measured.write_text(json.dumps({'source':'offline test fixture; not real measurement','observed_utc':'2026-09-20T00:00:00Z','entry_delay_ms':[25],'exit_delay_ms':[5]}))
            research=root/'research.json';raw=json.loads((ROOT/'config/research_ab.json').read_text());raw['portfolio_position_limits']=[1];raw['latency_scenarios']=raw['latency_scenarios'][-1:];research.write_text(json.dumps(raw))
            summary=compare_capture(source,'run',root/'out.sqlite',root/'reports',base_config(),bconfig(),research,measured)
            self.assertEqual(len(summary['results']),4)
            selected=[r for r in summary['results'] if r['latency_scenario']=='MEASURED_EXECUTION_DISTRIBUTION' and r['strategy_id']==bconfig().strategy_id][0]
            self.assertEqual(selected['latency']['entry_execution_delay_ns']['p50'],25*NS_PER_MS)

    def test_shared_safety_stale_quote_and_migration(self):
        for migration in (False,True):
            h=make_b();h.feed('helius_standard',create_payload(),START)
            for n in range(5):
                h.feed('helius_standard',pump_frame(h,MINT_A,10000,'b'+str(n),101+n,_pubkey(str(n))),START+(10+n)*NS_PER_MS)
            if migration:h.feed('helius_standard',migrate_frame_logs and logs_notification('migrate',110,migrate_frame_logs(MINT_A)),END-NS_PER_MS)
            h.advance(END)
            self.assertEqual(h.owner.counters.entry_intents,0)
            self.assertTrue(h.owner.counters.ineligible)

    def test_gap_during_window_is_not_repaired_by_later_buys(self):
        h=make_b();h.feed('helius_standard',create_payload(),START)
        for feed in h.owner.eligible_trade_feeds:h.owner.on_feed_health(feed,False,START+NS_PER_MS)
        for feed in h.owner.eligible_trade_feeds:h.owner.on_feed_health(feed,True,START+2*NS_PER_MS)
        buys(h,[_pubkey(str(n)) for n in range(5)]);h.advance(END)
        self.assertEqual(h.owner.counters.entry_intents,0)
        self.assertIn('OBSERVATION_COVERAGE_GAP',h.owner.counters.ineligible)
        row=h.rows('select * from strategy_observations')[0]
        self.assertIsNone(row['qualified'])
        self.assertEqual(row['status'],'DATA_INCOMPLETE')

    def test_gap_with_filter_miss_is_data_incomplete_not_false_negative(self):
        h=make_b();h.feed('helius_standard',create_payload(),START)
        for feed in h.owner.eligible_trade_feeds:h.owner.on_feed_health(feed,False,START+NS_PER_MS)
        for feed in h.owner.eligible_trade_feeds:h.owner.on_feed_health(feed,True,START+2*NS_PER_MS)
        buys(h,[_pubkey(str(n)) for n in range(4)]);h.advance(END)
        self.assertEqual(h.owner.counters.entry_intents,0)
        self.assertIn('OBSERVATION_COVERAGE_GAP',h.owner.counters.ineligible)
        row=h.rows('select * from strategy_observations')[0]
        self.assertEqual((row['buy_count'],row['unique_buyers']),(4,4))
        self.assertIsNone(row['qualified'])
        self.assertEqual(row['status'],'DATA_INCOMPLETE')

    def test_partial_duplicate_upgrade_counts_once(self):
        h=make_b();h.feed('helius_standard',create_payload(),START)
        valid=pump_frame(h,MINT_A,10000,'upgrade',101,BUYER)
        parsed=json.loads(valid);parsed['params']['result']['value']['logs'].insert(-1,'Program data: AAAAAAAAAAA=')
        h.feed('helius_standard',json.dumps(parsed),START+10*NS_PER_MS)
        self.assertEqual(h.owner.strategy.observations[MINT_A].buy_count,0)
        h.feed('existing_rpc',valid,START+20*NS_PER_MS)
        self.assertEqual(h.owner.strategy.observations[MINT_A].buy_count,1)
        h.feed('helius_standard',valid,START+30*NS_PER_MS)
        self.assertEqual(h.owner.strategy.observations[MINT_A].buy_count,1)

    def test_stale_failed_and_missing_actor_never_count(self):
        h=make_b();h.feed('helius_standard',create_payload(),START)
        failed=json.loads(pump_frame(h,MINT_A,10000,'failed',101,BUYER));failed['params']['result']['value']['err']={'failure':True}
        h.feed('helius_standard',json.dumps(failed),START+NS_PER_MS)
        h.feed('helius_standard',pump_frame(h,MINT_A,10000,'stale',99,BUYER),START+2*NS_PER_MS)
        self.assertEqual(h.owner.strategy.observations[MINT_A].buy_count,0)
        from darkbot.market import EventApply
        from darkbot.domain import ValidationStatus, EventType
        from types import SimpleNamespace
        event=SimpleNamespace(mint=MINT_A,event_type=EventType.BUY,actor=None)
        applied=SimpleNamespace(event=event,is_new=True,validation=ValidationStatus.VALID,applied_state=True)
        h.owner.strategy.on_events(h.owner,SimpleNamespace(events=[applied]),SimpleNamespace(receive_mono_ns=START+3*NS_PER_MS),SimpleNamespace(signature='new',completeness=__import__('darkbot.domain',fromlist=['Completeness']).Completeness.COMPLETE))
        self.assertEqual(h.owner.strategy.observations[MINT_A].buy_count,0)

    def test_swap_budget_inverse_uses_modern_fee_rounding(self):
        h=make_b();h.feed('helius_standard',create_payload(),START)
        engine=h.owner.engine;st=h.owner.reducer.states[MINT_A];curve=h.owner._curve(st)
        q=engine.quote_buy_swap_budget(curve,h.fees.basis(),START,'budget',st.coverage_state,100_000_000)
        self.assertLessEqual(q.swap_wallet_debit_lamports,100_000_000)
        nextq=engine.quote_buy(curve,h.fees.basis(),START,'budget',st.coverage_state,q.curve_input_lamports+1)
        self.assertGreater(nextq.swap_wallet_debit_lamports,100_000_000)
        self.assertGreater(q.entry_cash_debit,q.swap_wallet_debit_lamports)
