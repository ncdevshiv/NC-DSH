from __future__ import annotations

import html.parser
import os
import re
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json, write_text
from .config import REPO_ROOT, clear_proxy_env
from .process import ProcessResult
from .stats import summarize
from .synthetic_compare import WEBFETCH_TARGETS, target_metadata
from .top_sites import (
    DEFAULT_TOP_SITES_MIN_BODY_BYTES,
    DEFAULT_TOP_SITES_PARALLELISM,
    DEFAULT_TOP_SITES_PROFILE,
    DEFAULT_TOP_SITES_SOURCE,
    TOP_SITES_PROFILES,
    _classify,
    _ok_categories,
    _run_top_sites_target,
    _top_sites_target_metadata,
    load_top_sites_entries,
    resolve_top_sites_source,
)


DEFAULT_RENDER_COMPARE_BASELINE = "chrome"
DEFAULT_RENDER_COMPARE_NGRAM_SIZE = 4
DEFAULT_RENDER_COMPARE_MATCH_THRESHOLD = 0.65
DEFAULT_RENDER_COMPARE_PARTIAL_THRESHOLD = 0.35
DEFAULT_RENDER_COMPARE_KEY_HIT_THRESHOLD = 0.70
DEFAULT_RENDER_COMPARE_PARTIAL_KEY_HIT_THRESHOLD = 0.40
DEFAULT_RENDER_COMPARE_MIN_BASELINE_TEXT_CHARS = 500
DEFAULT_RENDER_COMPARE_KEY_PHRASES = 12


_TITLE_RE = re.compile(r"<title[^>]*>(.*?)</title>", re.IGNORECASE | re.DOTALL)
_PHRASE_SPLIT_RE = re.compile(r"[\n\r。！？!?；;：:]+")


