"""Entry point for `python -m darkbot`; delegates to cli.main."""

import sys

from darkbot.cli import main

if __name__ == "__main__":
    sys.exit(main())
