"""Compatibility entry for the pre-reform command

    python3 -m unittest discover -s project_0/scripts -p 'test_*.py'

The tests live in project_0/tests/{python,integration,regression} since structure iteration 13; this module loads
exactly that suite, so the old command keeps running every test instead of silently finding none.
"""
from pathlib import Path

PROJECT=Path(__file__).resolve().parents[1]


def load_tests(loader,tests,pattern):
    return loader.discover(str(PROJECT/'tests'),pattern='test*.py',top_level_dir=str(PROJECT))
