from __future__ import annotations

import os
import re
import html.parser
from hashlib import sha256
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json, write_text
from .chrome_dcl import run_chrome_dcl_dump, run_served_cdp_dcl_dump
from .config import REPO_ROOT, clear_proxy_env
from .process import ProcessResult, run_process
from .stats import summarize
from .synthetic_compare import (
    WEBFETCH_TARGETS,
    target_enables_all_resource_fetch,
    target_is_cdp,
    target_metadata,
)


TOP_SITES_LIST_PATH = REPO_ROOT / "docs" / "chinese-community-top100-websites.md"
GLOBAL_TOP_SITES_LIST_PATH = REPO_ROOT / "docs" / "global-top-websites-seed-list.md"
WEBFETCH_LONGTAIL_LIST_PATH = REPO_ROOT / "docs" / "webfetch-longtail-seed-list.md"
RENDER_QUALITY_LIST_PATH = REPO_ROOT / "docs" / "render-quality-seed-list.md"
LEGACY_ENCODING_LIST_PATH = REPO_ROOT / "docs" / "legacy-encoding-websites-seed-list.md"

TOP_SITES_SOURCES: dict[str, dict[str, Any]] = {
    "chinese-community": {
        "path": TOP_SITES_LIST_PATH,
        "label": "Chinese community top 100 (curated)",
    },
    "global": {
        "path": GLOBAL_TOP_SITES_LIST_PATH,
        "label": "Tranco-derived English-world top sites",
    },
    "webfetch-longtail": {
        "path": WEBFETCH_LONGTAIL_LIST_PATH,
        "label": "Observed WebFetch longtail URL failures",
    },
    "render-quality": {
        "path": RENDER_QUALITY_LIST_PATH,
        "label": "Curated article/document URLs for rendered-DOM quality checks",
    },
    "legacy-encoding": {
        "path": LEGACY_ENCODING_LIST_PATH,
        "label": "Curated non-UTF-8 public pages for document/script encoding checks",
    },
}

COMPOSITE_TOP_SITES_SOURCES = ("mixed", "webfetch-mix")

DEFAULT_TOP_SITES_SOURCE = "chinese-community"

TOP_SITES_PROFILES: dict[str, dict[str, Any]] = {
    "quick": {"limit": 20, "default_runs": 1},
    "full": {"limit": 100, "default_runs": 1},
    "webfetch": {"limit": 300, "default_runs": 1},
}

DEFAULT_TOP_SITES_PROFILE = "quick"
DEFAULT_TOP_SITES_MIN_BODY_BYTES = 256
DEFAULT_TOP_SITES_PARALLELISM_CAP = 8


def _default_top_sites_parallelism() -> int:
    if hasattr(os, "sched_getaffinity"):
        try:
            return min(DEFAULT_TOP_SITES_PARALLELISM_CAP, max(1, len(os.sched_getaffinity(0))))
        except OSError:
            pass
    return min(DEFAULT_TOP_SITES_PARALLELISM_CAP, max(1, os.cpu_count() or 1))


DEFAULT_TOP_SITES_PARALLELISM = _default_top_sites_parallelism()
DEFAULT_TOP_SITES_TEXT_SAMPLE_CHARS = 500

