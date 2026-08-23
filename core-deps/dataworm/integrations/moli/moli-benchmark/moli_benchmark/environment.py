from __future__ import annotations

import os
import platform
import socket
import subprocess
from pathlib import Path
from typing import Any

from .config import PROXY_ENV_KEYS


def _read_first_existing(paths: list[Path]) -> str | None:
    for path in paths:
        try:
            if path.exists():
                return path.read_text(encoding="utf-8", errors="replace").strip()
        except OSError:
            continue
    return None


def _command_version(command: list[str]) -> str | None:
    try:
        completed = subprocess.run(command, capture_output=True, text=True, timeout=5, check=False)
    except (OSError, subprocess.TimeoutExpired):
        return None
    output = (completed.stdout or completed.stderr).strip()
    return output.splitlines()[0] if output else None


def collect_environment() -> dict[str, Any]:
    os_release = _read_first_existing([Path("/etc/os-release")])
    cpuinfo = _read_first_existing([Path("/proc/cpuinfo")])
    meminfo = _read_first_existing([Path("/proc/meminfo")])
    cpu_model = None
    if cpuinfo:
        for line in cpuinfo.splitlines():
            if line.startswith("model name"):
                cpu_model = line.split(":", 1)[1].strip()
                break
    memory_total_kb = None
    if meminfo:
        for line in meminfo.splitlines():
            if line.startswith("MemTotal:"):
                parts = line.split()
                if len(parts) >= 2:
                    memory_total_kb = int(parts[1])
                break

    cgroup = {
        "memory_current": _read_first_existing([Path("/sys/fs/cgroup/memory.current")]),
        "memory_peak": _read_first_existing([Path("/sys/fs/cgroup/memory.peak")]),
        "cpu_stat": _read_first_existing([Path("/sys/fs/cgroup/cpu.stat")]),
    }

    return {
        "hostname": socket.gethostname(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "python": platform.python_version(),
        "os_release": os_release,
        "kernel": platform.release(),
        "glibc": _command_version(["ldd", "--version"]),
        "cpu_model": cpu_model,
        "logical_cores": os.cpu_count(),
        "memory_total_kb": memory_total_kb,
        "cgroup": cgroup,
        "proxy_env": {key: os.environ.get(key) for key in PROXY_ENV_KEYS if os.environ.get(key)},
    }
