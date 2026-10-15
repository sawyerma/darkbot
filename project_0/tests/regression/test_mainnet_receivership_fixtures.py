"""Regression: the six preserved mainnet receivership transactions (docs/evidence/AUDIT_20261005, read in place)."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0/scripts on sys.path
import json
import unittest
import audit_market_quick as a

ROOT=p0_paths.PROJECT


class MainnetFixtures(unittest.TestCase):
    def test_real_six_mainnet_receivership_fixtures(self):
        fixtures=[]
        for p in (ROOT/'docs/evidence/AUDIT_20261005').glob('*.json'):
            v=json.loads(p.read_text())
            if isinstance(v,dict):v=v.get('result',v)
            if isinstance(v,dict) and 'transaction' in v:
                fixtures.append((p,v))
        self.assertEqual(len(fixtures),6)
        for p,t in fixtures:
            r=a.detect(t,t['slot'])
            self.assertEqual(r['classification'],'CLASSIFIED',(p.name,r['unknown_reasons']))
            self.assertEqual(r['program_events'].get('RECEIVERSHIP'),1,p.name)
            self.assertEqual(r['signature'],p.stem)


if __name__=='__main__':unittest.main(verbosity=2)
