#!/usr/bin/env python3
"""Install/verify the fixed common SC font; never shrink player-name coverage."""
from pathlib import Path
import runpy

if __name__ == '__main__':
    runpy.run_path(str(Path(__file__).resolve().parents[1] / 'tools/install_cjk_font.py'), run_name='__main__')
