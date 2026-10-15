#!/usr/bin/env python3
"""Tests of the C1.9, C1.9b and C1.10 tooling logic.

Every input here is synthetic and built inside the test. The tests check
that the tools compute and refuse correctly; they produce no evidence and no
measurement.

    python3 -m unittest test_c1_qualification_tools -v     (in bsc/tools)
"""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import c19_analyze  # noqa: E402
import c19b_hook_path  # noqa: E402
import c1_freeze  # noqa: E402


def ingress_rows(peers=2, warmup=2, samples=3, l2_gap=7_000):
    """A consistent run: lanes 16.., one row per sample, in file order."""
    rows, index = [], 0
    for sample in range(warmup + samples):
        for peer in range(peers):
            base = 1_000_000 + index * 100_000
            rows.append({
                "index": index, "phase": "warmup" if sample < warmup else "qualification",
                "lane_id": 16 + peer, "lane_sequence": sample, "tx_hash": f"{index:064x}",
                "decoded": base, "ingress": base + 1_000, "l1_pre": base + 3_000, "l1_post": base + 3_500,
                "l2": base + 1_000 + l2_gap, "queue_depth": 1,
            })
            index += 1
    return rows


def manifest(peers=2, warmup=2, samples=3, workers=2):
    return {"peers": str(peers), "warmup_per_peer": str(warmup), "samples_per_peer": str(samples),
            "lane_ids": ",".join(str(16 + peer) for peer in range(peers)), "clock_id": "1",
            "outbound_workers": str(workers)}


def outbound_rows(ingress, workers=2, statuses=None):
    rows = []
    for row in ingress:
        commit = row["l2"] + 500
        slot_statuses = (statuses or {}).get(row["index"], ["HANDED_OFF"] * workers)
        handed = sum(1 for status in slot_statuses if status == "HANDED_OFF")
        rows.append({"tx_hash": "b" * 64, "egress_sequence": row["index"], "commit": commit,
                     "origin": "Coordinator", "worker": -1, "status": "DISPATCHED", "selected": handed,
                     "queued": handed, "observed": commit + 2_000, "third": commit, "event": commit + 2_000,
                     "peer": "0" * 64})
        for worker, status in enumerate(slot_statuses):
            done = commit + 10_000 * (worker + 1)
            rows.append({"tx_hash": "b" * 64, "egress_sequence": row["index"], "commit": commit,
                         "origin": "Worker", "worker": worker, "status": status, "selected": handed,
                         "queued": handed, "observed": commit + 2_000, "third": done - 4_000, "event": done,
                         "peer": f"{worker:064x}"})
    return rows


class DescribeTest(unittest.TestCase):
    def test_max_is_reported_and_a_miss_starts_at_exactly_one_millisecond(self):
        stats = c19_analyze.describe([10, 999_999, 1_000_000, 5])
        self.assertEqual((stats["min_ns"], stats["max_ns"], stats["samples"]), (5, 1_000_000, 4))
        self.assertEqual(stats["misses_ge_1ms"], 1)
        self.assertEqual(stats["bands"], {"lt_200us": 2, "200us_to_lt_500us": 0, "500us_to_lt_1ms": 1, "ge_1ms": 1})

    def test_extreme_percentiles_need_enough_samples(self):
        small = c19_analyze.describe(list(range(1, 1000)))
        self.assertIsNone(small["p99_9_ns"])
        self.assertIsNone(small["p99_99_ns"])
        self.assertIn("p99_9_note", small)
        medium = c19_analyze.describe(list(range(1, 1001)))
        self.assertEqual(medium["p99_9_ns"], 999)
        self.assertIsNone(medium["p99_99_ns"])
        large = c19_analyze.describe(list(range(1, 10_001)))
        self.assertEqual((large["p99_99_ns"], large["max_ns"]), (9_999, 10_000))
        self.assertEqual((large["p50_ns"], large["p99_ns"]), (5_000, 9_900))

    def test_no_samples_is_stated(self):
        self.assertEqual(c19_analyze.describe([])["samples"], 0)


