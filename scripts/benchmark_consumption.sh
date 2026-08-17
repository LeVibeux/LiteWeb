#!/usr/bin/env bash
# Collect CPU and memory usage for LiteWeb and every WebKit child process.
# This runs in the current graphical user session and does not require sudo.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/benchmark_collect.sh
source "$SCRIPT_DIR/lib/benchmark_collect.sh"

OUTPUT_DIR="${REPO_ROOT}/benchmark-results/run-$(date +%Y%m%d-%H%M%S)"

usage() {
  cat <<'EOF'
Usage: ./scripts/benchmark_consumption.sh [--output DIRECTORY]

Runs the idle, normal, and aggressive LiteWeb suspension scenarios.
Results go to benchmark-results/ by default (~15 min).

For the 3-page Normal vs Ultra engine comparison (~5 min), use:
  ./scripts/benchmark_ultra.sh
EOF
}

if [[ "${1:-}" == "--output" ]]; then
  [[ $# -eq 2 ]] || { usage >&2; exit 2; }
  OUTPUT_DIR="$2"
elif [[ $# -ne 0 ]]; then
  usage >&2
  exit 2
fi

benchmark_preflight
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd -- "$OUTPUT_DIR" && pwd)"
cp -- "$0" "$OUTPUT_DIR/benchmark_consumption.sh"
write_url_list "$OUTPUT_DIR/urls.csv" "${BENCHMARK_URLS[@]}"
init_summary_files "LiteWeb consumption benchmark"
benchmark_build
benchmark_install_trap

run_scenario idle
run_scenario normal
run_scenario aggressive

cat >> "$OUTPUT_DIR/summary.md" <<'EOF'

## Method

- One fresh LiteWeb profile per scenario; CPU and memory are collected from its
  user cgroup, including WebKit subprocesses.
- `idle`: Google homepage only, after a 30-second warmup.
- `normal` and `aggressive`: ten fixed public pages plus one active blank
  sentinel tab, so all ten measured pages can become inactive.
- Results are most useful when the computer is plugged in, brightness is fixed,
  and no other browser or heavy process is running.

This benchmark measures cgroup CPU and RAM, not electrical energy. Web content
can change over time; the exact URL list is stored in `urls.csv` for each run.
EOF

echo "Benchmark complete: $OUTPUT_DIR/summary.md"
