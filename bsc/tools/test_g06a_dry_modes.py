"""Orchestration regressions of g06a_final_dry.sh (plan v9.1 D2-D6).

The real harness and shell scope run in a temporary tree whose tools are
recording stand-ins: text "records" (name=value lines), synthetic values and
keys, no network, no signature. Each stand-in logs its argv and environment,
so the tests check what every child of the harness received. They never
qualify anything; the typed Rust checks have their own tests.
"""
import os
import pathlib
import shutil
import stat
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
FROZEN_REL = "bsc/native/evidence/g06a/g07_binary_import_20261016"
A_SALT = "0x" + "a5" * 32
A_TOKEN = "0x" + "aa" * 18 + "7777"
B_SALT = "0x" + "b5" * 32
B_TOKEN = "0x" + "bb" * 18 + "7777"
B_COMMITMENT = "0x" + "bc" * 32
ID = "0x" + "1d" * 32
OUTPUT = "176693134169499757962"
TEST_KEY = "0x" + "0" * 63 + "1"
RPC = "https://rpc.example.invalid/CREDENTIAL_TOKEN"

STUB_HEAD = """#!/bin/sh
LOG={log}
n=$(ls "$LOG" | wc -l | tr -d ' ')
printf '%s\\n' "{name}" "$@" > "$LOG/$n.{name}.argv"
env > "$LOG/$n.{name}.env"
S={scenario}
val() { grep "^$1=" "$S" | head -1 | cut -d= -f2-; }
"""

STUBS = {
    "cargo": "exit 0\n",
    "git": "exit 0\n",
    "dbrec_show": """if [ "$1" = "--get" ]; then
  v=$(grep "^$2=" "$3" | head -1 | cut -d= -f2-); [ -n "$v" ] || exit 1; printf '%s' "$v"
else cat "$1"; fi
""",
    "dbrec_compare": "exit 0\n",
    "g06a_preflight_record": """while [ $# -gt 0 ]; do case "$1" in --out) out=$2; shift;; --upgrade-out) up=$2; shift;; --from) from=$2; shift;; esac; shift; done
if [ -n "$from" ]; then printf 'scanned_from_block=%s\\nscanned_to_block=%s\\nheaders_verified=7\\nbloom_candidate_blocks=0\\n' "$from" "$(val head)" > "$out"; exit 0; fi
printf 'latest_block_number=%s\\nportal_implementation=0x7a94f37b61a82c395f9812cf266b444783cc5e8c\\nwallet_qualification.addresses_bound=true\\nwallet_qualification.distinct=true\\nwallet_qualification.funding_pass=true\\nlauncher_pending_nonce=0\\nbuyer_pending_nonce=0\\n' "$(val head)" > "$out"
printf 'scanned_from_block=127865761\\nscanned_to_block=%s\\nheaders_verified=7\\nbloom_candidate_blocks=0\\nupgrades[0].block = 1\\n' "$(val head)" > "$up"
""",
    "g06a_native_bnb_quote": "printf 'expected_output=%s\\n' \"$(val output)\" > \"$2\"\n",
    "g06a_builder_policy_record": "printf 'strictest_explicit_verified_floor_wei=50000000\\n' > \"$2\"\n",
    "g06a_self_launch_bundle": """printf 'dry_pass.DRY_1_bytes_and_signers=true\\ndry_pass.DRY_2_independent_local_simulation=true\\ndry_pass.DRY_3_gas_floor_and_budget=true\\ndry_pass.DRY_4_expiry_nonces_zero_egress=true\\ndry_pass.all=true\\nnetwork_write_capability=NONE_IN_THIS_BINARY\\ntransactions.launch_tx_hash=%s\\ntransactions.buy_tx_hash=%s\\ntransactions.token=%s\\ntransactions.min_output=1\\n' "$(val launch)" "$(val buy)" "$(val dry_token)" > "$4"
""",
    "g06a_requal_check": "[ \"$(val requal)\" = STOP ] && { echo FLAP_REQUALIFICATION=STOP; exit 1; }\necho FLAP_REQUALIFICATION=PASS\nexit 0\n",
    "g06a_dry_handoff": """case "$1" in
selection) [ "$3" = "$(val selected)" ] || { echo "STOP: selected $(val selected)" >&2; exit 2; }
  printf 'profile=%s\\ncandidate_id=%s\\ngeneration=4\\ntoken=%s\\nsalt_commitment=%s\\n' "$3" "{id}" "$(val token)" "$(val commitment)";;
exec-salt) while [ "$1" != "--" ]; do shift; done; shift
  exec env DARKBOT_G06A_SALT="{b_salt}" "$@";;
prepare) [ "$(val handoff)" = "refuse" ] && { echo "STOP: R0 record is not coherent" >&2; exit 2; }
  printf 'mode=PREPARE_ONLY\nprofile=%s\n' "$5" > "$3/prepare_handoff.dbrec";;
verify) [ "$(val verify)" = "refuse" ] && { echo "STOP: the R0 record is not the one the handoff binds" >&2; exit 2; }
  p=$3; o=$7; mkdir -m 700 "$o" || exit 2
  cp "$p/portal_upgrade.dbrec" "$o/portal_upgrade_scan.dbrec"
  printf 'token=%s\nsalt_commitment=%s\n' "$(val token)" "$(val commitment)" > "$o/candidate.dbrec"
  to=$(grep '^scanned_to_block=' "$p/portal_upgrade.dbrec" | cut -d= -f2)
  printf 'profile=%s\ncandidate_id={id}\ngeneration=4\nr0_to_block=%s\n' "$(val selected)" "$to";;
*) exit 2;;
esac
""",
    "node": """case "$1" in
*crosscheck.cjs) if [ "$4" = "--private" ]; then
    printf 'status=EVM_SEQUENCE_PASS\\nsimulated_received_amount=%s\\nsalt_commitment=%s\\ntoken=%s\\n' "$(val econ_output)" "$(val econ_commitment)" "$(val econ_token)" > "$3"
  else
    printf 'status=EVM_SEQUENCE_PASS\\nsimulated_received_amount=%s\\nsalt=%s\\ntoken=%s\\n' "$(val econ_output)" "{a_salt}" "$(val econ_token)" > "$3"
  fi;;
*trace.cjs) printf 'launch_success=true\\nbuy_success=true\\n' > "$3";;
*simulate.cjs) printf 'launcher=0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd\\nbuyer=0x9c8a0db41914beee52dd49790c4f087e252743de\\noutput_difference=0\\nlaunch_success=true\\nbuy_success=true\\n' > "$2";;
*) exit 9;;
esac
""",
}


