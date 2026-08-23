"""Detached daemon entrypoint.

This is the process that ``ensure_daemon()`` spawns. It constructs a
``Daemon``, binds the HTTP server, and blocks on ``serve_forever()`` until it
receives a ``shutdown`` RPC (or SIGTERM / Ctrl-C). On exit it flushes the
graph to SQLite and removes the port-file.

Run directly:
    python -m dataworm.daemon_runner --db .dataworm/graph.db --port 8765 --token <hex> [--rust|--no-rust]
"""

from __future__ import annotations

import argparse
import logging
import signal
import sys

from dataworm.core import DEFAULT_DB
from dataworm.logging_setup import setup_logging
from dataworm.server import Daemon, DEFAULT_PORT


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="dataworm-daemon")
    parser.add_argument("--db", default=DEFAULT_DB, help="SQLite graph DB path")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="TCP port")
    parser.add_argument("--token", default="", help="auth token (generated if empty)")
    parser.add_argument("--rust", dest="prefer_rust", action="store_true", default=True)
    parser.add_argument("--no-rust", dest="prefer_rust", action="store_false")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)

    # Diagnostics go to the shared rotating dw.log (INFO; DEBUG with
    # --verbose) instead of nothing/console-only — the daemon runs detached,
    # so a file is the only durable place its logs can live.
    setup_logging(level=logging.DEBUG if args.verbose else logging.INFO)

    daemon = Daemon(
        db_path=args.db,
        port=args.port,
        token=args.token,
        prefer_rust=args.prefer_rust,
    )

    # Graceful shutdown on SIGTERM (POSIX) / Ctrl-C.
    def _shutdown(signum, frame):
        daemon.stop()
    signal.signal(signal.SIGTERM, _shutdown)
    signal.signal(signal.SIGINT, _shutdown)

    daemon.start()
    daemon.serve_forever()


if __name__ == "__main__":
    main()
