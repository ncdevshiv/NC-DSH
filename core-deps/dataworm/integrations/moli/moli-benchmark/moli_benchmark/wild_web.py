from __future__ import annotations

import html.parser
import os
from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json, write_text
from .chrome_dcl import run_chrome_dcl_dump, run_served_cdp_dcl_dump
from .config import REPO_ROOT, clear_proxy_env
from .process import run_process
from .stats import summarize
from .synthetic_compare import (
    WEBFETCH_TARGETS,
    target_enables_all_resource_fetch,
    target_is_cdp,
    target_metadata,
)


WILD_WEB_SEEDS: dict[str, str] = {
    "baidu-home": "https://www.baidu.com/",
    "bilibili-home": "https://www.bilibili.com/",
    "zhihu-home": "https://www.zhihu.com/",
    "toutiao-home": "https://www.toutiao.com/",
}

WILD_WEB_SEED_ASSERTIONS: dict[str, dict[str, Any]] = {
    "baidu-home": {"title_any": ("百度", "baidu"), "text_any": ("百度", "baidu"), "min_text_length": 20},
    "bilibili-home": {"title_any": ("哔哩", "bilibili"), "text_any": ("哔哩", "bilibili"), "min_text_length": 20},
    "zhihu-home": {"title_any": ("知乎", "zhihu"), "text_any": ("知乎", "zhihu"), "min_text_length": 20},
    "toutiao-home": {"title_any": ("头条", "toutiao"), "text_any": ("头条", "toutiao"), "min_text_length": 20},
}


