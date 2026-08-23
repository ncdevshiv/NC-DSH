#!/usr/bin/env python3
"""
openmem — console-script entry point.

Thin top-level module mirroring main.py: it validates the layout and delegates
every operation to bin/launcher.py via runpy. Exposed through
[project.scripts] as `openmem = "openmem_cli:main"`; returns an exit code.
"""

import sys
from pathlib import Path

# Editable installs point this at the source checkout; wheel installs of bin/
# are not shipped, so the launcher must be found next to this module.
BASE_DIR = Path(__file__).resolve().parent


def main():
    """Delegate to bin/launcher.py for all operations. Returns an exit code."""
    launcher_script = BASE_DIR / "bin" / "launcher.py"

    if not launcher_script.exists():
        print("[OpenMem] bin/launcher.py not found")
        print("Run: python bin/install.py --all  (or install from a source checkout)")
        return 1

    sys.argv[0] = str(launcher_script)

    import runpy
    try:
        runpy.run_path(str(launcher_script), run_name="__main__")
        return 0
    except SystemExit as e:
        return e.code if e.code is not None else 0
    except Exception as e:
        print(f"[OpenMem] Launcher error: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
