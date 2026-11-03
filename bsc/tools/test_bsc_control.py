#!/usr/bin/env python3
"""REV.1.0 R10: the BSC controller's terminal wrappers (ops/bsc-control.sh), hermetic in a temporary directory:
a stand-in runtime records its arguments and launchctl is a stand-in that records its calls. Nothing starts a job,
reads a key or reaches a host (a started bsc_runtime would contact the canonical RPCs)."""
import os, pathlib, subprocess, tempfile, unittest
REPO = pathlib.Path(__file__).resolve().parents[2]
CONTROL = REPO / 'ops/bsc-control.sh'
HARNESS = r'''set -euo pipefail
ROOT="$W/repo"; APP_HOME="$W/app"; RUNDIR="$W/run"; LOGDIR="$W/logs"; SOCK="$W/p.sock"; LABEL="com.darkbot.unit.gateway"
DOMAIN="gui/0"; STOP_BUDGET_S=5; APP_CONF="$W/app.conf"
say() { printf '%s\n' "$*"; }
die() { printf 'darkbotctl: %s\n' "$*" >&2; exit 1; }
locked() { "$@"; }
app_conf() { printf '%s' "${2:-}"; }
xml_escape() { cat; }
plist_env() { :; }
assert_not_down() { :; }
launchctl() { printf 'launchctl %s\n' "$*" >> "$W/launchctl.log"; [ "$1" != list ]; }
. "$CONTROL"
"$@"
'''
# The stand-in runtime: logs each call (one argument per line, a call ends with ---); refuses while W/refuse exists.
RUNTIME = '''#!/bin/sh
for a in "$@"; do printf '%s\\n' "$a" >> "{w}/runtime.log"; done; echo --- >> "{w}/runtime.log"
if [ -f "{w}/refuse" ]; then echo "STOP: refused" >&2; exit 2; fi
echo "config=OK live=OFF"
'''

class ControlTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='darkbot-bsc-control-')
        self.w = pathlib.Path(self.tmp.name)
        for d in ['repo/bsc/config', 'app/current', 'app/bsc-source/bsc/config', 'run', 'logs']:
            (self.w / d).mkdir(parents=True)
        self.rt = self.w / 'app/current/bsc_runtime'
        self.rt.write_text(RUNTIME.format(w=self.w)); self.rt.chmod(0o755)
        self.snapshot = self.w / 'app/bsc-source'
    def tearDown(self): self.tmp.cleanup()
    def sh(self, *args, env=None):
        p = subprocess.run(['/bin/bash', '-c', HARNESS, 'harness', *args], capture_output=True, text=True, timeout=30,
                           env={'PATH': '/usr/bin:/bin', 'W': str(self.w), 'CONTROL': str(CONTROL), **(env or {})})
        return p.returncode, p.stdout + p.stderr
    def calls(self):
        log = self.w / 'runtime.log'
        return [c.strip('\n').split('\n') for c in log.read_text().split('---\n') if c.strip()] if log.exists() else []
    def launchd(self):
        log = self.w / 'launchctl.log'
        return log.read_text() if log.exists() else ''

    def test_a_refused_configuration_is_never_handed_to_launchd(self):
        (self.w / 'refuse').touch()
        code, out = self.sh('bsc_runtime_up')
        self.assertEqual(code, 0, out); self.assertIn('BSC Runtime nicht gestartet: Konfiguration verweigert', out)
        self.assertNotIn('bootstrap', self.launchd())
        self.assertEqual(self.calls(), [['config-check', '--config', f'{self.snapshot}/bsc/config/ingress.public.env']])
        self.assertFalse(list((self.w / 'run').glob('*.plist')))
        (self.w / 'refuse').unlink()
        code, out = self.sh('bsc_runtime_up')
        self.assertEqual(code, 0, out); self.assertIn('launchctl bootstrap gui/0', self.launchd())
        # A deploy checks the checkout's configuration it is about to sync, before the snapshot moves.
        self.sh('bsc_config_check', str(self.rt), f'{self.w}/repo/bsc/config/ingress.public.env')
        self.assertEqual(self.calls()[-1], ['config-check', '--config', f'{self.w}/repo/bsc/config/ingress.public.env'])

    def test_qualify_passes_only_its_evidence_flags_and_replaces(self):
        for args, why in [(['--root', '/x'], 'unknown flag --root'), (['--venue'], '--venue needs a value'),
                          (['--venue', '--out'], '--venue needs a value'), (['--runtime-binary', '/x'], 'unknown flag'), ([], 'bsc live status')]:
            code, out = self.sh('cmd_bsc', 'live', 'qualify', *args)
            self.assertEqual(code, 1, out); self.assertIn(why, out)
        self.assertEqual(self.calls(), [])
        code, out = self.sh('cmd_bsc', 'live', 'qualify', '--venue', 'BSC_VENUE_FLAP@ev/a.dbrec', '--risk-statement', 'two words')
        self.assertEqual(code, 0, out); self.assertIn('weiter: darkbotctl bsc sync', out)
        self.assertEqual(self.calls(), [['live', 'qualify', '--root', f'{self.w}/repo', '--runtime-binary', str(self.rt),
                                         '--venue', 'BSC_VENUE_FLAP@ev/a.dbrec', '--risk-statement', 'two words', '--replace']])
        code, out = self.sh('cmd_bsc', 'live', 'qualify', '--venue', 'X', env={'DARKBOT_STAGED': '1'})
        self.assertEqual(code, 1, out); self.assertIn('Original-Repository', out)

    def test_key_init_takes_an_absolute_path_and_the_checkout_as_the_repository(self):
        code, out = self.sh('cmd_bsc', 'live', 'key-init', 'relative.key')
        self.assertEqual(code, 1, out); self.assertIn('absolute path', out)
        code, out = self.sh('cmd_bsc', 'live', 'key-init', '/keys/live.key')
        self.assertEqual(code, 0, out)
        self.assertEqual(self.calls(), [['live', 'key', 'init', '--key', '/keys/live.key', '--root', f'{self.w}/repo']])

    def test_the_campaign_commands_read_the_snapshots_manifest(self):
        for cmd in ['attempts', 'campaign-precommit']:
            code, out = self.sh('cmd_bsc', 'live', cmd)
            self.assertEqual(code, 0, out)
            self.assertIn(f'{self.snapshot}/bsc/config/live_campaign.env', self.calls()[-1])
        self.assertEqual(self.calls()[-1][-2], '--decided-by')

    def test_status_names_the_runtime_what_live_holds_and_the_configuration(self):
        code, out = self.sh('bsc_status')
        self.assertEqual(code, 0, out)
        for line in ['bsc runtime: gestoppt', 'bsc reporter-socket: fehlt', 'bsc snapshot: fehlt', 'bsc live: nichts offen', 'bsc config: config=OK live=OFF']:
            self.assertIn(line, out)
        (self.w / 'refuse').touch()
        (self.w / 'run/bsc-sources').mkdir()
        code, out = self.sh('bsc_status')
        self.assertEqual(code, 0, out)
        self.assertIn('bsc live: STOP: refused', out); self.assertIn('bsc config: STOP: refused', out)

    def test_loopback_identity_and_adopt_take_their_arguments_strictly(self):
        code, out = self.sh('cmd_bsc', 'live', 'loopback-identity', 'drop', '/s')
        self.assertEqual(code, 1, out)
        code, out = self.sh('cmd_bsc', 'live', 'adopt-g07', 'g08.dbrec', 'cap', 'bad id!')
        self.assertEqual(code, 1, out); self.assertIn('letters, digits and hyphens', out)
        self.assertEqual(self.calls(), [])
        code, out = self.sh('cmd_bsc', 'live', 'loopback-identity', 'show', '/s')
        self.assertEqual(code, 0, out)
        self.assertEqual(self.calls(), [['live', 'loopback-identity', 'show', '--state', '/s', '--root', f'{self.w}/repo']])

if __name__ == '__main__':
    unittest.main()
