"""Daemon lifecycle tests: ensure_daemon spawn-or-reuse, liveness, stop.

These spawn a real detached daemon process and verify the full lifecycle.
"""

from __future__ import annotations

import time

import pytest

from dataworm.server import (
    ensure_daemon,
    stop_daemon,
    _read_port_file,
    _remove_port_file,
    _is_alive,
)


@pytest.fixture
def db_path(tmp_path):
    db = str(tmp_path / "graph.db")
    _remove_port_file(db)
    yield db
    # Teardown: make sure no daemon lingers.
    try:
        stop_daemon(db)
    except Exception:
        pass


def test_ensure_daemon_spawns_and_is_alive(db_path):
    handle = ensure_daemon(db_path=db_path, prefer_rust=True)
    assert handle.port > 0
    assert _is_alive(handle.port, handle.token)

    info = _read_port_file(db_path)
    assert info is not None
    assert int(info["port"]) == handle.port
    assert info["token"] == handle.token


def test_ensure_daemon_reuses_existing(db_path):
    """A second ensure_daemon call must reuse the running daemon, not spawn again."""
    h1 = ensure_daemon(db_path=db_path, prefer_rust=True)
    h2 = ensure_daemon(db_path=db_path, prefer_rust=True)
    assert h1.port == h2.port
    assert h1.pid == h2.pid
    assert h1.token == h2.token


def test_daemon_responds_to_rpc(db_path):
    handle = ensure_daemon(db_path=db_path, prefer_rust=True)
    result = handle.call("ping")
    assert result["ok"] is True
    assert result["backend"] in ("rust", "python")


def test_stop_daemon_removes_port_file(db_path):
    handle = ensure_daemon(db_path=db_path, prefer_rust=True)
    assert _is_alive(handle.port, handle.token)
    result = stop_daemon(db_path)
    assert result["ok"] is True
    time.sleep(0.5)
    assert not _is_alive(handle.port, handle.token)
    assert _read_port_file(db_path) is None


def test_stop_when_not_running_is_idempotent(db_path):
    result = stop_daemon(db_path)
    assert result["ok"] is True
    assert result["status"] == "not running"
