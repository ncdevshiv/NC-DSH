#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${BENCH_DIR}/.." && pwd)"

BUILD="${BUILD:-1}"
SOURCE="${SOURCE:-webfetch-mix}"
PROFILE="${PROFILE:-webfetch}"
RUNS="${RUNS:-1}"
TIMEOUT="${TIMEOUT:-30}"
PARALLELISM="${PARALLELISM:-20}"
CHROME_PARALLELISM="${CHROME_PARALLELISM:-2}"
LIMIT="${LIMIT:-}"
TARGETS="${TARGETS:-moli moli-cdp moli-full moli-full-cdp}"
OUTPUT_DIR="${OUTPUT_DIR:-}"
MOLI_BIN="${MOLI_BIN:-${REPO_ROOT}/target/release/moli}"
LIGHTPANDA_BIN="${LIGHTPANDA_BIN:-}"
OBSCURA_BIN="${OBSCURA_BIN:-}"
CHROME_BIN="${CHROME_BIN:-}"

detect_command() {
  local name="$1"
  if command -v "${name}" >/dev/null 2>&1; then
    command -v "${name}"
  fi
}

if [[ -z "${LIGHTPANDA_BIN}" ]]; then
  LIGHTPANDA_BIN="$(detect_command lightpanda || true)"
fi

if [[ -z "${OBSCURA_BIN}" ]]; then
  OBSCURA_BIN="$(detect_command obscura || true)"
fi

if [[ -z "${CHROME_BIN}" ]]; then
  if [[ -x "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" ]]; then
    CHROME_BIN="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
  elif command -v google-chrome >/dev/null 2>&1; then
    CHROME_BIN="$(command -v google-chrome)"
  elif command -v chromium >/dev/null 2>&1; then
    CHROME_BIN="$(command -v chromium)"
  elif command -v chromium-browser >/dev/null 2>&1; then
    CHROME_BIN="$(command -v chromium-browser)"
  fi
fi

if [[ -z "${OUTPUT_DIR}" ]]; then
  OUTPUT_DIR="${BENCH_DIR}/results/webfetch-mix-benchmark-$(date +%Y%m%d-%H%M%S)"
