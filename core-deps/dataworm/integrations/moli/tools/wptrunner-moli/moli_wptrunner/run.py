from __future__ import annotations

import os
import sys


def main() -> int:
    wpt_root = os.environ.get("WPT_ROOT")
    if not wpt_root:
        raise SystemExit("WPT_ROOT must point to an upstream WPT checkout")

    sys.path.insert(0, wpt_root)
    from tools import localpaths  # noqa: F401
    from wptrunner import wptrunner

    return int(wptrunner.main() or 0)


if __name__ == "__main__":
    raise SystemExit(main())
