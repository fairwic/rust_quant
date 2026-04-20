#!/usr/bin/env python3
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from _bsc_meme_event_backtest import *  # noqa: F401,F403,E402
from _bsc_meme_event_backtest.cli import main  # noqa: E402


if __name__ == "__main__":
    main()
