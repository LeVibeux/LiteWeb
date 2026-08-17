#!/usr/bin/env bash
# Render SVG charts from a result folder created by benchmark_consumption.sh.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/visualize_benchmark.sh BENCHMARK_RESULT_DIRECTORY

Creates readable SVG charts in BENCHMARK_RESULT_DIRECTORY/charts/:
  - memory-over-time.svg  — idle/normal/aggressive, shared linear scale
  - cpu-over-time.svg     — same series, shared log Y scale
  - memory-summary.svg    — before/after bars with MiB labels
  - memory-loaded.svg     — loaded vs ultra (3 live pages), if present
  - cpu-loaded.svg
  - memory-loaded-summary.svg

Vertical markers show when tabs start suspending / are all suspended
(from events-*.csv). Shutdown samples (inactive, zero mem, negative CPU)
are excluded.
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 2
fi

command -v gnuplot >/dev/null || {
  echo "gnuplot is required (sudo apt install gnuplot)." >&2
  exit 1
}

RUN_DIR="$1"
[[ -d "$RUN_DIR" ]] || { echo "Result directory not found: $RUN_DIR" >&2; exit 1; }
RUN_DIR="$(cd -- "$RUN_DIR" && pwd)"

for file in summary.csv samples-idle.csv samples-normal.csv samples-aggressive.csv; do
  [[ -f "$RUN_DIR/$file" ]] || {
    echo "Missing benchmark CSV: $RUN_DIR/$file" >&2
    exit 1
  }
done

CHARTS="$RUN_DIR/charts"
FILTERED="$CHARTS/.filtered"
mkdir -p "$FILTERED"

# Live samples only (drop shutdown spikes such as -5000% CPU).
filter_samples() {
  awk -F ',' 'BEGIN { OFS="," }
    NR == 1 { print; next }
    $3 == "active" && ($5 + 0) > 0 && ($7 + 0) >= 0 { print }
  ' "$1" > "$2"
}

filter_samples "$RUN_DIR/samples-idle.csv" "$FILTERED/samples-idle.csv"
filter_samples "$RUN_DIR/samples-normal.csv" "$FILTERED/samples-normal.csv"
filter_samples "$RUN_DIR/samples-aggressive.csv" "$FILTERED/samples-aggressive.csv"

# Floor CPU at a tiny epsilon so log scale can show near-idle points.
for scenario in idle normal aggressive; do
  awk -F ',' 'BEGIN { OFS="," }
    NR == 1 { print; next }
    {
      cpu = $7 + 0
      if (cpu < 0.01) cpu = 0.01
      $7 = sprintf("%.4f", cpu)
      print
    }
  ' "$FILTERED/samples-${scenario}.csv" > "$FILTERED/samples-${scenario}-log.csv"
done

# Shared axis limits.
limits="$(
  awk -F ',' '
    FNR == 1 { next }
    {
      t = $2 + 0; m = $5 / 1048576; c = $7 + 0
      if (t > xmax) xmax = t
      if (m > memmax) memmax = m
      if (c > cpumax) cpumax = c
    }
    END {
      if (xmax < 1) xmax = 1
      if (memmax < 1) memmax = 1
      if (cpumax < 1) cpumax = 1
      printf "%.3f %.2f %.2f\n", xmax * 1.02, memmax * 1.10, cpumax * 1.15
    }
  ' "$FILTERED/samples-idle.csv" \
    "$FILTERED/samples-normal.csv" \
    "$FILTERED/samples-aggressive.csv"
)"
# shellcheck disable=SC2086
set -- $limits
X_MAX="$1"
MEM_MAX="$2"
CPU_MAX="$3"

# Suspension milestones from events-*.csv → vertical arrow objects.
# Format per line: scenario,event,elapsed_s
MARKERS="$FILTERED/suspension-markers.csv"
: > "$MARKERS"
for scenario in idle normal aggressive; do
  ev="$RUN_DIR/events-${scenario}.csv"
  [[ -f "$ev" ]] || continue
  awk -F ',' -v sc="$scenario" '
    NR == 1 { next }
    $1 == "first_suspension" || $1 == "all_suspended" {
      printf "%s,%s,%.3f\n", sc, $1, ($2 + 0) / 1000.0
    }
  ' "$ev" >> "$MARKERS"
