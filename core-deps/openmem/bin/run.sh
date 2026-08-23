#!/usr/bin/env bash
# OpenMem Launcher (Linux/macOS)
# Usage: ./bin/run.sh [args]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

if command -v source &> /dev/null; then
    source bin/portable_env.sh > /dev/null 2>&1
fi

if [ $# -eq 0 ]; then
    python3 bin/launcher.py --status
else
    python3 bin/launcher.py "$@"
fi
