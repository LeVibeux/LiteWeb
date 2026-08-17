#!/usr/bin/env bash
# Compare live-engine RAM and CPU: same 3 pages, Normal vs Ultra.
# Independent of the 10-tab suspension suite. ~5 min, graphical session.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/benchmark_collect.sh
source "$SCRIPT_DIR/lib/benchmark_collect.sh"

OUTPUT_DIR="${REPO_ROOT}/benchmark-results/ultra-$(date +%Y%m%d-%H%M%S)"

usage() {
  cat <<'EOF'
Usage: ./scripts/benchmark_ultra.sh [--output DIRECTORY]

Loads the same three public pages (Wikipedia, rust-lang, HN) twice:
  loaded  — Normal engine, pages stay alive
  ultra   — Ultra engine (no JS / images / media / GPU, reader flatten)

Collects cgroup CPU and RAM over ~150 s per scenario. Charts are written
into the result directory when gnuplot is installed.

To overlay these series onto an existing suspension-run folder:
  ./scripts/visualize_benchmark.sh benchmark-results/run-YYYYMMDD-HHMMSS \
      --also benchmark-results/ultra-YYYYMMDD-HHMMSS
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
cp -- "$0" "$OUTPUT_DIR/benchmark_ultra.sh"
write_url_list "$OUTPUT_DIR/urls-loaded.csv" "${LOADED_BENCHMARK_URLS[@]}"
init_summary_files "LiteWeb Ultra engine benchmark"
printf 'suite,ultra\n' >> "$OUTPUT_DIR/metadata.csv"
benchmark_build
benchmark_install_trap

run_scenario loaded
run_scenario ultra

cat >> "$OUTPUT_DIR/summary.md" <<'EOF'

## Method

- One fresh LiteWeb profile per scenario; CPU and memory are collected from its
  user cgroup, including WebKit subprocesses.
- `loaded` and `ultra` open the same three pages and keep every WebView alive
  for 120 s after a 30 s warmup. Suspension is disabled for this pair.
- `ultra` disables JavaScript, images, media and GPU, then flattens each page
  to a reader document. Compare cruise RAM (`memory_before_mib`) — not
  after-suspend RAM from the 10-tab suite.
- Results are most useful when the computer is plugged in, brightness is fixed,
  and no other browser or heavy process is running.

This benchmark measures cgroup CPU and RAM, not electrical energy. The URL list
is stored in `urls-loaded.csv`.
EOF

echo "Ultra benchmark complete: $OUTPUT_DIR/summary.md"
maybe_visualize