done

# Build gnuplot arrow/label commands for markers (normal + aggressive mainly).
MARKER_CMDS_MEM="$FILTERED/markers-mem.gp"
MARKER_CMDS_CPU="$FILTERED/markers-cpu.gp"
: > "$MARKER_CMDS_MEM"
: > "$MARKER_CMDS_CPU"

# Colours match series; styles distinguish first vs all suspended.
python3 - "$MARKERS" "$MARKER_CMDS_MEM" "$MARKER_CMDS_CPU" "$MEM_MAX" "$CPU_MAX" "$X_MAX" <<'PY'
import sys
from collections import defaultdict

markers_path, mem_gp, cpu_gp, mem_max, cpu_max, xmax = sys.argv[1:7]
mem_max = float(mem_max)
cpu_max = float(cpu_max)
xmax = float(xmax)

# scenario -> list of (event, t)
rows = []
with open(markers_path) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        sc, ev, t = line.split(",")
        rows.append((sc, ev, float(t)))

# Prefer all_suspended; if first==all, only draw one marker per scenario.
by_sc = defaultdict(dict)
for sc, ev, t in rows:
    by_sc[sc][ev] = t

colors = {
    "idle": "#0072B2",
    "normal": "#D55E00",
    "aggressive": "#009E73",
}

n = 0
with open(mem_gp, "w") as fm, open(cpu_gp, "w") as fc:
    for sc in ("idle", "normal", "aggressive"):
        evs = by_sc.get(sc, {})
        if not evs:
            continue
        color = colors[sc]
        items = []
        if "first_suspension" in evs and "all_suspended" in evs:
            t0, t1 = evs["first_suspension"], evs["all_suspended"]
            if abs(t0 - t1) < 0.05:
                items.append((t0, f"{sc}: all tabs suspended", "1", "high"))
            else:
                items.append((t0, f"{sc}: first suspension", "0", "mid"))
                items.append((t1, f"{sc}: all suspended", "1", "high"))
        elif "all_suspended" in evs:
            items.append((evs["all_suspended"], f"{sc}: all suspended", "1", "high"))
        elif "first_suspension" in evs:
            items.append((evs["first_suspension"], f"{sc}: first suspension", "0", "mid"))

        # Vertical label anchors (scenario base + event tier) to avoid collisions.
        base_mem = {"idle": 0.18, "normal": 0.88, "aggressive": 0.62}[sc]
        base_cpu = {"idle": 0.03, "normal": 120.0, "aggressive": 25.0}[sc]
        tier_mem = {"high": 0.0, "mid": -0.10}
        tier_cpu_mul = {"high": 1.0, "mid": 0.35}

        for t, label, dashtype, tier in items:
            n += 1
            y_mem = mem_max * (base_mem + tier_mem[tier])
            y_cpu = base_cpu * tier_cpu_mul[tier]
            if y_cpu < 0.02:
                y_cpu = 0.02
            # Near the right edge, anchor labels to the left of the line.
            if t > xmax * 0.82:
                anchor = "right"
                off = "-0.6,0"
            else:
                anchor = "left"
                off = "0.6,0"
            fm.write(
                f"set arrow {n} from {t},0 to {t},{mem_max} nohead "
                f"lc rgb '{color}' lw 1.8 dt {dashtype}\n"
            )
            fm.write(
                f"set label {n} '{label}' at {t},{y_mem} "
                f"rotate by 90 {anchor} offset {off} "
                f"tc rgb '{color}' font 'Sans,11'\n"
            )
            m = n + 100
            fc.write(
                f"set arrow {m} from {t},0.01 to {t},{cpu_max} nohead "
                f"lc rgb '{color}' lw 1.8 dt {dashtype}\n"
            )
            fc.write(
                f"set label {m} '{label}' at {t},{y_cpu} "
                f"rotate by 90 {anchor} offset {off} "
                f"tc rgb '{color}' font 'Sans,11'\n"
            )