class World:
    """A temporary tree with the real harness and recording stand-ins."""

    def __init__(self, scenario, secrets=None):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="g06a-modes-"))
        self.root = self.dir / "repo"
        self.log = self.dir / "log"
        self.log.mkdir()
        tools = self.root / "bsc/tools"
        tools.mkdir(parents=True)
        for name in ["g06a_final_dry.sh", "g06a_shell_scope.sh"]:
            shutil.copy(ROOT / "bsc/tools" / name, tools / name)
        guard = self.root / "bsc/engine/crates/execution/src/acceptance_guard.rs"
        guard.parent.mkdir(parents=True)
        guard.write_text('pub const FROZEN_APPROVED_V5_SHA256: &str = "UNARMED";\n')
        frozen = self.root / FROZEN_REL
        frozen.mkdir(parents=True)
        (frozen / "economics_crosscheck.dbrec").write_text(f"salt={A_SALT}\n")
        (frozen / "preflight.dbrec").write_text("latest_block_number=127865760\n")
        (frozen / "builder_policy.dbrec").write_text("strictest_explicit_verified_floor_wei=50000000\n")
        (frozen / "dry_result.dbrec").write_text(
            "transactions.launch_tx_hash=0x" + "1a" * 32 + "\ntransactions.buy_tx_hash=0x" + "2a" * 32
            + f"\ntransactions.token={A_TOKEN}\ntransactions.min_output=1\n")
        values = {"head": "129400000", "output": OUTPUT, "econ_output": OUTPUT, "selected": "SELF_A",
                  "token": A_TOKEN, "econ_token": A_TOKEN, "commitment": B_COMMITMENT,
                  "econ_commitment": B_COMMITMENT, "launch": "0x" + "1a" * 32, "buy": "0x" + "2a" * 32,
                  "dry_token": A_TOKEN}
        values.update(scenario)
        self.scenario = self.dir / "scenario"
        self.scenario.write_text("".join(f"{k}={v}\n" for k, v in values.items()))
        self.bin = self.dir / "bin"
        debug = self.root / "bsc/engine/target/debug"
        for d in [self.bin, debug]:
            d.mkdir(parents=True, exist_ok=True)
        for name, body in STUBS.items():
            target = (self.bin if name in ("cargo", "git", "node") else debug) / name
            text = STUB_HEAD + body
            for key, value in [("{log}", self.log), ("{name}", name), ("{scenario}", self.scenario),
                               ("{id}", ID), ("{b_salt}", B_SALT), ("{a_salt}", A_SALT)]:
                text = text.replace(key, str(value))
            target.write_text(text)
            target.chmod(0o755)
        self.secrets = self.root / "bsc/secrets.env"
        if secrets is not None:
            self.secrets.write_text(secrets)
            self.secrets.chmod(0o600)

    def set(self, **values):
        lines = [l for l in self.scenario.read_text().splitlines() if l.split("=")[0] not in values]
        self.scenario.write_text("".join(f"{l}\n" for l in lines) + "".join(f"{k}={v}\n" for k, v in values.items()))

    def run(self, *args, env=None):
        base = {"PATH": f"{self.bin}:/usr/bin:/bin", "HOME": str(self.dir)}
        base.update(env or {})
        return subprocess.run(["/bin/bash", str(self.root / "bsc/tools/g06a_final_dry.sh"), *args],
                              env=base, capture_output=True, text=True, timeout=120)

    def calls(self):
        """(tool, argv, env) of every stand-in call, in order."""
        out = []
        for argv in sorted(self.log.glob("*.argv"), key=lambda p: int(p.name.split(".")[0])):
            lines = argv.read_text().splitlines()
            env = argv.with_suffix(".env").read_text()
            out.append((lines[0], lines[1:], env))
        return out

    def outputs_holding(self, needle):
        """Every evidence or SIGN output file whose bytes contain `needle`."""
        hits = []
        for tree in [self.root / "bsc/native/evidence", self.root / "bsc/.local/g06a_sign"]:
            for path in tree.rglob("*") if tree.exists() else []:
                if path.is_file() and needle.encode() in path.read_bytes():
                    hits.append(path)
        return hits

    def close(self):
        if self.secrets.exists():
            self.secrets.chmod(0o600)
        shutil.rmtree(self.dir)


