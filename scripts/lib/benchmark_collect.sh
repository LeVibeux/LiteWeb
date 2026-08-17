# Shared collection helpers for LiteWeb consumption benches.
# Sourced by benchmark_consumption.sh and benchmark_ultra.sh.
# Caller must set SCRIPT_DIR, REPO_ROOT, and OUTPUT_DIR.

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
LOADED_BENCHMARK_URLS=(
  "https://fr.wikipedia.org/wiki/Navigateur_web"
  "https://www.rust-lang.org/fr/"
  "https://news.ycombinator.com/"
)

current_unit=""
current_runtime=""
BIN=""

benchmark_preflight() {
  local command
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
}

benchmark_build() {
  echo "Building LiteWeb in release mode..."
  (cd "$REPO_ROOT" && cargo build --release)
  BIN="$REPO_ROOT/target/release/liteweb"
}

benchmark_cleanup() {
  if [[ -n "$current_unit" ]]; then
    systemctl --user stop "$current_unit" >/dev/null 2>&1 || true
  fi
  if [[ -n "$current_runtime" && -d "$current_runtime" ]]; then
    rm -rf -- "$current_runtime"
  fi
}

benchmark_install_trap() {
  trap benchmark_cleanup EXIT INT TERM
}

write_url_list() {
  local dest="$1"
  shift
  local index=0
  printf 'index,url\n' > "$dest"
  for url in "$@"; do
    index=$((index + 1))
    printf '%s,%s\n' "$index" "$url" >> "$dest"
  done
}

init_summary_files() {
  local title="$1"
  printf 'key,value\n' > "$OUTPUT_DIR/metadata.csv"
  printf 'generated_at,%s\n' "$(date -Is)" >> "$OUTPUT_DIR/metadata.csv"
  printf 'warmup_seconds,30\nidle_measurement_seconds,120\npost_suspension_seconds,30\n' >> "$OUTPUT_DIR/metadata.csv"
  printf 'scenario,first_suspension_s,all_suspended_s,memory_before_mib,memory_after_mib,memory_saved_mib,memory_saved_pct,cpu_before_pct,cpu_after_pct\n' > "$OUTPUT_DIR/summary.csv"
  printf '# %s\n\n' "$title" > "$OUTPUT_DIR/summary.md"
  printf 'Generated: %s\n\n' "$(date -Is)" >> "$OUTPUT_DIR/summary.md"
  printf '| Scenario | First suspension | All suspended | RAM before | RAM after | RAM saved | CPU before | CPU after |\n' >> "$OUTPUT_DIR/summary.md"
  printf '|---|---:|---:|---:|---:|---:|---:|---:|\n' >> "$OUTPUT_DIR/summary.md"
}

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

  # idle/loaded/ultra: ~150s; normal: 600s timeout + margins; aggressive: 60s + margins.
  local max_seconds=240
  case "$scenario" in
    idle|loaded|ultra) max_seconds=240 ;;
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

  local first_sample_ms
  first_sample_ms="$(date +%s%3N)"
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
  if [[ "$scenario" == "normal" || "$scenario" == "aggressive" ]] && [[ -z "$first_ms" || -z "$all_ms" ]]; then
    echo "The $scenario scenario did not report all suspension milestones." >&2
    cat "$events" >&2 || true
    return 1
  fi
  before_start="$warmup_ms"
  before_end="$completed_ms"
  after_start="$warmup_ms"
  if [[ "$scenario" == "normal" || "$scenario" == "aggressive" ]]; then
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

maybe_visualize() {
  if ! command -v gnuplot >/dev/null; then
    echo "gnuplot not found; skip charts. Install it then run:" >&2
    echo "  ./scripts/visualize_benchmark.sh $OUTPUT_DIR" >&2
    return 0
  fi
  "$SCRIPT_DIR/visualize_benchmark.sh" "$OUTPUT_DIR"
}