print(f"markers: {n}", file=sys.stderr)
PY

# Summary bars with value labels.
SUMMARY_PLOT="$FILTERED/summary-bars.csv"
awk -F ',' 'BEGIN { OFS=","; print "scenario,before,after" }
  NR == 1 { next }
  $1 == "idle" || $1 == "normal" || $1 == "aggressive" {
    printf "%s,%.2f,%.2f\n", $1, $4+0, $5+0
  }
' "$RUN_DIR/summary.csv" > "$SUMMARY_PLOT"

gnuplot 2>"$CHARTS/gnuplot.err" <<GNUPLOT
set datafile separator comma
set terminal svg size 1400,900 dynamic enhanced font 'Sans,14'
set border linewidth 1.2
set grid ytics xtics lt 0 lc rgb '#CCCCCC'
set key bottom center horizontal outside samplen 2 spacing 1.2 font 'Sans,13'
set tics font 'Sans,12'
set xlabel font 'Sans,13'
set ylabel font 'Sans,13'
set title font 'Sans,16'
set lmargin 12
set rmargin 6
set tmargin 3
set bmargin 6

# Colourblind-friendly, high contrast
set style line 1 lc rgb '#0072B2' lw 2.8 lt 1
set style line 2 lc rgb '#D55E00' lw 2.8 lt 1
set style line 3 lc rgb '#009E73' lw 2.8 lt 1
set style line 10 lc rgb '#6B5B95' lw 1
set style line 11 lc rgb '#009E73' lw 1

# ---- Memory over time ----
set output '${CHARTS}/memory-over-time.svg'
set title 'LiteWeb memory over time — Idle / Normal / Aggressive'
set xlabel 'Elapsed time (seconds)'
set ylabel 'Memory (MiB)'
set xrange [0:${X_MAX}]
set yrange [0:${MEM_MAX}]
load '${MARKER_CMDS_MEM}'
plot \
  '${FILTERED}/samples-idle.csv' every ::1 using 2:(\$5/1048576) with lines ls 1 title 'Idle', \
  '${FILTERED}/samples-normal.csv' every ::1 using 2:(\$5/1048576) with lines ls 2 title 'Normal', \
  '${FILTERED}/samples-aggressive.csv' every ::1 using 2:(\$5/1048576) with lines ls 3 title 'Aggressive'
set output
unset arrow
unset label

# ---- CPU log scale (one panel, shared X/Y, 3 series + markers) ----
set output '${CHARTS}/cpu-over-time.svg'
set title 'LiteWeb CPU over time (log scale) — Idle / Normal / Aggressive'
set xlabel 'Elapsed time (seconds)'
set ylabel 'CPU (%)  [log scale]'
set xrange [0:${X_MAX}]
set logscale y
set yrange [0.01:${CPU_MAX}]
set format y '%g'
set ytics (0.01, 0.1, 0.5, 1, 5, 10, 50, 100, 500)
load '${MARKER_CMDS_CPU}'
plot \
  '${FILTERED}/samples-idle-log.csv' every ::1 using 2:7 with lines ls 1 title 'Idle', \
  '${FILTERED}/samples-normal-log.csv' every ::1 using 2:7 with lines ls 2 title 'Normal', \
  '${FILTERED}/samples-aggressive-log.csv' every ::1 using 2:7 with lines ls 3 title 'Aggressive'
set output
unset arrow
unset label
unset logscale y
set format y '%g'

