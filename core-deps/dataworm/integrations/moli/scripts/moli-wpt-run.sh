#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
wpt_root="${WPT_ROOT:-$repo_root/../wpt}"
venv="${WPT_MOLI_VENV:-$wpt_root/.venv-moli}"
moli_bin="${MOLI_BIN:-$repo_root/target/debug/moli}"

usage() {
    cat <<'USAGE'
Usage:
  scripts/moli-wpt-run.sh [--wpt-root PATH] [--venv PATH] [--moli-bin PATH] [--] [WPT_TEST_OR_RUN_ARG...]

Runs upstream WPT wdspec tests through the Moli wptrunner product.
The Python environment is created, installed, and executed via uv.
USAGE
}

args=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --wpt-root)
            shift
            wpt_root="${1:?missing --wpt-root value}"
            venv="${WPT_MOLI_VENV:-$wpt_root/.venv-moli}"
            ;;
        --venv)
            shift
            venv="${1:?missing --venv value}"
            ;;
        --moli-bin)
            shift
            moli_bin="${1:?missing --moli-bin value}"
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --)
            shift
            args+=("$@")
            break
            ;;
        *)
            args+=("$1")
            ;;
    esac
    shift
done

if [[ ! -x "$moli_bin" ]]; then
    echo "moli binary is not executable: $moli_bin" >&2
    exit 2
fi

if [[ ! -x "$venv/bin/python" ]]; then
    uv venv "$venv" --seed
fi
uv pip install --python "$venv/bin/python" \
    -r "$wpt_root/tools/manifest/requirements.txt" \
    -r "$wpt_root/tools/wpt/requirements.txt" \
    -r "$wpt_root/tools/wptrunner/requirements.txt" \
    -e "$repo_root/tools/wptrunner-moli"

# Some upstream wdspec files in local checkouts can miss an explicit
# pytest.mark.asyncio marker even though their async tests are intended to run.
# WPT's PytestExecutor treats pytest's implicit async skip as a harness error,
# so default to auto mode while allowing callers to override it.
case " ${PYTEST_ADDOPTS:-} " in
    *"--asyncio-mode"*|*"asyncio_mode"*) ;;
    *) export PYTEST_ADDOPTS="${PYTEST_ADDOPTS:+$PYTEST_ADDOPTS }--asyncio-mode=auto" ;;
esac

exec env WPT_ROOT="$wpt_root" uv run --no-project --python "$venv/bin/python" \
    python -m moli_wptrunner.run \
    --metadata "$wpt_root" \
    --tests "$wpt_root" \
    --manifest "$wpt_root/MANIFEST.json" \
    --ssl-type pregenerated \
    --ca-cert-path "$wpt_root/tools/certs/cacert.pem" \
    --host-key-path "$wpt_root/tools/certs/web-platform.test.key" \
    --host-cert-path "$wpt_root/tools/certs/web-platform.test.pem" \
    --manifest-update --processes 1 --test-types wdspec \
    --binary "$moli_bin" \
    --product moli \
    "${args[@]}"