class IngressIntegrityTest(unittest.TestCase):
    def check(self, rows, **kwargs):
        failures = []
        window = c19_analyze.check_ingress(rows, manifest(**kwargs), failures)
        return window, failures

    def test_a_consistent_run_yields_the_fixed_window(self):
        window, failures = self.check(ingress_rows())
        self.assertEqual(failures, [])
        self.assertEqual(len(window), 6)
        self.assertTrue(all(row["phase"] == "qualification" for row in window))

    def test_a_gap_a_moved_window_and_a_reordered_boundary_fail(self):
        gap = ingress_rows()
        gap[4]["lane_sequence"] = 9
        self.assertTrue(any("gap or reorder" in f for f in self.check(gap)[1]))
        moved = ingress_rows()
        moved[2]["phase"] = "qualification"
        self.assertTrue(any("window fixes warmup" in f for f in self.check(moved)[1]))
        late = ingress_rows()
        late[3]["decoded"] = late[3]["ingress"] + 1
        self.assertTrue(any("boundary order" in f for f in self.check(late)[1]))
        missing = ingress_rows()[:-1]
        self.assertTrue(any("manifest fixes" in f for f in self.check(missing)[1]))
        duplicate = ingress_rows()
        duplicate[1]["tx_hash"] = duplicate[0]["tx_hash"]
        self.assertTrue(any("duplicate" in f for f in self.check(duplicate)[1]))
        failures = []
        c19_analyze.check_ingress(ingress_rows(), dict(manifest(), clock_id="2"), failures)
        self.assertTrue(any("CLOCK_MONOTONIC" in f for f in failures))


class OutboundMetricsTest(unittest.TestCase):
    def metrics(self, statuses=None, drop=None):
        ingress = ingress_rows()
        failures = []
        window = c19_analyze.check_ingress(ingress, manifest(), failures)
        rows = outbound_rows(ingress, statuses=statuses)
        if drop is not None:
            rows = [row for row in rows if drop(row)]
        return c19_analyze.outbound_metrics(rows, ingress, window, 2, failures), failures

    def test_first_each_and_all_handoffs_are_separate_measurements(self):
        metrics, failures = self.metrics()
        self.assertEqual(failures, [])
        self.assertEqual(metrics["buys_in_window"], 6)
        first = metrics["egress_commit_to_first_selected_peer_handoff"]
        each = metrics["egress_commit_to_each_selected_peer_handoff"]
        everyone = metrics["egress_commit_to_all_selected_peer_handoffs"]
        self.assertEqual((first["samples"], first["max_ns"]), (6, 10_000))
        self.assertEqual((each["samples"], each["max_ns"]), (12, 20_000))
        self.assertEqual((everyone["samples"], everyone["min_ns"]), (6, 20_000))
        self.assertEqual(metrics["worker_send_call_duration"]["max_ns"], 4_000)
        # T_ingress -> first handoff is measured per sample, not summed.
        direct = metrics["prepared_buy_path"]["T_ingress_to_first_selected_peer_handoff"]
        self.assertEqual(direct["max_ns"], 7_000 + 500 + 10_000)

    def test_a_failed_peer_is_a_partial_handoff_not_a_missing_sample(self):
        failed = {4: ["SEND_ERROR_UNKNOWN", "HANDED_OFF"]}
        metrics, failures = self.metrics(statuses=failed)
        self.assertEqual(failures, [])
        self.assertEqual(metrics["worker_slot_outcomes"], {"HANDED_OFF": 11, "SEND_ERROR_UNKNOWN": 1})
        self.assertEqual(metrics["buys_without_all_selected_handoffs"], 0)
        self.assertEqual(metrics["egress_commit_to_first_selected_peer_handoff"]["max_ns"], 20_000)

    def test_missing_evidence_is_an_integrity_failure(self):
        _, failures = self.metrics(drop=lambda row: not (row["egress_sequence"] == 5 and row["worker"] == 1))
        self.assertTrue(any("every worker slot" in f for f in failures))
        _, failures = self.metrics(drop=lambda row: row["egress_sequence"] != 6)
        self.assertTrue(any("no outbound evidence" in f for f in failures))