# ---- Memory summary bars ----
set terminal svg size 1200,800 dynamic enhanced font 'Sans,14'
set output '${CHARTS}/memory-summary.svg'
unset logscale
unset arrow
unset label
set format x '%g'
set format y '%g'
set ytics auto
set xtics auto
set title 'Average memory before and after tab suspension' font 'Sans,16'
set xlabel ''
set ylabel 'Memory (MiB)'
set style data histograms
set style histogram clustered gap 1
set style fill solid 0.85 border lc rgb '#333333'
set boxwidth 0.85 relative
set key bottom center horizontal outside font 'Sans,13'
set xtics font 'Sans,14'
set yrange [0:*]
set xrange [*:*]
set grid ytics lt 0 lc rgb '#CCCCCC'
set grid noxtics
set bmargin 6
set lmargin 12
set tmargin 3
set rmargin 4
plot \
  '${SUMMARY_PLOT}' every ::1 using 2:xtic(1) with histogram ls 10 title 'Before', \
  '' every ::1 using 3 with histogram ls 11 title 'After', \
  '' every ::1 using (\$0-0.18):(\$2):(sprintf('%.0f', \$2)) with labels offset 0,0.7 font 'Sans,11' notitle, \
  '' every ::1 using (\$0+0.18):(\$3):(sprintf('%.0f', \$3)) with labels offset 0,0.7 font 'Sans,11' notitle
set output
GNUPLOT

if [[ ! -s "$CHARTS/memory-over-time.svg" || ! -s "$CHARTS/cpu-over-time.svg" || ! -s "$CHARTS/memory-summary.svg" ]]; then
  echo "gnuplot failed to create charts:" >&2
  cat "$CHARTS/gnuplot.err" >&2 || true
  exit 1
fi
rm -f "$CHARTS/gnuplot.err"

if [[ -f "$RUN_DIR/samples-loaded.csv" && -f "$RUN_DIR/samples-ultra.csv" ]]; then
  filter_samples "$RUN_DIR/samples-loaded.csv" "$FILTERED/samples-loaded.csv"
  filter_samples "$RUN_DIR/samples-ultra.csv" "$FILTERED/samples-ultra.csv"
  for scenario in loaded ultra; do
    awk -F ',' 'BEGIN { OFS="," }
      NR == 1 { print; next }
      {
        cpu = $7 + 0
        if (cpu < 0.01) cpu = 0.01
        $7 = sprintf("%.4f", cpu)
        print
      }
    ' "$FILTERED/samples-${scenario}.csv" > "$FILTERED/samples-${scenario}-log.csv"
  done
  loaded_limits="$(
    awk -F ',' '
      FNR == 1 { next }
      {
        t = $2 + 0; m = $5 / 1048576; c = $7 + 0
        if (t > xmax) xmax = t
        if (m > memmax) memmax = m
        if (c > cpumax) cpumax = c
      }
      END {
        if (xmax < 1) xmax = 1
        if (memmax < 1) memmax = 1
        if (cpumax < 1) cpumax = 1
        printf "%.3f %.2f %.2f\n", xmax * 1.02, memmax * 1.10, cpumax * 1.15
      }
    ' "$FILTERED/samples-loaded.csv" "$FILTERED/samples-ultra.csv"
  )"
  # shellcheck disable=SC2086
  set -- $loaded_limits
  LX_MAX="$1"
  LMEM_MAX="$2"
  LCPU_MAX="$3"
  LOADED_SUMMARY="$FILTERED/loaded-bars.csv"
  awk -F ',' 'BEGIN { OFS=","; print "scenario,memory" }
    NR == 1 { next }
    $1 == "loaded" || $1 == "ultra" { printf "%s,%.2f\n", $1, $4+0 }
  ' "$RUN_DIR/summary.csv" > "$LOADED_SUMMARY"

  gnuplot 2>"$CHARTS/gnuplot-loaded.err" <<GNUPLOT
set datafile separator comma
set terminal svg size 1400,900 dynamic enhanced font 'Sans,14'
set border linewidth 1.2
set grid ytics xtics lt 0 lc rgb '#CCCCCC'
set key bottom center horizontal outside samplen 2 spacing 1.2 font 'Sans,13'
set tics font 'Sans,12'
set xlabel font 'Sans,13'
set ylabel font 'Sans,13'
set title font 'Sans,16'
set lmargin 12
set rmargin 6
set tmargin 3
set bmargin 6
set style line 2 lc rgb '#D55E00' lw 2.8 lt 1
set style line 4 lc rgb '#CC79A7' lw 2.8 lt 1
set style line 10 lc rgb '#6B5B95' lw 1
set style line 12 lc rgb '#CC79A7' lw 1

