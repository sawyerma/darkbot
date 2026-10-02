#!/usr/bin/env python3
"""
C0-01 / C0-02 (Teil 1): Project-0 Account-Census.

Zieht alle Accounts des Project-0-Programms per getProgramAccounts (ein Call,
Helius Free: gPA 5 req/s), schreibt sie als JSONL und erzeugt einen Typ-/Größen-
Census über Anchor-Discriminators. Health-Klassifikation (C0-02 Teil 2) folgt,
sobald das P1A-Manifest das MarginfiAccount-Layout des deployed Tags pinnt.

Nur Standardbibliothek. Kein Paid-Provider. Kein Wert wird als 0 erfunden:
fehlende Messungen stehen als "N/A".

Aufruf:
  export P0_RPC_URL=https://...
  python3 project_0/scripts/c0_census.py --out project_0/data/c0/
  # alternativ: HELIUS_API_KEY=... oder --rpc https://...
  python3 project_0/scripts/c0_census.py --selftest      # offline

Die Mainnet-Program-ID ist aus dem gepinnten marginfi-v2-Source verifiziert.
Die Default-Group-ID bleibt bis zur separaten Mainnet-/P1A-Verifikation als
PRÜFEN markiert.
"""
import argparse
import base64
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request
from collections import Counter, defaultdict
from datetime import datetime, timezone

DEFAULT_PROGRAM_ID = "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"  # source-verified: marginfi-v2 mainnet id
DEFAULT_GROUP_ID = "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8"  # PRÜFEN via P1A/Mainnet evidence

# Anchor: sha256("account:<Name>")[:8]. Namen aus marginfi-v2 (PRÜFEN gegen IDL des deployed Tags).
ACCOUNT_TYPES = [
    "MarginfiAccount",
    "Bank",
    "MarginfiGroup",
    "FeeState",
    "LiquidationRecord",
    "StakedSettings",
    "RebalanceOrder",
]


def anchor_discriminator(name: str) -> bytes:
    return hashlib.sha256(f"account:{name}".encode()).digest()[:8]


DISCRIMINATORS = {anchor_discriminator(n): n for n in ACCOUNT_TYPES}


def rpc_call(url: str, method: str, params, timeout: float = 120.0, retries: int = 5):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    delay = 2.0
    last_err = None
    for attempt in range(1, retries + 1):
        req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                payload = json.loads(resp.read().decode())
            if "error" in payload:
                raise RuntimeError(f"rpc error: {payload['error']}")
            return payload["result"]
        except urllib.error.HTTPError as e:
            last_err = e
            if e.code == 429 or 500 <= e.code < 600:
                time.sleep(delay)
                delay = min(delay * 2, 30.0)
                continue
            raise
        except (urllib.error.URLError, TimeoutError) as e:
            last_err = e
            time.sleep(delay)
            delay = min(delay * 2, 30.0)
    raise RuntimeError(f"rpc failed after {retries} attempts: {last_err}")


def fetch_program_accounts(url: str, program_id: str):
    params = [
        program_id,
        {"encoding": "base64", "commitment": "confirmed", "withContext": True},
    ]
    result = rpc_call(url, "getProgramAccounts", params)
    slot = result["context"]["slot"]
    return slot, result["value"]


def census(accounts, context_slot, program_id, group_id, rpc_host, source_note):
    by_type = Counter()
    sizes_by_type = defaultdict(Counter)
    unknown_disc = Counter()
    total_bytes = 0
    for acc in accounts:
        data_b64 = acc["account"]["data"][0]
        raw = base64.b64decode(data_b64)
        total_bytes += len(raw)
        disc = raw[:8]
        name = DISCRIMINATORS.get(disc)
        if name is None:
            unknown_disc[disc.hex()] += 1
            by_type["UNKNOWN_DISCRIMINATOR"] += 1
            sizes_by_type["UNKNOWN_DISCRIMINATOR"][len(raw)] += 1
        else:
            by_type[name] += 1
            sizes_by_type[name][len(raw)] += 1
    return {
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "ticket": "C0-01",
        "source": source_note,
        "rpc_host": rpc_host,
        "program_id": program_id,
        "program_id_status": "SOURCE_VERIFIED: pinned marginfi-v2 mainnet program id",
        "group_id": group_id,
        "group_id_status": "PRÜFEN: via P1A-01",
        "context_slot": context_slot,
        "accounts_total": len(accounts),
        "bytes_total": total_bytes,
        "by_type": dict(by_type),
        "sizes_by_type": {k: dict(sorted(v.items())) for k, v in sizes_by_type.items()},
        "unknown_discriminators": dict(unknown_disc),
        "health_classification": "N/A — C0-02 Teil 2 benötigt P1A-Layout",
        "long_tail_candidates": "N/A — nach Health-Klassifikation",
    }


