"""Independent read-only r3 audit probes; imports the frozen snapshot, never edits it."""
import base64
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'snapshot'))
from tests.helpers import Harness, base_config, MINT_A, CREATOR, BUYER, T0, open_position, creator_sell_frame, trade_frame_logs, logs_notification
from darkbot.market import encode_fields
from darkbot.fees import FeeResolver
from darkbot.clock import ReplayClock

results = {}

# A verified layout label must not make an account of a different discriminator VALID.
cfg = base_config()
ga = dict(cfg.protocol['global_account'])
ga['verification_status'] = 'VERIFIED_AUDIT_LAYOUT'
cfg = cfg.with_overrides(protocol={'global_account': ga})
values = [(name, CREATOR if kind == 'pubkey' else True if kind == 'bool' else 100)
          for name, kind in ga['layout_after_discriminator']]
payload = b'WRONGDIS' + encode_fields(values, ga['layout_after_discriminator'])
def rpc(method, params):
    assert method == 'getAccountInfo'
    return {'context': {'slot': 1}, 'value': {'owner': cfg.protocol['program_id'], 'data': [base64.b64encode(payload).decode(), 'base64']}}
snap = FeeResolver(cfg, rpc, ReplayClock('audit', T0, 0), 'audit', 'FIXTURE').resolve_protocol()
results['wrong_global_discriminator'] = {'expected': 'invalid/unknown', 'actual_status': snap.validation_status, 'evidence': snap.evidence.value, 'bug_reproduced': snap.validation_status == 'VALID'}

# Trades captured before a CREATE have no verified SOL scope.
h = Harness(economic=False)
h.feed('helius_standard', logs_notification('orphan', 1, trade_frame_logs(MINT_A, 10, 10, True, BUYER, 31_000_000_000, 10**15, 7*10**14, tail=b'unpinned_quote_tail')), T0+1_000_000)
row = dict(h.rows('select virtual_sol_reserves, quote_scope_status from market_events')[0])
results['unknown_quote_trade_sol_columns'] = {'expected': 'NULL SOL reserves until quote scope verified', 'actual': row, 'bug_reproduced': row['virtual_sol_reserves'] is not None and row['quote_scope_status'] is None}

# The live creator-sell query must recognize an already-detected creator sell.
h = Harness()
open_position(h)
h.feed('helius_standard', creator_sell_frame(h, MINT_A, 'creator_sell', 101), T0+2_000_000)
h.advance(T0+3_000_000)
actual = h.rows("SELECT COUNT(*) FROM market_events WHERE run_id='run' AND event_type='SELL' AND actor=creator")[0][0]
known = h.owner.reducer.states[MINT_A].creator_sell_event_id is not None
results['doctor_creator_sell_count'] = {'expected': 1, 'actual': actual, 'runtime_creator_sell_detected': known, 'bug_reproduced': known and actual == 0}

# ONE_SUBMITTED_ENTRY_PER_MINT_PER_CAMPAIGN must survive starting a second run.
with tempfile.TemporaryDirectory() as d:
    db = str(Path(d) / 'campaign.sqlite')
    h1 = Harness(db_path=db, run_id='first')
    open_position(h1)
    h1.finish(T0+4_000_000)
    h2 = Harness(db_path=db, run_id='second')
    open_position(h2)
    h2.flush()
    rows = [dict(r) for r in h2.rows("SELECT r.campaign_id,p.run_id,p.mint,p.state FROM positions p JOIN runs r USING(run_id)")]
    results['campaign_reentry_after_restart'] = {'expected': 'no second submitted BUY for same campaign/mint', 'actual_positions': rows, 'bug_reproduced': len(rows) == 2 and rows[0]['mint'] == rows[1]['mint']}

(Path(__file__).parent / 'counterexamples.json').write_text(json.dumps(results, indent=2))
print(json.dumps(results, indent=2))
