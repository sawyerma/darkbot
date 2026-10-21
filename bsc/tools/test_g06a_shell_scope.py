"""Isolated fake-key/typed-field regressions; never run the real DRY pipeline."""
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
HELPER = ROOT / "bsc/tools/g06a_shell_scope.sh"

class Scope(unittest.TestCase):
    def shell(self, code, env=None):
        return subprocess.run(["/bin/bash", "-c", 'set -euo pipefail; source "$1"; ' + code, "test", str(HELPER)],
            env={"PATH": os.environ["PATH"], "HOME": os.environ["HOME"], **(env or {})}, capture_output=True, text=True)

    def test_inherited_and_source_exported_keys_only_reach_explicit_signers(self):
        code = r'''clear_exports
export BSC_LAUNCHER_PRIVATE_KEY=FAKE_LAUNCHER_ONLY_FOR_TEST
export BSC_BUYER_PRIVATE_KEY=FAKE_BUYER_ONLY_FOR_TEST
export UNEXPECTED_TOKEN=FAKE_UNEXPECTED
run /bin/bash -c '[[ ! ${BSC_LAUNCHER_PRIVATE_KEY+x} && ! ${BSC_BUYER_PRIVATE_KEY+x} && ! ${DARKBOT_G06A_LAUNCHER_PRIVATE_KEY+x} && ! ${UNEXPECTED_TOKEN+x} ]]'
with_keys /bin/bash -c '[[ ${DARKBOT_G06A_LAUNCHER_PRIVATE_KEY+x} && ${DARKBOT_G06A_BUYER_PRIVATE_KEY+x} && ! ${BSC_LAUNCHER_PRIVATE_KEY+x} && ! ${UNEXPECTED_TOKEN+x} ]]'
with_keys /bin/bash -c 'exit 19' && exit 1 || [[ $? == 19 ]]
run /bin/bash -c '[[ ! ${DARKBOT_G06A_LAUNCHER_PRIVATE_KEY+x} ]]'
'''
        out=self.shell(code,{"DARKBOT_G06A_LAUNCHER_PRIVATE_KEY":"FAKE_INHERITED", "UNEXPECTED_TOKEN":"FAKE_INHERITED"})
        self.assertEqual(out.returncode,0,out.stderr)
        self.assertEqual(out.stdout,"")
        self.assertNotIn("FAKE_",out.stderr)

    def test_missing_failed_empty_and_zero_fields(self):
        for status,value,ok in [(0,"0",True),(0,"",False),(1,"",False),(2,"0",False),(3,"",False),(0,"garbage",False)]:
            code='get() { printf "%s" "$VALUE"; return "$STATUS"; }; v="$(get_required launcher_pending_nonce fixture)" || exit 23; [[ "$v" == 0 ]]'
            out=self.shell(code,{"VALUE":value,"STATUS":str(status)})
            self.assertEqual(out.returncode == 0,ok,(status,out.stderr))
        out=self.shell('get() { return 3; }; left="$(get_required salt one)" || exit 23; right="$(get_required salt two)" || exit 23; [[ "$left" == "$right" ]]')
        self.assertEqual(out.returncode,23)

    def test_sha_missing_invalid_and_valid(self):
        for digest,code,ok in [("a"*64,0,True),("z"*64,0,False),("",0,False),("a"*64,1,False)]:
            script='run() { if [[ "$1" == shasum ]]; then printf "%s  fixture\\n" "$DIGEST"; return "$RC"; else command "$@"; fi; }; value="$(required_sha256 fixture)" || exit 24'
            out=self.shell(script,{"DIGEST":digest,"RC":str(code)})
            self.assertEqual(out.returncode==0,ok,out.stderr)

    def test_legacy_tools_stop_before_credentials_or_network(self):
        for tool in ["g07_find_min_executable_buy.sh","g07_min_spend_probe.sh"]:
            out=subprocess.run(["/bin/bash",str(ROOT/"bsc/tools"/tool)],env={"PATH":"/no-tools"},capture_output=True,text=True)
            self.assertEqual(out.returncode,2)
            self.assertIn("SUPERSEDED_INCOMPATIBLE_WITH_V4_DBREC",out.stderr)

    def test_required_reads_and_signing_scopes_in_real_harness(self):
        text=(ROOT/"bsc/tools/g06a_final_dry.sh").read_text()
        self.assertNotIn('$(get ',text)
        self.assertNotIn('export ',text)
        self.assertEqual(text.count('with_keys '),2)
        self.assertIn('with_keys node bsc/tools/g06a_local_simulate.cjs',text)
        self.assertIn('with_keys "$ENGINE/target/debug/g06a_self_launch_bundle"',text)
        for line in text.splitlines():
            if '$(get_required ' in line or '$(required_sha256 ' in line:
                self.assertIn('|| die ',line)
        source=(ROOT/"bsc/engine/crates/qualification/src/bin/g06a_self_launch_bundle.rs").read_text()
        for forbidden in ['std::process::Command', 'Command::new(', 'TcpStream::connect', 'UdpSocket::bind', 'eth_sendRawTransaction']:
            self.assertTrue(forbidden not in source, forbidden)
        self.assertIn('DRY_STRUCTURAL_SCOPE_NOTE',source)
        # Historical v2 field remains a schema-compatible historical value.
        self.assertIn('budget.v2_proposal_per_attempt_ceiling_wei',source)

if __name__ == "__main__": unittest.main()