FULL_SECRETS = (f"BSC_LAUNCHER_PRIVATE_KEY={TEST_KEY}\nBSC_BUYER_PRIVATE_KEY={TEST_KEY[:-1]}2\n"
                f"BSC_MAINNET_RPC_HTTPS={RPC}\n")


class Prepare(unittest.TestCase):
    def world(self, scenario=None, secrets=None):
        w = World(scenario or {}, secrets)
        self.addCleanup(w.close)
        return w

    def assert_no_secret_to_ordinary_children(self, w, salt, salt_consumers):
        for tool, argv, env in w.calls():
            consumer = tool == "node" and any(s in argv[0] for s in salt_consumers)
            self.assertNotIn("PRIVATE_KEY", env, (tool, argv))
            self.assertEqual(f"DARKBOT_G06A_SALT={salt}" in env, consumer, (tool, argv))
            self.assertNotIn(RPC, " ".join(argv), (tool, argv))

    def test_prepare_a_is_key_free_and_never_reads_the_secrets_file(self):
        w = self.world(secrets="exit 99\n")
        w.secrets.chmod(0)  # unreadable: any read would fail the run
        out = w.run("--prepare", env={"BSC_MAINNET_RPC_HTTPS": RPC})
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertIn("MODE=PREPARE_ONLY  REQUALIFICATION=NOT_RUN", out.stdout)
        self.assertNotIn("FLAP_REQUALIFICATION=PASS", out.stdout)
        self.assertNotIn("CREDENTIAL_TOKEN", out.stdout + out.stderr)
        tools = [c[0] for c in w.calls()]
        for absent in ["git", "g06a_self_launch_bundle", "g06a_requal_check", "dbrec_compare"]:
            self.assertNotIn(absent, tools)
        self.assertFalse(any("simulate.cjs" in c[1][0] for c in w.calls() if c[0] == "node"))
        # A's public salt reaches only the key-free launch-path trace.
        self.assert_no_secret_to_ordinary_children(w, A_SALT, ["trace.cjs"])
        runs = list((w.root / "bsc/native/evidence/g06a").glob("g06a_prepare_*"))
        self.assertEqual(len(runs), 1)
        self.assertIn("PREPARE_ONLY / NOT_REQUALIFIED", (runs[0] / "PREPARE_ONLY.txt").read_text())
        self.assertTrue((runs[0] / "SHA256SUMS").exists())
        self.assertTrue((runs[0] / "prepare_handoff.dbrec").exists())
        self.assertIn("prepare_handoff.dbrec", (runs[0] / "SHA256SUMS").read_text())
        self.assertFalse((runs[0] / "dry_result.dbrec").exists())
        self.assertFalse((runs[0] / "local_simulation.dbrec").exists())

    def test_prepare_without_a_secrets_file_needs_only_the_rpc_endpoint(self):
        w = self.world()
        self.assertEqual(w.run("--prepare", env={"BSC_MAINNET_RPC_HTTPS": RPC}).returncode, 0)
        missing = self.world().run("--prepare")
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn("PREPARE reads no key file", missing.stderr)

    def test_prepare_b_gives_the_salt_only_to_its_two_key_free_consumers(self):
        w = self.world({"selected": "SELF_B", "token": B_TOKEN, "econ_token": B_TOKEN})
        out = w.run("--prepare", "--profile", "B", env={"BSC_MAINNET_RPC_HTTPS": RPC})
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assert_no_secret_to_ordinary_children(w, B_SALT, ["crosscheck.cjs", "trace.cjs"])
        cross = [c for c in w.calls() if c[0] == "node" and "crosscheck.cjs" in c[1][0]]
        self.assertEqual(cross[0][1][-1], "--private")
        self.assertNotIn(B_SALT[2:], out.stdout + out.stderr)
        handed = [c for c in w.calls() if c[0] == "g06a_dry_handoff" and c[1][0] == "exec-salt"]
        self.assertEqual(len(handed), 2)
        self.assertEqual(w.outputs_holding(B_SALT[2:]), [])

    def test_prepare_b_stops_on_a_wrong_selection_commitment_or_token(self):
        for scenario, message in [
            ({"selected": "SELF_A"}, "no verified selected SELF_B candidate"),
            ({"selected": "SELF_B", "token": B_TOKEN, "econ_token": B_TOKEN, "econ_commitment": "0x" + "cd" * 32},
             "not of the selected candidate's salt"),
            ({"selected": "SELF_B", "token": B_TOKEN, "econ_token": A_TOKEN}, "not the selected candidate's"),
        ]:
            w = self.world(scenario)
            out = w.run("--prepare", "--profile", "B", env={"BSC_MAINNET_RPC_HTTPS": RPC})
            self.assertNotEqual(out.returncode, 0, scenario)
            self.assertIn(message, out.stderr, scenario)
            self.assertFalse(any(c[0] == "node" and "trace.cjs" in c[1][0] for c in w.calls()) and "selected" in message)

    def test_prepare_without_a_valid_handoff_does_not_complete(self):
        w = self.world({"handoff": "refuse"})
        out = w.run("--prepare", env={"BSC_MAINNET_RPC_HTTPS": RPC})
        self.assertNotEqual(out.returncode, 0)
        self.assertIn("cannot be handed off", out.stderr)
        self.assertNotIn("MODE=PREPARE_ONLY", out.stdout)

    def test_prepare_stops_on_another_output_than_the_quote(self):
        w = self.world({"econ_output": "1"})
        out = w.run("--prepare", env={"BSC_MAINNET_RPC_HTTPS": RPC})
        self.assertIn("quote and independent EVM output differ", out.stderr)

    def test_unknown_arguments_are_refused_before_any_tool(self):
        for args in [["--prepare", "--profile", "C"], ["--sign"], ["--sign", ""], ["--prepare", "B"], ["x"]]:
            w = self.world()
            out = w.run(*args, env={"BSC_MAINNET_RPC_HTTPS": RPC})
            self.assertEqual(out.returncode, 2, args)
            self.assertEqual(w.calls(), [], args)


