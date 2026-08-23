#!/usr/bin/env bash
# OpenMem Portable Environment Setup (Linux/macOS)
# Usage: source bin/portable_env.sh

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OPENMEM_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Add OpenMem to Python path
export PYTHONPATH="$OPENMEM_ROOT:${PYTHONPATH:-}"

# Add bin directory to PATH
export PATH="$OPENMEM_ROOT/bin:${PATH:-}"

# Data directory
export OPENMEM_DATA="$OPENMEM_ROOT/data"

# Auto-detect agent (override with OPENMEM_AGENT)
if [ -z "$OPENMEM_AGENT" ]; then
    # Check for agent indicators
    [ -d "$PWD/.qwen" ] && export OPENMEM_AGENT="qwen_code"
    [ -d "$PWD/.claude" ] && export OPENMEM_AGENT="claude_code"
    [ -d "$PWD/.cursor" ] && export OPENMEM_AGENT="cursor"
    [ -d "$PWD/.vscode" ] && export OPENMEM_AGENT="vscode"
    [ -f "$PWD/CLAUDE.md" ] && export OPENMEM_AGENT="claude_code"
    [ -d "$PWD/.windsurf" ] && export OPENMEM_AGENT="windsurf"
    [ -d "$PWD/.opencode" ] && export OPENMEM_AGENT="opencode"
    [ -d "$PWD/.codex" ] && export OPENMEM_AGENT="codex_cli"
fi

# Default to generic if not detected
export OPENMEM_AGENT="${OPENMEM_AGENT:-generic}"

echo "[OpenMem] Environment configured:"
echo "  ROOT:    $OPENMEM_ROOT"
echo "  DATA:    $OPENMEM_DATA"
echo "  AGENT:   $OPENMEM_AGENT"
echo "  PYTHON:  $(python3 --version 2>/dev/null || echo 'not found')"
echo ""
echo "Quick commands:"
echo "  python bin/launcher.py          - Status"
echo "  python bin/launcher.py --install - Install"
echo "  python bin/launcher.py --agents  - List agents"
echo "  python bin/launcher.py --skill all - Install all skills"
