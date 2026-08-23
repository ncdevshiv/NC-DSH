from __future__ import annotations

import os
import subprocess
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


def _process_tree(root_pid: int) -> list[int]:
    ppid_by_pid: dict[int, int] = {}
    for stat_path in Path("/proc").glob("[0-9]*/stat"):
        try:
            raw = stat_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        close = raw.rfind(")")
        if close < 0:
            continue
        parts = raw[close + 2 :].split()
        if len(parts) < 2:
            continue
        try:
            pid = int(stat_path.parent.name)
            ppid = int(parts[1])
        except ValueError:
            continue
        ppid_by_pid[pid] = ppid

    result = [root_pid]
    seen = {root_pid}
    changed = True
    while changed:
        changed = False
        for pid, ppid in ppid_by_pid.items():
            if pid not in seen and ppid in seen:
                result.append(pid)
                seen.add(pid)
                changed = True
    return result


def _read_pss_bytes(pid: int) -> int | None:
    try:
        text = Path(f"/proc/{pid}/smaps_rollup").read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    for line in text.splitlines():
        if line.startswith("Pss:"):
            parts = line.split()
            if len(parts) >= 2:
                return int(parts[1]) * 1024
    return None


def _read_rss_bytes(pid: int) -> int | None:
    try:
        text = Path(f"/proc/{pid}/status").read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    for line in text.splitlines():
        if line.startswith("VmRSS:"):
            parts = line.split()
            if len(parts) >= 2:
                return int(parts[1]) * 1024
    return None


