#!/usr/bin/env bash
# Stock Chromium/Chrome on the same 3 live pages as LiteWeb loaded/ultra.
# Independent of the 10-tab suite. ~2.5 min, graphical session.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/benchmark_collect.sh
source "$SCRIPT_DIR/lib/benchmark_collect.sh"

OUTPUT_DIR="${REPO_ROOT}/benchmark-results/chromium-$(date +%Y%m%d-%H%M%S)"

usage() {
  cat <<'EOF'
Usage: ./scripts/benchmark_chromium.sh [--output DIRECTORY]

Loads the same three public pages as the Ultra pair (Wikipedia, rust-lang,
HN) in stock Chromium/Chrome. Collects cgroup CPU and RAM for ~150 s
(30 s warmup + 120 s cruise).

Point --output at an existing ultra-YYYYMMDD folder to append chromium
samples there, then visualize overlays Chromium on the loaded/ultra charts:

  ./scripts/benchmark_chromium.sh --output benchmark-results/ultra-YYYYMMDD-HHMMSS
  ./scripts/visualize_benchmark.sh benchmark-results/ultra-YYYYMMDD-HHMMSS
EOF
}

if [[ "${1:-}" == "--output" ]]; then
  [[ $# -eq 2 ]] || { usage >&2; exit 2; }
  OUTPUT_DIR="$2"
elif [[ $# -ne 0 ]]; then
  usage >&2
  exit 2
fi

for command in systemctl systemd-run awk date mktemp; do
  command -v "$command" >/dev/null || {
    echo "Missing required command: $command" >&2
    exit 1
  }
done
if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  echo "A graphical X11 or Wayland session is required." >&2
  exit 1
fi
find_chromium >/dev/null || {
  echo "Install Chromium or Google Chrome first." >&2
  exit 1
}

mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd -- "$OUTPUT_DIR" && pwd)"
cp -- "$0" "$OUTPUT_DIR/benchmark_chromium.sh"
write_url_list "$OUTPUT_DIR/urls-loaded.csv" "${LOADED_BENCHMARK_URLS[@]}"
if [[ ! -f "$OUTPUT_DIR/summary.csv" ]]; then
  init_summary_files "LiteWeb vs Chromium engine benchmark"
fi
printf 'chromium_browser,%s\n' "$(find_chromium)" >> "$OUTPUT_DIR/metadata.csv"
benchmark_install_trap

run_chromium_scenario

if ! grep -q 'stock Chromium' "$OUTPUT_DIR/summary.md"; then
  cat >> "$OUTPUT_DIR/summary.md" <<'EOF'

## Chromium method

- Stock Chromium/Chrome, fresh `--user-data-dir`, same three URLs as
  `loaded` / `ultra`.
- Timed externally (no LiteWeb events): 30 s warmup + 120 s cruise.
- Compare cruise RAM/CPU with LiteWeb Ultra to show the stripped engine
  against the default web engine, not against after-suspend RAM.
EOF
fi

echo "Chromium benchmark complete: $OUTPUT_DIR/summary.md"
maybe_visualize
