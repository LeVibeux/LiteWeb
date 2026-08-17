# LiteWeb browser security audit

**Date:** 2026-08-17  
**Branch:** `security/browser-audit`  
**Scope:** Linux WebKitGTK 4.1 shell (`src/browser`, `src/commands`, `src/storage`, `src/ui`, `src/adblock`)  
**Method:** source review of every navigation, policy, storage, and reader path, plus unit tests for the allowlist, sanitizer, and profile-directory checks.

This is not a WebKit engine audit. The engine is the system WebKitGTK; LiteWeb is responsible for the policy around it.

## Summary

The browser already had a solid baseline: HTTP(S)/`about:blank` only, sandbox on, TLS errors fatal, automation off, web permissions denied, file chooser cancelled, SQLite `0600` / profile dirs `0700`.

The review still found several holes in *our* policy layer. The most serious was fail-open navigation (missing URI → allow) and unmanaged `target=_blank` views that would have bypassed every LiteWeb handler. Those, plus download, popup, cookie, fullscreen, and reader-document gaps, are fixed on this branch.

| Severity | Found | Fixed here | Still open |
|----------|------:|-----------:|-----------:|
| High     | 2     | 2          | 0          |
| Medium   | 7     | 7          | 0          |
| Low / residual | 8 | 6        | 2          |

## Already in good shape

- Scheme allowlist rejected `file:`, `data:`, `javascript:`, `about:config`, and URLs with userinfo (`https://user:secret@host`).
- `javascript_can_open_windows_automatically` and clipboard access were off; file-URL access and top-level `data:` navigation were off.
- `permission-request` denied (camera, mic, geolocation, notifications). `show-notification` closed. File chooser cancelled.
- WebContext: `set_sandbox_enabled(true)`, `TLSErrorsPolicy::Fail`, `set_automation_allowed(false)`, multi-process model.
- WebRTC already off in every energy mode (no STUN IP leak from the policy struct).
- History/bookmarks use parameterized SQLite. New DB files are `0600`; the config directory is `0700`; symlink DB paths are refused.
- gtk-rs `MessageDialog::new` already formats with `"%s"`, so page titles in `:history` / `:bookmark list` are **not** a printf bug (verified in gtk-rs 0.16 `message_dialog.rs`).
- Ultra `flatten_html` already dropped script/style/iframe/img and only emitted `http`/`https` `href`s.

## Findings and fixes

### H1 — Policy handler failed open when the URI was missing

`connect_decide_policy` only acted on `NavigationAction` / `NewWindowAction`. If the URI could not be extracted, it returned `false` and let WebKit proceed. Response decisions (and any future type) were also unhandled.

**Fix:** `should_allow_navigation(None, _)` is false. Missing URI, blocked host, or unknown decision type → `decision.ignore()`. Unsupported MIME types are ignored so WebKit cannot start an implicit download.

### H2 — `target=_blank` could spawn an unmanaged WebView

`javascript_can_open_windows_automatically` only blocks *scripted* `window.open`. A clicked `target=_blank` still emits `NewWindowAction` then `create`. The default WebKit handler creates a new view that copies *settings* but **not** LiteWeb signal handlers (no scheme check, no permission deny, no file-chooser cancel).

**Fix:** `connect_create` returns `None` so WebKit never owns a view. A safe `NewWindowAction` URL is opened as a real LiteWeb tab (same allowlist + blockers). Unsafe URLs are dropped.

### M1 — Downloads were not cancelled

Downloads are not a product feature, but `download-started` was unconnected. Depending on WebKit version, an attachment response can write to disk.

**Fix:** every `download-started` is cancelled. Combined with the Response MIME check above.

### M2 — Address bar updated only on `LoadEvent::Finished`

In-page navigations left the previous URL in the bar until the new document finished (all subresources). Classic address-bar spoofing window.

**Fix:** on `Committed` / `Redirected`, the tab URL and address bar update immediately. An unsafe committed URI aborts and loads `about:blank`.

### M3 — WebKit profile directory accepted symlinks

SQLite rejected a symlink DB. `~/.local/share/liteweb/webkit` did not. A planted symlink could point cookies/localStorage at another tree.

**Fix:** `prepare_private_dir` refuses a symlink (or non-directory) at the profile path, chmod `0700`s the parent `liteweb` directory as well as `webkit`.

### M4 — Reader HTML had no CSP

`flatten_html` is a custom sanitizer loaded via `load_html(..., Some(original_uri))`, so a sanitizer miss would run in the **page origin**.