set output '${CHARTS}/memory-loaded.svg'
set title 'LiteWeb memory — same 3 pages loaded (Normal vs Ultra)'
set xlabel 'Elapsed time (seconds)'
set ylabel 'Memory (MiB)'
set xrange [0:${LX_MAX}]
set yrange [0:${LMEM_MAX}]
plot \
  '${FILTERED}/samples-loaded.csv' every ::1 using 2:(\$5/1048576) with lines ls 2 title 'Loaded (Normal engine)', \
  '${FILTERED}/samples-ultra.csv' every ::1 using 2:(\$5/1048576) with lines ls 4 title 'Ultra (stripped engine)'
set output

set output '${CHARTS}/cpu-loaded.svg'
set title 'LiteWeb CPU — same 3 pages loaded (log scale)'
set xlabel 'Elapsed time (seconds)'
set ylabel 'CPU (%)  [log scale]'
set xrange [0:${LX_MAX}]
set logscale y
set yrange [0.01:${LCPU_MAX}]
set format y '%g'
set ytics (0.01, 0.1, 0.5, 1, 5, 10, 50, 100, 500)
plot \
  '${FILTERED}/samples-loaded-log.csv' every ::1 using 2:7 with lines ls 2 title 'Loaded (Normal engine)', \
  '${FILTERED}/samples-ultra-log.csv' every ::1 using 2:7 with lines ls 4 title 'Ultra (stripped engine)'
set output
unset logscale y
set format y '%g'

set terminal svg size 1200,800 dynamic enhanced font 'Sans,14'
set output '${CHARTS}/memory-loaded-summary.svg'
unset logscale
set format x '%g'
set format y '%g'
set ytics auto
set xtics auto
set title 'Cruise memory with 3 pages still loaded' font 'Sans,16'
set xlabel ''
set ylabel 'Memory (MiB)'
set style data histograms
set style histogram clustered gap 1
set style fill solid 0.85 border lc rgb '#333333'
set boxwidth 0.85 relative
set key off
set xtics font 'Sans,14'
set yrange [0:*]
set xrange [*:*]
set grid ytics lt 0 lc rgb '#CCCCCC'
set grid noxtics
set bmargin 6
set lmargin 12
set tmargin 3
set rmargin 4
plot \
  '${LOADED_SUMMARY}' every ::1 using 2:xtic(1) with histogram ls 12 title 'Cruise RAM', \
  '' every ::1 using (\$0):(\$2):(sprintf('%.0f', \$2)) with labels offset 0,0.7 font 'Sans,11' notitle
set output
GNUPLOT

  if [[ ! -s "$CHARTS/memory-loaded.svg" || ! -s "$CHARTS/cpu-loaded.svg" || ! -s "$CHARTS/memory-loaded-summary.svg" ]]; then
    echo "gnuplot failed to create loaded/ultra charts:" >&2
    cat "$CHARTS/gnuplot-loaded.err" >&2 || true
    exit 1
  fi
  rm -f "$CHARTS/gnuplot-loaded.err"
fi

echo "Charts created in: $CHARTS"
echo "  memory-over-time.svg  (markers = suspension times)"
echo "  cpu-over-time.svg     (log Y + suspension markers)"
echo "  memory-summary.svg"
if [[ -s "$CHARTS/memory-loaded.svg" ]]; then
  echo "  memory-loaded.svg / cpu-loaded.svg / memory-loaded-summary.svg"
fi
echo "  scales: x<=${X_MAX}s  mem<=${MEM_MAX} MiB  cpu_log<=${CPU_MAX}%"
if [[ -s "$MARKERS" ]]; then
  echo "  suspension markers:"
  column -t -s, "$MARKERS" | sed 's/^/    /'
fi