class HookPathTest(unittest.TestCase):
    def launches(self, **overrides):
        data = {"chain_id": 97, "clock_id": 1, "host_boot_id": "boot-1",
                "handoff_boundary": "return of the launcher's raw transaction send",
                "launches": [{"tx_hash": "0x" + "a" * 64, "handoff_ns": 1_000},
                             {"tx_hash": "b" * 64, "handoff_ns": 5_000},
                             {"tx_hash": "c" * 64, "handoff_ns": 9_000}]}
        data.update(overrides)
        return data

    def write(self, directory, launches, rows):
        launch_path = Path(directory) / "launches.json"
        launch_path.write_text(json.dumps(launches))
        csv_path = Path(directory) / "observations.csv"
        header = "tx_hash,peer,source,t_packet_decoded_ns,t_ingress_ns,clock_id,host_boot_id\n"
        csv_path.write_text(header + "".join(",".join(str(v) for v in row) + "\n" for row in rows))
        return launch_path, csv_path

    def test_sources_are_compared_and_misses_are_counted(self):
        rows = [("a" * 64, "p1", "eth-pooled", 1_300, 1_400, 1, "boot-1"),
                ("a" * 64, "p2", "eth-broadcast", 1_100, 1_150, 1, "boot-1"),
                ("b" * 64, "p1", "eth-pooled", 5_200, 5_260, 1, "boot-1")]
        with tempfile.TemporaryDirectory() as directory:
            launch_path, csv_path = self.write(directory, self.launches(), rows)
            data = c19b_hook_path.load_launches(launch_path)
            report = c19b_hook_path.analyze(data, c19b_hook_path.load_observations(csv_path, "boot-1"))
        self.assertEqual((report["launches"], report["launches_observed"], report["launches_missed"]), (3, 2, 1))
        self.assertEqual(report["missed_tx_hashes"], ["c" * 64])
        self.assertEqual(report["handoff_to_earliest_T_ingress"]["max_ns"], 260)
        self.assertEqual(report["broadcast_vs_pooled"]["eth-broadcast"]["first_for_launches"], 1)
        self.assertEqual(report["broadcast_vs_pooled"]["eth-pooled"]["first_for_launches"], 1)
        pooled = next(s for s in report["per_source_and_peer"] if s["peer"] == "p1")
        self.assertEqual((pooled["launches_observed"], pooled["launches_not_observed"]), (2, 0))
        self.assertEqual(pooled["T_ingress_behind_first_source"]["max_ns"], 250)
        self.assertIn("MAINNET_INGRESS_COVERAGE = NOT_PROVEN", report["non_claims"])

    def test_mainnet_and_incomparable_clocks_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            for bad in (self.launches(chain_id=56), self.launches(clock_id=2), self.launches(host_boot_id=""),
                        self.launches(handoff_boundary=""), self.launches(launches=[])):
                launch_path, _ = self.write(directory, bad, [])
                with self.assertRaises(c19b_hook_path.Refused):
                    c19b_hook_path.load_launches(launch_path)
            for row in (("a" * 64, "p1", "eth-pooled", 10, 20, 2, "boot-1"),
                        ("a" * 64, "p1", "eth-pooled", 10, 20, 1, "boot-2"),
                        ("a" * 64, "p1", "txpool", 10, 20, 1, "boot-1"),
                        ("a" * 64, "p1", "eth-pooled", 30, 20, 1, "boot-1")):
                _, csv_path = self.write(directory, self.launches(), [row])
                with self.assertRaises(c19b_hook_path.Refused):
                    c19b_hook_path.load_observations(csv_path, "boot-1")

    def test_a_launch_seen_before_its_handoff_is_excluded_not_measured(self):
        rows = [("a" * 64, "p1", "eth-pooled", 900, 950, 1, "boot-1")]
        with tempfile.TemporaryDirectory() as directory:
            launch_path, csv_path = self.write(directory, self.launches(), rows)
            data = c19b_hook_path.load_launches(launch_path)
            report = c19b_hook_path.analyze(data, c19b_hook_path.load_observations(csv_path, "boot-1"))
        self.assertEqual(report["launches_excluded_observed_before_handoff"], ["a" * 64])
        self.assertEqual(report["handoff_to_earliest_T_ingress"]["samples"], 0)


class FreezeTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self.tmp.name)
        for relative in c1_freeze.PRIOR_EVIDENCE:
            self.put(relative, "{}")
        for name in ("geth", "spec.md", "tool.py", "host.json", "build.txt", "rust-a", "config.env", "tests.log",
                     "raw.csv", "e5.json", "e6.json", "e7.json", "e8.json", "e9.json", "e9b.json"):
            self.put("in/" + name, name)
        self.put("in/e9.json", json.dumps({
            "schema": "darkbot.c1.native_latency.v1", "integrity": "PASS",
            "outbound_mode": "production_devp2p_selected_peer_handoff",
            "c1_9_qualification_eligible": True}))
        self.put("in/overlay/v2.go", "package fastlane")
        self.put("bsc/native/crates/test/src/lib.rs", "// synthetic source")
        self.put(".gitignore", "/inputs.json\n/record.json\n")
        for args in (("init", "-q"), ("add", "."),
                     ("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "fixture")):
            subprocess.run(["git", "-C", str(self.repo), *args], check=True, capture_output=True)
        self.head = c1_freeze.git_head(self.repo)
        self.source_hash = c1_freeze.source_binding(self.repo)["sha256"]

    def tearDown(self):
        self.tmp.cleanup()

    def put(self, relative, text):
        path = self.repo / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def inputs(self):
        gate = lambda name, evidence: {"gate": name, "verdict": "PASS", "accepted_by": "reviewer",
                                       "accepted_at": "2026-10-20", "evidence": ["in/" + evidence]}
        return {
            "darkbot_commit": self.head, "final_c1_source_sha256": self.source_hash,
            "schema": c1_freeze.SCHEMA_INPUTS, "pinned_geth_commit": c1_freeze.PIN, "geth_checkout": "vendor",
            "artifacts": {"geth_binary": "in/geth", "wire_v2_spec": "in/spec.md", "overlay_sources": "in/overlay",
                          "overlay_tool": "in/tool.py", "host_profile_evidence": "in/host.json",
                          "build_commands": "in/build.txt", "rust_binaries": ["in/rust-a"],
                          "c1_config": ["in/config.env"], "test_inventory": ["in/tests.log"]},
            "raw_measurement_evidence": ["in/raw.csv"],
            "gates": [gate("C1.5", "e5.json"), gate("C1.6", "e6.json"), gate("C1.7", "e7.json"),
                      gate("C1.8", "e8.json"), gate("C1.9", "e9.json"), gate("C1.9b", "e9b.json")],
            "non_claims": list(c1_freeze.REQUIRED_NON_CLAIMS),
        }

    def evaluate(self, inputs, head=c1_freeze.PIN):
        return c1_freeze.evaluate(inputs, self.repo, geth_head=lambda _path: head)

    def test_source_binding_refuses_missing_head_dirty_and_missing_hash(self):
        inputs = self.inputs()
        for key in ("darkbot_commit", "final_c1_source_sha256"):
            broken = dict(inputs)
            broken.pop(key)
            self.assertEqual(self.evaluate(broken)["status"], "INCOMPLETE")
        broken = dict(inputs, final_c1_source_sha256="0" * 64)
        self.assertEqual(self.evaluate(broken)["status"], "INCOMPLETE")
        self.put("untracked.rs", "// unbound source")
        self.assertTrue(any("dirty" in x for x in self.evaluate(inputs)["missing"]))
        (self.repo / "untracked.rs").unlink()
        git = self.repo / ".git"
        git.rename(self.repo / "saved-git")
        self.assertEqual(self.evaluate(inputs)["status"], "INCOMPLETE")

    def test_source_change_detected_even_if_git_is_told_to_ignore_it(self):
        path = "bsc/native/crates/test/src/lib.rs"
        subprocess.run(["git", "-C", str(self.repo), "update-index", "--assume-unchanged", path], check=True)
        self.put(path, "// modified despite identical HEAD")
        self.assertEqual(c1_freeze.git_head(self.repo), self.head)
        record = self.evaluate(self.inputs())
        self.assertTrue(any("differs from committed" in x for x in record["missing"]))
        self.assertTrue(any("hash missing or mismatch" in x for x in record["missing"]))

    def test_complete_inputs_are_ready_and_bind_every_artifact(self):
        record = self.evaluate(self.inputs())
        self.assertEqual((record["status"], record["missing"]), ("READY_TO_FREEZE", []))
        bindings = record["bindings"]
        self.assertEqual(bindings["pinned_geth_commit"], c1_freeze.PIN)
        self.assertEqual(len(bindings["prior_gate_evidence"]), 5)
        self.assertEqual(sorted(bindings["gates"]), sorted(c1_freeze.GATES))
        self.assertTrue(all(bindings["artifacts"][name] for name in c1_freeze.ARTIFACTS))
        # The record never calls itself frozen on its own.
        self.assertNotEqual(record["status"], c1_freeze.FROZEN)

    def test_the_binding_changes_with_any_bound_file(self):
        before = self.evaluate(self.inputs())["bindings_sha256"]
        self.put("in/raw.csv", "another measurement")
        self.assertNotEqual(self.evaluate(self.inputs())["bindings_sha256"], before)
        self.put("in/raw.csv", "raw.csv")
        self.assertEqual(self.evaluate(self.inputs())["bindings_sha256"], before)
        self.put("in/overlay/extra.go", "package fastlane")
        self.assertNotEqual(self.evaluate(self.inputs())["bindings_sha256"], before)

    def test_missing_or_unaccepted_inputs_leave_it_incomplete(self):
        cases = {}
        missing_file = self.inputs()
        missing_file["artifacts"]["geth_binary"] = "in/absent"
        cases["does not exist"] = missing_file
        no_gate = self.inputs()
        no_gate["gates"] = no_gate["gates"][:-1]
        cases["C1.9b: no acceptance entry"] = no_gate
        failed = self.inputs()
        failed["gates"][3]["verdict"] = "FAIL"
        cases["C1.8: not locally accepted"] = failed
        unsigned = self.inputs()
        unsigned["gates"][0]["accepted_by"] = ""
        cases["C1.5: not locally accepted"] = unsigned
        no_evidence = self.inputs()
        no_evidence["gates"][1]["evidence"] = []
        cases["C1.6 evidence: no file given"] = no_evidence
        no_raw = self.inputs()
        no_raw["raw_measurement_evidence"] = []
        cases["raw_measurement_evidence: no file given"] = no_raw
        weak = self.inputs()
        weak["non_claims"] = weak["non_claims"][1:]
        cases["non-claim missing: T0_TO_T_BUY_LT_1MS"] = weak
        other_pin = self.inputs()
        other_pin["pinned_geth_commit"] = "0" * 40
        cases["pinned_geth_commit is not"] = other_pin
        for expected, inputs in cases.items():
            record = self.evaluate(inputs)
            self.assertEqual(record["status"], "INCOMPLETE", expected)
            self.assertTrue(any(expected in reason for reason in record["missing"]), (expected, record["missing"]))
        wrong_head = self.evaluate(self.inputs(), head="1" * 40)
        self.assertTrue(any("geth checkout HEAD" in reason for reason in wrong_head["missing"]))

    def test_a_later_gate_cannot_be_accepted_before_an_earlier_one(self):
        inputs = self.inputs()
        inputs["gates"][1]["verdict"] = "PENDING"
        record = self.evaluate(inputs)
        self.assertEqual(record["status"], "INCOMPLETE")
        self.assertTrue(any("C1.6: not locally accepted" in reason for reason in record["missing"]))
        for later in ("C1.7", "C1.8", "C1.9", "C1.9b"):
            self.assertTrue(any(f"{later}: accepted before C1.6" in reason for reason in record["missing"]))

    def test_loopback_cannot_qualify_c19_for_freeze(self):
        report = json.loads((self.repo / "in/e9.json").read_text())
        report["outbound_mode"] = "loopback_microbenchmark"
        self.put("in/e9.json", json.dumps(report))
        record = self.evaluate(self.inputs())
        self.assertTrue(any("eligible production devp2p" in x for x in record["missing"]))

    def test_a_template_is_never_evidence(self):
        self.put("in/e9.json", '{"status": "TEMPLATE_NOT_EVIDENCE"}')
        record = self.evaluate(self.inputs())
        self.assertEqual(record["status"], "INCOMPLETE")
        self.assertTrue(any("is a template, not evidence" in reason for reason in record["missing"]))

    def test_the_command_line_freezes_only_complete_inputs_and_verifies_them(self):
        inputs_path = self.repo / "inputs.json"
        record_path = self.repo / "record.json"
        original_head, original_argv = c1_freeze.git_head, sys.argv
        c1_freeze.git_head = lambda path: c1_freeze.PIN if path.name == "vendor" else original_head(path)

        def run(*extra):
            sys.argv = ["c1_freeze.py", "--repo", str(self.repo), "--inputs", str(inputs_path), *extra]
            return c1_freeze.main()

        try:
            incomplete = self.inputs()
            incomplete["gates"][5]["verdict"] = "PENDING"
            inputs_path.write_text(json.dumps(incomplete))
            self.assertEqual(run("--freeze", "--output", str(record_path)), 2)
            self.assertEqual(json.loads(record_path.read_text())["status"], "INCOMPLETE")

            inputs_path.write_text(json.dumps(self.inputs()))
            self.assertEqual(run("--output", str(record_path)), 0)
            self.assertEqual(json.loads(record_path.read_text())["status"], "READY_TO_FREEZE")
            self.assertEqual(run("--verify", str(record_path)), 3)

            self.assertEqual(run("--freeze", "--output", str(record_path)), 0)
            self.assertEqual(json.loads(record_path.read_text())["status"], c1_freeze.FROZEN)
            self.assertEqual(run("--verify", str(record_path)), 0)
            # Every requested source refusal must also apply to --verify.
            good_inputs = self.inputs()
            for key in ("darkbot_commit", "final_c1_source_sha256"):
                bad = dict(good_inputs)
                bad.pop(key)
                inputs_path.write_text(json.dumps(bad))
                self.assertEqual(run("--verify", str(record_path)), 3)
            inputs_path.write_text(json.dumps(dict(good_inputs, final_c1_source_sha256="0" * 64)))
            self.assertEqual(run("--verify", str(record_path)), 3)
            inputs_path.write_text(json.dumps(good_inputs))
            source = "bsc/native/crates/test/src/lib.rs"
            self.put(source, "// changed after freeze")
            self.assertEqual(run("--verify", str(record_path)), 3)
            self.put(source, "// synthetic source")
            git = self.repo / ".git"
            git.rename(self.repo / "saved-git")
            self.assertEqual(run("--verify", str(record_path)), 3)
            (self.repo / "saved-git").rename(git)
            self.assertEqual(run("--verify", str(record_path)), 0)
            self.put("in/geth", "a different binary")
            self.assertEqual(run("--verify", str(record_path)), 3)
        finally:
            c1_freeze.git_head, sys.argv = original_head, original_argv


if __name__ == "__main__":
    unittest.main()