_TOP_LIST_HEADING = re.compile(r"^##\s+Top\s+\d+\b", re.IGNORECASE)
_NEXT_SECTION_HEADING = re.compile(r"^##\s+")
_LIST_ENTRY = re.compile(r"^\s*(\d+)\.\s+`([^`]+)`")
_BLOCKED_TITLE_403 = re.compile(r"^\s*(?:http\s*)?403\b")
_SAFE_ARTIFACT_TOKEN = re.compile(r"[^A-Za-z0-9._-]+")
_TITLE_RE = re.compile(r"<title[^>]*>(.*?)</title>", re.IGNORECASE | re.DOTALL)
_NETWORK_ERROR_MARKERS = (
    "privacy error",
    "your connection is not private",
    "net::err_cert",
    "this site can't be reached",
    "this site can\u2019t be reached",
    "err_name_not_resolved",
    "err_connection",
    "err_timed_out",
    "dns_probe_finished",
    "could not resolve",
    "could not resolve host",
    "could not resolve hostname",
    "could not connect to server",
    "name resolution",
    "connection refused",
    "connection reset",
    "connection timed out",
    "no route to host",
    "network is unreachable",
    "ssl handshake",
    "ssl error",
    "ssl connect error",
    "tls handshake",
    "timed out connecting",
    "timeout was reached",
    "request timeout",
    "i/o error",
    "broken pipe",
    "operation timed out",
    "failed to connect",
    "dns lookup",
    "curl request failed",
    "recv failure",
)
_CLI_DEADLINE_COMMAND_MARKERS = (
    "fetch_document_allow_http_error_with_wait_until",
    "fetch document allow-http-error",
    "fetch allow-http-error wait_until",
    "fetch wait_until",
)
_CLI_DEADLINE_TIMEOUT_MARKERS = (
    "timed out after",
)
_RAW_DOCUMENT_BODY_PROGRESS_RE = re.compile(
    r"with\s+(\d+)\s+out\s+of\s+(\d+)\s+bytes\s+received",
    re.IGNORECASE,
)
_ELAPSED_FAILURE_TIMEOUT_GRACE_SECONDS = 1.0
_CAPTCHA_MARKERS = (
    "captcha",
    "安全验证",
    "验证你是真人",
    "人机验证",
    "human verification",
    "verification",
    "verify you are human",
    "verify that you're not a robot",
    "verify that you are not a robot",
    "are you a robot",
    "not a robot",
    "bot or not",
    "checking your browser",
    "perform security check",
    "security verification",
    "performing security verification",
    "请完成验证",
)
_LOGIN_MARKERS = (
    "sign in",
    "log in",
    "login",
    "登录",
    "注册",
    "账号",
)
_LOGIN_FORM_MARKERS = (
    "login to your account",
    "email/username",
    "your password is a required field",
    "forgot password",
    "create a free account",
)
_LOGIN_CONTEXT_MARKERS = (
    "password",
    "验证码",
    "短信验证码",
    "语音验证码",
    "手机号",
    "手机验证",
    "邮箱",
    "email",
)
_JS_CHALLENGE_MARKERS = (
    "__cf_chl_",
    "c2wf946j0/probe",
    "cf-challenge",
    "__tencent_chaos_vm",
    "__eo_jschallenge_vm",
    "teojschallengesdk.js",
    "eojschallengesdk",
    "window.solvechallenge(",
    "jsl_clearance",
    "acw_sc__v2",
    "window._phantom",
    "_$jsvmprt",
    "awswaf",
    "aliyun_waf",
    "aliyun waf",
    "_waf_",
    "just a moment...",
    "vercel security checkpoint",
    "we're verifying your browser",
    "we\u2019re verifying your browser",
    "enable javascript and cookies to continue",
    "javascript is needed to access this site",
)
_BLOCKED_BODY_MARKERS = (
    "unable to give you access to our site",
    "access denied",
    "access to this page has been denied",
    "sorry, you have been blocked",
    "security issue was automatically identified",
    "waf拦截",
    "被waf拦截",
)
_NOT_FOUND_MARKERS = (
    "404 not found",
    "file not found",
    "page not found",
    "this page cannot be found",
    "page you requested is missing",
    "page you requested could not be found",
)
_FORBIDDEN_ERROR_MARKERS = (
    "403 forbidden",
    "returned 403",
    "http 403",
)
_NOT_FOUND_ERROR_MARKERS = (
    "returned 404",
    "http 404",
)
_SHELL_MARKERS = (
    'id="root"',
    'id="app"',
    'id="__next"',
    'data-reactroot',
    "webpackJsonp",
    "__NUXT__",
    "__NEXT_DATA__",
)


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


def _extract_title_and_text(stdout: bytes) -> dict[str, Any]:
    html_text = stdout.decode("utf-8", errors="replace")
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
    visible_text = re.sub(r"\s+", " ", visible_text).strip()
    return {
        "title": title,
        "text_length": len(visible_text),
        "text_sample": visible_text[:DEFAULT_TOP_SITES_TEXT_SAMPLE_CHARS],
    }


