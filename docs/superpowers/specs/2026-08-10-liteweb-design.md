# LiteWeb — Design Specification

**Date:** 2026-08-10  
**Status:** Approved  
**Platform v1:** Linux (WebKitGTK + GTK3)  
**Note:** GTK4/libadwaita prévu lorsque les dépendances système seront disponibles.
**Platform phase 2:** Windows (WebView2), macOS (WKWebView)

## Goal

Build a lightweight web browser optimized for CPU, RAM, and energy usage, targeting everyday web compatibility (news, email, forums, e-commerce) without Chrome-level heaviness.

## Requirements Summary

| Requirement | Detail |
|-------------|--------|
| Compatibility | Between minimal (A) and standard (B) — most daily sites |
| UI | Minimal classic: address bar, tabs, basic toolbar; keyboard-first emphasis |
| Features v1 | Navigation, tabs, history, bookmarks, downloads, adblock, energy modes, command palette |
| Platform | Linux first; portable architecture for Windows/macOS later |

## Architecture

```
┌─────────────────────────────────────────────────┐
│                   LiteWeb UI                     │
│  (GTK4/libadwaita — barre d'adresse, onglets)   │
├─────────────────────────────────────────────────┤
│              Command Palette (Ctrl+L)            │
├──────────┬──────────┬──────────┬──────────────┤
│ Tab Mgr  │ History  │ Bookmarks│  Downloads   │
├──────────┴──────────┴──────────┴──────────────┤
│            Energy Manager                        │
├─────────────────────────────────────────────────┤
│         Content Blocker (EasyList/uBlock)        │
├─────────────────────────────────────────────────┤
│      Engine Trait → WebKitGTK (Linux v1)        │
└─────────────────────────────────────────────────┘
```

### Components

- **Tab Manager** — max 20 active WebViews in RAM; beyond that, auto-suspend. Suspended tabs store URL, title, favicon, scroll position in SQLite.
- **Energy Manager** — three levels (Normal, Eco, Aggressive) controlling suspend timeouts, JS throttling, media autoplay, max active tabs.
- **Content Blocker** — EasyList + EasyPrivacy filters; network interception before load via WebKit APIs.
- **Command Palette** — `:` prefix commands for power users.
- **Storage** — SQLite at `~/.config/liteweb/liteweb.db`.

### Engine Abstraction

```rust
pub trait BrowserEngine {
    fn create_view(&self) -> Box<dyn WebView>;
    fn apply_content_rules(&self, rules: &[ContentRule]);
    fn set_js_throttle(&self, level: ThrottleLevel);
}
```

Linux v1 implements this with WebKitGTK 4.1. Future platforms swap the backend without changing UI/business logic.

## User Interface

- GTK4 + libadwaita, follows system dark/light theme
- Toolbar (~36px): back, forward, reload, address bar, bookmark, downloads, eco toggle
- Tab bar: title (truncated), favicon, suspend indicator (💤), modified indicator (•)
- No sidebar in v1 — bookmarks/history via command palette
- Optional status bar (hidden by default)

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+L | Focus address bar |
| Ctrl+T | New tab |
| Ctrl+W | Close tab |
| Ctrl+Tab | Next tab |
| Ctrl+Shift+Tab | Previous tab |
| Ctrl+R / F5 | Reload |
| Alt+← / Alt+→ | Back / Forward |
| Ctrl+D | Add bookmark |
| Ctrl+Shift+E | Toggle eco mode |
| `:` | Open command palette |

### Command Palette

| Command | Description |
|---------|-------------|
| `:open <url>` | Open URL |
| `:tab <n>` | Switch to tab n |
| `:suspend` | Suspend current tab |
| `:suspend-all` | Suspend all except active |
| `:eco on\|off\|aggressive` | Energy mode |
| `:bookmark add\|list` | Bookmarks |
| `:history` | Recent history |
| `:download list` | Downloads |

## Energy Modes

| Level | Suspend after | Max active tabs | JS throttle | Autoplay |
|-------|---------------|-----------------|-------------|----------|
| Normal | 10 min | 20 | Off | Allowed |
| Eco | 3 min | 10 | Background tabs | Blocked |
| Aggressive | 1 min | 5 | All non-active | Blocked |

Suspended tab: WebView destroyed, state in SQLite. Reactivation recreates WebView (~200ms). Expected savings: 80–150 MB RAM per suspended tab.

## Content Blocker

- Filters: EasyList + EasyPrivacy (bundled, updatable every 7 days)
- Interception at WebKit network layer before request
- Toolbar counter: "N blocked"

## Error Handling

- Network errors → local lightweight error page (no external WebView)
- WebView crash → mark tab errored, other tabs unaffected
- Corrupt SQLite → recreate DB with user warning

## Testing

- Unit tests: adblock filter parser, tab manager logic, energy rules
- Integration tests: open URL, suspend/reactivate tab
- No heavy E2E in v1

## System Dependencies (Linux)

```
webkit2gtk-4.1  libadwaita-1  gtk4  sqlite3
```

Build: `cargo build --release` → `liteweb` binary (~5–8 MB)

## Out of Scope (v1)

- Extensions / sync / password manager
- Windows / macOS ports
- Sidebar UI
- Heavy E2E test suite
