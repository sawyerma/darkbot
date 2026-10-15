"""Forward an old `python3 project_0/scripts/<module>.py` command line to its module under operations/.

Kept for documented historical commands (PHASENPLAN, ledger, evidence); review for removal in structure step 30.
"""
from pathlib import Path
import runpy
import sys


def run(module):
    project=Path(__file__).resolve().parents[1]
    sys.path.insert(0,str(project))
    sys.argv[0]=str(project/(module.replace('.','/')+'.py'))
    runpy.run_module(module,run_name='__main__',alter_sys=True)