class _VisibleTextExtractor(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._skip_depth = 0
        self.parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag.lower() in {"script", "style", "noscript", "svg", "template"}:
            self._skip_depth += 1

    def handle_endtag(self, tag: str) -> None:
        if tag.lower() in {"script", "style", "noscript", "svg", "template"} and self._skip_depth:
            self._skip_depth -= 1

    def handle_data(self, data: str) -> None:
        if self._skip_depth:
            return
        text = " ".join(data.split())
        if text:
            self.parts.append(text)


def _decode_output(stdout: bytes) -> str:
    return stdout.decode("utf-8", errors="replace")


def extract_visible_text(stdout: bytes) -> dict[str, Any]:
    html_text = _decode_output(stdout)
    title = ""
    match = _TITLE_RE.search(html_text)
    if match:
        title = re.sub(r"\s+", " ", match.group(1)).strip()
    parser = _VisibleTextExtractor()
    try:
        parser.feed(html_text)
    except html.parser.HTMLParseError:
        pass
    visible_text = " ".join(parser.parts)
    return {
        "title": title,
        "visible_text": visible_text,
        "visible_text_length": len(visible_text),
        "visible_text_sample": visible_text[:500],
    }


def _compact_text(text: str) -> str:
    lowered = text.lower()
    return "".join(ch for ch in lowered if ch.isalnum() or "\u4e00" <= ch <= "\u9fff")


def _ngrams(text: str, size: int) -> set[str]:
    compact = _compact_text(text)
    if len(compact) < size:
        return {compact} if compact else set()
    return {compact[index : index + size] for index in range(0, len(compact) - size + 1)}


def _ngram_containment(baseline_text: str, target_text: str, size: int) -> float:
    baseline_grams = _ngrams(baseline_text, size)
    if not baseline_grams:
        return 0.0
    target_grams = _ngrams(target_text, size)
    return len(baseline_grams & target_grams) / len(baseline_grams)


def _ngram_jaccard(baseline_text: str, target_text: str, size: int) -> float:
    baseline_grams = _ngrams(baseline_text, size)
    target_grams = _ngrams(target_text, size)
    union = baseline_grams | target_grams
    if not union:
        return 0.0
    return len(baseline_grams & target_grams) / len(union)


def _key_phrases(text: str, limit: int) -> list[str]:
    phrases: list[str] = []
    seen: set[str] = set()
    for part in _PHRASE_SPLIT_RE.split(text):
        normalized = " ".join(part.split())
        compact = _compact_text(normalized)
        if len(compact) < 12:
            continue
        phrase = normalized[:80]
        key = _compact_text(phrase)
        if key in seen:
            continue
        seen.add(key)
        phrases.append(phrase)
        if len(phrases) >= limit:
            break
    return phrases


def _phrase_hit_rate(phrases: list[str], text: str) -> float:
    if not phrases:
        return 0.0
    compact_text = _compact_text(text)
    hits = sum(1 for phrase in phrases if _compact_text(phrase) in compact_text)
    return hits / len(phrases)


def _capped_ratio(numerator: int, denominator: int) -> float:
    if denominator <= 0:
        return 0.0
    return min(1.0, max(0.0, numerator / denominator))


def _quality_score(*, containment: float, key_hit_rate: float, text_ratio: float) -> float:
    score = 100.0 * (0.55 * containment + 0.35 * key_hit_rate + 0.10 * text_ratio)
    return round(max(0.0, min(100.0, score)), 2)


def compare_to_baseline(
    *,
    baseline_stdout: bytes,
    baseline_category: str,
    target_stdout: bytes,
    target_stderr: bytes,
    target_category: str,
    ngram_size: int = DEFAULT_RENDER_COMPARE_NGRAM_SIZE,
    match_threshold: float = DEFAULT_RENDER_COMPARE_MATCH_THRESHOLD,
    partial_threshold: float = DEFAULT_RENDER_COMPARE_PARTIAL_THRESHOLD,
    key_hit_threshold: float = DEFAULT_RENDER_COMPARE_KEY_HIT_THRESHOLD,
    partial_key_hit_threshold: float = DEFAULT_RENDER_COMPARE_PARTIAL_KEY_HIT_THRESHOLD,
    min_baseline_text_chars: int = DEFAULT_RENDER_COMPARE_MIN_BASELINE_TEXT_CHARS,
    key_phrase_limit: int = DEFAULT_RENDER_COMPARE_KEY_PHRASES,
) -> dict[str, Any]:
    baseline_snapshot = extract_visible_text(baseline_stdout)
    target_snapshot = extract_visible_text(target_stdout)
    baseline_text = str(baseline_snapshot["visible_text"])
    target_text = str(target_snapshot["visible_text"])
    phrases = _key_phrases(baseline_text, key_phrase_limit)
    containment = _ngram_containment(baseline_text, target_text, ngram_size)
    jaccard = _ngram_jaccard(baseline_text, target_text, ngram_size)
    key_hit_rate = _phrase_hit_rate(phrases, target_text)
    raw_text = _decode_output(target_stdout) + "\n" + target_stderr.decode("utf-8", errors="replace")
    raw_ngram_containment = _ngram_containment(baseline_text, raw_text, ngram_size)
    raw_key_hit_rate = _phrase_hit_rate(phrases, raw_text)
    baseline_visible_length = int(baseline_snapshot["visible_text_length"])
    target_visible_length = int(target_snapshot["visible_text_length"])
    visible_text_ratio = _capped_ratio(target_visible_length, baseline_visible_length)
    raw_text_ratio = _capped_ratio(len(_compact_text(raw_text)), max(len(_compact_text(baseline_text)), 1))
    render_quality_score = _quality_score(
        containment=containment,
        key_hit_rate=key_hit_rate,
        text_ratio=visible_text_ratio,
    )
    raw_content_score = _quality_score(
        containment=raw_ngram_containment,
        key_hit_rate=raw_key_hit_rate,
        text_ratio=raw_text_ratio,
    )

    if baseline_category not in _ok_categories():
        category = "baseline-unusable"
    elif baseline_visible_length < min_baseline_text_chars:
        category = "baseline-thin"
    elif (
        (raw_ngram_containment >= partial_threshold or raw_key_hit_rate >= partial_key_hit_threshold)
        and containment < partial_threshold
        and key_hit_rate < partial_key_hit_threshold
    ):
        category = "state-only-content"
    elif target_category not in _ok_categories():
        category = target_category
    elif containment >= match_threshold and key_hit_rate >= key_hit_threshold:
        category = "render-match"
    elif containment >= max(match_threshold, 0.80):
        category = "render-match"
    elif containment >= partial_threshold or key_hit_rate >= partial_key_hit_threshold:
        category = "render-partial"
    else:
        category = "render-mismatch"
    excluded = category.startswith("baseline-")

    return {
        "category": category,
        "excluded": excluded,
        "ok": category == "render-match",
        "baseline_title": baseline_snapshot["title"],
        "target_title": target_snapshot["title"],
        "baseline_visible_text_length": baseline_visible_length,
        "target_visible_text_length": target_visible_length,
        "visible_text_ratio": visible_text_ratio,
        "baseline_visible_text_sample": baseline_snapshot["visible_text_sample"],
        "target_visible_text_sample": target_snapshot["visible_text_sample"],
        "ngram_size": ngram_size,
        "ngram_containment": containment,
        "ngram_jaccard": jaccard,
        "key_phrase_count": len(phrases),
        "key_phrase_hit_rate": key_hit_rate,
        "raw_ngram_containment": raw_ngram_containment,
        "raw_key_phrase_hit_rate": raw_key_hit_rate,
        "render_quality_score": render_quality_score,
        "raw_content_score": raw_content_score,
        "key_phrases": phrases,
    }


def _domain_to_url(domain: str) -> str:
    if domain.startswith("http://") or domain.startswith("https://"):
        return domain
    return f"https://{domain}"


def _execute_fetch(
    *,
    target: str,
    info: dict[str, Any],
    rank: int,
    domain: str,
    timeout_seconds: float,
    min_body_bytes: int,
    proc_env: dict[str, str],
) -> dict[str, Any]:
    metadata = _top_sites_target_metadata(target)
    url = _domain_to_url(domain)
    if not info.get("available") or not info.get("path"):
        return {
            "target": target,
            **metadata,
            "rank": rank,
            "domain": domain,
            "url": url,
            "category": "target-unavailable",
            "ok": False,
            "returncode": None,
            "timed_out": False,
            "elapsed_ms": None,
            "stdout_bytes": 0,
            "stderr_bytes": 0,
            "stdout": b"",
            "stderr": b"",
            "peak_pss_bytes": None,
        }
    result: ProcessResult = _run_top_sites_target(
        target=target,
        binary=Path(info["path"]),
        url=url,
        timeout_seconds=timeout_seconds,
        proc_env=proc_env,
    )
    category = _classify(result.stdout, result.stderr, result.returncode, result.timed_out, min_body_bytes)
    return {
        "target": target,
        **metadata,
        "rank": rank,
        "domain": domain,
        "url": url,
        "category": category,
        "ok": category in _ok_categories(),
        "returncode": result.returncode,
        "timed_out": result.timed_out,
        "elapsed_ms": result.elapsed_ms,
        "stdout_bytes": len(result.stdout),
        "stderr_bytes": len(result.stderr),
        "stdout": result.stdout,
        "stderr": result.stderr,
        "stderr_tail": result.stderr[-512:].decode("utf-8", errors="replace"),
        "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
        "peak_rss_bytes": result.resources.get("peak_rss_bytes"),
    }


def _baseline_site_row(
    *,
    baseline: dict[str, Any],
    baseline_target: str,
    min_baseline_text_chars: int,
) -> dict[str, Any]:
    snapshot = extract_visible_text(baseline["stdout"])
    visible_length = int(snapshot["visible_text_length"])
    if str(baseline["category"]) not in _ok_categories():
        category = "baseline-unusable"
        usable = False
    elif visible_length < min_baseline_text_chars:
        category = "baseline-thin"
        usable = False
    else:
        category = "baseline-usable"
        usable = True
    return {
        "rank": baseline["rank"],
        "domain": baseline["domain"],
        "url": baseline["url"],
        "baseline_target": baseline_target,
        "baseline_fetch_category": baseline["category"],
        "category": category,
        "usable": usable,
        "baseline_title": snapshot["title"],
        "baseline_visible_text_length": visible_length,
        "baseline_visible_text_sample": snapshot["visible_text_sample"],
        "baseline_elapsed_ms": baseline["elapsed_ms"],
        "baseline_returncode": baseline["returncode"],
        "baseline_timed_out": baseline["timed_out"],
        "baseline_stdout_bytes": baseline["stdout_bytes"],
        "baseline_stderr_bytes": baseline["stderr_bytes"],
        "baseline_peak_pss_bytes": baseline.get("peak_pss_bytes"),
        "baseline_peak_rss_bytes": baseline.get("peak_rss_bytes"),
        "baseline_stderr_tail": baseline.get("stderr_tail", ""),
    }


def _write_compare_artifact(*, suite_dir: Path, row: dict[str, Any], target_result: dict[str, Any]) -> str:
    failures_dir = suite_dir / "failures"
    domain = re.sub(r"[^A-Za-z0-9._-]+", "_", str(row["domain"])).strip("._-") or "site"
    name = f"{row['target']}-rank{int(row['rank']):03d}-{domain[:80]}"
    json_path = failures_dir / f"{name}.json"
    write_json(json_path, row)
    if target_result.get("stderr"):
        write_text(json_path.with_suffix(".stderr.txt"), target_result["stderr"][-16 * 1024 :].decode("utf-8", errors="replace"))
    if target_result.get("stdout"):
        write_text(json_path.with_suffix(".stdout.html"), target_result["stdout"][-32 * 1024 :].decode("utf-8", errors="replace"))
    return str(json_path.relative_to(suite_dir))


def run_render_compare_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    targets: tuple[str, ...],
    baseline_target: str = DEFAULT_RENDER_COMPARE_BASELINE,
    profile: str = DEFAULT_TOP_SITES_PROFILE,
    list_path: Path | None = None,
    source: str = DEFAULT_TOP_SITES_SOURCE,
    timeout_seconds: float = 30.0,
    gate_target: str = "moli",
    parallelism: int = DEFAULT_TOP_SITES_PARALLELISM,
    min_body_bytes: int = DEFAULT_TOP_SITES_MIN_BODY_BYTES,
    limit_override: int | None = None,
    ngram_size: int = DEFAULT_RENDER_COMPARE_NGRAM_SIZE,
    match_threshold: float = DEFAULT_RENDER_COMPARE_MATCH_THRESHOLD,
    partial_threshold: float = DEFAULT_RENDER_COMPARE_PARTIAL_THRESHOLD,
    key_hit_threshold: float = DEFAULT_RENDER_COMPARE_KEY_HIT_THRESHOLD,
    partial_key_hit_threshold: float = DEFAULT_RENDER_COMPARE_PARTIAL_KEY_HIT_THRESHOLD,
    min_baseline_text_chars: int = DEFAULT_RENDER_COMPARE_MIN_BASELINE_TEXT_CHARS,
) -> dict[str, Any]:
    selected_targets = tuple(dict.fromkeys(targets))
    all_targets = tuple(dict.fromkeys((baseline_target, *selected_targets)))
    unknown_targets = [target for target in all_targets if target not in WEBFETCH_TARGETS]
    if unknown_targets:
        raise RuntimeError(f"unknown webfetch target(s): {', '.join(unknown_targets)}")
    if baseline_target not in WEBFETCH_TARGETS:
        raise RuntimeError(f"unknown baseline target: {baseline_target}")
    if gate_target not in WEBFETCH_TARGETS:
        raise RuntimeError(f"unknown gate target: {gate_target}")
    if gate_target not in selected_targets:
        raise RuntimeError("render-compare gate target must be one of the selected --target values")
    if profile not in TOP_SITES_PROFILES:
        raise RuntimeError(f"unknown top-sites profile `{profile}`; expected one of {sorted(TOP_SITES_PROFILES)}")
    limit = int(limit_override if limit_override is not None else TOP_SITES_PROFILES[profile]["limit"])
    if limit <= 0:
        raise RuntimeError("render-compare limit must be positive")
    if parallelism <= 0:
        raise RuntimeError("render-compare parallelism must be positive")

    suite_dir = output_dir / "render-compare"
    resolved_source, primary_path = resolve_top_sites_source(source, list_path)
    entries_all, list_source_labels = load_top_sites_entries(resolved_source, list_path)
    entries = entries_all[:limit]
    proc_env = clear_proxy_env(os.environ)

    baseline_rows: list[dict[str, Any]] = []
    baseline_info = target_matrix.get(target_metadata(baseline_target)["binary_key"], {})
    with ThreadPoolExecutor(max_workers=parallelism) as executor:
        future_to_spec = {
            executor.submit(
                _execute_fetch,
                target=baseline_target,
                info=baseline_info,
                rank=rank,
                domain=domain,
                timeout_seconds=timeout_seconds,
                min_body_bytes=min_body_bytes,
                proc_env=proc_env,
            ): (baseline_target, rank, domain)
            for rank, domain in entries
        }
        for future in as_completed(future_to_spec):
            row = future.result()
            row["stage"] = "baseline"
            baseline_rows.append(row)

    baseline_rows.sort(key=lambda row: int(row["rank"]))
    baseline_site_rows = [
        _baseline_site_row(
            baseline=row,
            baseline_target=baseline_target,
            min_baseline_text_chars=min_baseline_text_chars,
        )
        for row in baseline_rows
    ]
    baseline_usable_entries = [
        (int(row["rank"]), str(row["domain"]))
        for row in baseline_site_rows
        if row["usable"]
    ]

    target_fetch_rows: list[dict[str, Any]] = []
    target_job_specs = [
        (target, target_matrix.get(target_metadata(target)["binary_key"], {}), rank, domain)
        for target in selected_targets
        if target != baseline_target
        for rank, domain in baseline_usable_entries
    ]
    if target_job_specs:
        with ThreadPoolExecutor(max_workers=parallelism) as executor:
            future_to_spec = {
                executor.submit(
                    _execute_fetch,
                    target=target,
                    info=info,
                    rank=rank,
                    domain=domain,
                    timeout_seconds=timeout_seconds,
                    min_body_bytes=min_body_bytes,
                    proc_env=proc_env,
                ): (target, rank, domain)
                for target, info, rank, domain in target_job_specs
            }
            for future in as_completed(future_to_spec):
                row = future.result()
                row["stage"] = "target"
                target_fetch_rows.append(row)

    target_fetch_rows.sort(key=lambda row: (int(row["rank"]), row["target"]))
    fetch_rows = [*baseline_rows, *target_fetch_rows]
    by_site_target = {(int(row["rank"]), str(row["domain"]), row["target"]): row for row in fetch_rows}

    compare_rows: list[dict[str, Any]] = []
    baseline_site_by_key = {(int(row["rank"]), str(row["domain"])): row for row in baseline_site_rows}
    for rank, domain in baseline_usable_entries:
        baseline = by_site_target[(rank, domain, baseline_target)]
        for target in selected_targets:
            target_result = by_site_target[(rank, domain, target)] if target != baseline_target else baseline
            comparison = compare_to_baseline(
                baseline_stdout=baseline["stdout"],
                baseline_category=str(baseline["category"]),
                target_stdout=target_result["stdout"],
                target_stderr=target_result["stderr"],
                target_category=str(target_result["category"]),
                ngram_size=ngram_size,
                match_threshold=match_threshold,
                partial_threshold=partial_threshold,
                key_hit_threshold=key_hit_threshold,
                partial_key_hit_threshold=partial_key_hit_threshold,
                min_baseline_text_chars=min_baseline_text_chars,
            )
            row = {
                "target": target,
                **target_metadata(target),
                "rank": rank,
                "domain": domain,
                "url": _domain_to_url(domain),
                "baseline_target": baseline_target,
                "baseline_category": baseline["category"],
                "baseline_gate_category": baseline_site_by_key[(rank, domain)]["category"],
                "target_fetch_category": target_result["category"],
                "category": comparison["category"],
                "ok": comparison["ok"],
                "elapsed_ms": target_result["elapsed_ms"],
                "baseline_elapsed_ms": baseline["elapsed_ms"],
                "returncode": target_result["returncode"],
                "timed_out": target_result["timed_out"],
                "stdout_bytes": target_result["stdout_bytes"],
                "stderr_bytes": target_result["stderr_bytes"],
                "peak_pss_bytes": target_result["peak_pss_bytes"],
                "peak_rss_bytes": target_result.get("peak_rss_bytes"),
                "stderr_tail": target_result.get("stderr_tail", ""),
                **comparison,
            }
            if not row["ok"] and not row["excluded"]:
                row["failure_artifact"] = _write_compare_artifact(
                    suite_dir=suite_dir,
                    row={key: value for key, value in row.items() if key != "key_phrases"},
                    target_result=target_result,
                )
            compare_rows.append(row)

    csv_rows = [{key: value for key, value in row.items() if key != "key_phrases"} for row in compare_rows]
    fetch_json_rows = [
        {key: value for key, value in row.items() if key not in {"stdout", "stderr"}}
        for row in fetch_rows
    ]
    baseline_json_rows = [
        {key: value for key, value in row.items() if key not in {"stdout", "stderr"}}
        for row in baseline_rows
    ]
    gate_failures = sum(
        1
        for row in compare_rows
        if row["target"] == gate_target and not row["excluded"] and not row["ok"]
    )
    total_failures = sum(1 for row in compare_rows if not row["excluded"] and not row["ok"])
    summary: dict[str, Any] = {
        "suite": "render-compare",
        "profile": profile,
        "limit": limit,
        "source": resolved_source,
        "list_source": str(primary_path.relative_to(REPO_ROOT)) if primary_path.is_relative_to(REPO_ROOT) else str(primary_path),
        "list_sources": list_source_labels,
        "site_count": len(entries),
        "baseline_site_count": len(baseline_site_rows),
        "evaluated_site_count": len(baseline_usable_entries),
        "baseline_excluded_site_count": len(baseline_site_rows) - len(baseline_usable_entries),
        "baseline_target": baseline_target,
        "targets": {},
        "timeout_seconds": timeout_seconds,
        "parallelism": parallelism,
        "min_body_bytes": min_body_bytes,
        "min_baseline_text_chars": min_baseline_text_chars,
        "ngram_size": ngram_size,
        "match_threshold": match_threshold,
        "partial_threshold": partial_threshold,
        "key_hit_threshold": key_hit_threshold,
        "partial_key_hit_threshold": partial_key_hit_threshold,
        "gate_target": gate_target,
        "gate_failures": gate_failures,
        "total_failures": total_failures,
        "excluded_rows": (len(baseline_site_rows) - len(baseline_usable_entries)) * len(selected_targets),
        "skipped_target_rows": (len(baseline_site_rows) - len(baseline_usable_entries)) * len(selected_targets),
    }
    baseline_categories: dict[str, int] = {}
    for row in baseline_site_rows:
        baseline_categories[str(row["category"])] = baseline_categories.get(str(row["category"]), 0) + 1
    summary["baseline_categories"] = baseline_categories
    for target in selected_targets:
        target_rows = [row for row in compare_rows if row["target"] == target]
        evaluated_rows = [row for row in target_rows if not row["excluded"]]
        categories: dict[str, int] = {}
        for row in target_rows:
            categories[str(row["category"])] = categories.get(str(row["category"]), 0) + 1
        summary["targets"][target] = {
            **_top_sites_target_metadata(target),
            "sites": len(entries),
            "evaluated_sites": len(evaluated_rows),
            "excluded_sites": len(baseline_site_rows) - len(baseline_usable_entries),
            "passes": sum(1 for row in evaluated_rows if row["ok"]),
            "failures": sum(1 for row in evaluated_rows if not row["ok"]),
            "categories": categories,
            "elapsed_ms": summarize(row["elapsed_ms"] for row in evaluated_rows if row.get("elapsed_ms") is not None),
            "ngram_containment": summarize(row["ngram_containment"] for row in evaluated_rows),
            "raw_ngram_containment": summarize(row["raw_ngram_containment"] for row in evaluated_rows),
            "key_phrase_hit_rate": summarize(row["key_phrase_hit_rate"] for row in evaluated_rows),
            "raw_key_phrase_hit_rate": summarize(row["raw_key_phrase_hit_rate"] for row in evaluated_rows),
            "render_quality_score": summarize(row["render_quality_score"] for row in evaluated_rows),
            "raw_content_score": summarize(row["raw_content_score"] for row in evaluated_rows),
            "peak_pss_bytes": summarize(row["peak_pss_bytes"] for row in evaluated_rows if row.get("peak_pss_bytes") is not None),
            "peak_rss_bytes": summarize(row["peak_rss_bytes"] for row in evaluated_rows if row.get("peak_rss_bytes") is not None),
        }

    write_csv(suite_dir / "raw-runs.csv", csv_rows)
    write_json(suite_dir / "runs.json", compare_rows)
    write_json(suite_dir / "fetch-runs.json", fetch_json_rows)
    write_json(suite_dir / "baseline-runs.json", baseline_json_rows)
    write_json(suite_dir / "baseline-sites.json", baseline_site_rows)
    write_csv(suite_dir / "baseline-sites.csv", baseline_site_rows)
    write_json(suite_dir / "summary.json", summary)
    return summary
