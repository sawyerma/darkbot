#!/usr/bin/env python3
"""FE-00: small archived fixtures only. Reuse the existing decoder; never query RPC."""
from pathlib import Path
import argparse
import json
import sys
if __package__ in (None,''):sys.path.insert(0,str(Path(__file__).resolve().parents[2]))  # run as a file: make `operations` importable
from operations.verification import audit_market_quick as audit
from operations.cli import measurement_contract as contract

ROOT=Path(__file__).resolve().parents[2]

def report(root=ROOT):
    historical=[]
    for path in sorted((root/'docs/evidence/AUDIT_20261005').glob('*.json')):
        value=json.loads(path.read_text())
        if isinstance(value,dict):value=value.get('result',value)
        if not isinstance(value,dict) or 'transaction' not in value:continue
        tx=audit.detect(value,value['slot'])
        historical.append({'source':str(path.relative_to(root)),'source_sha256':contract.file_hash(path),
            'evidence_scope':'ARCHIVED_RPC_FIXTURE; NOT_CURRENT_VERIFICATION',
            'signature':tx['signature'],'slot':tx['slot'],'block_time':value.get('blockTime'),
            'classification':tx['classification'],'success':tx['success'],'events':tx['events'],
            'actual_meta_fee_lamports':value['meta'].get('fee'),
            'requested_priority_fee_lamports':audit.requested_priority_fee(value),
            'priority_included_in_meta_fee':True,'current_executable':False,'realized_net_pnl_usd':None,
            'unknowns':['CURRENT_HEALTH','CURRENT_PROGRAM_PARITY','CURRENT_ROUTE','OUR_WALLET_INVENTORY','FULL_EXIT_AND_COST_LEDGER']})
    probes=[]
    for name in ('R122_juplend_complete.json','R124_rpc_economics.json'):
        path=root/'docs/evidence'/name;data=json.loads(path.read_text())
        for row in data.get('candidate_records',data.get('liquidatable_decisions',[])):
            probes.append({'source':str(path.relative_to(root)),'source_sha256':contract.file_hash(path),
                'account':row.get('account'),'decision':row.get('decision'),'funding':row.get('funding'),
                'sign_slot':row.get('sign_slot'),'serialized_bytes':row.get('serialized_bytes'),
                'locked_accounts':row.get('locked_accounts'),'simulated_cu':row.get('simulated_cu'),
                'candidate_costs':row.get('candidate_costs',[]),'reason':row.get('reason'),
                'current_executable':False})
    return {'schema':'Project0/FE00/1','status':'NO_ECONOMIC_GO','historical':historical,'archived_shadow_probes':probes,
        'selected_for_requalification':'ZERO_PREFUND_SAME_TX','pilot_ready':False,
        'decision_basis':'R122 proves historical full technical build/simulation; R124 known RPC net edges are negative. Current candidate/route/profit remains unproved.',
        'funding_comparison':{
            'NO_SWAP_INVENTORY':{'status':'BLOCKED','reason':'builder passes realizable value None: unsold collateral is not cash proceeds; no qualified exit instructions/costs, no current wallet inventory proof'},
            'ZERO_PREFUND_SAME_TX':{'status':'REQUALIFY_ONLY','reason':'Existing same-TX collateral-to-liability exit, floor checked >= repay, compiled full TX. Fresh route, actual fee, inventory/rent and positive conservative edge still required.'},
            'JIT_SHORTFALL_EXACT_IN':{'status':'REQUIRED_IF_FIRST_TWO_FAIL_CURRENT_QUALIFICATION','reason':'Bounded ExactIn solve, funding/liability holds and P0_JIT_FUNDING_BANK sizing exist offline only. Requires a configured funding bank, current quotes, bundle simulation and collateral exit; do not pretend these exist.'}},
        'live_enabled':False,'submission_enabled':False,
        'next_mainnet_evidence':['CURRENT_PROGRAMDATA_SOURCE_PARITY','CURRENT_TARGET_HEALTH_BANK_ORACLE_STATE','EXACT_ROUTE_FLOOR_AND_TX_FIT','CURRENT_FEE_AND_WALLET_BALANCES','FULL_TX_SIMULATION_AND_POSITIVE_CONSERVATIVE_EDGE'],
        'five_hour_replay':'NOT_RUN_USER_OWNED'}

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();contract.immutable(args.output,report())
    print(args.output)

if __name__=='__main__':main()
