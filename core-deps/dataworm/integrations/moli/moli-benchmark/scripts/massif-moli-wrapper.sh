#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

real_binary="${MOLI_REAL_BINARY:-$repo_root/target/release/moli}"
massif_output="${MASSIF_OUT:-/tmp/massif-moli.%p.out}"

exec valgrind \
  --tool=massif \
  --pages-as-heap=yes \
  --time-unit=B \
  --threshold=0.2 \
  --detailed-freq=1 \
  --max-snapshots=200 \
  --massif-out-file="$massif_output" \
  "$real_binary" "$@"
