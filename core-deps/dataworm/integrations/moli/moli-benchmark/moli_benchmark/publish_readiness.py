from __future__ import annotations

from pathlib import Path
from typing import Any


REQUIRED_TARGETS = ("moli", "lightpanda", "chrome", "obscura")
TOP_LEVEL_ARTIFACTS = (
    "environment.json",
    "versions.json",
    "summary.json",
    "summary.md",
    "publish-readiness.json",
    "report-data.json",
    "index.html",
)


def _summary_by_suite(summaries: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {str(summary.get("suite")): summary for summary in summaries if summary.get("suite")}


def _suite_failures(summary: dict[str, Any] | None) -> int | None:
    if not summary:
        return None
    value = summary.get("gate_failures", summary.get("total_failures"))
    try:
        return int(value or 0)
    except (TypeError, ValueError):
        return None


def _compact_cdp_client_rows(rows: list[Any]) -> list[dict[str, Any]]:
    compact = []
    for row in rows:
        if not isinstance(row, dict):
            continue
        compact.append(
            {
                "client": row.get("client"),
                "required": row.get("required"),
                "covered": row.get("covered"),
                "gate_ok": row.get("gate_ok"),
                "failure_kind": row.get("failure_kind"),
                "record_count": row.get("record_count"),
                "groups": row.get("groups"),
            }
        )
    return compact


def _compact_gate_rows(rows: list[Any]) -> list[dict[str, Any]]:
    compact = []
    for row in rows:
        if not isinstance(row, dict):
            continue
        compact.append(
            {
                "gate": row.get("gate"),
                "ok": row.get("ok"),
                "actual": row.get("actual"),
                "required": row.get("required"),
                "failure_kind": row.get("failure_kind"),
            }
        )
    return compact


def _target_is_available(versions: dict[str, Any], name: str) -> bool:
    target = (versions.get("targets") or {}).get(name)
    return isinstance(target, dict) and bool(target.get("available"))


def _check(
    checks: list[dict[str, Any]],
    *,
    name: str,
    ok: bool,
    required: Any,
    actual: Any,
    detail: str,
) -> None:
    checks.append(
        {
            "name": name,
            "ok": bool(ok),
            "required": required,
            "actual": actual,
            "detail": detail,
        }
    )


def _has_full_target_matrix(summary: dict[str, Any] | None) -> bool:
    if not summary:
        return False
    targets = summary.get("targets")
    if not isinstance(targets, dict):
        return False
    engines: set[str] = set()
    for target, result in targets.items():
        if isinstance(result, dict) and isinstance(result.get("engine"), str):
            engines.add(result["engine"])
        elif isinstance(target, str):
            engines.add(target.removesuffix("-cdp"))
    return all(target in engines for target in REQUIRED_TARGETS)


def build_publish_readiness(
    *,
    output_dir: Path,
    versions: dict[str, Any],
    summaries: list[dict[str, Any]],
) -> dict[str, Any]:
    by_suite = _summary_by_suite(summaries)
    checks: list[dict[str, Any]] = []

    missing_top_level = [path for path in TOP_LEVEL_ARTIFACTS if not (output_dir / path).exists()]
    _check(
        checks,
        name="top-level artifacts",
        ok=not missing_top_level,
        required=list(TOP_LEVEL_ARTIFACTS),
        actual=[path for path in TOP_LEVEL_ARTIFACTS if path not in missing_top_level],
        detail="Formal reports must carry machine-readable metadata, summaries, and the human summary.",
    )

    available_targets = [target for target in REQUIRED_TARGETS if _target_is_available(versions, target)]
    _check(
        checks,
        name="target matrix",
        ok=len(available_targets) == len(REQUIRED_TARGETS),
        required=list(REQUIRED_TARGETS),
        actual=available_targets,
        detail="Publishable comparison reports require local Moli, Lightpanda, Chrome, and Obscura measurements.",
    )

    synthetic = by_suite.get("synthetic-matrix")
    synthetic_gate_rows = synthetic.get("formal_gate_rows", []) if isinstance(synthetic, dict) else []
    if not isinstance(synthetic_gate_rows, list):
        synthetic_gate_rows = []
    _check(
        checks,
        name="synthetic formal matrix",
        ok=bool(
            synthetic
            and synthetic.get("profile") == "formal"
            and _suite_failures(synthetic) == 0
            and synthetic_gate_rows
            and all(isinstance(row, dict) and row.get("ok") is True for row in synthetic_gate_rows)
        ),
        required="synthetic-matrix formal profile with every formal gate row passing",
        actual={
            "suite": synthetic.get("suite") if synthetic else None,
            "profile": synthetic.get("profile") if synthetic else None,
            "gate_failures": _suite_failures(synthetic),
            "formal_gate_rows": _compact_gate_rows(synthetic_gate_rows),
        },
        detail="Suite B is the first formalization target because it is local, deterministic, and regression-friendly.",
    )

    startup = by_suite.get("startup")
    startup_gate_rows = startup.get("formal_gate_rows", []) if isinstance(startup, dict) else []
    if not isinstance(startup_gate_rows, list):
        startup_gate_rows = []
    _check(
        checks,
        name="startup and size",
        ok=bool(
            startup
            and startup.get("profile") == "formal"
            and _suite_failures(startup) == 0
            and startup_gate_rows
            and all(isinstance(row, dict) and row.get("ok") is True for row in startup_gate_rows)
        ),
        required="startup formal profile with every formal gate row passing",
        actual={
            "suite": startup.get("suite") if startup else None,
            "profile": startup.get("profile") if startup else None,
            "gate_failures": _suite_failures(startup),
            "formal_gate_rows": _compact_gate_rows(startup_gate_rows),
        },
        detail="Suite E must archive startup, size, CDP first page, warm page, idle footprint, resource, and readiness evidence.",
    )

    wpt = by_suite.get("wpt")
    wpt_summary = wpt.get("summary", {}) if isinstance(wpt, dict) else {}
    _check(
        checks,
        name="wpt p0 smoke",
        ok=bool(
            wpt
            and int(wpt.get("cases", 0) or 0) > 0
            and _suite_failures(wpt) == 0
            and int(wpt_summary.get("unexpected_fail", 0) or 0) == 0
            and int(wpt_summary.get("skip", 0) or 0) == 0
            and int(wpt_summary.get("known_fail", 0) or 0) == 0
        ),
        required="non-empty WPT P0 smoke report with 0 unexpected fail, 0 skip, 0 known-fail",
        actual={
            "cases": wpt.get("cases") if wpt else None,
            "gate_failures": _suite_failures(wpt),
            "unexpected_fail": wpt_summary.get("unexpected_fail"),
            "skip": wpt_summary.get("skip"),
            "known_fail": wpt_summary.get("known_fail"),
        },
        detail="WPT remains the compatibility gate; real-site results cannot replace it.",
    )

    cdp = by_suite.get("cdp-smoke")
    cdp_client_rows = cdp.get("client_rows", []) if isinstance(cdp, dict) else []
    if not isinstance(cdp_client_rows, list):
        cdp_client_rows = []
    cdp_clients = {
        str(row.get("client")): row
        for row in cdp_client_rows
        if isinstance(row, dict) and row.get("client") is not None
    }
    _check(
        checks,
        name="cdp workflow",
        ok=bool(
            cdp
            and cdp.get("profile") == "formal"
            and _suite_failures(cdp) == 0
            and cdp_clients.get("raw_cdp", {}).get("gate_ok") is True
            and cdp_clients.get("playwright", {}).get("gate_ok") is True
            and cdp_clients.get("puppeteer", {}).get("gate_ok") is True
        ),
        required="formal CDP smoke with raw CDP, Playwright, and Puppeteer coverage and zero gate failures",
        actual={
            "suite": cdp.get("suite") if cdp else None,
            "profile": cdp.get("profile") if cdp else None,
            "gate_failures": _suite_failures(cdp),
            "client_rows": _compact_cdp_client_rows(cdp_client_rows),
        },
        detail="Suite D must prove real ecosystem clients, not only raw protocol navigation.",
    )

    amiibo = by_suite.get("amiibo-crawler")
    _check(
        checks,
        name="amiibo formal crawler",
        ok=bool(amiibo and amiibo.get("profile") == "formal" and _suite_failures(amiibo) == 0),
        required="Amiibo crawler formal profile with all 933 pages and zero gate failures",
        actual={
            "suite": amiibo.get("suite") if amiibo else None,
            "profile": amiibo.get("profile") if amiibo else None,
            "gate_failures": _suite_failures(amiibo),
        },
        detail="Suite A must use the real Lightpanda Amiibo workload for external credibility.",
    )

    wild_web = by_suite.get("wild-web")
    selected_seeds = wild_web.get("seeds", []) if isinstance(wild_web, dict) else []
    wild_targets = wild_web.get("targets", {}) if isinstance(wild_web, dict) else {}
    wild_extraction_failures = sum(
        int(target.get("extraction_failures", 0) or 0)
        for target in wild_targets.values()
        if isinstance(target, dict)
    )
    _check(
        checks,
        name="wild-web p0 seeds",
        ok=bool(
            wild_web
            and _suite_failures(wild_web) == 0
            and wild_extraction_failures == 0
            and {"zhihu-home", "toutiao-home"}.issubset(set(selected_seeds))
        ),
        required={"seeds": ["zhihu-home", "toutiao-home"], "extraction_failures": 0},
        actual={"seeds": selected_seeds, "gate_failures": _suite_failures(wild_web), "extraction_failures": wild_extraction_failures},
        detail="Wild-web is a product-value signal and implementation-gap discovery source.",
    )

    full_compare = any(_has_full_target_matrix(summary) for summary in summaries)
    _check(
        checks,
        name="horizontal comparison",
        ok=full_compare,
        required=list(REQUIRED_TARGETS),
        actual=[
            {
                "suite": summary.get("suite"),
                "targets": list((summary.get("targets") or {}).keys()),
            }
            for summary in summaries
            if summary.get("targets")
        ],
        detail="External reports need at least one suite that measured Moli, Lightpanda, Chrome, and Obscura side by side.",
    )

    failed = [check for check in checks if not check["ok"]]
    status = "publishable" if not failed else "investigation"
    return {
        "schema_version": 1,
        "status": status,
        "checks": checks,
        "known_invalid_items": [
            {
                "name": check["name"],
                "required": check["required"],
                "actual": check["actual"],
                "detail": check["detail"],
            }
            for check in failed
        ],
    }
