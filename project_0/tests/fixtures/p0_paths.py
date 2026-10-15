"""Shared test paths and environment. Importing it puts project_0 on sys.path.

Tests import production modules from the `operations` package (operations.verification.audit_market_quick, ...),
independent of the current working directory. Historical evidence under docs/evidence is referenced in place, never copied here.
"""
import os
from pathlib import Path
import sys

PROJECT=Path(__file__).resolve().parents[2]
REPO=PROJECT.parent
SCRIPTS=PROJECT/'scripts'
OPERATIONS=PROJECT/'operations'
EVIDENCE=PROJECT/'docs/evidence'
RELEASE=PROJECT/'native/target/release'
if str(PROJECT) not in sys.path:sys.path.insert(0,str(PROJECT))


def clean_env():
    return {k: v for k, v in os.environ.items() if not k.startswith(('P0_', 'HELIUS_', 'JITO_'))}
