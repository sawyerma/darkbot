import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0/scripts on sys.path
import unittest
from fe_candidate_report import report

class Candidates(unittest.TestCase):
    def test_historical_positive_technical_is_not_current_profit(self):
        value=report()
        self.assertEqual(len(value['historical']),6)
        self.assertTrue(all(r['signature'] and r['slot'] and not r['current_executable'] for r in value['historical']))
        self.assertEqual(sum(r['decision']=='ShadowTechnicalReady' for r in value['archived_shadow_probes']),2)
        net=[c['net_profit_usd_micro'] for r in value['archived_shadow_probes'] for c in r['candidate_costs']]
        self.assertEqual(net,[-121889,-118606,-91691])
        self.assertEqual(value['status'],'NO_ECONOMIC_GO')
        self.assertFalse(value['pilot_ready']);self.assertFalse(value['live_enabled']);self.assertFalse(value['submission_enabled'])
        self.assertEqual(value['five_hour_replay'],'NOT_RUN_USER_OWNED')