class Full(unittest.TestCase):
    def test_no_mode_and_unflagged_full_or_sign_never_read_keys_or_sign(self):
        # BSC-001 (P2-D4): a call without arguments must not sign; FULL and
        # SIGN both need the explicit signing flag before any read.
        for args, env in [([], {}), (["--full"], {}), (["--full"], {"DARKBOT_G06A_SIGN": "no"}),
                          (["--sign", "/nonexistent"], {})]:
            w = World({}, FULL_SECRETS)
            self.addCleanup(w.close)
            w.secrets.chmod(0)
            out = w.run(*args, env=env)
            self.assertEqual(out.returncode, 2, (args, out.stdout + out.stderr))
            self.assertEqual(w.calls(), [], args)
            self.assertNotIn(TEST_KEY, out.stdout + out.stderr)

    def test_full_mode_keeps_the_a_sequence_and_scopes_keys_and_the_public_salt(self):
        w = World({}, FULL_SECRETS)
        self.addCleanup(w.close)
        out = w.run("--full", env={"DARKBOT_G06A_SIGN": "YES"})
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertIn("FLAP_REQUALIFICATION=PASS", out.stdout)
        for tool, argv, env in w.calls():
            signing = tool == "g06a_self_launch_bundle" or (tool == "node" and "simulate.cjs" in argv[0])
            salted = signing or (tool == "node" and "trace.cjs" in argv[0])
            self.assertEqual("DARKBOT_G06A_LAUNCHER_PRIVATE_KEY=" in env, signing, (tool, argv))
            self.assertNotIn("BSC_LAUNCHER_PRIVATE_KEY", env, (tool, argv))
            self.assertEqual(f"DARKBOT_G06A_SALT={A_SALT}" in env, salted, (tool, argv))
        self.assertNotIn(TEST_KEY, out.stdout + out.stderr)