def write_outputs(out_dir, accounts, report):
    os.makedirs(out_dir, exist_ok=True)
    jsonl_path = os.path.join(out_dir, f"accounts_slot{report['context_slot']}.jsonl")
    h = hashlib.sha256()
    with open(jsonl_path, "w") as f:
        for acc in accounts:
            line = json.dumps(
                {
                    "pubkey": acc["pubkey"],
                    "lamports": acc["account"]["lamports"],
                    "owner": acc["account"]["owner"],
                    "data_b64": acc["account"]["data"][0],
                    "slot": report["context_slot"],
                },
                separators=(",", ":"),
            )
            f.write(line + "\n")
            h.update(line.encode())
    report["raw_evidence_path"] = jsonl_path
    report["raw_evidence_sha256"] = h.hexdigest()
    report_path = os.path.join(out_dir, f"c0_census_slot{report['context_slot']}.json")
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)
    return jsonl_path, report_path


def selftest():
    # Discriminator-Berechnung ist deterministisch (Anchor-Konvention).
    d = anchor_discriminator("MarginfiAccount")
    assert len(d) == 8
    assert anchor_discriminator("Bank") != d
    # Fixture: zwei Accounts, einer bekannt, einer unbekannt.
    known = anchor_discriminator("Bank") + b"\x00" * 1856  # Rohlänge = 8 + 1856 (Payload ohne Discriminator)
    unknown = b"\xff" * 8 + b"\x00" * 100
    fixture = [
        {"pubkey": "A", "account": {"lamports": 1, "owner": "P", "data": [base64.b64encode(known).decode(), "base64"]}},
        {"pubkey": "B", "account": {"lamports": 2, "owner": "P", "data": [base64.b64encode(unknown).decode(), "base64"]}},
    ]
    rep = census(fixture, 123, DEFAULT_PROGRAM_ID, DEFAULT_GROUP_ID, "selftest", "fixture")
    assert rep["accounts_total"] == 2
    assert rep["by_type"]["Bank"] == 1
    assert rep["by_type"]["UNKNOWN_DISCRIMINATOR"] == 1
    assert rep["sizes_by_type"]["Bank"] == {1864: 1}
    assert rep["health_classification"].startswith("N/A")
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        jl, rp = write_outputs(tmp, fixture, rep)
        assert os.path.getsize(jl) > 0 and os.path.getsize(rp) > 0
        with open(jl) as f:
            assert sum(1 for _ in f) == 2
    print("selftest OK")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rpc", help="RPC URL; Default: P0_RPC_URL, danach HELIUS_API_KEY")
    ap.add_argument("--program", default=DEFAULT_PROGRAM_ID)
    ap.add_argument("--group", default=DEFAULT_GROUP_ID)
    ap.add_argument("--out", default="project_0/data/c0")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        selftest()
        return 0

    url = args.rpc or os.environ.get("P0_RPC_URL", "").strip()
    if not url:
        key = os.environ.get("HELIUS_API_KEY", "").strip()
        placeholder = key.upper() in {"DEIN_KEY", "YOUR_KEY", "API_KEY", "HELIUS_API_KEY", "..."}
        if not key or placeholder:
            print(
                "FEHLER: gueltige --rpc/P0_RPC_URL oder echte HELIUS_API_KEY erforderlich; "
                "Platzhalter wie DEIN_KEY sind nicht gueltig.",
                file=sys.stderr,
            )
            return 2
        url = f"https://mainnet.helius-rpc.com/?api-key={key}"
    rpc_host = url.split("?")[0]

    t0 = time.monotonic()
    try:
        slot, accounts = fetch_program_accounts(url, args.program)
    except urllib.error.HTTPError as e:
        if e.code == 401:
            print(
                f"FEHLER: RPC HTTP 401 Unauthorized ({rpc_host}). API-Key/Endpoint pruefen.",
                file=sys.stderr,
            )
            return 2
        print(f"FEHLER: RPC HTTP {e.code} ({rpc_host}): {e.reason}", file=sys.stderr)
        return 2
    except Exception as e:
        print(f"FEHLER: Census-RPC fehlgeschlagen ({rpc_host}): {e}", file=sys.stderr)
        return 2
    dt = time.monotonic() - t0
    report = census(accounts, slot, args.program, args.group, rpc_host, "getProgramAccounts confirmed")
    report["gpa_duration_s"] = round(dt, 3)
    jl, rp = write_outputs(args.out, accounts, report)
    print(json.dumps({k: report[k] for k in ("context_slot", "accounts_total", "bytes_total", "by_type", "unknown_discriminators", "gpa_duration_s")}, indent=2))
    print(f"raw:    {jl}\nreport: {rp}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
