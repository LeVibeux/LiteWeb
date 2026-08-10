<p align="center">
  <img src="assets/liteweb-logo-grok.jpg" alt="LiteWeb logo" width="96">
</p>

# LiteWeb

Lightweight Linux web browser optimized for CPU, memory, and energy usage.

## Features

- Standard navigation (back, forward, reload, tabs)
- Blocks navigation to advertising and tracking domains (~50 high-impact domains)
- Energy-saving modes (Normal / Eco / Aggressive) with automatic suspension of inactive tabs
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

- Navigation is limited to `http://`, `https://`, and `about:blank`; local or active schemes (`file:`, `data:`, `javascript:`) are rejected.
- Web permissions (camera, microphone, geolocation, notifications) and file choosers are denied by default.
- WebKit sandboxing is enabled, TLS errors are blocking, and remote automation is disabled.
- The SQLite database uses mode `0600` and profile directories use mode `0700` on Unix.

The built-in filter applies to navigations. It is not a complete subresource blocker such as EasyList.

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
:eco on|off|aggressive
:bookmark list
:history
```

## Energy modes

| Mode | Suspend after | Maximum active tabs |
|------|---------------|---------------------|
| Normal | 10 min | 20 |
| Eco | 3 min | 10 |
| Aggressive | 1 min | 5 |

Suspended tabs release their WebView (a significant memory saving) and resume when clicked.

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
