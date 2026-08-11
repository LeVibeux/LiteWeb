#!/usr/bin/env bash
# Collect CPU and memory usage for LiteWeb and every WebKit child process.
# This runs in the current graphical user session and does not require sudo.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="${REPO_ROOT}/benchmark-results/run-$(date +%Y%m%d-%H%M%S)"
BENCHMARK_URLS=(
  "https://www.google.com/"
  "https://fr.wikipedia.org/wiki/Navigateur_web"
  "https://www.rust-lang.org/fr/"
  "https://developer.mozilla.org/fr/"
  "https://github.com/"
  "https://www.mozilla.org/fr/"
  "https://ubuntu.com/"
  "https://stackoverflow.com/"
  "https://news.ycombinator.com/"
  "https://www.reddit.com/"
)

usage() {
  cat <<'EOF'
Usage: ./scripts/benchmark_consumption.sh [--output DIRECTORY]

Runs the idle, normal, and aggressive LiteWeb consumption scenarios. Results
are written to benchmark-results/ by default. The full run takes about 15 min.
EOF
}

if [[ "${1:-}" == "--output" ]]; then
  [[ $# -eq 2 ]] || { usage >&2; exit 2; }
  OUTPUT_DIR="$2"
elif [[ $# -ne 0 ]]; then
  usage >&2
  exit 2
fi

for command in cargo systemctl systemd-run awk date mktemp pgrep; do
  command -v "$command" >/dev/null || {
    echo "Missing required command: $command" >&2
    exit 1
  }
done

if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  echo "A graphical X11 or Wayland session is required." >&2
  exit 1
fi

if pgrep -x liteweb >/dev/null; then
  echo "Close the existing LiteWeb instance before starting the benchmark." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd -- "$OUTPUT_DIR" && pwd)"
cp -- "$0" "$OUTPUT_DIR/benchmark_consumption.sh"
printf 'index,url\n' > "$OUTPUT_DIR/urls.csv"
for index in "${!BENCHMARK_URLS[@]}"; do
  printf '%s,%s\n' "$((index + 1))" "${BENCHMARK_URLS[$index]}" >> "$OUTPUT_DIR/urls.csv"
done
printf 'key,value\n' > "$OUTPUT_DIR/metadata.csv"
printf 'generated_at,%s\n' "$(date -Is)" >> "$OUTPUT_DIR/metadata.csv"
printf 'warmup_seconds,30\nidle_measurement_seconds,120\npost_suspension_seconds,30\n' >> "$OUTPUT_DIR/metadata.csv"
printf 'scenario,first_suspension_s,all_suspended_s,memory_before_mib,memory_after_mib,memory_saved_mib,memory_saved_pct,cpu_before_pct,cpu_after_pct\n' > "$OUTPUT_DIR/summary.csv"
printf '# LiteWeb consumption benchmark\n\n' > "$OUTPUT_DIR/summary.md"
printf 'Generated: %s\n\n' "$(date -Is)" >> "$OUTPUT_DIR/summary.md"
printf '| Scenario | First suspension | All suspended | RAM before | RAM after | RAM saved | CPU before | CPU after |\n' >> "$OUTPUT_DIR/summary.md"
printf '|---|---:|---:|---:|---:|---:|---:|---:|\n' >> "$OUTPUT_DIR/summary.md"

echo "Building LiteWeb in release mode..."
(cd "$REPO_ROOT" && cargo build --release)
BIN="$REPO_ROOT/target/release/liteweb"

current_unit=""
current_runtime=""
cleanup() {
  if [[ -n "$current_unit" ]]; then
    systemctl --user stop "$current_unit" >/dev/null 2>&1 || true
  fi
  if [[ -n "$current_runtime" && -d "$current_runtime" ]]; then
    rm -rf -- "$current_runtime"
  fi
}
trap cleanup EXIT INT TERM

event_wall_ms() {
  local event_file="$1"
  local event_name="$2"
  awk -F ',' -v name="$event_name" '$1 == name { value=$3 } END { print value }' "$event_file"
}

event_elapsed_seconds() {
  local event_file="$1"
  local event_name="$2"
  awk -F ',' -v name="$event_name" '$1 == name { value=$2 / 1000 } END { if (value != "") printf "%.1f", value }' "$event_file"
}

average_column() {
  local csv="$1"
  local start_ms="$2"
  local end_ms="$3"
  local column="$4"
  awk -F ',' -v start="$start_ms" -v end="$end_ms" -v col="$column" '
    NR > 1 && $1 >= start && $1 <= end { sum += $col; count += 1 }
    END { if (count > 0) printf "%.4f", sum / count; else print "" }
  ' "$csv"
}

to_mib() {
  awk -v value="$1" 'BEGIN { if (value == "") print ""; else printf "%.2f", value / 1048576 }'
}

run_scenario() {
  local scenario="$1"
  local unit="liteweb-bench-${scenario}-$$"
  local events="$OUTPUT_DIR/events-${scenario}.csv"
  local samples="$OUTPUT_DIR/samples-${scenario}.csv"
  local runtime
  runtime="$(mktemp -d "${TMPDIR:-/tmp}/liteweb-benchmark.XXXXXX")"
  current_unit="$unit"
  current_runtime="$runtime"

  # Hard cap so a stuck suspension policy cannot hang the whole suite.
  # idle: ~150s; normal: 600s timeout + margins; aggressive: 60s + margins.
  local max_seconds=240
  case "$scenario" in
    idle) max_seconds=240 ;;
    normal) max_seconds=900 ;;
    aggressive) max_seconds=420 ;;
  esac

  printf 'wall_time_ms,elapsed_s,active_state,cpu_usage_ns,memory_current_bytes,memory_peak_bytes,cpu_percent\n' > "$samples"
  echo "Running $scenario scenario (timeout ${max_seconds}s)..."
  systemd-run --user --unit="$unit" --quiet \
    -p CPUAccounting=yes -p MemoryAccounting=yes \
    env \
      XDG_CONFIG_HOME="$runtime/config" \
      XDG_DATA_HOME="$runtime/data" \
      XDG_CACHE_HOME="$runtime/cache" \
      DISPLAY="${DISPLAY:-}" \
      WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" \
      XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" \
      DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" \
      "$BIN" --benchmark "$scenario" --benchmark-state "$events"

  local start_wait=$SECONDS
  while true; do
    local active
    active="$(systemctl --user show "$unit" -p ActiveState --value 2>/dev/null || true)"
    if [[ "$active" == "active" ]]; then
      break
    fi
    if [[ "$active" == "failed" || "$active" == "inactive" || $((SECONDS - start_wait)) -ge 30 ]]; then
      echo "LiteWeb did not start the $scenario scenario." >&2
      journalctl --user -u "$unit" --no-pager -n 30 >&2 || true
      return 1
    fi
    sleep 0.2
  done

  local first_sample_ms="$(date +%s%3N)"
  local sample_started=$SECONDS
  local previous_wall_ms=""
  local previous_cpu_ns=""
  while true; do
    local now_ms props active cpu_ns memory_current memory_peak cpu_pct elapsed
    now_ms="$(date +%s%3N)"
    props="$(systemctl --user show "$unit" -p ActiveState -p CPUUsageNSec -p MemoryCurrent -p MemoryPeak 2>/dev/null || true)"
    active="$(awk -F= '$1 == "ActiveState" { print $2 }' <<<"$props")"
    cpu_ns="$(awk -F= '$1 == "CPUUsageNSec" { print $2 }' <<<"$props")"
    memory_current="$(awk -F= '$1 == "MemoryCurrent" { print $2 }' <<<"$props")"
    memory_peak="$(awk -F= '$1 == "MemoryPeak" { print $2 }' <<<"$props")"
    # systemd may report "[not set]" when the unit is stopping.
    [[ "$cpu_ns" =~ ^[0-9]+$ ]] || cpu_ns=0
    [[ "$memory_current" =~ ^[0-9]+$ ]] || memory_current=0
    [[ "$memory_peak" =~ ^[0-9]+$ ]] || memory_peak=0
    elapsed="$(awk -v now="$now_ms" -v start="$first_sample_ms" 'BEGIN { printf "%.3f", (now - start) / 1000 }')"
    cpu_pct="0"
    if [[ -n "$previous_wall_ms" && "$now_ms" -gt "$previous_wall_ms" && "$cpu_ns" -ge "${previous_cpu_ns:-0}" ]]; then
      cpu_pct="$(awk -v cpu="$cpu_ns" -v previous_cpu="$previous_cpu_ns" -v now="$now_ms" -v previous_now="$previous_wall_ms" 'BEGIN { printf "%.4f", 100 * (cpu - previous_cpu) / ((now - previous_now) * 1000000) }')"
    fi
    # Skip shutdown rows: inactive cgroup + unset counters produce bogus CPU deltas.
    if [[ "$active" == "active" || "$active" == "activating" ]]; then
      printf '%s,%s,%s,%s,%s,%s,%s\n' "$now_ms" "$elapsed" "${active:-unknown}" "$cpu_ns" "$memory_current" "$memory_peak" "$cpu_pct" >> "$samples"
      previous_wall_ms="$now_ms"
      previous_cpu_ns="$cpu_ns"
    fi

    if [[ "$active" != "active" && "$active" != "activating" ]]; then
      break
    fi
    if (( SECONDS - sample_started >= max_seconds )); then
      echo "The $scenario scenario exceeded ${max_seconds}s; stopping unit." >&2
      systemctl --user stop "$unit" >/dev/null 2>&1 || true
      # Drain a few final samples while it stops.
      sleep 1
      break
    fi
    sleep 1
  done

  local completed_ms
  completed_ms="$(event_wall_ms "$events" completed)"
  if [[ -z "$completed_ms" ]]; then
    echo "The $scenario scenario ended without a completed event." >&2
    if [[ -f "$events" ]]; then
      echo "Events recorded:" >&2
      cat "$events" >&2 || true
    fi
    return 1
  fi

  local warmup_ms first_ms all_ms before_start before_end after_start
  warmup_ms="$(event_wall_ms "$events" warmup_complete)"
  first_ms="$(event_wall_ms "$events" first_suspension)"
  all_ms="$(event_wall_ms "$events" all_suspended)"
  if [[ -z "$warmup_ms" ]]; then
    echo "The $scenario scenario did not complete its warmup." >&2
    return 1
  fi
  if [[ "$scenario" != "idle" && ( -z "$first_ms" || -z "$all_ms" ) ]]; then
    echo "The $scenario scenario did not report all suspension milestones." >&2
    cat "$events" >&2 || true
    return 1
  fi
  before_start="$warmup_ms"
  before_end="$completed_ms"
  after_start="$warmup_ms"
  if [[ "$scenario" != "idle" ]]; then
    before_start=$((first_ms - 30000))
    before_end="$first_ms"
    after_start="$all_ms"
  fi

  local memory_before_raw memory_after_raw cpu_before cpu_after memory_before memory_after memory_saved memory_pct
  memory_before_raw="$(average_column "$samples" "$before_start" "$before_end" 5)"
  memory_after_raw="$(average_column "$samples" "$after_start" "$completed_ms" 5)"
  cpu_before="$(average_column "$samples" "$before_start" "$before_end" 7)"
  cpu_after="$(average_column "$samples" "$after_start" "$completed_ms" 7)"
  memory_before="$(to_mib "$memory_before_raw")"
  memory_after="$(to_mib "$memory_after_raw")"
  memory_saved="$(awk -v before="$memory_before" -v after="$memory_after" 'BEGIN { if (before == "" || after == "") print ""; else printf "%.2f", before - after }')"
  memory_pct="$(awk -v before="$memory_before" -v saved="$memory_saved" 'BEGIN { if (before == "" || before == 0 || saved == "") print ""; else printf "%.1f", 100 * saved / before }')"
  local first_s all_s
  first_s="$(event_elapsed_seconds "$events" first_suspension)"
  all_s="$(event_elapsed_seconds "$events" all_suspended)"

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$scenario" "$first_s" "$all_s" "$memory_before" "$memory_after" "$memory_saved" "$memory_pct" "$cpu_before" "$cpu_after" >> "$OUTPUT_DIR/summary.csv"
  printf '| %s | %s s | %s s | %s MiB | %s MiB | %s MiB (%s%%) | %s%% | %s%% |\n' \
    "$scenario" "${first_s:--}" "${all_s:--}" "$memory_before" "$memory_after" "$memory_saved" "$memory_pct" "$cpu_before" "$cpu_after" >> "$OUTPUT_DIR/summary.md"

  current_unit=""
  rm -rf -- "$runtime"
  current_runtime=""
}

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