class _TextExtractor(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._hidden_depth = 0
        self._in_title = False
        self.title_parts: list[str] = []
        self.text_parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in {"script", "style", "noscript"}:
            self._hidden_depth += 1
        if tag == "title":
            self._in_title = True

    def handle_endtag(self, tag: str) -> None:
        if tag in {"script", "style", "noscript"} and self._hidden_depth:
            self._hidden_depth -= 1
        if tag == "title":
            self._in_title = False

    def handle_data(self, data: str) -> None:
        text = " ".join(data.split())
        if not text:
            return
        if self._in_title:
            self.title_parts.append(text)
        if self._hidden_depth == 0:
            self.text_parts.append(text)


def _extract_page_snapshot(stdout: bytes) -> dict[str, Any]:
    text = stdout.decode("utf-8", errors="replace")
    parser = _TextExtractor()
    parser.feed(text)
    title = " ".join(parser.title_parts).strip()
    body_text = " ".join(parser.text_parts).strip()
    return {
        "title": title,
        "text_length": len(body_text),
        "text_sample": body_text[:500],
    }


def _wild_web_extraction_failures(seed: str, snapshot: dict[str, Any]) -> list[str]:
    assertions = WILD_WEB_SEED_ASSERTIONS.get(seed, {})
    failures: list[str] = []
    title = str(snapshot.get("title") or "")
    sample = str(snapshot.get("text_sample") or "")
    text_length = int(snapshot.get("text_length") or 0)
    title_lower = title.lower()
    sample_lower = sample.lower()
    title_any = tuple(str(value).lower() for value in assertions.get("title_any", ()))
    text_any = tuple(str(value).lower() for value in assertions.get("text_any", ()))
    min_text_length = int(assertions.get("min_text_length", 1))
    if not title:
        failures.append("missing-title")
    elif title_any and not any(value in title_lower for value in title_any):
        failures.append("title-keyword-mismatch")
    if text_length < min_text_length:
        failures.append("short-body-text")
    elif text_any and not any(value in sample_lower for value in text_any):
        failures.append("text-keyword-mismatch")
    return failures


def _wild_command_for_target(target: str, binary: Path, url: str, timeout_seconds: float) -> list[str]:
    metadata = target_metadata(target)
    if target_is_cdp(target):
        raise RuntimeError(f"{target} is a CDP target; use the cdp-session suite")
    if target == "chrome":
        raise RuntimeError("chrome wild-web uses the CDP DCL runner")
    timeout_ms = str(int(timeout_seconds * 1000))
    if target in {"moli", "moli-full"}:
        compatibility_args = (
            ["--layout", "--resource"]
            if target_enables_all_resource_fetch(target)
            else []
        )
        return [
            str(binary),
            "fetch",
            *compatibility_args,
            "--dump",
            "html",
            "--wait-until",
            "done",
            "--timeout",
            timeout_ms,
            url,
        ]
    if target == "lightpanda":
        return [
            str(binary),
            "fetch",
            "--dump",
            "html",
            "--wait-until",
            "done",
            "--wait-ms",
            timeout_ms,
            "--http-timeout",
            timeout_ms,
            "--terminate-ms",
            timeout_ms,
            url,
        ]
    if target == "obscura":
        return [
            str(binary),
            "fetch",
            "--dump",
            "html",
            "--wait-until",
            "load",
            "--wait",
            "0",
            "--timeout",
            str(max(1, int(timeout_seconds))),
            url,
        ]
    raise RuntimeError(f"unknown target: {target}")


def _wild_web_target_metadata(target: str) -> dict[str, str]:
    metadata = dict(target_metadata(target))
    if target == "chrome" or target_is_cdp(target):
        metadata["driver"] = "cdp-dcl"
        prefix = "moli full" if target == "moli-full-cdp" else metadata["engine"]
        metadata["label"] = f"{prefix} / cdp-dcl"
    return metadata


def _classify(stdout: bytes, stderr: bytes, returncode: int | None, timed_out: bool) -> str:
    if timed_out:
        return "timeout"
    if returncode != 0:
        return "error"
    snapshot = _extract_page_snapshot(stdout)
    visible_text = " ".join(
        str(snapshot.get(key) or "") for key in ("title", "text_sample")
    ).lower()
    diagnostic_text = stderr.decode("utf-8", errors="replace").lower()
    text = f"{visible_text}\n{diagnostic_text}"
    if "captcha" in text or "verify" in text or "challenge" in text or "安全验证" in text:
        return "challenge"
    if "login" in text or "登录" in text:
        return "login"
    if "blocked" in text or "forbidden" in text or "403" in text:
        return "blocked"
    return "success" if stdout.strip() else "empty"


def _failure_kind(category: str, extraction_failures: list[str], error: str | None = None) -> str | None:
    if error == "target binary unavailable":
        return "target-unavailable"
    if category == "timeout":
        return "timeout"
    if category == "blocked":
        return "blocked"
    if category == "challenge":
        return "challenge"
    if category == "login":
        return "login"
    if category == "empty":
        return "empty-response"
    if category == "error":
        return "process-error"
    if extraction_failures:
        return "extraction-failure"
    return None


def _write_failure_artifacts(
    *,
    suite_dir: Path,
    row: dict[str, Any],
    stdout: bytes,
    stderr: bytes,
) -> str:
    name = f"{row['target']}-run-{row['run']}-{row['seed']}"
    failures_dir = suite_dir / "failures"
    json_path = failures_dir / f"{name}.json"
    stdout_path = failures_dir / f"{name}.stdout.html"
    stderr_path = failures_dir / f"{name}.stderr.txt"
    write_json(json_path, row)
    write_text(stdout_path, stdout[-128 * 1024 :].decode("utf-8", errors="replace"))
    write_text(stderr_path, stderr[-32 * 1024 :].decode("utf-8", errors="replace"))
    return str(json_path.relative_to(suite_dir))


def _write_replay_artifact(
    *,
    suite_dir: Path,
    row: dict[str, Any],
    stdout: bytes,
) -> str:
    name = f"{row['target']}-run-{row['run']}-{row['seed']}.html"
    replay_dir = suite_dir / "replay"
    replay_path = replay_dir / name
    write_text(replay_path, stdout.decode("utf-8", errors="replace"))
    return str(replay_path.relative_to(suite_dir))


def run_wild_web_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    targets: tuple[str, ...],
    seeds: tuple[str, ...],
    runs: int,
    timeout_seconds: float,
    gate_target: str,
    capture_replay: bool = False,
) -> dict[str, Any]:
    unknown_targets = [target for target in targets if target not in WEBFETCH_TARGETS]
    if unknown_targets:
        raise RuntimeError(f"unknown webfetch target(s): {', '.join(unknown_targets)}")
    if gate_target not in WEBFETCH_TARGETS:
        raise RuntimeError(f"unknown gate target: {gate_target}")

    suite_dir = output_dir / "wild-web"
    rows: list[dict[str, Any]] = []
    replay_manifest: list[dict[str, Any]] = []
    selected = seeds or tuple(WILD_WEB_SEEDS.keys())
    for seed in selected:
        if seed not in WILD_WEB_SEEDS:
            raise RuntimeError(f"unknown wild web seed: {seed}")
    for target in targets:
        metadata = _wild_web_target_metadata(target)
        info = target_matrix.get(metadata["binary_key"], {})
        path = info.get("path")
        for run_id in range(1, runs + 1):
            for seed in selected:
                url = WILD_WEB_SEEDS[seed]
                if not info.get("available") or not path:
                    row = {
                        "target": target,
                        **metadata,
                        "run": run_id,
                        "seed": seed,
                        "url": url,
                        "category": "error",
                        "classification_ok": False,
                        "extraction_ok": False,
                        "ok": False,
                        "extraction_failures": ["target-unavailable"],
                        "extraction_failure_count": 1,
                        "failure_kind": "target-unavailable",
                        "error": "target binary unavailable",
                    }
                    row["failure_artifact"] = _write_failure_artifacts(suite_dir=suite_dir, row=row, stdout=b"", stderr=b"")
                    rows.append(row)
                    continue
                proc_env = clear_proxy_env(os.environ)
                if target == "chrome":
                    result = run_chrome_dcl_dump(
                        Path(path),
                        url,
                        cwd=REPO_ROOT,
                        timeout_seconds=timeout_seconds,
                        env=proc_env,
                    )
                elif target_is_cdp(target):
                    result = run_served_cdp_dcl_dump(
                        target,
                        Path(path),
                        url,
                        cwd=REPO_ROOT,
                        timeout_seconds=timeout_seconds,
                        env=proc_env,
                    )
                else:
                    result = run_process(
                        _wild_command_for_target(target, Path(path), url, timeout_seconds),
                        cwd=REPO_ROOT,
                        timeout_seconds=timeout_seconds + 2,
                        env=proc_env,
                    )
                category = _classify(result.stdout, result.stderr, result.returncode, result.timed_out)
                snapshot = _extract_page_snapshot(result.stdout)
                extraction_failures = _wild_web_extraction_failures(seed, snapshot)
                classification_ok = category in {"success", "login", "challenge"}
                extraction_ok = not extraction_failures
                ok = classification_ok and extraction_ok
                row = {
                    "target": target,
                    **metadata,
                    "run": run_id,
                    "seed": seed,
                    "url": url,
                    "category": category,
                    "classification_ok": classification_ok,
                    "extraction_ok": extraction_ok,
                    "ok": ok,
                    "title": snapshot["title"],
                    "text_length": snapshot["text_length"],
                    "text_sample": snapshot["text_sample"],
                    "extraction_failures": extraction_failures,
                    "extraction_failure_count": len(extraction_failures),
                    "failure_kind": None if ok else _failure_kind(category, extraction_failures),
                    "elapsed_ms": result.elapsed_ms,
                    "returncode": result.returncode,
                    "timed_out": result.timed_out,
                    "stdout_bytes": len(result.stdout),
                    "stderr_bytes": len(result.stderr),
                    "stdout_tail": result.stdout[-1024:].decode("utf-8", errors="replace"),
                    "stderr_tail": result.stderr[-1024:].decode("utf-8", errors="replace"),
                    "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
                }
                if not ok:
                    row["failure_artifact"] = _write_failure_artifacts(
                        suite_dir=suite_dir,
                        row=row,
                        stdout=result.stdout,
                        stderr=result.stderr,
                    )
                elif capture_replay:
                    row["replay_artifact"] = _write_replay_artifact(
                        suite_dir=suite_dir,
                        row=row,
                        stdout=result.stdout,
                    )
                    replay_manifest.append(
                        {
                            "target": target,
                            **metadata,
                            "run": run_id,
                            "seed": seed,
                            "url": url,
                            "category": category,
                            "title": snapshot["title"],
                            "text_length": snapshot["text_length"],
                            "artifact": row["replay_artifact"],
                        }
                    )
                rows.append(row)

    gate_failures = sum(1 for row in rows if row["target"] == gate_target and not row.get("ok"))
    summary: dict[str, Any] = {
        "suite": "wild-web",
        "runs": runs,
        "seeds": list(selected),
        "timeout_seconds": timeout_seconds,
        "gate_target": gate_target,
        "gate_failures": gate_failures,
        "targets": {},
        "total_failures": sum(1 for row in rows if not row.get("ok")),
        "replay_capture": bool(capture_replay),
        "replay_artifacts": len(replay_manifest),
    }
    for target in targets:
        target_rows = [row for row in rows if row["target"] == target]
        categories: dict[str, int] = {}
        failure_kinds: dict[str, int] = {}
        for row in target_rows:
            categories[row["category"]] = categories.get(row["category"], 0) + 1
            failure_kind = row.get("failure_kind")
            if isinstance(failure_kind, str) and failure_kind:
                failure_kinds[failure_kind] = failure_kinds.get(failure_kind, 0) + 1
        summary["targets"][target] = {
            **_wild_web_target_metadata(target),
            "seeds": len(target_rows),
            "runs": runs,
            "passes": sum(1 for row in target_rows if row.get("ok")),
            "failures": sum(1 for row in target_rows if not row.get("ok")),
            "extraction_failures": sum(int(row.get("extraction_failure_count", 0) or 0) for row in target_rows),
            "categories": categories,
            "failure_kinds": failure_kinds,
            "elapsed_ms": summarize(row["elapsed_ms"] for row in target_rows if row.get("elapsed_ms") is not None),
            "peak_pss_bytes": summarize(row["peak_pss_bytes"] for row in target_rows if row.get("peak_pss_bytes") is not None),
        }
    write_csv(suite_dir / "raw-runs.csv", rows)
    write_json(suite_dir / "runs.json", rows)
    write_json(suite_dir / "summary.json", summary)
    if capture_replay:
        write_json(
            suite_dir / "replay" / "manifest.json",
            {
                "schema_version": 1,
                "note": "Captured only when --capture-replay or --wild-web-capture-replay is explicitly provided. Caller is responsible for robots/ToS review before publishing.",
                "artifacts": replay_manifest,
            },
        )
    return summary
