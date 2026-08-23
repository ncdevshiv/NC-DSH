"""Process-wide logging setup shared by the CLI and the detached daemon.

One rotating file carries INFO+ diagnostics (%LOCALAPPDATA%\dataworm\dw.log,
falling back to %TEMP%\dataworm-dw.log when LOCALAPPDATA is missing or
read-only); the console stays quiet at WARNING+. Both entrypoints call
:func:`setup_logging` exactly once so bug reports can just attach dw.log.
"""

from __future__ import annotations

import logging
import os
import tempfile
from logging.handlers import RotatingFileHandler
from pathlib import Path

_FORMAT = "%(asctime)s %(name)s %(levelname)s %(message)s"
_MAX_BYTES = 1_000_000  # ~1 MB per dw.log, 2 rotated backups
_BACKUPS = 2

# Set by setup_logging(); "" means "no writable log file this process".
_LOG_FILE: str | None = None
_configured = False


def _candidate_paths():
    """Preferred locations in order: %LOCALAPPDATA%\\dataworm\\dw.log, then
    %TEMP%\\dataworm-dw.log."""
    local = os.environ.get("LOCALAPPDATA")
    if local:
        yield Path(local) / "dataworm" / "dw.log"
    yield Path(tempfile.gettempdir()) / "dataworm-dw.log"


def setup_logging(level: int = logging.INFO) -> str:
    """Configure root logging once. Returns the active log file path
    ("" when only the console could be configured)."""
    global _LOG_FILE, _configured
    if _configured:
        return _LOG_FILE or ""
    _configured = True

    root = logging.getLogger()
    root.setLevel(level)
    fmt = logging.Formatter(_FORMAT)

    console = logging.StreamHandler()
    console.setLevel(logging.WARNING)
    console.setFormatter(fmt)
    root.addHandler(console)

    for candidate in _candidate_paths():
        try:
            candidate.parent.mkdir(parents=True, exist_ok=True)
            file_handler = RotatingFileHandler(
                candidate, maxBytes=_MAX_BYTES, backupCount=_BACKUPS,
                encoding="utf-8")
            break
        except OSError:
            continue  # unwritable — try the next fallback location
    else:
        file_handler = None

    if file_handler is not None:
        file_handler.setLevel(level)
        file_handler.setFormatter(fmt)
        root.addHandler(file_handler)
        _LOG_FILE = str(candidate)
        return _LOG_FILE
    _LOG_FILE = ""
    return ""


def log_file_hint() -> str:
    """Where this process's logs go (human-facing; used by CLI summaries)."""
    return _LOG_FILE or "console only (no writable log file found)"
