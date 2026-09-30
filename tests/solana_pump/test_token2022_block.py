"""R9: real recorded CreateV2 evidence plus integrated adversarial cost/replay cases.
No network access, no signed transactions. RPC fee fixtures model fees, not execution.
"""
import base64
import copy
import json
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from darkbot.clock import ReplayClock
from darkbot.solana_pump.domain import AttemptStatus, InvalidQuote, Side
from darkbot.solana_pump.execution import evaluate_fill, fee_terms
from darkbot.solana_pump.fees import FeeResolver, build_trade_fee_message
from darkbot.main import economic_profile_samples
from darkbot.solana_pump.market import b58decode, b58encode
from darkbot.solana_pump.replay import digest, load_replay_input, run_replay
from darkbot.solana_pump.token_accounts import ASSOCIATED_TOKEN_PROGRAM, create_account_profiles, find_pda
from tests.helpers import ROOT, T0, Harness, base_config, fee_basis, dump_frame, complete_frame_logs, logs_notification

FIXTURES = ROOT / 'tests/fixtures'


def recorded_create(index=1):
    return copy.deepcopy(json.loads((FIXTURES / 'mainnet_create_transactions.json').read_text())[index]['payload'])


def costs(cfg):
    cache = fee_basis(cfg, T0)
    calls = []
    def rpc(method, params):
        calls.append((method, params))
        if method == 'getLatestBlockhash':
            return {'context': {'slot': 1}, 'value': {'blockhash': b58encode(bytes([1]) * 32)}}
        if method == 'getFeeForMessage':
            return {'context': {'slot': 1}, 'value': 7000}
        if method == 'getMinimumBalanceForRentExemption':
            return {165: 2039280, 170: 2074080}[params[0]]
        raise AssertionError(method)
    resolver = FeeResolver(cfg, rpc, ReplayClock('e', T0, 0), 'run', 'FIXTURE')
    cache.network = resolver.resolve_network(cache.priority_policy)
    cache.rent = resolver.resolve_rent()
    return cache, calls


def decode_message(msg):
    """Independent compact-u16 parser; inspect the actual serialized account metas."""
    cursor = 3
    def short():
        nonlocal cursor
        n, shift = 0, 0
        while True:
            v = msg[cursor]; cursor += 1
            n |= (v & 127) << shift
            if not v & 128:
                return n
            shift += 7
    count = short()
    keys = [msg[cursor+i*32:cursor+(i+1)*32] for i in range(count)]
    cursor += count*32 + 32
    instructions = []
    for _ in range(short()):
        program = keys[msg[cursor]]; cursor += 1
        n = short(); indexes = list(msg[cursor:cursor+n]); cursor += n
        size = short(); data = msg[cursor:cursor+size]; cursor += size
        instructions.append((program, indexes, data))
    assert cursor == len(msg)
    return msg[:3], keys, instructions


