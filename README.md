<p align="center">
  <img src="assets/liteweb-logo-grok.jpg" alt="LiteWeb logo" width="96">
</p>

# LiteWeb

Lightweight Linux web browser optimized for CPU, memory, and energy usage.

> **Preview (v0.1).** Linux + system WebKit only. This is not a Firefox/Chrome replacement: no downloads yet, the filter applies to navigations only, and everyday sites that need HTTP Basic or pop-up-heavy flows will break. Use it as a light reader / energy-saving browser.

## Features

- Standard navigation (back, forward, reload, tabs)
- Blocks navigation to advertising and tracking domains (~50 high-impact domains)
- Energy-saving modes (Normal / Eco / Aggressive / Ultra) with automatic suspension of inactive tabs
- History and bookmarks (local SQLite)
- Keyboard shortcuts and a command palette (`:`)

## Requirements (Linux)

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsqlite3-dev build-essential
```

## Build

```bash
cargo build --release
```

The binary is located at `target/release/liteweb`.

## Run

```bash
./target/release/liteweb
```

User data: `~/.config/liteweb/liteweb.db`

## Security

- Navigation is limited to `http://`, `https://`, and `about:blank`; local or active schemes (`file:`, `data:`, `javascript:`) are rejected. Missing or unparsable URIs fail closed.
- Popups (`target=_blank`, `window.open`) never create an unmanaged WebView. A safe URL opens in a new LiteWeb tab; anything else is dropped. Downloads and HTTP Basic prompts are cancelled until those features exist.
- Web permissions (camera, microphone, geolocation, notifications) and file choosers are denied by default. Fullscreen is disabled so a page cannot cover the address bar.
- WebKit sandboxing is enabled, TLS errors are blocking, remote automation is disabled, and third-party cookies are rejected.
- The address bar updates on the committed URL (not only when the load finishes), shows punycode for IDN hosts, and strips bidi overrides. Ultra reader pages are flattened and shipped with a strict CSP.
- If `bwrap` is missing, the hint bar warns that the WebKit sandbox cannot start.
- The SQLite database uses mode `0600` and profile directories use mode `0700` on Unix. Symlinked profile or database paths are refused.