class Sign(unittest.TestCase):
    """SIGN at H2 from a PREPARE run (D3/D4), synthetic keys only."""

    def prepared(self, scenario=None, profile="A"):
        w = World(scenario or {}, FULL_SECRETS)
        self.addCleanup(w.close)
        args = ["--prepare"] + (["--profile", "B"] if profile == "B" else [])
        out = w.run(*args, env={"BSC_MAINNET_RPC_HTTPS": RPC})
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        run = next((w.root / "bsc/native/evidence/g06a").glob("g06a_prepare_*"))
        before = {p.name: p.read_bytes() for p in run.iterdir()}
        w.calls_before = len(w.calls())
        w.evidence_before = sorted(p.name for p in (w.root / "bsc/native/evidence/g06a").iterdir())
        w.set(head="129500000")  # H2
        return w, run, before

    def sign(self, w, run, **env):
        return w.run("--sign", str(run), env={"DARKBOT_G06A_SIGN": "YES", **env})

    def test_sign_needs_the_explicit_authorization_flag_before_any_read(self):
        w, run, _ = self.prepared()
        out = w.run("--sign", str(run))
        self.assertEqual(out.returncode, 2)
        self.assertIn("AWAITING_USER_AUTHORIZATION", out.stderr)
        self.assertEqual(len(w.calls()), w.calls_before)

    def test_sign_a_recomputes_everything_at_h2_in_a_new_private_dir(self):
        w, run, before = self.prepared()
        out = self.sign(w, run)
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertIn("SIGNED_BYTES=IDENTICAL_TO_FROZEN_DRY", out.stdout)
        signs = list((w.root / "bsc/.local/g06a_sign").glob("g06a_sign_*"))
        self.assertEqual(len(signs), 1)
        sign = signs[0]
        self.assertEqual(stat.S_IMODE(sign.stat().st_mode), 0o700)
        # Only PREPARE wrote evidence; the PREPARE run is unchanged.
        self.assertEqual(sorted(p.name for p in (w.root / "bsc/native/evidence/g06a").iterdir()), w.evidence_before)
        self.assertEqual({p.name: p.read_bytes() for p in run.iterdir()}, before)
        calls = w.calls()[w.calls_before:]
        dry0 = [c for c in calls if c[0] == "g06a_preflight_record"]
        self.assertEqual(len(dry0), 1)
        self.assertIn(str(sign / "portal_upgrade_scan.dbrec"), dry0[0][1])
        self.assertIn("latest_block_number=129500000", (sign / "preflight.dbrec").read_text())
        # Every H2 step reads the fresh H2 preflight, never PREPARE's records.
        for tool, argv, _ in calls:
            self.assertFalse(any(str(run) in a for a in argv if not a.endswith("prepare_dir")) and tool != "g06a_dry_handoff", (tool, argv))
        for name in ["native_quote.dbrec", "economics_crosscheck.dbrec", "builder_policy.dbrec", "launch_path.dbrec",
                     "local_simulation.dbrec", "dry_result.dbrec", "SHA256SUMS"]:
            self.assertTrue((sign / name).exists(), name)
        requal = [c for c in calls if c[0] == "g06a_requal_check"]
        self.assertNotIn("--candidate", requal[0][1])
        for tool, argv, env in calls:
            signing = tool == "g06a_self_launch_bundle" or (tool == "node" and "simulate.cjs" in argv[0])
            self.assertEqual("PRIVATE_KEY=" in env, signing, (tool, argv))

    def test_sign_b_keeps_the_salt_in_the_handoff_and_binds_the_candidate(self):
        w, run, _ = self.prepared({"selected": "SELF_B", "token": B_TOKEN, "econ_token": B_TOKEN,
                                   "dry_token": B_TOKEN}, profile="B")
        out = self.sign(w, run)
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertIn("OWN_SELF_B_BYTES", out.stdout)
        calls = w.calls()[w.calls_before:]
        for tool, argv, env in calls:
            consumer = tool == "g06a_self_launch_bundle" or (tool == "node")
            signing = tool == "g06a_self_launch_bundle" or (tool == "node" and "simulate.cjs" in argv[0])
            self.assertEqual(f"DARKBOT_G06A_SALT={B_SALT}" in env, consumer, (tool, argv))
            self.assertEqual("DARKBOT_G06A_LAUNCHER_PRIVATE_KEY=" in env, signing or (tool == "g06a_dry_handoff" and "--" in argv and any("simulate" in a or "bundle" in a for a in argv)), (tool, argv))
        requal = [c for c in calls if c[0] == "g06a_requal_check"][0]
        self.assertEqual(requal[1][-2:], ["--candidate", requal[1][0] + "/candidate.dbrec"])
        self.assertNotIn(B_SALT[2:], out.stdout + out.stderr)
        self.assertEqual(w.outputs_holding(B_SALT[2:]), [])

    def test_sign_stops_on_h2_drift_a_stale_head_a_bad_handoff_or_a_requal_stop(self):
        cases = [
            ({"econ_output": "1"}, "quote and independent EVM output differ"),
            ({"head": "129400000"}, "fresh head H2"),
            ({"verify": "refuse"}, "does not verify"),
            ({"requal": "STOP"}, "FLAP_REQUALIFICATION=STOP"),
        ]
        for values, message in cases:
            w, run, before = self.prepared()
            w.set(**values)
            out = self.sign(w, run)
            self.assertNotEqual(out.returncode, 0, values)
            self.assertIn(message, out.stderr, values)
            self.assertEqual({p.name: p.read_bytes() for p in run.iterdir()}, before)
            if "verify" in values:
                self.assertFalse(any(c[0] == "g06a_preflight_record" for c in w.calls()[w.calls_before:]))


if __name__ == "__main__":
    unittest.main()