def _looks_like_binary_content(body: bytes) -> bool:
    sample = body[:4096]
    if sample.startswith(b"%PDF-"):
        return True
    if b"\x00" in sample:
        return True
    decoded = sample.decode("utf-8", errors="replace")
    replacement_count = decoded.count("\ufffd")
    return replacement_count > max(8, len(decoded) // 20)


def _looks_like_raw_binary_main_resource_timeout(text: str, min_body_bytes: int) -> bool:
    if "failed to read raw document body" not in text:
        return False
    if "timeout was reached" not in text and "operation timed out" not in text:
        return False
    match = _RAW_DOCUMENT_BODY_PROGRESS_RE.search(text)
    if match is None:
        return False
    received = int(match.group(1))
    total = int(match.group(2))
    return received >= min_body_bytes and total >= received


def _parse_top_sites_sections(path: Path) -> list[tuple[str, list[tuple[int, str]]]]:
    if not path.exists():
        raise RuntimeError(f"top sites list not found: {path}")
    in_section = False
    current_heading = ""
    current_entries: list[tuple[int, str]] = []
    sections: list[tuple[str, list[tuple[int, str]]]] = []
    seen_domains: set[str] = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.rstrip()
        if not in_section:
            if _TOP_LIST_HEADING.match(line):
                in_section = True
                current_heading = line
                current_entries = []
                seen_domains = set()
            continue
        if _NEXT_SECTION_HEADING.match(line):
            if current_entries:
                sections.append((current_heading, current_entries))
            top_heading = _TOP_LIST_HEADING.match(line)
            in_section = bool(top_heading)
            current_heading = line if top_heading else ""
            current_entries = []
            seen_domains = set()
            continue
        match = _LIST_ENTRY.match(line)
        if match:
            rank = int(match.group(1))
            domain = match.group(2).strip()
            if domain and domain not in seen_domains:
                seen_domains.add(domain)
                current_entries.append((rank, domain))
    if in_section and current_entries:
        sections.append((current_heading, current_entries))
    if not sections:
        raise RuntimeError(f"no Top entries parsed from {path}")
    return sections


def parse_top_sites_list(path: Path) -> list[tuple[int, str]]:
    return _parse_top_sites_sections(path)[0][1]


def _parse_top_sites_list_by_count(path: Path, count: int) -> list[tuple[int, str]]:
    heading = re.compile(rf"^##\s+Top\s+{count}\b", re.IGNORECASE)
    for section_heading, entries in _parse_top_sites_sections(path):
        if heading.match(section_heading):
            return entries
    raise RuntimeError(f"no Top {count} entries parsed from {path}")


def resolve_top_sites_source(source: str, list_path: Path | None) -> tuple[str, Path]:
    if list_path is not None:
        return ("custom", list_path)
    if source not in TOP_SITES_SOURCES:
        if source == "mixed":
            return (source, REPO_ROOT / "docs" / "mixed-top-websites")
        if source == "webfetch-mix":
            return (source, REPO_ROOT / "docs" / "webfetch-mix-websites")
        raise RuntimeError(
            f"unknown top-sites source `{source}`; expected one of "
            f"{sorted(TOP_SITES_SOURCES) + list(COMPOSITE_TOP_SITES_SOURCES)} or a --list-path override"
        )
    return (source, TOP_SITES_SOURCES[source]["path"])


def _interleave_entries(*entry_lists: list[tuple[int, str]]) -> list[tuple[int, str]]:
    interleaved: list[tuple[int, str]] = []
    seen: set[str] = set()
    max_len = max((len(entries) for entries in entry_lists), default=0)
    for index in range(max_len):
        for entries in entry_lists:
            if index >= len(entries):
                continue
            _, domain = entries[index]
            if domain in seen:
                continue
            seen.add(domain)
            interleaved.append((len(interleaved) + 1, domain))
    return interleaved


def _append_unique(
    base: list[tuple[int, str]],
    extra: list[tuple[int, str]],
    *,
    limit: int | None = None,
) -> list[tuple[int, str]]:
    combined: list[tuple[int, str]] = []
    seen: set[str] = set()
    for _, domain in [*base, *extra]:
        if domain in seen:
            continue
        seen.add(domain)
        combined.append((len(combined) + 1, domain))
        if limit is not None and len(combined) >= limit:
            break
    return combined


def load_top_sites_entries(source: str, list_path: Path | None) -> tuple[list[tuple[int, str]], list[str]]:
    """Return (entries, source_labels) for the requested source.

    `mixed` interleaves entries from chinese-community and global sources, taking
    one from each in rank order, to make a balanced cross-region list.

    `webfetch-mix` keeps that top-site coverage but caps it at 100 entries, then
    appends observed longtail URL paths from the WebFetch failure corpus.
    """
    if list_path is not None:
        return parse_top_sites_list(list_path), [f"custom:{list_path.name}"]
    if source == "mixed":
        cn = parse_top_sites_list(TOP_SITES_SOURCES["chinese-community"]["path"])
        gl = _parse_top_sites_list_by_count(GLOBAL_TOP_SITES_LIST_PATH, 100)
        interleaved = _interleave_entries(cn, gl)
        labels = [
            f"chinese-community:{TOP_SITES_SOURCES['chinese-community']['path'].name}",
            f"global:{GLOBAL_TOP_SITES_LIST_PATH.name}",
        ]
        return interleaved, labels
    if source == "webfetch-mix":
        cn = parse_top_sites_list(TOP_SITES_SOURCES["chinese-community"]["path"])
        gl = _parse_top_sites_list_by_count(GLOBAL_TOP_SITES_LIST_PATH, 100)
        top_site_mix = _interleave_entries(cn, gl)[:100]
        longtail = parse_top_sites_list(WEBFETCH_LONGTAIL_LIST_PATH)
        entries = _append_unique(top_site_mix, longtail)
        labels = [
            f"chinese-community:{TOP_SITES_SOURCES['chinese-community']['path'].name}",
            f"global:{GLOBAL_TOP_SITES_LIST_PATH.name}",
            f"webfetch-longtail:{WEBFETCH_LONGTAIL_LIST_PATH.name}",
        ]
        return entries, labels
    if source not in TOP_SITES_SOURCES:
        raise RuntimeError(
            f"unknown top-sites source `{source}`; expected one of "
            f"{sorted(TOP_SITES_SOURCES) + list(COMPOSITE_TOP_SITES_SOURCES)} or a --list-path override"
        )
    info = TOP_SITES_SOURCES[source]
    entries = (
        _parse_top_sites_list_by_count(info["path"], 100)
        if source == "global"
        else parse_top_sites_list(info["path"])
    )
    return entries, [f"{source}:{info['path'].name}"]


def _top_command_for_target(target: str, binary: Path, url: str, timeout_seconds: float) -> list[str]:
    metadata = target_metadata(target)
    if target_is_cdp(target):
        raise RuntimeError(f"{target} is a CDP target; use the cdp-session suite")
    if target == "chrome":
        raise RuntimeError("chrome top-sites uses the CDP DCL runner")
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
            "domcontentloaded",
            "--timeout",
            timeout_ms,
            "--http-timeout",
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
            "domcontentloaded",
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


def _top_sites_target_metadata(target: str) -> dict[str, str]:
    metadata = dict(target_metadata(target))
    if target == "chrome" or target_is_cdp(target):
        metadata["driver"] = "cdp-dcl"
        prefix = "moli full" if target == "moli-full-cdp" else metadata["engine"]
        metadata["label"] = f"{prefix} / cdp-dcl"
    return metadata


def _run_top_sites_target(
    *,
    target: str,
    binary: Path,
    url: str,
    timeout_seconds: float,
    proc_env: dict[str, str],
) -> ProcessResult:
    if target == "chrome":
        return run_chrome_dcl_dump(
            binary,
            url,
            cwd=REPO_ROOT,
            timeout_seconds=timeout_seconds,
            env=proc_env,
        )
    if target_is_cdp(target):
        return run_served_cdp_dcl_dump(
            target,
            binary,
            url,
            cwd=REPO_ROOT,
            timeout_seconds=timeout_seconds,
            env=proc_env,
        )
    command = _top_command_for_target(target, binary, url, timeout_seconds)
    return run_process(
        command,
        cwd=REPO_ROOT,
        timeout_seconds=timeout_seconds + 2,
        env=proc_env,
    )


def _classify(
    stdout: bytes,
    stderr: bytes,
    returncode: int | None,
    timed_out: bool,
    min_body_bytes: int,
    snapshot: dict[str, Any] | None = None,
) -> str:
    if timed_out:
        return "timeout"
    combined_text = (stdout + b"\n" + stderr).decode("utf-8", errors="replace").lower()
    if "operationtimedout" in combined_text and (
        "navigate failed" in combined_text or "navigation failed" in combined_text
    ):
        return "timeout"
    if (
        returncode != 0
        and any(marker in combined_text for marker in _CLI_DEADLINE_COMMAND_MARKERS)
        and any(marker in combined_text for marker in _CLI_DEADLINE_TIMEOUT_MARKERS)
    ):
        return "timeout"
    if returncode != 0 and _looks_like_raw_binary_main_resource_timeout(
        combined_text,
        min_body_bytes,
    ):
        return "success-binary-main-resource"
    if returncode != 0:
        if any(marker in combined_text for marker in _FORBIDDEN_ERROR_MARKERS):
            return "blocked-or-forbidden"
        if any(marker in combined_text for marker in _NOT_FOUND_ERROR_MARKERS):
            return "not-found"
        if any(marker in combined_text for marker in _NETWORK_ERROR_MARKERS):
            return "network-error"
        return "process-error"
    body = stdout.strip()
    if not body:
        if any(marker in combined_text for marker in _NETWORK_ERROR_MARKERS):
            return "network-error"
        return "empty-response"
    if len(body) >= min_body_bytes and _looks_like_binary_content(body):
        return "success-binary-content"
    if snapshot is None:
        snapshot = _extract_title_and_text(stdout)
    text_length = int(snapshot.get("text_length") or 0)
    title = str(snapshot.get("title") or "").lower()
    sample = str(snapshot.get("text_sample") or "").lower()
    page_text = f"{title}\n{sample}"
    if any(marker in page_text for marker in _NETWORK_ERROR_MARKERS):
        return "network-error"
    if (
        "blocked" in title
        or "forbidden" in title
        or _BLOCKED_TITLE_403.search(title)
        or "access restricted" in title
    ):
        return "blocked-or-forbidden"
    if any(marker in page_text for marker in _NOT_FOUND_MARKERS):
        return "not-found"
    if any(marker in page_text for marker in _BLOCKED_BODY_MARKERS):
        return "blocked-or-forbidden"
    if any(marker in page_text for marker in _CAPTCHA_MARKERS):
        return "captcha-or-verification"
    if any(marker in combined_text for marker in _JS_CHALLENGE_MARKERS):
        return "js-challenge"
    if _looks_like_login_wall(title, sample, text_length):
        return "login-wall"
    if len(body) < min_body_bytes:
        return "app-shell-only"
    if text_length < min_body_bytes:
        return "app-shell-only"
    return "success-content"


def _elapsed_failure_reached_timeout(elapsed_ms: float | None, timeout_seconds: float) -> bool:
    if elapsed_ms is None or timeout_seconds <= 0:
        return False
    grace_seconds = min(_ELAPSED_FAILURE_TIMEOUT_GRACE_SECONDS, timeout_seconds * 0.05)
    timeout_floor_ms = max(0.0, (timeout_seconds - grace_seconds) * 1000.0)
    return elapsed_ms >= timeout_floor_ms


def _looks_like_login_wall(title: str, sample: str, text_length: int) -> bool:
    page_text = f"{title}\n{sample}"
    if sum(1 for marker in _LOGIN_FORM_MARKERS if marker in page_text) >= 2:
        return True
    if text_length >= 800:
        return False
    login_hits = {marker for marker in _LOGIN_MARKERS if marker in page_text}
    if not login_hits:
        return False
    title_has_login = any(marker in title for marker in _LOGIN_MARKERS)
    if title_has_login and len(login_hits) >= 2:
        return True
    if len(login_hits) >= 2 and any(marker in page_text for marker in _LOGIN_CONTEXT_MARKERS):
        return True
    return False


def _ok_categories() -> set[str]:
    return {"success-content", "success-binary-content", "success-binary-main-resource"}


def _failure_kind(category: str, error: str | None) -> str | None:
    if error == "target binary unavailable":
        return "target-unavailable"
    if category not in _ok_categories():
        return category
    return None


_SITE_UNREACHABLE_FAILURE_KINDS = {
    "network-error",
    "timeout",
    "empty-response",
    "process-error",
}


def _site_unreachable_exclusions(rows: list[dict[str, Any]]) -> dict[str, str]:
    rows_by_domain: dict[str, list[dict[str, Any]]] = {}
    for row in rows:
        rows_by_domain.setdefault(str(row["domain"]), []).append(row)

    excluded: dict[str, str] = {}
    for domain, domain_rows in rows_by_domain.items():
        if any(row.get("ok") for row in domain_rows):
            continue
        failure_kinds = {
            str(row.get("failure_kind") or _failure_kind(str(row.get("category")), None) or "")
            for row in domain_rows
        }
        failure_kinds.discard("")
        if (
            failure_kinds
            and failure_kinds <= _SITE_UNREACHABLE_FAILURE_KINDS
            and (failure_kinds - {"process-error"})
        ):
            excluded[domain] = "site-unreachable"
    return excluded


def _artifact_filename_token(value: str, *, max_length: int = 80) -> str:
    token = _SAFE_ARTIFACT_TOKEN.sub("_", value)
    while ".." in token:
        token = token.replace("..", "_")
    token = token.strip("._-") or "site"
    if token == value and len(token) <= max_length:
        return token
    digest = sha256(value.encode("utf-8", errors="surrogatepass")).hexdigest()[:12]
    token = token[:max_length].rstrip("._-") or "site"
    return f"{token}-{digest}"


def _write_failure_artifact(
    *,
    suite_dir: Path,
    row: dict[str, Any],
    stdout: bytes,
    stderr: bytes,
) -> str:
    domain = _artifact_filename_token(str(row["domain"]))
    name = f"{row['target']}-run-{row['run']}-rank{row['rank']:03d}-{domain}"
    failures_dir = suite_dir / "failures"
    json_path = failures_dir / f"{name}.json"
    stderr_path = failures_dir / f"{name}.stderr.txt"
    write_json(json_path, row)
    write_text(stderr_path, stderr[-16 * 1024 :].decode("utf-8", errors="replace"))
    if stdout.strip():
        stdout_path = failures_dir / f"{name}.stdout.html"
        write_text(stdout_path, stdout[-32 * 1024 :].decode("utf-8", errors="replace"))
    return str(json_path.relative_to(suite_dir))


def _domain_to_url(domain: str) -> str:
    if domain.startswith("http://") or domain.startswith("https://"):
        return domain
    return f"https://{domain}"


def _execute_one(
    *,
    suite_dir: Path,
    target: str,
    metadata: dict[str, str],
    info: dict[str, Any],
    run_id: int,
    rank: int,
    domain: str,
    timeout_seconds: float,
    min_body_bytes: int,
    proc_env: dict[str, str],
) -> dict[str, Any]:
    url = _domain_to_url(domain)
    if not info.get("available") or not info.get("path"):
        row = {
            "target": target,
            **metadata,
            "run": run_id,
            "rank": rank,
            "domain": domain,
            "url": url,
            "category": "error",
            "ok": False,
            "failure_kind": "target-unavailable",
            "error": "target binary unavailable",
        }
        row["failure_artifact"] = _write_failure_artifact(suite_dir=suite_dir, row=row, stdout=b"", stderr=b"")
        return row
    result = _run_top_sites_target(
        target=target,
        binary=Path(info["path"]),
        url=url,
        timeout_seconds=timeout_seconds,
        proc_env=proc_env,
    )
    snapshot = _extract_title_and_text(result.stdout)
    category = _classify(
        result.stdout,
        result.stderr,
        result.returncode,
        result.timed_out,
        min_body_bytes,
        snapshot,
    )
    if category not in _ok_categories() and _elapsed_failure_reached_timeout(result.elapsed_ms, timeout_seconds):
        category = "timeout"
    ok = category in _ok_categories()
    row: dict[str, Any] = {
        "target": target,
        **metadata,
        "run": run_id,
        "rank": rank,
        "domain": domain,
        "url": url,
        "command": result.command,
        "category": category,
        "ok": ok,
        "failure_kind": None if ok else _failure_kind(category, None),
        "elapsed_ms": result.elapsed_ms,
        "returncode": result.returncode,
        "timed_out": result.timed_out,
        "stdout_bytes": len(result.stdout),
        "stderr_bytes": len(result.stderr),
        "title": snapshot["title"],
        "text_length": snapshot["text_length"],
        "text_sample": snapshot["text_sample"],
        "stdout_tail": result.stdout[-512:].decode("utf-8", errors="replace"),
        "stderr_tail": result.stderr[-512:].decode("utf-8", errors="replace"),
        "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
        "peak_rss_bytes": result.resources.get("peak_rss_bytes"),
    }
    if not ok:
        row["failure_artifact"] = _write_failure_artifact(
            suite_dir=suite_dir, row=row, stdout=result.stdout, stderr=result.stderr
        )
    return row


def run_top_sites_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    targets: tuple[str, ...],
    profile: str = DEFAULT_TOP_SITES_PROFILE,
    list_path: Path | None = None,
    source: str = DEFAULT_TOP_SITES_SOURCE,
    runs: int | None = None,
    timeout_seconds: float = 15.0,
    gate_target: str = "moli",
    parallelism: int = DEFAULT_TOP_SITES_PARALLELISM,
    chrome_parallelism: int = 1,
    min_body_bytes: int = DEFAULT_TOP_SITES_MIN_BODY_BYTES,
    limit_override: int | None = None,
) -> dict[str, Any]:
    unknown_targets = [target for target in targets if target not in WEBFETCH_TARGETS]
    if unknown_targets:
        raise RuntimeError(f"unknown webfetch target(s): {', '.join(unknown_targets)}")
    if gate_target not in WEBFETCH_TARGETS:
        raise RuntimeError(f"unknown gate target: {gate_target}")
    if profile not in TOP_SITES_PROFILES:
        raise RuntimeError(f"unknown top-sites profile `{profile}`; expected one of {sorted(TOP_SITES_PROFILES)}")
    profile_config = TOP_SITES_PROFILES[profile]
    limit = int(limit_override if limit_override is not None else profile_config["limit"])
    if limit <= 0:
        raise RuntimeError("top-sites limit must be positive")
    runs_count = int(runs if runs is not None else profile_config["default_runs"])
    if runs_count <= 0:
        raise RuntimeError("top-sites runs must be positive")
    if parallelism <= 0:
        raise RuntimeError("top-sites parallelism must be positive")
    if chrome_parallelism <= 0:
        raise RuntimeError("top-sites chrome parallelism must be positive")

    suite_dir = output_dir / "top-sites"
    resolved_source, primary_path = resolve_top_sites_source(source, list_path)
    entries_all, list_source_labels = load_top_sites_entries(resolved_source, list_path)
    entries = entries_all[:limit]
    list_source = primary_path
    proc_env = clear_proxy_env(os.environ)

    rows: list[dict[str, Any]] = []
    for target in targets:
        metadata = _top_sites_target_metadata(target)
        info = target_matrix.get(metadata["binary_key"], {})
        job_specs = [
            (run_id, rank, domain)
            for run_id in range(1, runs_count + 1)
            for rank, domain in entries
        ]
        target_workers = chrome_parallelism if target == "chrome" else parallelism
        with ThreadPoolExecutor(max_workers=target_workers) as executor:
            future_to_spec = {
                executor.submit(
                    _execute_one,
                    suite_dir=suite_dir,
                    target=target,
                    metadata=metadata,
                    info=info,
                    run_id=run_id,
                    rank=rank,
                    domain=domain,
                    timeout_seconds=timeout_seconds,
                    min_body_bytes=min_body_bytes,
                    proc_env=proc_env,
                ): (target, run_id, rank, domain)
                for run_id, rank, domain in job_specs
            }
            for future in as_completed(future_to_spec):
                rows.append(future.result())

    rows.sort(key=lambda row: (row["target"], row["run"], row["rank"]))

    excluded_domains = _site_unreachable_exclusions(rows) if len(targets) > 1 else {}
    for row in rows:
        exclusion_reason = excluded_domains.get(str(row["domain"]))
        row["excluded"] = exclusion_reason is not None
        row["exclusion_reason"] = exclusion_reason

    counted_rows = [row for row in rows if not row.get("excluded")]
    gate_failures = sum(1 for row in counted_rows if row["target"] == gate_target and not row.get("ok"))
    summary: dict[str, Any] = {
        "suite": "top-sites",
        "profile": profile,
        "limit": limit,
        "source": resolved_source,
        "list_source": str(list_source.relative_to(REPO_ROOT)) if list_source.is_relative_to(REPO_ROOT) else str(list_source),
        "list_sources": list_source_labels,
        "site_count": len(entries),
        "counted_site_count": len(entries) - len(excluded_domains),
        "excluded_site_count": len(excluded_domains),
        "excluded_sites": [
            {"domain": domain, "reason": reason}
            for domain, reason in sorted(
                excluded_domains.items(),
                key=lambda item: next((rank for rank, entry_domain in entries if entry_domain == item[0]), 0),
            )
        ],
        "runs": runs_count,
        "timeout_seconds": timeout_seconds,
        "min_body_bytes": min_body_bytes,
        "parallelism": parallelism,
        "chrome_parallelism": chrome_parallelism,
        "gate_target": gate_target,
        "gate_failures": gate_failures,
        "total_failures": sum(1 for row in counted_rows if not row.get("ok")),
        "total_excluded_runs": sum(1 for row in rows if row.get("excluded")),
        "targets": {},
    }
    for target in targets:
        all_target_rows = [row for row in rows if row["target"] == target]
        target_rows = [row for row in all_target_rows if not row.get("excluded")]
        categories: dict[str, int] = {}
        failure_kinds: dict[str, int] = {}
        for row in target_rows:
            categories[row["category"]] = categories.get(row["category"], 0) + 1
            failure_kind = row.get("failure_kind")
            if isinstance(failure_kind, str) and failure_kind:
                failure_kinds[failure_kind] = failure_kinds.get(failure_kind, 0) + 1
        summary["targets"][target] = {
            **_top_sites_target_metadata(target),
            "sites": len(target_rows),
            "raw_sites": len(all_target_rows),
            "excluded_runs": len(all_target_rows) - len(target_rows),
            "runs": runs_count,
            "passes": sum(1 for row in target_rows if row.get("ok")),
            "failures": sum(1 for row in target_rows if not row.get("ok")),
            "categories": categories,
            "failure_kinds": failure_kinds,
            "elapsed_ms": summarize(row["elapsed_ms"] for row in target_rows if row.get("elapsed_ms") is not None),
            "peak_pss_bytes": summarize(row["peak_pss_bytes"] for row in target_rows if row.get("peak_pss_bytes") is not None),
            "peak_rss_bytes": summarize(row["peak_rss_bytes"] for row in target_rows if row.get("peak_rss_bytes") is not None),
        }

    write_csv(suite_dir / "raw-runs.csv", rows)
    write_json(suite_dir / "runs.json", rows)
    write_json(suite_dir / "summary.json", summary)
    return summary