The built-in filter applies to navigations. It is not a complete subresource blocker such as EasyList. See `docs/security/2026-08-17-browser-audit.md`.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+L` | Focus the address bar |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `Ctrl+R` / `F5` | Reload |
| `Alt+←` / `Alt+→` | Back / forward |
| `Ctrl+D` | Add a bookmark |
| `Ctrl+Shift+E` | Change energy mode |
| `:` | Command palette |

## Commands

```
:open example.com
:tab 2
:suspend
:suspend-all
:eco on|off|aggressive|ultra
:bookmark list
:history
```

## Energy modes

| Mode | Suspend after | Maximum active tabs | Page engine |
|------|---------------|---------------------|-------------|
| Normal | 10 min | 20 | Full WebKit |
| Eco | 3 min | 10 | Full WebKit |
| Aggressive | 1 min | 5 | Full WebKit |
| Ultra | 15 s | 1 | No JS, no images, no media, no GPU, reader flatten |

Suspended tabs release their WebView (a significant memory saving) and resume when clicked.

**Ultra** is the emergency / reading mode. It reloads live tabs as a stripped article (no JavaScript, images, or media). Web apps that need JS will look empty; cycle back with `Ctrl+Shift+E` or `:eco off`.

## Consumption benchmark

### Sample results (local machine)

Cgroup CPU/RAM for LiteWeb + WebKit children. **Cruise figures are the arithmetic mean** of ~1 Hz samples from `warmup_complete` → `completed` (not median, not a single last sample). Startup/load before warmup is excluded.

#### Tab suspension suite (2026-08-11)

10 fixed public pages + 1 blank sentinel (`idle` = 1 Google tab).

| Scenario | All tabs suspended | RAM before → after | RAM saved | CPU after (mean) |
|----------|-------------------:|-------------------:|----------:|-----------------:|
| idle | — | 394 → 394 MiB | 0% | 0.07% |
| normal | 600.6 s | 1498 → 164 MiB | **89%** (−1.3 GiB) | 0.33% |
| aggressive | 60.1 s | 1395 → 204 MiB | **85%** (−1.2 GiB) | 1.8% |

<p align="center">
  <img src="assets/benchmark/memory-summary.png" alt="Memory before/after tab suspension" width="640">
</p>
<p align="center">
  <img src="assets/benchmark/memory-over-time.png" alt="Memory over time by scenario" width="720">
</p>
<p align="center">
  <img src="assets/benchmark/cpu-over-time.png" alt="CPU over time (log scale) by scenario" width="720">
</p>

#### Live engine suite — Normal vs Ultra vs Chromium (2026-08-16 / 17)

Same 3 pages kept alive (Wikipedia, rust-lang, HN); **no tab suspension**. 30 s warmup + 120 s measure. Chromium is stock Google Chrome 151 (same URLs, fresh profile).

| Scenario | RAM cruise (mean) | CPU mean | CPU median | vs Chromium |
|----------|------------------:|---------:|-----------:|----------|
| chromium | 488 MiB | 2.52% | 0.49% | — |
| loaded (Normal) | 451.5 MiB | 1.74% | 0.05% | −7.5% RAM |
| ultra | 312.3 MiB | 0.52% | 0.03% | **−36% RAM**, **−80% CPU** |

<p align="center">
  <img src="assets/benchmark/memory-loaded-summary.png" alt="Cruise memory Chromium vs LiteWeb Normal vs Ultra" width="640">
</p>
<p align="center">
  <img src="assets/benchmark/memory-loaded.png" alt="Memory over time Chromium vs LiteWeb" width="720">
</p>
<p align="center">
  <img src="assets/benchmark/cpu-loaded.png" alt="CPU over time (log) Chromium vs LiteWeb" width="720">
</p>
<p align="center">
  <img src="assets/benchmark/cpu-loaded-summary.png" alt="Cruise CPU mean vs median" width="640">
</p>

CPU **mean** is `cpu_after_pct` over the post-warmup window. It is **not a median**. Medians stay near idle (see the last chart); a few spikes pull the mean up. Ultra mainly damps those spikes. Prefer the mean for “CPU budget while the browser is open”.

**What the gains mean**

- Most of the suspension-suite win is **RAM**: suspended tabs drop their WebView; only the active blank tab keeps a live engine. That is why after-suspension memory can fall *below* the idle Google baseline.
- **Normal** waits the full 10 min inactivity timeout, then suspends all 10 pages at once → largest, cleanest memory drop.
- **Aggressive** hits the max-active-tabs limit first (~30 s, 6 tabs), then the 1 min timeout (~60 s, 10/10) → same kind of saving, much sooner.
- **Ultra** is measured against **loaded** and **stock Chromium**, not against after-suspend RAM: same 3 pages stay alive; Ultra only strips the engine (no JS/images/media/GPU, reader flatten). Run `./scripts/benchmark_ultra.sh` then `./scripts/benchmark_chromium.sh --output …`.
- **CPU** after suspension stays low; startup/load spikes are excluded from cruise windows. The benchmark measures cgroup CPU/RAM, not wall-power watts, and does not claim JS throttling as the sole source of CPU savings.
- Numbers depend on machine load and live page weight; re-run locally for your hardware.

### How to run

```bash
# 10-tab suspension suite (~15 min)
./scripts/benchmark_consumption.sh
./scripts/visualize_benchmark.sh benchmark-results/run-YYYYMMDD-HHMMSS

# Same 3 pages, Normal vs Ultra (~5 min) — writes loaded/ultra charts
./scripts/benchmark_ultra.sh

# Overlay Ultra / Chromium charts onto an existing suspension-run folder
./scripts/visualize_benchmark.sh benchmark-results/run-YYYYMMDD-HHMMSS \
    --also benchmark-results/ultra-YYYYMMDD-HHMMSS

# Stock Chromium on the same 3 pages (~2.5 min); append onto the Ultra folder
./scripts/benchmark_chromium.sh --output benchmark-results/ultra-YYYYMMDD-HHMMSS
```

Graphical session, fresh profile per scenario. Outputs CSV + `summary.md` under `benchmark-results/`. For comparable runs: AC power, fixed brightness, no other heavy browsers.

## Architecture

- **Rust** + **WebKitGTK 4.1** + **GTK3**
- System-shared WebKit engine (lighter than Chromium)
- Cache disabled, DNS prefetch disabled, and media autoplay blocked by default

## Logo

The logo was generated with **Grok Image** and cropped to a 1:1 aspect ratio for the LiteWeb icon.

## Roadmap

- [ ] File downloads
- [ ] Automatic EasyList filter updates
- [ ] Windows (WebView2) / macOS (WKWebView) ports
- [ ] GTK4 + libadwaita (when available on the target platform)