find_chromium() {
  local candidate
  # Prefer the real Chrome/Chromium binary. Snap's /snap/bin/chromium is a
  # wrapper that leaves the cgroup, so systemd MemoryCurrent collapses to ~0.
  for candidate in google-chrome-stable google-chrome chromium-browser chromium; do
    if command -v "$candidate" >/dev/null; then
      command -v "$candidate"
      return 0
    fi
  done
  return 1
}

write_event() {
  local event_file="$1"
  local name="$2"
  local elapsed_ms="$3"
  local wall_ms="$4"
  printf '%s,%s,%s,0\n' "$name" "$elapsed_ms" "$wall_ms" >> "$event_file"
}

# Same 3 live pages as loaded/ultra, stock Chromium (or Chrome) profile.
run_chromium_scenario() {
  local scenario="chromium"
  local browser
  browser="$(find_chromium)" || {
    echo "Chromium/Chrome is required for the $scenario scenario." >&2
    return 1
  }
  local unit="liteweb-bench-${scenario}-$$"
  local events="$OUTPUT_DIR/events-${scenario}.csv"
  local samples="$OUTPUT_DIR/samples-${scenario}.csv"
  local runtime
  runtime="$(mktemp -d "${HOME}/.cache/liteweb-chromium-bench.XXXXXX")"
  current_unit="$unit"
  current_runtime="$runtime"

  local max_seconds=240
  printf 'event,elapsed_ms,wall_time_ms,suspended_tabs\n' > "$events"
  printf 'wall_time_ms,elapsed_s,active_state,cpu_usage_ns,memory_current_bytes,memory_peak_bytes,cpu_percent\n' > "$samples"
  echo "Running $scenario scenario via $browser (timeout ${max_seconds}s)..."

  systemd-run --user --unit="$unit" --quiet \
    -p CPUAccounting=yes -p MemoryAccounting=yes \
    -p Delegate=yes -p KillMode=control-group \
    env \
      DISPLAY="${DISPLAY:-}" \
      WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" \
      XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" \
      DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" \
      "$browser" \
        --user-data-dir="$runtime/profile" \
        --no-first-run \
        --no-default-browser-check \
        --disable-session-crashed-bubble \
        --disable-sync \
        --disable-extensions \
        --disable-features=TranslateUI \
        --password-store=basic \
        --noerrdialogs \
        "${LOADED_BENCHMARK_URLS[@]}"

  local start_wait=$SECONDS
  while true; do
    local active
    active="$(systemctl --user show "$unit" -p ActiveState --value 2>/dev/null || true)"
    if [[ "$active" == "active" ]]; then
      break
    fi
    if [[ "$active" == "failed" || "$active" == "inactive" || $((SECONDS - start_wait)) -ge 30 ]]; then
      echo "Chromium did not start the $scenario scenario." >&2
      journalctl --user -u "$unit" --no-pager -n 30 >&2 || true
      return 1
    fi
    sleep 0.2
  done

  local first_sample_ms
  first_sample_ms="$(date +%s%3N)"
  write_event "$events" run_started 0 "$first_sample_ms"
  local sample_started=$SECONDS
  local previous_wall_ms=""
  local previous_cpu_ns=""
  local warmup_written=0
  while true; do
    local now_ms props active cpu_ns memory_current memory_peak cpu_pct elapsed elapsed_ms
    now_ms="$(date +%s%3N)"
    props="$(systemctl --user show "$unit" -p ActiveState -p CPUUsageNSec -p MemoryCurrent -p MemoryPeak 2>/dev/null || true)"
    active="$(awk -F= '$1 == "ActiveState" { print $2 }' <<<"$props")"
    cpu_ns="$(awk -F= '$1 == "CPUUsageNSec" { print $2 }' <<<"$props")"
    memory_current="$(awk -F= '$1 == "MemoryCurrent" { print $2 }' <<<"$props")"
    memory_peak="$(awk -F= '$1 == "MemoryPeak" { print $2 }' <<<"$props")"
    [[ "$cpu_ns" =~ ^[0-9]+$ ]] || cpu_ns=0
    [[ "$memory_current" =~ ^[0-9]+$ ]] || memory_current=0
    [[ "$memory_peak" =~ ^[0-9]+$ ]] || memory_peak=0
    elapsed="$(awk -v now="$now_ms" -v start="$first_sample_ms" 'BEGIN { printf "%.3f", (now - start) / 1000 }')"
    elapsed_ms="$(awk -v now="$now_ms" -v start="$first_sample_ms" 'BEGIN { printf "%.0f", now - start }')"
    cpu_pct="0"
    if [[ -n "$previous_wall_ms" && "$now_ms" -gt "$previous_wall_ms" && "$cpu_ns" -ge "${previous_cpu_ns:-0}" ]]; then
      cpu_pct="$(awk -v cpu="$cpu_ns" -v previous_cpu="$previous_cpu_ns" -v now="$now_ms" -v previous_now="$previous_wall_ms" 'BEGIN { printf "%.4f", 100 * (cpu - previous_cpu) / ((now - previous_now) * 1000000) }')"
    fi
    if [[ "$active" == "active" || "$active" == "activating" ]]; then
      printf '%s,%s,%s,%s,%s,%s,%s\n' "$now_ms" "$elapsed" "${active:-unknown}" "$cpu_ns" "$memory_current" "$memory_peak" "$cpu_pct" >> "$samples"
      previous_wall_ms="$now_ms"
      previous_cpu_ns="$cpu_ns"
    fi

    if [[ "$warmup_written" -eq 0 && "$elapsed_ms" -ge 30000 ]]; then
      write_event "$events" warmup_complete "$elapsed_ms" "$now_ms"
      warmup_written=1
    fi

    if [[ "$active" != "active" && "$active" != "activating" ]]; then
      break
    fi
    if (( SECONDS - sample_started >= 150 )); then
      write_event "$events" completed "$elapsed_ms" "$now_ms"
      systemctl --user stop "$unit" >/dev/null 2>&1 || true
      sleep 1
      break
    fi
    if (( SECONDS - sample_started >= max_seconds )); then
      echo "The $scenario scenario exceeded ${max_seconds}s; stopping unit." >&2
      systemctl --user stop "$unit" >/dev/null 2>&1 || true
      sleep 1
      break
    fi
    sleep 1
  done

  if ! grep -q '^completed,' "$events"; then
    echo "The $scenario scenario ended without a completed event." >&2
    cat "$events" >&2 || true
    return 1
  fi

  local warmup_ms completed_ms
  warmup_ms="$(event_wall_ms "$events" warmup_complete)"
  completed_ms="$(event_wall_ms "$events" completed)"
  if [[ -z "$warmup_ms" || -z "$completed_ms" ]]; then
    echo "The $scenario scenario did not complete its warmup." >&2
    return 1
  fi

  local memory_before_raw cpu_before memory_before
  memory_before_raw="$(average_column "$samples" "$warmup_ms" "$completed_ms" 5)"
  cpu_before="$(average_column "$samples" "$warmup_ms" "$completed_ms" 7)"
  memory_before="$(to_mib "$memory_before_raw")"

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$scenario" "" "" "$memory_before" "$memory_before" "0.00" "0.0" "$cpu_before" "$cpu_before" >> "$OUTPUT_DIR/summary.csv"
  printf '| %s | %s s | %s s | %s MiB | %s MiB | %s MiB (%s%%) | %s%% | %s%% |\n' \
    "$scenario" "-" "-" "$memory_before" "$memory_before" "0.00" "0.0" "$cpu_before" "$cpu_before" >> "$OUTPUT_DIR/summary.md"

  current_unit=""
  rm -rf -- "$runtime"
  current_runtime=""
}