**Fix:** drop `object`/`embed`/`applet`/`base`/`meta`/`template`/`math`/… entirely. Emit

`default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`.

### M5 — Fullscreen chrome spoofing

A page could request fullscreen and cover the GTK address bar.

**Fix:** `enable_fullscreen` is false; `enter-fullscreen` is consumed.

### M6 — HTTP authentication dialog

Unhandled `authenticate` lets WebKit prompt. Fine for a full browser; surprising for a locked-down one, and a phishing vector (fake 401 on a lookalike host).

**Fix:** authentication requests are cancelled. Sites that need HTTP Basic will fail until a real credential UI exists.

### M7 — Third-party cookies (privacy)

Default WebKit cookie policy is accept-all.

**Fix:** `CookieAcceptPolicy::NoThirdParty`.

### L1 — Dot-only hosts

`http://.` and `http://..` parsed as HTTP with host `"."` / `".."` and were allowed.

**Fix:** host must be non-empty and not only dots; control bytes in the raw string are rejected.

### L2 — Extra WebKit switches left at defaults

Now explicitly off: hyperlink auditing (`<a ping>`), Java, NPAPI plugins, EME, mock capture devices, console-to-stdout. XSS auditor forced on (no-op on current WebKit, harmless).

### L3 — Bookmarks stored whatever was in `tab.url`

**Fix:** `bookmark_current` only writes URLs that pass the allowlist.

### L4 — Address-bar IDN / bidi spoofing

WebKit can report a Unicode host (`аpple.com`) or a URL containing U+202E, so the bar looks like a trusted site.

**Fix:** `display_navigation_url` strips bidi controls and re-serializes via `url::Url` (punycode host). Tab titles, history, and bookmarks go through `sanitize_ui_text`.

### L5 — Silent sandbox failure

`set_sandbox_enabled(true)` is a no-op if `bwrap` is missing.

**Fix:** probe `bwrap` at startup. If absent, log to stderr and replace the hint bar with a warning. Pages still load (WebKit has no hard “refuse if unsandboxed” switch in this binding).

## Residual risks (not fixed)

These are either engine limits, product choices, or would change everyday browsing too much without a design pass.

1. **Filter is navigation-only.** `assets/filters.txt` never sees subresource requests (scripts, pixels, XHR). Trackers still load. Documented in the README; not a chrome bypass, but it is not EasyList. `UserContentFilter` is not bound in webkit2gtk 0.19.
2. **No site isolation like Chromium.** `ProcessModel::MultipleSecondaryProcesses` is better than one process, not a site-isolated renderer.
3. **Plain HTTP is allowed.** There is no HTTPS-Only mode and no HSTS preload list. MITM on `http://` remains possible.
4. **Local / link-local addresses are allowed.** `http://127.0.0.1`, `http://[::1]`, `http://169.254.169.254` are valid navigations. Expected for a general browser; do not treat LiteWeb as an SSRF-safe URL fetcher.
5. **HTTP Basic is now impossible.** Same tradeoff as M6.
6. **Sandbox warning is not a hard stop.** We detect a missing `bwrap` but still allow navigation.

## Test coverage added

- `navigation_allowlist_rejects_dot_only_hosts_and_non_web_schemes`
- `navigation_policy_fails_closed_without_a_uri`
- `reader_document_ships_a_strict_csp`
- `reader_strips_active_and_embedding_markup` (`javascript:`, `data:`, object/embed/base/meta/template)
- `rejects_webkit_profile_symlink`
- `display_url_uses_punycode_for_idn_hosts`
- `display_url_strips_bidi_overrides`
- `ui_text_strips_bidi_overrides_from_page_titles`
- `sandbox_probe_rejects_missing_binaries`
- `sandbox_probe_finds_a_real_system_binary`

`cargo test --offline`: **49 passed, 0 failed**.

GTK/WebKit signal handlers (`create`, `download-started`, `decide-policy`) are not exercised by unit tests; they have no display-free harness in this crate. Confirm them by building `target/debug/liteweb` and trying `javascript:`, `file:///etc/passwd`, a `target=_blank` link, and a `Content-Disposition: attachment` URL.

## Suggested follow-ups

1. Optional HTTPS-Only mode.
2. Wire WebKit content filters so adblock applies to subresources (needs a newer webkit2gtk binding).
3. Refuse to load pages when the sandbox cannot start, instead of only warning.
4. Download UI with an explicit save dialog, still default-deny.
