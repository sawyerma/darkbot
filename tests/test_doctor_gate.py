"""Doctor's final economic gate: fixture/replay failures must block release."""
import asyncio
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import AsyncMock, patch

from darkbot.main import run_doctor
from tests.helpers import Harness, T0, base_config, create_payload


class DoctorGate(unittest.TestCase):
    def test_fixture_and_replay_failures_block_economic_readiness(self):
        for failed in (None, 'D04', 'D06'):
            with self.subTest(failed=failed), tempfile.TemporaryDirectory() as tmp:
                cfg = base_config(doctor={'minimum_valid_create_samples': 1, 'minimum_valid_trade_samples': 1})
                db = Path(tmp) / 'doctor.sqlite'
                h = Harness(cfg, db_path=str(db), economic=False)
                try:
                    h.conn.execute("UPDATE runs SET mode='DOCTOR'")
                    h.feed('helius_standard', create_payload(initial_buy=(100_000_000, 3_000_000_000_000)), T0+1_000_000)
                    h.finish(T0+2_000_000)
                    summary = {'readiness': 'ECONOMIC_PAPER_READY', 'owner': h.owner.status(), 'feeds': {'helius_standard': {'acks': 1, 'ack_failures': 0, 'messages': 1}},
                               'feed_setup': {}, 'fee_errors': [], 'writer': {'committed_records': 1, 'disk_error': False},
                               'economic_profile_samples_at_run_end': 1}
                    app = types.SimpleNamespace(run_id=h.run_id, owner=h.owner, run=AsyncMock(return_value=summary))
                    # Deliberately unequal replay results for D06; all other prerequisite evidence stays equal.
                    replays = [types.SimpleNamespace(digest={'result': 1}, owner=h.owner),
                               types.SimpleNamespace(digest={'result': 2 if failed == 'D06' else 1}, owner=h.owner)]
                    with patch('darkbot.main.Application', return_value=app), \
                         patch('darkbot.main.doctor_fixture_checks', return_value={'all_pass': failed != 'D04'}), \
                         patch('darkbot.replay.run_replay', side_effect=replays), \
                         patch.dict('sys.modules', {'websockets': types.SimpleNamespace(__version__='offline-gate-test')}):
                        result = asyncio.run(run_doctor(cfg, db))
                    for check, values in result['checks'].items():
                        self.assertEqual(values['status'], 'FAIL' if check == failed else 'PASS', check)
                    self.assertEqual(result['readiness'], 'ECONOMIC_PAPER_READY' if failed is None else 'CAPTURE_READY')
                    self.assertEqual(result['summary']['readiness'], result['readiness'])
                    self.assertEqual(result['summary']['runtime_readiness'], 'ECONOMIC_PAPER_READY')
                    self.assertEqual(h.conn.execute('select readiness from runs where run_id=?',(h.run_id,)).fetchone()[0], result['readiness'])
                finally:
                    h.conn.close()