elif [[ "${OUTPUT_DIR}" != /* ]]; then
  OUTPUT_DIR="${REPO_ROOT}/${OUTPUT_DIR}"
fi

if [[ "${BUILD}" == "1" ]]; then
  (cd "${REPO_ROOT}" && cargo build --release -p moli)
fi

if [[ ! -x "${MOLI_BIN}" ]]; then
  echo "missing executable MOLI_BIN=${MOLI_BIN}" >&2
  exit 2
fi

cmd=(
  uv run moli-benchmark top-sites
  --source "${SOURCE}"
  --profile "${PROFILE}"
  --runs "${RUNS}"
  --timeout "${TIMEOUT}"
  --parallelism "${PARALLELISM}"
  --chrome-parallelism "${CHROME_PARALLELISM}"
  --output-dir "${OUTPUT_DIR}"
  --moli-bin "${MOLI_BIN}"
)

if [[ -n "${CHROME_BIN}" ]]; then
  cmd+=(--chrome-bin "${CHROME_BIN}")
fi
if [[ -n "${LIGHTPANDA_BIN}" ]]; then
  cmd+=(--lightpanda-bin "${LIGHTPANDA_BIN}")
fi
if [[ -n "${OBSCURA_BIN}" ]]; then
  cmd+=(--obscura-bin "${OBSCURA_BIN}")
fi
if [[ -n "${LIMIT}" ]]; then
  cmd+=(--limit "${LIMIT}")
fi

for target in ${TARGETS}; do
  cmd+=(--target "${target}")
done

echo "repo: ${REPO_ROOT}"
echo "benchmark: ${BENCH_DIR}"
echo "output: ${OUTPUT_DIR}"
echo "targets: ${TARGETS}"
echo "source=${SOURCE} profile=${PROFILE} runs=${RUNS} timeout=${TIMEOUT}s parallelism=${PARALLELISM} chrome_parallelism=${CHROME_PARALLELISM}"
echo "moli=${MOLI_BIN}"
echo "lightpanda=${LIGHTPANDA_BIN:-auto/default}"
echo "chrome=${CHROME_BIN:-auto/default}"
echo "obscura=${OBSCURA_BIN:-auto/default}"
echo
printf '+'
printf ' %q' "${cmd[@]}"
printf '\n\n'

set +e
run_output="$(cd "${BENCH_DIR}" && "${cmd[@]}" 2>&1)"
status=$?
set -e

echo "${run_output}"

if [[ ! -f "${OUTPUT_DIR}/top-sites/summary.json" ]]; then
  echo "benchmark exited ${status}; summary not found at ${OUTPUT_DIR}/top-sites/summary.json" >&2
  exit "${status}"
fi

python3 - "${OUTPUT_DIR}" <<'PY'
import collections
import json
import pathlib
import sys

result_dir = pathlib.Path(sys.argv[1])
summary = json.loads((result_dir / "top-sites" / "summary.json").read_text())
runs = json.loads((result_dir / "top-sites" / "runs.json").read_text())


def fmt_ms(value):
    return "-" if value is None else f"{value / 1000:.2f}s"


def fmt_bytes(value):
    return "-" if value is None else f"{value / 1024 / 1024:.0f}MiB"


lines = []
lines.append("# WebFetch Mix Benchmark Summary")
lines.append("")
lines.append(f"Result dir: `{result_dir}`")
lines.append("")
lines.append(
    "Config: "
    f"source={summary['source']}, profile={summary['profile']}, runs={summary['runs']}, "
    f"timeout={summary['timeout_seconds']}s, parallelism={summary['parallelism']}, "
    f"chrome_parallelism={summary['chrome_parallelism']}, counted={summary['counted_site_count']}, "
    f"excluded={summary['excluded_site_count']}"
)
lines.append("")
lines.append(
    "| target | pass | fail | timed_out flag | failure timeout | median | p95 | "
    "PSS median/p95 | RSS median/p95 | failure kinds |"
)
lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---|")

for name, row in summary["targets"].items():
    sites = row["sites"]
    passes = row["passes"]
    rate = passes / sites * 100 if sites else 0
    elapsed = row.get("elapsed_ms") or {}
    pss = row.get("peak_pss_bytes") or {}
    rss = row.get("peak_rss_bytes") or {}
    timed_out = sum(
        1
        for run in runs
        if run.get("target") == name and not run.get("excluded") and run.get("timed_out")
    )
    failure_timeout = (row.get("failure_kinds") or {}).get("timeout", 0)
    failure_kinds = ", ".join(
        f"{key}:{value}" for key, value in sorted((row.get("failure_kinds") or {}).items())
    )
    lines.append(
        f"| {name} | {passes}/{sites} ({rate:.1f}%) | {row['failures']} | {timed_out} | "
        f"{failure_timeout} | {fmt_ms(elapsed.get('median'))} | {fmt_ms(elapsed.get('p95'))} | "
        f"{fmt_bytes(pss.get('median'))}/{fmt_bytes(pss.get('p95'))} | "
        f"{fmt_bytes(rss.get('median'))}/{fmt_bytes(rss.get('p95'))} | {failure_kinds} |"
    )

lines.append("")
lines.append("## Overlaps")
by_site = collections.defaultdict(dict)
for run in runs:
    if run.get("excluded"):
        continue
    by_site[run["domain"]][run["target"]] = run

for left, right in (
    ("moli", "moli-full"),
    ("moli-cdp", "moli-full-cdp"),
    ("moli", "moli-cdp"),
    ("moli-full", "moli-full-cdp"),
    ("lightpanda", "lightpanda-cdp"),
    ("obscura", "obscura-cdp"),
):
    paired = [site_runs for site_runs in by_site.values() if left in site_runs and right in site_runs]
    if not paired:
        continue
    both = sum(bool(site_runs[left].get("ok")) and bool(site_runs[right].get("ok")) for site_runs in paired)
    left_only = sum(bool(site_runs[left].get("ok")) and not bool(site_runs[right].get("ok")) for site_runs in paired)
    right_only = sum((not bool(site_runs[left].get("ok"))) and bool(site_runs[right].get("ok")) for site_runs in paired)
    neither = sum(
        (not bool(site_runs[left].get("ok"))) and (not bool(site_runs[right].get("ok")))
        for site_runs in paired
    )
    lines.append(
        f"- `{left}` vs `{right}`: both={both}, `{left}` only={left_only}, "
        f"`{right}` only={right_only}, neither={neither}"
    )

text = "\n".join(lines) + "\n"
(result_dir / "run-summary.md").write_text(text, encoding="utf-8")
print(text)
print(f"summary written: {result_dir / 'run-summary.md'}")
PY

exit "${status}"
