#!/usr/bin/env python3
"""F0-VAR: reproducible lexical inventory of tracked Project_0 sources.

Reads committed source files; writes only project_0/docs/F0_VAR_*.tsv/json.
This is a source census, NOT a validated runtime, schema, or ownership mapping.
No credentials or .env values are exported. Stdlib only; no network access.
"""
from __future__ import annotations

import csv
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "project_0" / "docs"
ENV_RE = re.compile(r"""["']((?:P0|HELIUS|DARKBOT)_[A-Z0-9_]+)["']""")
TYPE_RE = re.compile(r"^\s*(pub(?:\([^)]*\))?\s+)?(struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)\b")
FIELD_RE = re.compile(r"^\s*(pub(?:\([^)]*\))?\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+?)\s*,?\s*$")
VARIANT_RE = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?:[,({=]|$)")
PROP_RE = re.compile(r"<(?:DetailRow|Section|UnknownMetric|GraphiteShell|TabStrip)\b[^>]*\b(?:title|label|subtitle)=[\"']([^\"']+)[\"']")
UI_TEXT_RE = re.compile(r"<(?:button|label|option|h1|h2)\b[^>]*>([^<{]+)<")
DECL_TS_RE = re.compile(r"^\s*(?:export\s+)?(?:interface|type)\s+([A-Za-z_][A-Za-z0-9_]*)")
ANCHOR_RE = re.compile(r"\b(?:publish_(?:market|storage|transport)|on_ingress_event|on_slot_frame|stop_requested|onConnectionDown|run_from_env_with|topics\.onState|topics\.onEvent|gateway\.connect|applyStatus|setStatusTopic)\b")

def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()

def tracked_files() -> list[Path]:
    raw = subprocess.check_output(["git", "-C", str(ROOT), "ls-files", "-z"])
    chosen = []
    for b in raw.split(b"\0"):
        if not b:
            continue
        rel = Path(b.decode("utf-8"))
        s = rel.as_posix()
        in_scope = (
            (s.startswith("project_0/native/crates/") and s.endswith(".rs"))
            or (s.startswith("frontend/src/features/project0/") and s.endswith((".ts", ".tsx")))
            or (s.startswith("frontend/src/features/overview/") and s.endswith((".ts", ".tsx")))
            or (s.startswith("frontend/src/core/realtime/") and s.endswith((".ts", ".tsx")) and "/generated/" not in s)
            or (s.startswith("frontend/src/app/") and s.endswith((".ts", ".tsx")))
            or (s.startswith("protocol/schemas/") and s.endswith(".proto"))
            or (s.startswith("project_0/operations/") and s.endswith((".py", ".sh", ".rs")))
            or (s.startswith("project_0/config/") and s.endswith((".env", ".json", ".toml", ".yaml", ".yml")))
            or (s.startswith("gateway/src/") and s.endswith(".rs"))
            or (s.startswith("ops/") and (s.endswith(".sh") or s == "ops/darkbotctl"))
        )
        if in_scope and (ROOT / rel).is_file():
            chosen.append(rel)
    return sorted(chosen)

def table(filename: str, header: list[str], rows: list[list[object]]) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    with (OUT / filename).open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(header)
        w.writerows([["-" if value == "" else value for value in row] for row in rows])

def analyze_type(lines: list[str], path: str, out: list[list[object]]) -> None:
    """Lexer deliberately records unparsed declarations; check complex fields manually."""
    active = False
    brace_started = False
    depth = 0
    kind = name = visibility = ""
    for n, line in enumerate(lines, 1):
        stripped = line.strip()
        if not active:
            m = TYPE_RE.match(line)
            if not m:
                continue
            visibility = (m.group(1) or "private").strip()
            kind, name = m.group(2), m.group(3)
            out.append(["DECL", path, n, kind, visibility, name, "", "", ""])
            active = True
            brace_started = "{" in line
            depth = line.count("{") - line.count("}")
            if (stripped.endswith(";") or stripped.endswith(");")) and not brace_started:
                active = False
            continue
        if not brace_started:
            if "{" in line:
                brace_started = True
                depth += line.count("{") - line.count("}")
            continue
        if depth == 1 and stripped and not stripped.startswith(("//", "#", "/*", "*")):
            if kind == "struct":
                m = FIELD_RE.match(line)
                if m:
                    out.append(["FIELD", path, n, kind, (m.group(1) or "private").strip(),
                                name, m.group(2), m.group(3).strip(), n])
            else:
                m = VARIANT_RE.match(line)
                if m:
                    out.append(["VARIANT", path, n, kind, visibility, name, m.group(1), "", n])
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            active = False
            brace_started = False

def main() -> None:
    files = tracked_files()
    rust, ui, env, anchors = [], [], [], []
    counts = Counter()
    for rel in files:
        path = rel.as_posix()
        lines = (ROOT / rel).read_text(encoding="utf-8", errors="replace").splitlines()
        if path.startswith("project_0/native/crates/") and path.endswith(".rs"):
            counts["p0_rust_files"] += 1
        if path.startswith("frontend/"):
            counts["frontend_files"] += 1
        if path.endswith(".proto"):
            counts["proto_files"] += 1
        if path.endswith(".rs"):
            analyze_type(lines, path, rust)
        for n, line in enumerate(lines, 1):
            for m in ENV_RE.finditer(line):
                env.append([m.group(1), path, n])
            if path.startswith("frontend/"):
                for m in PROP_RE.finditer(line):
                    ui.append(["DISPLAY", path, n, m.group(1)])
                for m in UI_TEXT_RE.finditer(line):
                    if m.group(1).strip():
                        ui.append(["CONTROL_OR_TEXT", path, n, m.group(1).strip()])
                m = DECL_TS_RE.match(line)
                if m:
                    ui.append(["TS_DECLARATION", path, n, m.group(1)])
            if ANCHOR_RE.search(line):
                anchors.append([path, n, line.strip()[:240]])
    table("F0_VAR_RUST_FIELDS.tsv",
          ["record", "file", "line", "kind", "visibility", "type", "member", "member_type", "member_line"], rust)
    table("F0_VAR_GUI_SURFACES.tsv", ["kind", "file", "line", "label_or_type"], ui)
    table("F0_VAR_ENV_REFERENCES.tsv", ["variable", "file", "line"], sorted(set(map(tuple, env))))
    table("F0_VAR_PUSH_ANCHORS.tsv", ["file", "line", "source_excerpt"], anchors)
    manifest = {
        "schema": "F0_VAR_STATIC_CENSUS_V1",
        "project": "project_0",
        "source_head": git("rev-parse", "HEAD"),
        "file_count": len(files),
        **dict(counts),
        "type_declarations": sum(r[0] == "DECL" for r in rust),
        "field_declarations": sum(r[0] == "FIELD" for r in rust),
        "gui_entries": len(ui),
        "environment_references": len(env),
        "push_anchors": len(anchors),
        "limitations": [
            "Lexical source census, not a Rust AST or verified field-to-topic mapping.",
            "Macro-generated and complex multiline fields can be missed.",
            "Dynamically constructed env names and JSX expressions need manual review.",
            "Source existence does not prove runtime availability or valid financial data.",
            "Never use this list as LIVE or money-transfer authorization."
        ],
    }
    (OUT / "F0_VAR_MANIFEST.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main()