class Token2022Block(unittest.TestCase):
    def harness(self, cfg=None, **kwargs):
        cfg = cfg or base_config()
        cache, calls = costs(cfg)
        h = Harness(cfg, fees=cache, **kwargs)
        self.addCleanup(h.conn.close)
        return h, calls

    def feed_create(self, h, payload=None):
        h.feed('helius_standard', json.dumps(payload or recorded_create()), T0+1_000_000)
        h.advance(T0+2_000_000)
        return next(iter(h.owner.reducer.states.values()))

    def test_recorded_create_buy_exit_refund_and_sql_provenance(self):
        h, calls = self.harness()
        before = len(calls)
        # No new fee-message/PDA/RPC work may enter the StateOwner path.
        with patch('darkbot.solana_pump.fees.build_trade_fee_message', side_effect=AssertionError('hot path message')), patch('darkbot.solana_pump.token_accounts.find_pda', side_effect=AssertionError('hot path PDA')):
            st = self.feed_create(h)
        self.assertEqual(len(calls), before)
        pos = h.position()
        self.assertEqual(pos.state.value, 'OPEN')
        self.assertEqual(pos.locked_rent_lamports, 2074080)
        h.feed('helius_standard', dump_frame(h, st.mint, 5_000_000_000_000, 'exit', st.last_state_slot+1, user=st.creator), T0+3_000_000)
        h.advance(T0+4_000_000)
        rows = h.rows('select * from execution_attempts order by submit_mono_ns')
        self.assertEqual([r['status'] for r in rows], ['FILLED','FILLED'])
        self.assertEqual([r['instruction_name'] for r in rows], ['buy_v2','sell_v2'])
        self.assertEqual(rows[1]['rent_refunded_lamports'], 2074080)
        for r in rows:
            self.assertEqual(r['token_program'], h.cfg.protocol['token_2022_program'])
            self.assertEqual(r['account_size_bytes'],170)
            self.assertTrue(r['message_template_hash'])
            q = h.rows('select * from quote_snapshots where quote_snapshot_id=?',(r['fill_quote_snapshot_id'],))[0]
            self.assertEqual(q['account_profile_id'],r['account_profile_id'])
            self.assertEqual(q['message_template_hash'],r['message_template_hash'])
        self.assertFalse(h.fk_check())
        self.assertFalse(h.disk_errors)

    def test_account_size_and_metadata_evidence_is_program_scoped(self):
        cfg=base_config(); payload=recorded_create(); logs=payload['params']['result']['value']['logs']
        self.assertEqual(len(create_account_profiles(logs,cfg.protocol)),1)
        token=cfg.protocol['token_2022_program']
        cases=[ [line.replace('qgAAAAAAAAA=','pQAAAAAAAAA=') for line in logs],
                [line.replace('MetadataPointerInstruction::Initialize','TransferFeeExtension: InitializeTransferFeeConfig') for line in logs],
                [line.replace(token,'FakeTokenProgram') if line.startswith('Program '+token) else line for line in logs],
                logs[:-1] ]
        for bad in cases:
            self.assertEqual(create_account_profiles(bad,cfg.protocol),{})
            p=copy.deepcopy(payload); p['params']['result']['value']['logs']=bad
            h,_=self.harness();self.feed_create(h,p)
            self.assertFalse(h.owner.positions)

    def test_unverified_program_and_scalar_costs_do_not_enable_token2022(self):
        cfg=base_config()
        h=Harness(cfg);self.addCleanup(h.conn.close)
        self.feed_create(h)
        self.assertFalse(h.owner.positions)  # valid create evidence, legacy-only fee evidence
        self.assertEqual(h.rows('select reason from decisions')[0][0],'UNQUALIFIED_COSTS')
        h,_=self.harness();st=self.feed_create(h)
        state=replace(h.owner._curve(st),token_program='unknown')
        self.assertEqual(fee_terms(h.fees.basis(),h.owner.policy,h.clock.monotonic_ns(),state).reason,'UNSUPPORTED_TOKEN_PROGRAM')
        state=replace(h.owner._curve(st),account_profile_id=None)
        self.assertEqual(fee_terms(h.fees.basis(),h.owner.policy,h.clock.monotonic_ns(),state).reason,'TOKEN_ACCOUNT_PROFILE_UNVERIFIED')

    def test_side_scoped_costs_and_missing_sell_reserve(self):
        h,_=self.harness();st=self.feed_create(h);state=h.owner._curve(st)
        network=copy.deepcopy(h.fees.network.normalized_fee_components)
        program=h.cfg.protocol['token_2022_program']
        sell=network['execution_profiles'][program]['SELL']
        sell.update(base_fee_lamports=8000,network_fee_total_lamports=10000)
        basis=replace(h.fees.basis(),network=replace(h.fees.network,normalized_fee_components=network))
        buy=fee_terms(basis,h.owner.policy,h.clock.monotonic_ns(),state,Side.BUY)
        sell_terms=fee_terms(basis,h.owner.policy,h.clock.monotonic_ns(),state,Side.SELL)
        self.assertEqual((buy.base_fee_lamports,sell_terms.base_fee_lamports),(5000,8000))
        self.assertEqual(h.owner.engine.exit_reserve_lamports(basis,h.clock.monotonic_ns(),state),10000*h.owner.policy.max_exit_attempts)
        del network['execution_profiles'][program]['SELL']
        self.assertIsNone(h.owner.engine.exit_reserve_lamports(basis,h.clock.monotonic_ns(),state))
        self.assertEqual(economic_profile_samples(h.owner,basis,h.clock.monotonic_ns()),0)

    def test_pending_fill_uses_selected_profile_never_another_program(self):
        cfg=base_config(latency_model={'entry_delay_ms':5})
        h,_=self.harness(cfg);st=self.feed_create(h)
        self.assertEqual(h.position().state.value,'ENTRY_PENDING')
        data=copy.deepcopy(h.fees.network.normalized_fee_components)
        program=cfg.protocol['token_2022_program']
        del data['execution_profiles'][program]
        snap=replace(h.fees.network,fee_snapshot_id='missing_profile',received_mono_ns=T0+3_000_000,normalized_fee_components=data)
        h.recorder.try_append(('fee_snapshot',(snap,h.run_id)));h.owner.on_fee_snapshots([snap])
        h.advance(T0+7_000_000)
        row=h.rows('select status,network_fee_total_lamports from execution_attempts')[0]
        self.assertEqual(tuple(row),('UNKNOWN',None))
        self.assertFalse(h.rows('select * from ledger'))

    def test_fill_refresh_and_replay_preserve_scope(self):
        cfg=base_config(latency_model={'entry_delay_ms':5})
        with tempfile.TemporaryDirectory() as tmp:
            h,_=self.harness(cfg,db_path=str(Path(tmp)/'run.sqlite'))
            self.feed_create(h)
            n=copy.deepcopy(h.fees.network.normalized_fee_components)
            program=cfg.protocol['token_2022_program']
            n['execution_profiles'][program]['BUY'].update(base_fee_lamports=7000,network_fee_total_lamports=9000,message_template_hash='changed-buy-profile')
            snap=replace(h.fees.network,fee_snapshot_id='refreshed_profile',received_mono_ns=T0+3_000_000,normalized_fee_components=n)
            h.recorder.try_append(('fee_snapshot',(snap,h.run_id)));h.owner.on_fee_snapshots([snap])
            h.finish(T0+7_000_000)
            row=h.rows('select base_fee_lamports,message_template_hash from execution_attempts')[0]
            self.assertEqual(tuple(row),(7000,'changed-buy-profile'))
            replay=run_replay(h.conn,load_replay_input(h.conn,'run',cfg),cfg,'replay','new',persist=False)
            self.assertEqual(replay.digest,digest(h.owner))
            self.assertEqual(next(iter(replay.owner.reducer.states.values())).account_profile_id,cfg.protocol['token_2022_create_profile']['profile_id'])

    def test_rent_refresh_refunds_the_original_deposit(self):
        h,_=self.harness();st=self.feed_create(h)
        rent=copy.deepcopy(h.fees.rent.normalized_fee_components)
        rent['account_profiles'][h.cfg.protocol['token_2022_program']]['token_account_rent_lamports']=9999999
        h.fees.rent=replace(h.fees.rent,fee_snapshot_id='rent_changed',received_mono_ns=T0+2_000_000,normalized_fee_components=rent)
        h.recorder.try_append(('fee_snapshot',(h.fees.rent,h.run_id)));h.owner.on_fee_snapshots([h.fees.rent])
        h.feed('helius_standard',dump_frame(h,st.mint,5_000_000_000_000,'exit',st.last_state_slot+1,user=st.creator),T0+3_000_000);h.advance(T0+4_000_000)
        self.assertEqual(h.rows("select rent_refunded_lamports from execution_attempts where side='SELL'")[0][0],2074080)

    def test_capture_only_and_unsupported_observations_cannot_certify_ready(self):
        h,_=self.harness(economic=False);self.feed_create(h)
        self.assertFalse(h.owner.positions)
        self.assertEqual(h.rows('select reason from decisions')[0][0],'CAPTURE_ONLY_MODE')
        self.assertEqual(economic_profile_samples(h.owner,h.fees.basis(),h.clock.monotonic_ns()),1)
        h2,_=self.harness();self.feed_create(h2,recorded_create(0)) # genuine Mayhem fixture
        self.assertEqual(economic_profile_samples(h2.owner,h2.fees.basis(),h2.clock.monotonic_ns()),0)

    def test_serialized_v2_metas_match_pinned_official_idl(self):
        cfg=base_config();fixture=json.loads((FIXTURES/'pump_v2_instruction_contract.json').read_text())
        for program in cfg.execution['token_account_policy']['profiles']:
            for ix in fixture['instructions']:
                side=ix['name'].split('_')[0]
                template=cfg.execution['message_templates'][side]
                for key in ('accounts','args','discriminator'):
                    self.assertEqual(template[key],ix[key])
                msg=build_trade_fee_message(cfg,side,1000,bytes(32),program)
                header,keys,instructions=decode_message(msg)
                self.assertEqual(header[0],1)
                self.assertLessEqual(len(msg)+65,1232)
                pump,indexes,data=next(i for i in instructions if i[0]==b58decode(cfg.protocol['program_id']))
                self.assertEqual(data[:8],bytes(ix['discriminator']))
                self.assertEqual(len(data),24)
                self.assertEqual(len(indexes),len(ix['accounts']))
                named={a['name']:keys[i] for a,i in zip(ix['accounts'],indexes)}
                for a,i in zip(ix['accounts'],indexes):
                    self.assertEqual(i<header[0],a.get('signer',False))
                    writable=(i<header[0]-header[1] if i<header[0] else i<len(keys)-header[2])
                    self.assertEqual(writable,a.get('writable',False),a['name'])
                    if 'address' in a: self.assertEqual(keys[i],b58decode(a['address']))
                self.assertEqual(named['base_token_program'],b58decode(program))
                self.assertEqual(named['quote_token_program'],b58decode(cfg.protocol['spl_token_program']))
                self.assertEqual(named['quote_mint'],b58decode(cfg.protocol['wsol_mint']))
                ata=find_pda(b58decode(ASSOCIATED_TOKEN_PROGRAM),named['user'],b58decode(program),named['base_mint'])
                self.assertEqual(named['associated_base_user'],ata)
                self.assertEqual(len(instructions),4) # two CU + base ATA create/close + Pump; no quote ATA/wrap
                if side=='sell': self.assertEqual(instructions[-1][0],b58decode(program))

    def test_pda_derivation_against_captured_mainnet_addresses(self):
        cfg=base_config()
        self.assertEqual(b58encode(find_pda(b58decode(cfg.protocol['program_id']),b'global')),cfg.protocol['global_account']['address'])
        tx=json.loads((FIXTURES/'mainnet_trade_transaction.json').read_text())['rpc']['result']
        keys=tx['transaction']['message']['accountKeys']+tx['meta']['loadedAddresses']['writable']+tx['meta']['loadedAddresses']['readonly']
        checked=0
        for balance in tx['meta']['postTokenBalances']:
            ata=find_pda(b58decode(ASSOCIATED_TOKEN_PROGRAM),b58decode(balance['owner']),b58decode(balance['programId']),b58decode(balance['mint']))
            self.assertEqual(b58encode(ata),keys[balance['accountIndex']]);checked+=1
        self.assertGreaterEqual(checked,2)

    def test_complete_duplicate_can_supply_missing_profile_without_rewinding_state(self):
        h,_=self.harness()
        payload=recorded_create()
        truncated=copy.deepcopy(payload)
        truncated['params']['result']['value']['logs'].pop()
        st=self.feed_create(h,truncated)
        self.assertFalse(h.owner.positions)
        self.assertIsNone(st.account_profile_id)
        prior=(st.virtual_sol_reserves,st.virtual_token_reserves,st.last_state_event_id)
        h.feed('helius_standard',json.dumps(payload),T0+3_000_000)
        h.advance(T0+4_000_000)
        self.assertEqual(h.position().state.value,'OPEN')
        self.assertEqual((st.virtual_sol_reserves,st.virtual_token_reserves,st.last_state_event_id),prior)
        h.feed('helius_standard',json.dumps(payload),T0+5_000_000)
        self.assertEqual(len(h.owner.positions),1)
        self.assertEqual(h.owner.counters.creates,1)
        self.assertEqual(len(h.rows('select * from execution_attempts')),1)
        row=h.rows("select account_profile_id,quality_status from market_events where event_type='CREATE'")[0]
        self.assertTrue(row['account_profile_id'])
        self.assertEqual(row['quality_status'],'VALID_UPGRADED_ACCOUNT_PROFILE')

    def test_status_short_circuits_and_shutdown_keeps_observed_evidence(self):
        from types import SimpleNamespace
        from darkbot.main import Application
        from darkbot.solana_pump.domain import Readiness
        h,_=self.harness(economic=False)
        st=self.feed_create(h)
        # Real max-size population; deterministic work-count assertion, not a wall-clock threshold.
        for i in range(h.cfg.storage['max_tracked_mints']-1):
            h.owner.reducer.states['replica'+str(i)]=st
        with patch('darkbot.solana_pump.execution.fee_terms',wraps=fee_terms) as terms:
            self.assertEqual(economic_profile_samples(h.owner,h.fees.basis(),h.clock.monotonic_ns(),limit=1),1)
            self.assertEqual(terms.call_count,2)
        h.owner.reducer.states={st.mint:st}
        app=Application.__new__(Application)
        app.owner=h.owner;app.resolver=SimpleNamespace(cache=h.fees)
        app.clock=h.clock;app.cfg=h.cfg;app.recorder=h.recorder;app.writer=None
        app._economic_sample_count_at_stop=None
        self.assertEqual(app._capture_end_readiness(),Readiness.ECONOMIC_PAPER_READY)
        h.owner.finalize(h.clock.monotonic_ns())
        self.assertFalse(h.owner.reducer.states)
        self.assertEqual(app._economic_sample_count_at_stop,1)
        self.assertEqual(app.compute_readiness(),Readiness.ECONOMIC_PAPER_READY)

    def test_pending_token2022_completion_keeps_profile_and_failure_fee(self):
        for side in ('BUY','SELL'):
            with self.subTest(side=side):
                cfg=base_config(latency_model={'entry_delay_ms':5 if side=='BUY' else 0,'exit_delay_ms':5})
                h,_=self.harness(cfg);st=self.feed_create(h)
                if side=='SELL':
                    h.feed('helius_standard',dump_frame(h,st.mint,5_000_000_000_000,'sell',st.last_state_slot+1,user=st.creator),T0+3_000_000)
                h.feed('helius_standard',logs_notification('complete',st.last_state_slot+1,complete_frame_logs(st.mint)),T0+4_000_000)
                h.advance(T0+9_000_000)
                row=h.rows('select * from execution_attempts where side=?',(side,))[0]
                self.assertEqual(row['status'],'VENUE_INVALID')
                self.assertEqual(row['fees_charged_lamports'],7000)
                self.assertEqual(row['token_program'],cfg.protocol['token_2022_program'])
                self.assertEqual(row['account_profile_id'],cfg.protocol['token_2022_create_profile']['profile_id'])
                ledger=h.rows('select * from ledger where attempt_id=?',(row['attempt_id'],))
                self.assertEqual(len(ledger),1)
                self.assertFalse(h.owner.counters.internal_errors)

    def test_rpc_refresh_budget_for_all_profiles(self):
        cfg=base_config();_,calls=costs(cfg)
        self.assertEqual([p[0] for m,p in calls if m=='getMinimumBalanceForRentExemption'],[165,170])
        self.assertEqual(sum(m=='getFeeForMessage' for m,p in calls),4)
        # 4 message probes + blockhash + priority every 20s, two protocol reads/minute.
        refreshes=60000/cfg.costs['snapshot_refresh_policy']['network_fee_refresh_interval_ms']
        self.assertLessEqual(refreshes*6+2+2,cfg.raw['rpc_http']['max_requests_per_minute'])

if __name__=='__main__': unittest.main()
