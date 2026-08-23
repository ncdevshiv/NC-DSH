#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

real_binary="${MOLI_REAL_BINARY:-$repo_root/target/release/moli}"
heaptrack_output="${HEAPTRACK_OUT:-/tmp/heaptrack-moli.%p.gz}"

exec heaptrack --output "$heaptrack_output" -- "$real_binary" "$@"