def _read_cpu_percent(pids: list[int]) -> float | None:
    if not pids:
        return None
    try:
        completed = subprocess.run(
            ["ps", "-o", "%cpu=", "-p", ",".join(str(pid) for pid in pids)],
            capture_output=True,
            text=True,
            timeout=1,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    total = 0.0
    found = False
    for line in completed.stdout.splitlines():
        try:
            total += float(line.strip())
            found = True
        except ValueError:
            continue
    return total if found else None


def _read_thread_count(pid: int) -> int | None:
    try:
        text = Path(f"/proc/{pid}/status").read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    for line in text.splitlines():
        if line.startswith("Threads:"):
            parts = line.split()
            if len(parts) >= 2:
                return int(parts[1])
    return None


def _read_fd_count(pid: int) -> int | None:
    try:
        return sum(1 for _ in Path(f"/proc/{pid}/fd").iterdir())
    except OSError:
        return None


def _read_cpu_identity(pid: int) -> tuple[tuple[int, int], int] | None:
    try:
        raw = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    close = raw.rfind(")")
    if close < 0:
        return None
    fields = raw[close + 2 :].split()
    if len(fields) <= 19:
        return None
    try:
        user_ticks = int(fields[11])
        system_ticks = int(fields[12])
        start_ticks = int(fields[19])
    except ValueError:
        return None
    return (pid, start_ticks), user_ticks + system_ticks


def snapshot_resources(root_pid: int, *, include_lifetime_cpu: bool = True) -> dict[str, Any]:
    pids = _process_tree(root_pid)
    pss_values = [_read_pss_bytes(pid) for pid in pids]
    rss_values = [_read_rss_bytes(pid) for pid in pids]
    thread_values = [_read_thread_count(pid) for pid in pids]
    fd_values = [_read_fd_count(pid) for pid in pids]
    pss_values = [value for value in pss_values if value is not None]
    rss_values = [value for value in rss_values if value is not None]
    thread_values = [value for value in thread_values if value is not None]
    fd_values = [value for value in fd_values if value is not None]
    return {
        "pids": pids,
        "process_count": len(pids),
        "pss_bytes": sum(pss_values) if len(pss_values) == len(pids) else None,
        "pss_process_count": len(pss_values),
        "rss_bytes": sum(rss_values) if len(rss_values) == len(pids) else None,
        "rss_process_count": len(rss_values),
        "thread_count": sum(thread_values)
        if len(thread_values) == len(pids)
        else None,
        "thread_process_count": len(thread_values),
        "fd_count": sum(fd_values) if len(fd_values) == len(pids) else None,
        "fd_process_count": len(fd_values),
        "cpu_percent": _read_cpu_percent(pids) if include_lifetime_cpu else None,
    }


def _sanitize_cgroup_name(name: str) -> str:
    return "".join(char if char.isalnum() or char in ("-", "_", ".") else "_" for char in name)


def _read_pid_cgroups(pid: int) -> list[dict[str, Any]]:
    try:
        lines = Path(f"/proc/{pid}/cgroup").read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return []
    entries = []
    for line in lines:
        parts = line.split(":", 2)
        if len(parts) != 3:
            continue
        hierarchy, controllers, relative_path = parts
        entries.append(
            {
                "hierarchy": hierarchy,
                "controllers": [controller for controller in controllers.split(",") if controller],
                "path": relative_path,
            }
        )
    return entries


def _candidate_cgroup_roots(entry: dict[str, Any]) -> list[Path]:
    relative = str(entry.get("path") or "/").lstrip("/")
    controllers = entry.get("controllers") or []
    roots = [Path("/sys/fs/cgroup") / relative]
    for controller in controllers:
        roots.append(Path("/sys/fs/cgroup") / str(controller) / relative)
    return roots


def collect_cgroup_artifacts(pid: int, output_dir: Path) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    entries = _read_pid_cgroups(pid)
    try:
        raw_cgroup = Path(f"/proc/{pid}/cgroup").read_text(encoding="utf-8", errors="replace")
    except OSError as error:
        raw_cgroup = f"unavailable: {error}\n"
    proc_cgroup_path = output_dir / "proc-cgroup.txt"
    proc_cgroup_path.write_text(raw_cgroup, encoding="utf-8")

    files = [
        "cgroup.controllers",
        "cgroup.events",
        "cgroup.procs",
        "cgroup.stat",
        "cpu.stat",
        "io.stat",
        "memory.current",
        "memory.events",
        "memory.max",
        "memory.peak",
        "memory.stat",
        "pids.current",
        "pids.max",
    ]
    copied: list[str] = [str(proc_cgroup_path)]
    roots: list[str] = []
    unavailable: list[str] = []
    seen_roots: set[Path] = set()
    for entry_index, entry in enumerate(entries):
        for root in _candidate_cgroup_roots(entry):
            if root in seen_roots or not root.exists():
                continue
            seen_roots.add(root)
            roots.append(str(root))
            label = f"{entry_index}-{_sanitize_cgroup_name(root.name or 'root')}"
            root_dir = output_dir / label
            root_dir.mkdir(parents=True, exist_ok=True)
            for file_name in files:
                source = root / file_name
                target = root_dir / file_name
                try:
                    target.write_text(source.read_text(encoding="utf-8", errors="replace"), encoding="utf-8")
                    copied.append(str(target))
                except OSError as error:
                    unavailable.append(f"{source}: {error}")
    if unavailable:
        unavailable_path = output_dir / "unavailable.txt"
        unavailable_path.write_text("\n".join(unavailable) + "\n", encoding="utf-8")
        copied.append(str(unavailable_path))
    return {
        "pid": pid,
        "entries": entries,
        "roots": roots,
        "artifact_dir": str(output_dir),
        "artifacts": copied,
        "available": bool(roots),
    }


@dataclass
class ResourceSampler:
    root_pid: int
    interval_seconds: float = 0.1
    samples: list[dict[str, Any]] = field(default_factory=list)
    _stop: threading.Event = field(default_factory=threading.Event)
    _thread: threading.Thread | None = None
    _started_monotonic: float | None = None
    _previous_monotonic: float | None = None
    _previous_cpu: dict[tuple[int, int], int] = field(default_factory=dict)
    _observer_error: str | None = None

    def start(self) -> None:
        if self.interval_seconds <= 0:
            raise ValueError("resource sample interval must be positive")
        if self._thread is not None:
            raise RuntimeError("resource sampler already started")
        self._started_monotonic = time.perf_counter()
        self._thread = threading.Thread(target=self._run, name="moli-resource-sampler", daemon=True)
        self._thread.start()

    def stop(self) -> dict[str, Any]:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=2)
        return self.summary()

    def _run(self) -> None:
        try:
            next_capture = time.perf_counter()
            while not self._stop.is_set():
                capture_started = time.perf_counter()
                sample = snapshot_resources(self.root_pid, include_lifetime_cpu=False)
                captured = time.perf_counter()
                identities = dict(
                    identity
                    for pid in sample.get("pids", [])
                    if (identity := _read_cpu_identity(int(pid))) is not None
                )
                cpu_percent = None
                if self._previous_monotonic is not None:
                    elapsed = captured - self._previous_monotonic
                    delta_ticks = sum(
                        max(0, ticks - self._previous_cpu[identity])
                        for identity, ticks in identities.items()
                        if identity in self._previous_cpu
                    )
                    if elapsed > 0:
                        try:
                            clock_ticks = float(os.sysconf("SC_CLK_TCK"))
                        except (OSError, ValueError):
                            clock_ticks = 100.0
                        if clock_ticks > 0:
                            cpu_percent = delta_ticks / clock_ticks / elapsed * 100.0
                self._previous_monotonic = captured
                self._previous_cpu = identities
                sample["cpu_percent"] = cpu_percent
                sample["timestamp"] = time.time()
                sample["elapsed_ms"] = (
                    (captured - self._started_monotonic) * 1000.0
                    if self._started_monotonic is not None
                    else 0.0
                )
                sample["capture_duration_ms"] = (captured - capture_started) * 1000.0
                sample["kind"] = "periodic"
                self.samples.append(sample)
                next_capture += self.interval_seconds
                now = time.perf_counter()
                if next_capture <= now:
                    missed = int((now - next_capture) / self.interval_seconds) + 1
                    next_capture += missed * self.interval_seconds
                remaining = max(0.0, next_capture - now)
                self._stop.wait(remaining)
        except BaseException as error:
            self._observer_error = f"{type(error).__name__}: {error}"

    def summary(self) -> dict[str, Any]:
        pss = [sample["pss_bytes"] for sample in self.samples if sample.get("pss_bytes") is not None]
        rss = [sample["rss_bytes"] for sample in self.samples if sample.get("rss_bytes") is not None]
        cpu = [sample["cpu_percent"] for sample in self.samples if sample.get("cpu_percent") is not None]
        process_counts = [sample["process_count"] for sample in self.samples if sample.get("process_count") is not None]
        thread_counts = [sample["thread_count"] for sample in self.samples if sample.get("thread_count") is not None]
        fd_counts = [sample["fd_count"] for sample in self.samples if sample.get("fd_count") is not None]
        capture_durations = [
            float(sample["capture_duration_ms"])
            for sample in self.samples
            if sample.get("capture_duration_ms") is not None
        ]
        intervals = [
            float(current["elapsed_ms"]) - float(previous["elapsed_ms"])
            for previous, current in zip(self.samples, self.samples[1:])
            if previous.get("elapsed_ms") is not None and current.get("elapsed_ms") is not None
        ]
        complete_pss_samples = sum(
            1
            for sample in self.samples
            if sample.get("pss_bytes") is not None
            and sample.get("pss_process_count") == sample.get("process_count")
        )
        return {
            "sample_count": len(self.samples),
            "interval_seconds": self.interval_seconds,
            "peak_pss_bytes": max(pss) if pss else None,
            "peak_rss_bytes": max(rss) if rss else None,
            "peak_cpu_percent": max(cpu) if cpu else None,
            "peak_process_count": max(process_counts) if process_counts else None,
            "peak_thread_count": max(thread_counts) if thread_counts else None,
            "peak_fd_count": max(fd_counts) if fd_counts else None,
            "average_cpu_percent": sum(cpu) / len(cpu) if cpu else None,
            "capture_duration_ms": {
                "average": sum(capture_durations) / len(capture_durations)
                if capture_durations
                else None,
                "max": max(capture_durations) if capture_durations else None,
            },
            "late_sample_count": sum(
                1 for interval in intervals if interval > self.interval_seconds * 1000.0 * 1.5
            ),
            "observed_interval_ms": {
                "average": sum(intervals) / len(intervals) if intervals else None,
                "max": max(intervals) if intervals else None,
            },
            "pss_complete_samples": complete_pss_samples,
            "pss_complete": bool(self.samples)
            and complete_pss_samples == len(self.samples),
            "observer_error": self._observer_error,
            "thread_alive_after_stop": bool(
                self._thread is not None and self._thread.is_alive()
            ),
            "sampling_method": "procfs_process_tree_cpu_ticks_smaps_rollup",
        }
