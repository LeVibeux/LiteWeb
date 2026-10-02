# Mode privé / anti-détection (privacy mode) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** make LiteWeb usable without leaking what a site can collect passively — per-site cookie/storage sandbox with a 5-minute purge, a **fixed** Chrome/Linux fingerprint (never randomized), ultra-hardened anti-tracking headers (all `Sec-CH-*` stripped), a bubblewrap OS sandbox launcher, and an application-level VPN kill-switch that blocks navigation when no VPN is up.

**Architecture:** one new top-level module `src/privacy/` with eight small files, each mostly **pure** and unit-testable, plus one shared `PrivacyPolicy` value object that is threaded through `create_webview`. Wiring order follows the spec's attack plan: foundation (`config` + `settings`) → cookies → headers + tracker → permissions → fingerprint → network kill-switch → OS launcher → UI → docs. Nothing changes the existing default-deny posture: every new layer only **removes** allowances.

**Tech Stack:** Rust, GTK3, WebKitGTK 4.1 (`webkit2gtk` crate 0.19, feature `v2_38`), `soup3` (`MessageHeaders`), rusqlite, `url`, `dirs`, bash + bubblewrap (`bwrap`).

**Spec:** `docs/superpowers/specs/privacy-mode.md` (do **not** edit it).

---

## Global Constraints

- **Crate is bin-only.** Unit tests run with `cargo test -- <filter>` — **never** `cargo test --lib`. Every task ends with a green `cargo test`.
- **The fingerprint is frozen, never randomized.** `PRIVACY_FINGERPRINT` is a `const`. No `rand`, no per-session jitter, no `Date.now()` / `performance.now()` perturbation, no noise on canvas or audio. A stable fingerprint is the point: a changing one is a detection signal.
- **Cookies never cross sites.** One cookie belongs to one origin (`scheme://host[:port]`). A request to another domain must never carry a cookie created by a different site, and no site can read another site's storage. Cross-site = zero.
- **Kill-switch never breaks localhost / LAN.** `is_internal_host` bypasses the VPN gate for `localhost`, `*.localhost`, `*.local`, `127.0.0.0/8`, `::1`, `10/8`, `172.16/12`, `192.168/16`, `169.254/16`. `about:blank` is never gated. The existing `is_safe_navigation_url` already allows `http://localhost:8080` — do not regress that test.
- **Fail closed.** Unparsable URL, missing host, unknown permission API, unreadable VPN interface state → deny.
- **Default deny for permissions.** No WebKit permission prompt is ever allowed. Even an origin listed as `granted` in SQLite still gets `request.deny()` in this iteration (WebKit has no "allow silently" path); the registry only records the user's intent and is reported by `:permission list`.
- **No silent storage.** No persistent HTTP cache (`CacheModel::DocumentViewer`), no persistent IndexedDB / localStorage / sessionStorage / ServiceWorker / DNS cache / favicon cache, no hyperlink auditing, no prefetch, no spell checking, no MediaStream / WebRTC.
- **Do not weaken existing protections.** `TLSErrorsPolicy::Fail`, `set_sandbox_enabled(true)`, `set_automation_allowed(false)`, `connect_authenticate` → `cancel()`, `connect_run_file_chooser` → `cancel()`, the download pipeline (`wire_downloads`, `decide_download_destination`) and the navigation allowlist stay exactly as they are.
- **Energy policy wins over privacy policy** for feature toggles: `apply_webview_policy(energy)` runs **after** `harden_settings`. Ultra (no JS) must never be re-enabled by the privacy layer; privacy only adds restrictions.
- **Never hold a `RefCell` borrow across a GTK nested main loop** (`dialog.run()`, `show_message`): clone what you need, drop borrows, then show.
- **Every SQLite access uses parameterized queries** and reports errors with `eprintln!("LiteWeb: …")` (existing pattern). Permission grants are capped at 200 rows.
- **Exact French UI strings** (do not reword): « VPN requis — connexion bloquée », « Connexion bloquée. Activez NordVPN (ou autre VPN) pour naviguer. », « Navigation bloquée (VPN) », « Navigation refusée », « Permission enregistrée », « Permissions », « Confidentialité », « Traqueurs bloqués : la liste EasyPrivacy est désactivée, les requêtes ne sont plus filtrées. », « Téléchargements » (existing).
- **Do NOT touch `README.md` / `README.fr.md`** before Task 11, and **never** edit the spec. Do **not** run `git commit` / `git push` — the orchestrator does that.
- `CookieAcceptPolicy` has only `Always` / `Never` / `NoThirdParty` in WebKitGTK 4.1 (`OnlyFromMainDocumentDomain`, mentioned by the spec, does **not** exist). Keep `NoThirdParty` and enforce « 1 cookie → 1 site » with the registry sweep in Task 2.

Note on tests that need GTK: `Settings::new()`, `WebContext::new()`, `UserScript::new()` and friends call `assert_initialized_main_thread!()`, which initializes GTK on first use inside `cargo test`. That works headless for plain object construction, but **no** test may create a `WebView`, run a dialog, or start a main loop. Everything that needs a real page is a manual checklist item.

---

## File map

| File | Role |
|------|------|
| `src/privacy/mod.rs` | NEW — declares the eight sub-modules, re-exports the public surface, holds `PrivacyPolicy` (tracker + kill-switch bundle consumed by `create_webview`) |
| `src/privacy/config.rs` | NEW — `PrivacySettings`, `PRIVACY_FINGERPRINT` (frozen Chrome/Linux values), `privacy_settings()`, `PRIVACY_DATA_DIR` + unit tests |
| `src/privacy/settings.rs` | NEW — `harden_settings(&Settings)`, `harden_web_context(&WebContext)`, `create_private_web_context()` + GTK-free assertions |
| `src/privacy/cookies.rs` | NEW — `site_origin`, `sandbox_dir_name`, `sandbox_dirs`, `should_accept_cookie`, `should_delete_cookie`, `SiteSandbox`, `SiteSandboxRegistry` (activity + 5-min purge), `purge_expired_sandboxes` |
| `src/privacy/headers.rs` | NEW — `STRIPPED_CLIENT_HINTS`, `referer_policy`, `apply_referer_policy`, `harden_request_headers`, `harden_outgoing_request` |
| `src/privacy/tracker.rs` | NEW — `TrackerBlocker`: EasyPrivacy loader + host matcher (exact, subdomain, wildcard) |
| `assets/privacy/easyprivacy.txt` | NEW — bundled tracker host list (`||host^` domain rules only) |
| `assets/privacy/fonts.conf` | NEW — reduced fontconfig profile used by the bwrap launcher |
| `src/privacy/permissions.rs` | NEW — `PERMISSION_APIS`, `permission_denied`, `describe_permission` |
| `src/privacy/fingerprint.rs` | NEW — `fingerprint_script()`, `inject_fingerprint(&UserContentManager)` |
| `src/privacy/network.rs` | NEW — `detect_vpn`, `vpn_interfaces_up`, `interface_is_up`, `default_route_interface`, `is_internal_host`, `vpn_kill_switch`, `kill_switch_uri_allowed`, `format_vpn_status`, `VPN_LOST_URI` + exact French strings |
| `src/main.rs` | add `mod privacy;` |
| `src/storage/db.rs` | `settings` + `permissions` tables, `PermissionRecord`, `get_setting` / `set_setting`, `set_permission` / `grant_permission` / `revoke_permission` / `is_permission_granted` / `list_permissions` + unit tests |
| `src/storage/mod.rs` | re-export `PermissionRecord` |
| `src/browser/mod.rs` | re-export the new `create_webview` signature items |
| `src/browser/webview.rs` | `create_webview(…, privacy: &PrivacyPolicy)`: hardened settings, hardened context, kill-switch gate in the three policy arms, `resource-load-started` header rewriting, permission registry hook |
| `src/commands/palette.rs` | new `CommandAction` variants (`VpnStatus`, `VpnRequire`, `PermissionList`, `PermissionGrant`, `PermissionRevoke`, `SettingTrackerBlock`, `PrivacySummary`) + parser + tests |
| `src/commands/mod.rs` | re-export nothing new (variants travel with `CommandAction`) |
| `src/ui/window.rs` | `AppState.privacy` / `AppState.sandboxes`, `start_privacy_timer`, `vpn_lost_kick`, `show_vpn_overlay`, `show_vpn_status`, `show_permissions`, `show_privacy_summary`, `update_chrome` indicator, `KEYBAR_COMMAND` |
| `scripts/liteweb-private.sh` | NEW — bubblewrap launcher (Axe 4): fake `/proc/cpuinfo`, DMI, hostname, machine-id, MAC, forced `TZ` / `LANG`, reduced fontconfig, `--unshare-all` |
| `packaging/liteweb-private.desktop` | NEW — dedicated desktop entry for the sandboxed launch |
| `README.md` / `README.fr.md` | Task 11 only — « Private mode / anti-detection » section with honest limits (JA3/JA4, IP, WebKit ≠ Blink) |

---

### Task 0: API spike — verify the WebKitGTK 4.1 surface before writing code

Some APIs the spec assumes do not exist in the pinned bindings. This task records what is really available so later tasks do not guess.

**Files:**
- Create: `docs/superpowers/notes/2026-09-30-privacy-webkit-api-spike.md`
- No production code.

**Interfaces:**
- Consumes: `~/.cargo/registry/src/*/webkit2gtk-0.19.2`, `pkg-config`.
- Produces: a written API inventory that Tasks 1–7 cite.

- [ ] **Step 1: Record the runtime + binding versions**

```bash
pkg-config --modversion webkit2gtk-4.1 || ls /usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so*
grep -n 'name = "webkit2gtk' -A2 Cargo.lock
grep -n 'webkit2gtk' Cargo.toml
```

Expected on this machine: shared lib `libwebkit2gtk-4.1.so.0.21.10` (WebKitGTK 2.50.x), crate `webkit2gtk 0.19.2`, feature `v2_38`.

- [ ] **Step 2: Probe the binding surface**

```bash
WK=$(ls -d ~/.cargo/registry/src/*/webkit2gtk-0.19.2)
grep -n "fn set_user_agent\b\|fn set_enable_webrtc\|fn set_enable_media_stream\|fn set_enable_smooth_scrolling\|fn set_enable_html5_local_storage\|fn set_enable_webgl\b" $WK/src/auto/settings.rs
grep -n "fn set_custom_accept_language\|fn set_enable_spell_checking\|fn set_enable_webgpu" $WK/src/auto/settings.rs
grep -n "fn time_zone_override\|fn set_preferred_languages\|fn set_spell_checking_enabled\|fn set_web_extensions_directory" $WK/src/auto/web_context.rs
grep -n "fn clear<" $WK/src/website_data_manager.rs
grep -n "const " $WK/src/auto/flags.rs | sed -n '/WebsiteDataType/,/ALL/p'
grep -n "fn remove\|fn replace\|fn append" ~/.cargo/registry/src/*/soup3-0.3.2/src/auto/message_headers.rs
```

- [ ] **Step 3: Write the spike note with these findings**

`docs/superpowers/notes/2026-09-30-privacy-webkit-api-spike.md`:

```markdown
# Spike — surface WebKitGTK 4.1 pour le mode privé

Date: 2026-09-30. Binding: `webkit2gtk 0.19.2` (feature `v2_38`) → `webkit2gtk-4.1`.
Runtime locale: `libwebkit2gtk-4.1.so.0.21.10` (WebKitGTK 2.50.x).

## Disponible

| Besoin | API retenue |
|---|---|
| UA figé | `SettingsExt::set_user_agent(Some(UA))` |
| Fuseau figé | `WebContext::builder().time_zone_override("Europe/Paris")` |
| Langues | `WebContextExt::set_preferred_languages(&["fr-FR","fr","en"])` |
| Cache HTTP off | `WebContextExt::set_cache_model(CacheModel::DocumentViewer)` |
| Pas de cache disque / SW / IDB | `WebsiteDataManager` builder (`disk_cache_directory`, `service_worker_registrations_directory`, `indexeddb_directory`) + `WebsiteDataTypes::{DISK_CACHE, INDEXEDDB_DATABASES, SERVICE_WORKER_REGISTRATIONS, DOM_CACHE, LOCAL_STORAGE, SESSION_STORAGE, COOKIES, HSTS_CACHE, ITP, MEMORY_CACHE, PLUGIN_DATA, WEBSQL_DATABASES, DEVICE_ID_HASH_SALT, ALL}` |
| Purge par tranche de temps | `WebsiteDataManagerExtManual::clear(types, timespan, None, cb)` (v2_16+) |
| Lecture / suppression de cookies | `CookieManagerExt::cookies_future(uri)`, `delete_cookie_future(&mut cookie)` — `soup::Cookie::domain()` / `name()` prennent `&mut self` |
| En-têtes sortants | `URIRequestExt::http_headers()` → `soup::MessageHeaders::{remove, replace, append}` |
| JS anti-fuite | `UserContentManagerExt::add_script(&UserScript::new(src, AllFrames, Start, &[], &[]))` |
| Permissions | `WebView::connect_permission_request` (déjà `deny()` global) |
| Sortie sandbox WebKit | `WebContextExt::add_path_to_sandbox`, `is_sandbox_enabled` |

## Indisponible (écart au spec)

| Le spec demande | Réalité 4.1 | Décision |
|---|---|---|
| `CookieAcceptPolicy::OnlyFromMainDocumentDomain` | enum = `Always` / `Never` / `NoThirdParty` seulement | garder `NoThirdParty` + purge par registre de sites (Task 2) |
| `Settings.set_custom_accept_language` | inexistant | `set_preferred_languages` sur le contexte + `replace("Accept-Language", …)` par requête (Task 5) |
| `Settings.set_enable_spell_checking` | inexistant sur `Settings` | `WebContextExt::set_spell_checking_enabled(false)` |
| `Settings.set_enable_webgpu` | inexistant | rien à faire : WebGPU n'est pas exposé par WebKitGTK 4.1 |
| `set_webrtc_ip_handling_policy` | inexistant | `set_enable_webrtc(false)` + `set_enable_media_stream(false)` (déjà) |
| `WebExtension` (`connect_web_extension`, `manifest.json` + `content.js`) | le binding ne fournit ni `WebExtensionContext` ni crate d'extension ; l'API d'extension reste du C (`libwebkit2gtk-web-extension`) | `UserScript` injecté à `DocumentStart` = le « fallback » du spec, mais en fait **le** mécanisme retenu. Aucun `.so` C à construire. |
| `Settings.set_enable_form_automation` | inexistant | déjà couvert : `set_javascript_can_access_clipboard(false)`, `set_allow_modal_dialogs(false)` |

## Conséquences pour le plan

1. Pas de fichier `manifest.json` ni de répertoire d'extensions WebKit : le JS vit dans `src/privacy/fingerprint.rs` (`include_str!` d'un asset n'est pas nécessaire).
2. `Accept-Language` est posé **par requête** (Task 5), sinon WebKit envoie les langues système.
3. La politique cookies reste `NoThirdParty` ; l'isolation « 1 cookie → 1 site » est obtenue par le registre + le sweep (Task 2), pas par l'enum.
```

- [ ] **Step 4: Sanity build (no code change)**

Run: `cargo build && cargo test`
Expected: both green — the spike touches documentation only.

---

### Task 1: `privacy::config` + `privacy::settings` — frozen profile and hardened engine

**Files:**
- Create: `src/privacy/mod.rs`, `src/privacy/config.rs`, `src/privacy/settings.rs`
- Modify: `src/main.rs`, `src/browser/webview.rs`, `src/ui/window.rs`
- Test: `src/privacy/config.rs`, `src/privacy/settings.rs` (`#[cfg(test)]` modules)

**Interfaces:**
- Consumes: `webkit2gtk::{Settings, SettingsExt, WebContext, WebContextExt, CacheModel, TLSErrorsPolicy, ProcessModel, WebsiteDataManager, WebsiteDataManagerExtManual, WebsiteDataTypes}`, `crate::browser::create_user_content_manager`.
- Produces (used by every later task):
  - `pub const PRIVACY_DATA_DIR: &str = "liteweb-privacy";`
  - `pub const PRIVACY_FINGERPRINT: PrivacySettings = …;` and `pub fn privacy_settings() -> &'static PrivacySettings`
  - `pub struct PrivacySettings { user_agent, platform, accept_language, time_zone, dnt, tracker_block, ephemeral, clear_on_quit, screen_width, screen_height, screen_avail_height, color_depth, device_pixel_ratio, hardware_concurrency, device_memory, max_touch_points, webgl_vendor, webgl_renderer, webgl_max_texture_size }`
  - `pub fn harden_settings(settings: &webkit2gtk::Settings)`
  - `pub fn harden_web_context(context: &webkit2gtk::WebContext)`
  - `pub fn create_private_web_context() -> webkit2gtk::WebContext`
  - `pub fn clear_previous_session_blocking(context: &webkit2gtk::WebContext)`
  - `pub struct PrivacyPolicy { tracker, kill_switch }` with `PrivacyPolicy::new(vpn_required)`, `is_blocked(url)`, `navigation_allowed(uri, is_blocked)`

- [ ] **Step 1: RED — write the failing tests**

Create `src/privacy/config.rs` with **only** the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_the_frozen_chrome_linux_profile() {
        let s = privacy_settings();
        assert_eq!(
            s.user_agent,
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/127.0.0.0 Safari/537.36"
        );
        assert_eq!(s.platform, "Linux x86_64");
        assert_eq!(s.accept_language, "fr-FR,fr;q=0.9,en;q=0.8");
        assert_eq!(s.time_zone, "Europe/Paris");
        assert_eq!(s.dnt, "1");
        assert!(s.tracker_block && s.ephemeral && s.clear_on_quit);
        assert_eq!((s.screen_width, s.screen_height), (1920, 1080));
        assert_eq!(s.screen_avail_height, 1050);
        assert_eq!(s.color_depth, 24);
        assert_eq!(s.device_pixel_ratio, 1.0);
        assert_eq!(s.hardware_concurrency, 8);
        assert_eq!(s.device_memory, 8);
        assert_eq!(s.max_touch_points, 0);
        assert_eq!(s.webgl_vendor, "Google Inc. (NVIDIA)");
        assert_eq!(
            s.webgl_renderer,
            "ANGLE (NVIDIA, NVIDIA GeForce GTX 1060 6GB, OpenGL 4.5)"
        );
        assert_eq!(s.webgl_max_texture_size, 16384);
    }

    #[test]
    fn fingerprint_is_a_const_and_never_randomized() {
        // Deux lectures successives renvoient exactement les mêmes valeurs :
        // aucune randomisation, aucun bruit, aucune graine de session.
        assert_eq!(privacy_settings(), &PRIVACY_FINGERPRINT);
        assert_eq!(privacy_settings().user_agent, PRIVACY_FINGERPRINT.user_agent);
    }

    #[test]
    fn accept_language_is_aligned_with_navigator_languages() {
        assert_eq!(PRIVACY_FINGERPRINT.languages(), ["fr-FR", "fr", "en"]);
        for tag in PRIVACY_FINGERPRINT.languages() {
            let token = tag.split(';').next().unwrap();
            assert!(
                PRIVACY_FINGERPRINT.accept_language.contains(token),
                "Accept-Language {:?} doit contenir {token}",
                PRIVACY_FINGERPRINT.accept_language
            );
        }
    }
}
```

Create `src/privacy/settings.rs` with **only** its test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use webkit2gtk::{CacheModel, SettingsExt, TLSErrorsPolicy, WebContextExt};

    #[test]
    fn hardened_settings_carry_the_frozen_fingerprint() {
        let settings = webkit2gtk::Settings::new();
        harden_settings(&settings);
        assert_eq!(settings.user_agent().unwrap(), PRIVACY_FINGERPRINT.user_agent);
        assert_eq!(settings.time_zone_override().unwrap(), PRIVACY_FINGERPRINT.time_zone);
    }

    #[test]
    fn hardened_settings_disable_every_leaky_feature() {
        let settings = webkit2gtk::Settings::new();
        harden_settings(&settings);
        assert!(!settings.enables_webrtc());
        assert!(!settings.enables_media_stream());
        assert!(!settings.enables_smooth_scrolling());
        assert!(!settings.enables_html5_local_storage());
        assert!(!settings.enables_html5_database());
        assert!(!settings.enables_offline_web_application_cache());
        assert!(!settings.enables_page_cache());
        assert!(!settings.enables_dns_prefetching());
        assert!(!settings.enables_hyperlink_auditing());
        assert!(!settings.enables_plugins());
        assert!(!settings.enables_java());
        assert!(!settings.enables_fullscreen());
        assert!(!settings.enables_encrypted_media());
        assert!(!settings.enables_mock_capture_devices());
        assert!(!settings.enables_developer_extras());
        assert!(!settings.enables_site_specific_quirks());
        assert!(!settings.allows_file_access_from_file_urls());
        assert!(!settings.allows_universal_access_from_file_urls());
        assert!(!settings.allows_top_navigation_to_data_urls());
        assert!(!settings.allows_modal_dialogs());
        assert!(!settings.javascript_can_access_clipboard());
        assert!(!settings.javascript_can_open_windows_automatically());
        assert!(settings.media_playback_requires_user_gesture());
    }

    #[test]
    fn hardened_context_is_private_and_fail_closed_on_tls() {
        let context = webkit2gtk::WebContext::new();
        harden_web_context(&context);
        assert_eq!(context.cache_model(), CacheModel::DocumentViewer);
        assert_eq!(context.tls_errors_policy(), TLSErrorsPolicy::Fail);
        assert!(context.is_sandbox_enabled());
        assert!(!context.is_automation_allowed());
        assert!(!context.is_spell_checking_enabled());
        assert_eq!(
            context.favicon_database_directory(),
            None,
            "pas de cache favicon sur disque"
        );
    }

    #[test]
    fn private_data_manager_is_on_disk_and_never_ephemeral() {
        let context = create_private_web_context();
        let manager = context.website_data_manager().expect("data manager");
        assert!(
            !manager.is_ephemeral(),
            "le repli éphémère perdrait l'isolation par site"
        );
        let base = manager.base_data_directory().unwrap().to_string();
        assert!(base.contains(PRIVACY_DATA_DIR), "base_data_directory = {base}");
    }
}
```

Create `src/privacy/mod.rs` (still empty of code, so the two test modules cannot resolve their imports yet):

```rust
pub mod config;
pub mod settings;
```

Add to `src/main.rs`:

```rust
mod privacy;
```

- [ ] **Step 2: RED — run and confirm the failure reason**

Run: `cargo test -- privacy::`
Expected: compile errors — `privacy_settings`, `PRIVACY_FINGERPRINT`, `harden_settings`, `harden_web_context`, `create_private_web_context`, `PRIVACY_DATA_DIR` not found.

- [ ] **Step 3: GREEN — minimal implementation**

`src/privacy/config.rs`:

```rust
/// Profil de confidentialité **figé** (Chrome stable sur Linux x86_64).
/// Aucune randomisation : un fingerprint stable est indistinguable d'un vrai
/// utilisateur, un fingerprint qui change est un signal de navigateur truqué.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrivacySettings {
    pub user_agent: &'static str,
    pub platform: &'static str,
    pub languages: &'static [&'static str],
    pub accept_language: &'static str,
    pub time_zone: &'static str,
    pub timezone_offset_minutes: i32,
    pub dnt: &'static str,
    pub tracker_block: bool,
    pub ephemeral: bool,
    pub clear_on_quit: bool,
    pub screen_width: u32,
    pub screen_height: u32,
    pub screen_avail_width: u32,
    pub screen_avail_height: u32,
    pub color_depth: u32,
    pub pixel_depth: u32,
    pub device_pixel_ratio: f64,
    pub hardware_concurrency: u32,
    pub device_memory: u32,
    pub max_touch_points: u32,
    pub webgl_vendor: &'static str,
    pub webgl_renderer: &'static str,
    pub webgl_max_texture_size: u32,
}

impl PrivacySettings {
    /// Aligné sur `navigator.languages` (sinon incohérence détectable).
    pub fn languages(&self) -> &'static [&'static str] {
        self.languages
    }
}

pub const PRIVACY_FINGERPRINT: PrivacySettings = PrivacySettings {
    user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36",
    platform: "Linux x86_64",
    languages: &["fr-FR", "fr", "en"],
    accept_language: "fr-FR,fr;q=0.9,en;q=0.8",
    time_zone: "Europe/Paris",
    timezone_offset_minutes: -120,
    dnt: "1",
    tracker_block: true,
    ephemeral: true,
    clear_on_quit: true,
    screen_width: 1920,
    screen_height: 1080,
    screen_avail_width: 1920,
    screen_avail_height: 1050,
    color_depth: 24,
    pixel_depth: 24,
    device_pixel_ratio: 1.0,
    hardware_concurrency: 8,
    device_memory: 8,
    max_touch_points: 0,
    webgl_vendor: "Google Inc. (NVIDIA)",
    webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GTX 1060 6GB, OpenGL 4.5)",
    webgl_max_texture_size: 16384,
};

/// Sous-répertoire du profil LiteWeb qui porte les sandbox par site.
pub const PRIVACY_DATA_DIR: &str = "liteweb-privacy";

pub fn privacy_settings() -> &'static PrivacySettings {
    &PRIVACY_FINGERPRINT
}
```

`src/privacy/settings.rs`:

```rust
use webkit2gtk::{
    CacheModel, CookieAcceptPolicy, CookieManagerExt, ProcessModel, Settings, SettingsExt,
    TLSErrorsPolicy, WebContext, WebContextExt, WebsiteDataManager, WebsiteDataManagerExtManual,
    WebsiteDataTypes,
};

use super::config::{PRIVACY_DATA_DIR, PRIVACY_FINGERPRINT};

/// Durcissement des `WebKitSettings` : empreinte figée + toutes les fuites
/// désactivées. Appelé **avant** `apply_webview_policy` (l'énergie gagne).
pub fn harden_settings(settings: &Settings) {
    settings.set_user_agent(Some(PRIVACY_FINGERPRINT.user_agent));
    settings.set_time_zone_override(PRIVACY_FINGERPRINT.time_zone);
    settings.set_enable_webrtc(false);
    settings.set_enable_media_stream(false);
    settings.set_enable_mediasource(false);
    settings.set_enable_encrypted_media(false);
    settings.set_enable_smooth_scrolling(false);
    settings.set_enable_html5_local_storage(false);
    settings.set_enable_html5_database(false);
    settings.set_enable_offline_web_application_cache(false);
    settings.set_enable_page_cache(false);
    settings.set_enable_dns_prefetching(false);
    settings.set_enable_hyperlink_auditing(false);
    settings.set_enable_plugins(false);
    settings.set_enable_java(false);
    settings.set_enable_fullscreen(false);
    settings.set_enable_mock_capture_devices(false);
    settings.set_enable_developer_extras(false);
    settings.set_enable_site_specific_quirks(false);
    settings.set_enable_write_console_messages_to_stdout(false);
    settings.set_allow_file_access_from_file_urls(false);
    settings.set_allow_universal_access_from_file_urls(false);
    settings.set_allow_top_navigation_to_data_urls(false);
    settings.set_allow_modal_dialogs(false);
    settings.set_javascript_can_access_clipboard(false);
    settings.set_javascript_can_open_windows_automatically(false);
    settings.set_media_playback_requires_user_gesture(true);
    settings.set_enable_xss_auditor(true);
}

/// Durcissement du contexte : pas de cache HTTP persistant, pas de cache
/// favicon, pas de vérification orthographique, TLS strict.
pub fn harden_web_context(context: &WebContext) {
    context.set_cache_model(CacheModel::DocumentViewer);
    context.set_process_model(ProcessModel::MultipleSecondaryProcesses);
    context.set_sandbox_enabled(true);
    context.set_tls_errors_policy(TLSErrorsPolicy::Fail);
    context.set_automation_allowed(false);
    context.set_spell_checking_enabled(false);
    context.set_favicon_database_directory(None);
    context.set_preferred_languages(PRIVACY_FINGERPRINT.languages());
    if let Some(cookies) = context.cookie_manager() {
        // 4.1 n'a pas `OnlyFromMainDocumentDomain` (voir note de spike) :
        // `NoThirdParty` + purge par registre de sites = « 1 cookie → 1 site ».
        cookies.set_accept_policy(CookieAcceptPolicy::NoThirdParty);
    }
}

/// `WebContext` dédié au mode privé : répertoires segregés sous
/// `~/.local/share/liteweb-privacy`, droits 0700, jamais éphémère en
/// production (un profil éphémère perdrait l'isolation par site).
pub fn create_private_web_context() -> WebContext {
    let manager = match private_data_manager() {
        Some(manager) => manager,
        None => {
            eprintln!(
                "LiteWeb: profil privé indisponible (permissions ou système de fichiers); \
                 repli sur un `WebsiteDataManager` par défaut — les données peuvent rester \
                 dans le profil WebKit système"
            );
            WebsiteDataManager::new()
        }
    };
    let context = WebContext::builder()
        .website_data_manager(&manager)
        .time_zone_override(PRIVACY_FINGERPRINT.time_zone)
        .build();
    harden_web_context(&context);
    clear_previous_session_blocking(&context);
    context
}

/// `clear_on_quit` : à chaque démarrage, tout le stockage de la session
/// précédente est effacé (cookies, IDB, cache, localStorage, ServiceWorker).
pub fn clear_previous_session_blocking(context: &WebContext) {
    let Some(manager) = context.website_data_manager() else {
        return;
    };
    let executor = glib::Executor::new().expect("glib executor");
    executor.block_on(manager.clear_future(WebsiteDataTypes::ALL, glib::TimeSpan::zero()))
        .unwrap_or_else(|error| {
            eprintln!("LiteWeb: purge du stockage de la session précédente impossible: {error}")
        });
}

fn private_data_manager() -> Option<WebsiteDataManager> {
    let data_dir = dirs::data_dir().map(|path| path.join(PRIVACY_DATA_DIR))?;
    let cache_dir = dirs::cache_dir().map(|path| path.join(PRIVACY_DATA_DIR))?;
    prepare_private_dir(&data_dir).ok()?;
    prepare_private_dir(&cache_dir).ok()?;
    Some(
        WebsiteDataManager::builder()
            .base_data_directory(data_dir.to_string_lossy().as_ref())
            .base_cache_directory(cache_dir.to_string_lossy().as_ref())
            .disk_cache_directory(cache_dir.join("http").to_string_lossy().as_ref())
            .indexeddb_directory(data_dir.join("indexeddb").to_string_lossy().as_ref())
            .local_storage_directory(data_dir.join("localstorage").to_string_lossy().as_ref())
            .service_worker_registrations_directory(
                data_dir.join("serviceworkers").to_string_lossy().as_ref(),
            )
            .dom_cache_directory(cache_dir.join("dom").to_string_lossy().as_ref())
            .offline_application_cache_directory(
                cache_dir.join("offline").to_string_lossy().as_ref(),
            )
            .build(),
    )
}

#[cfg(unix)]
fn prepare_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "le profil privé ne doit pas être un lien symbolique",
        )),
        Ok(metadata) if !metadata.file_type().is_dir() => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "le profil privé doit être un répertoire",
        )),
        Ok(_) | Err(_) => {
            std::fs::create_dir_all(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        }
    }
}

#[cfg(not(unix))]
fn prepare_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}
```

`src/privacy/mod.rs`:

```rust
//! Mode privé / anti-détection. Voir `docs/superpowers/specs/privacy-mode.md`.

pub mod config;
pub mod settings;

pub use config::{privacy_settings, PrivacySettings, PRIVACY_DATA_DIR, PRIVACY_FINGERPRINT};
pub use settings::{
    clear_previous_session_blocking, create_private_web_context, harden_settings,
    harden_web_context,
};
```

- [ ] **Step 4: GREEN — run the tests**

Run: `cargo test -- privacy::`
Expected: the 7 new tests PASS. If `clear_future` / `time_zone_override()` / `settings()` accessors differ on the installed bindings, fix the call to match the spike note — do **not** weaken an assertion.

- [ ] **Step 5: Wire the hardened context and settings (single atomic change)**

1. `src/browser/webview.rs` — imports: add

```rust
use crate::privacy::{harden_settings, PrivacyPolicy};
```

2. Replace `create_web_context` with a thin delegation, keeping the doc note:

```rust
/// NOTE téléchargements : aucun handler `download-started` n'est connecté ici.
/// `BrowserWindow::wire_downloads` DOIT être connecté avant le premier
/// chargement (avant `render_tabs`).
/// NOTE confidentialité : le contexte privé (empreinte figée, cache off,
/// purge au démarrage) est construit par `privacy::create_private_web_context`.
#[allow(deprecated)]
pub fn create_web_context() -> WebContext {
    crate::privacy::create_private_web_context()
}
```

Delete the now-unused `private_data_manager` / `prepare_private_dir` / `set_private_dir_permissions` helpers **only** if no test in `webview.rs` references them; otherwise keep them and mark the duplication with a `TODO(privacy): retirer après migration` (the existing `#[cfg(unix)]` tests must stay green).

3. Change the `create_webview` signature to take the policy:

```rust
#[allow(deprecated)]
pub fn create_webview(
    context: &WebContext,
    blocker: Rc<Blocker>,
    privacy: &PrivacyPolicy,
    content: &UserContentManager,
    policy: WebViewPolicy,
    on_new_tab: NewTabHandler,
) -> WebView {
    let webview = WebView::new_with_context_and_user_content_manager(context, content);
    if let Some(settings) = webview.settings() {
        harden_settings(&settings);
    }
    apply_webview_policy(&webview, policy); // l'énergie gagne, voir Global Constraints
    …
```

and replace the three `should_allow_navigation(uri.as_deref(), |candidate| blocker_policy.check(candidate))` calls inside `connect_decide_policy` with:

```rust
privacy_policy.navigation_allowed(uri.as_deref(), |candidate| blocker_policy.check(candidate))
```

(`privacy_policy` is a `PrivacyPolicy` cloned into the closure; `kill_switch` is inert until Task 7, so behaviour is unchanged for now.)

4. `src/privacy/mod.rs` — add the policy bundle:

```rust
use std::rc::Rc;

/// Faisceau consommé par `create_webview` : blocage traqueurs + kill-switch.
#[derive(Clone)]
pub struct PrivacyPolicy {
    pub tracker: Rc<TrackerBlocker>,
    pub kill_switch: KillSwitch,
}

impl PrivacyPolicy {
    pub fn new(vpn_required: bool) -> Self {
        Self {
            tracker: Rc::new(TrackerBlocker::load_bundled()),
            kill_switch: KillSwitch::new(vpn_required),
        }
    }

    /// true si l'URL doit être bloquée (traqueur EasyPrivacy).
    pub fn is_blocked(&self, url: &str) -> bool {
        self.tracker.is_blocked(url)
    }

    /// Allowlist existante + kill-switch VPN. Fail-closed : `None` → refus.
    pub fn navigation_allowed(
        &self,
        uri: Option<&str>,
        is_blocked: impl Fn(&str) -> bool,
    ) -> bool {
        match uri {
            Some(uri) if self.kill_switch.allow_navigation(uri) => {
                crate::commands::should_allow_navigation(Some(uri), is_blocked)
            }
            _ => false,
        }
    }
}
```

`TrackerBlocker` (Task 4) and `KillSwitch` (Task 7) do not exist yet: for this task, declare temporary inert stubs in `src/privacy/mod.rs` **and delete them in the task that introduces the real type**:

```rust
// TEMPORAIRE (Task 1) — remplacé par privacy::tracker puis privacy::network.
pub struct TrackerBlocker;
impl TrackerBlocker {
    pub fn load_bundled() -> Self { Self }
    pub fn is_blocked(&self, _url: &str) -> bool { false }
}

#[derive(Clone)]
pub struct KillSwitch { vpn_required: bool }
impl KillSwitch {
    pub fn new(vpn_required: bool) -> Self { Self { vpn_required } }
    pub fn allow_navigation(&self, _uri: &str) -> bool { !self.vpn_required }
}
```

5. `src/ui/window.rs`:
   - imports: `use crate::privacy::{PrivacyPolicy, SiteSandboxRegistry};` → for this task only `use crate::privacy::PrivacyPolicy;`
   - `AppState`: add `privacy: PrivacyPolicy,`
   - `BrowserWindow::new`: build it before the state and pass it in:

```rust
let web_context = create_web_context();
let vpn_required = storage
    .get_setting("vpn_required")
    .map(|value| value == "1")
    .unwrap_or(false); // défaut off : ne casse pas les benchmarks ni le LAN
let privacy = PrivacyPolicy::new(vpn_required);
```

(`Storage::get_setting` arrives in Task 6; for this task read `false` directly and leave a `// TODO(privacy-task6)` marker.)
   - both `create_webview(…)` call sites (`render_tabs` twice) get `&privacy` cloned out of the state borrow, in the same block that clones `blocker`.

- [ ] **Step 6: REFACTOR — full suite green**

Run: `cargo build && cargo test`
Expected: success; `privacy::`, `browser::webview`, `commands::palette` (including `navigation_allowlist_rejects_local_and_active_schemes` which asserts `http://localhost:8080` is safe) and `storage` all PASS. Remove any leftover duplicate helper from `webview.rs` and re-run.

- [ ] **Step 7: Manual smoke**

Run: `cargo run`, open `https://duckduckgo.com`, then check `ls -la ~/.local/share/liteweb-privacy` exists with `0700`, and `~/.cache/liteweb-privacy` too. Confirm the page still renders (Ultra mode still disables JS).

---

### Task 2: `privacy::cookies` — one site, one sandbox, 5-minute purge

**Files:**
- Create: `src/privacy/cookies.rs`
- Modify: `src/privacy/mod.rs`, `src/ui/window.rs`
- Test: `src/privacy/cookies.rs`

**Interfaces:**
- Consumes: `url::Url`, `chrono::{DateTime, Utc}`, `webkit2gtk::{CookieManagerExt, WebsiteDataManagerExtManual, WebsiteDataTypes}`, `glib::Executor`, `crate::privacy::PRIVACY_DATA_DIR`, `crate::commands::is_safe_navigation_url` (via `should_allow_navigation` in `PrivacyPolicy`).
- Produces (used by Tasks 4 and 8):
  - `pub const SANDBOX_IDLE_LIMIT: chrono::Duration = chrono::Duration::minutes(5);`
  - `pub const PURGE_TICK: std::time::Duration = std::time::Duration::from_secs(30);`
  - `pub fn site_origin(url: &str) -> Option<String>` — `None` for non-http(s) (`about:blank` is not a site)
  - `pub fn sandbox_dir_name(origin: &str) -> Option<String>`
  - `pub fn sandbox_dirs() -> Vec<PathBuf>` — all site sandbox directories on disk
  - `pub fn should_accept_cookie(first_party_host: &str, cookie_domain: &str, is_tracker: bool) -> bool`
  - `pub fn should_delete_cookie(cookie_domain: &str, allowed_origins: &HashSet<String>) -> bool`
  - `pub struct SiteSandbox { pub origin: String, pub dir: PathBuf, pub last_activity: DateTime<Utc> }`
  - `pub struct SiteSandboxRegistry` with `new(root: PathBuf)`, `touch(url) -> Option<SiteSandbox>`, `active_origins() -> HashSet<String>`, `purgeable(now, idle_limit) -> Vec<SiteSandbox>`, `forget(&str)`, `len()`
  - `pub async fn purge_expired_sandboxes(context: &WebContext, expired: &[SiteSandbox]) -> usize`

- [ ] **Step 1: RED — write the failing tests**

Create `src/privacy/cookies.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use std::collections::HashSet;

    fn unique_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "liteweb-privacy-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn site_origin_is_scheme_host_port_only() {
        assert_eq!(
            site_origin("https://Example.COM:443/a/b?c=d#e").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            site_origin("http://localhost:8080/x").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(site_origin("about:blank"), None);
        assert_eq!(site_origin("file:///etc/passwd"), None);
        assert_eq!(site_origin("not a url"), None);
    }

    #[test]
    fn sandbox_dir_name_is_filesystem_safe() {
        assert_eq!(
            sandbox_dir_name("https://example.com").as_deref(),
            Some("https__example.com".to_string())
        );
        assert_eq!(
            sandbox_dir_name("http://localhost:8080").as_deref(),
            Some("http__localhost_8080".to_string())
        );
        assert_eq!(sandbox_dir_name("../../etc"), None);
    }

    #[test]
    fn registry_touch_creates_one_sandbox_per_site() {
        let root = unique_root("touch");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        let first = registry
            .touch("https://example.com/page")
            .expect("sandbox créée");
        assert_eq!(first.origin, "https://example.com");
        assert!(first.dir.starts_with(&root));
        registry.touch("https://example.com/other").unwrap();
        registry.touch("https://other.example/page").unwrap();
        registry.touch("about:blank");
        assert_eq!(registry.len(), 2);
        let origins = registry.active_origins();
        assert!(origins.contains("https://example.com"));
        assert!(origins.contains("https://other.example"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn purgeable_is_empty_before_the_five_minute_idle_limit() {
        let root = unique_root("idle");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        registry.touch("https://example.com").unwrap();
        let now = Utc::now();
        assert!(registry.purgeable(now, Duration::minutes(5)).is_empty());
        assert_eq!(
            registry
                .purgeable(now + Duration::minutes(4) + Duration::seconds(59), Duration::minutes(5))
                .len(),
            0
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn purgeable_lists_sites_idle_for_five_minutes() {
        let root = unique_root("expired");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        registry.touch("https://old.example").unwrap();
        registry.touch("https://fresh.example").unwrap();
        let now = Utc::now() + Duration::minutes(6);
        // « fresh » vient d'être touché : seul « old » est périmé.
        registry.touch_at("https://fresh.example", now);
        let expired = registry.purgeable(now, Duration::minutes(5));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].origin, "https://old.example");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn forget_removes_the_sandbox_from_disk_and_registry() {
        let root = unique_root("forget");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        let sandbox = registry.touch("https://example.com").unwrap();
        std::fs::create_dir_all(sandbox.dir.join("indexeddb")).unwrap();
        registry.forget("https://example.com");
        assert!(!sandbox.dir.exists(), "le répertoire du site doit disparaître");
        assert_eq!(registry.len(), 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn third_party_and_tracker_cookies_are_refused() {
        assert!(should_accept_cookie("example.com", "example.com", false));
        assert!(should_accept_cookie("www.example.com", "example.com", false));
        assert!(should_accept_cookie("EXAMPLE.com", ".example.com", false));
        assert!(!should_accept_cookie("example.com", "ads.example.net", false));
        assert!(!should_accept_cookie("example.com", "example.com", true));
        assert!(!should_accept_cookie("", "example.com", false));
    }

    #[test]
    fn cookies_outside_the_visited_sites_are_deleted() {
        let mut allowed = HashSet::new();
        allowed.insert("https://example.com".to_string());
        assert!(!should_delete_cookie("example.com", &allowed));
        assert!(!should_delete_cookie(".example.com", &allowed));
        assert!(should_delete_cookie("doubleclick.net", &allowed));
        assert!(should_delete_cookie("", &allowed));
    }

    #[test]
    fn sandbox_dirs_lists_only_site_directories() {
        let root = unique_root("dirs");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        registry.touch("https://example.com").unwrap();
        registry.touch("https://other.example").unwrap();
        let dirs = sandbox_dirs_for_root(&root);
        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().all(|dir| dir.starts_with(&root)));
        std::fs::remove_dir_all(root).ok();
    }
}
```

- [ ] **Step 2: RED — run and confirm**

Run: `cargo test -- privacy::cookies`
Expected: compile errors (`site_origin`, `SiteSandboxRegistry`, `should_accept_cookie`, … not found).

- [ ] **Step 3: GREEN — implementation**

`src/privacy/cookies.rs` (above the test module):

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use webkit2gtk::{CookieManagerExt, WebContext, WebContextExt, WebsiteDataManagerExtManual, WebsiteDataTypes};

use crate::privacy::PRIVACY_DATA_DIR;

/// Inactivité maximale avant destruction complète de la sandbox d'un site.
pub const SANDBOX_IDLE_LIMIT: Duration = Duration::minutes(5);
/// Période du timer `privacy::cookies`.
pub const PURGE_TICK: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteSandbox {
    pub origin: String,
    pub dir: PathBuf,
    pub last_activity: DateTime<Utc>,
}

/// Registre des sites visités pendant cette session : « 1 site → 1 sandbox ».
#[derive(Debug, Default)]
pub struct SiteSandboxRegistry {
    root: PathBuf,
    sites: HashMap<String, SiteSandbox>,
}

impl SiteSandboxRegistry {
    pub fn new(root: PathBuf) -> Self {
        Self { root, sites: HashMap::new() }
    }

    /// Racine par défaut : `~/.local/share/liteweb-privacy`.
    pub fn default_root() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(PRIVACY_DATA_DIR)
    }

    /// Enregistre l'activité sur un site. `about:blank` et les URL non
    /// http(s) ne créent pas de sandbox (`None`).
    pub fn touch(&mut self, url: &str) -> Option<SiteSandbox> {
        self.touch_at(url, Utc::now())
    }

    pub fn touch_at(&mut self, url: &str, now: DateTime<Utc>) -> Option<SiteSandbox> {
        let origin = site_origin(url)?;
        let dir = self.root.join(sandbox_dir_name(&origin)?);
        let sandbox = self
            .sites
            .entry(origin.clone())
            .or_insert_with(|| SiteSandbox { origin: origin.clone(), dir: dir.clone(), last_activity: now });
        sandbox.last_activity = now;
        // Best effort : le répertoire porte IndexedDB / cache / SW du site.
        if let Err(error) = std::fs::create_dir_all(&sandbox.dir) {
            eprintln!("LiteWeb: sandbox de site indisponible ({}): {error}", sandbox.dir.display());
        }
        Some(sandbox.clone())
    }

    pub fn active_origins(&self) -> HashSet<String> {
        self.sites.keys().cloned().collect()
    }

    pub fn purgeable(&self, now: DateTime<Utc>, idle_limit: Duration) -> Vec<SiteSandbox> {
        let mut expired: Vec<_> = self
            .sites
            .values()
            .filter(|sandbox| now - sandbox.last_activity >= idle_limit)
            .cloned()
            .collect();
        expired.sort_by(|a, b| a.origin.cmp(&b.origin));
        expired
    }

    /// Détruit la sandbox (répertoire + entrée du registre).
    pub fn forget(&mut self, origin: &str) {
        if let Some(sandbox) = self.sites.remove(origin) {
            remove_dir_all_logged(&sandbox.dir);
        }
    }

    pub fn len(&self) -> usize {
        self.sites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}

/// Origine `scheme://host[:port]`, hôte en minuscules, port par défaut omis.
pub fn site_origin(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host.is_empty() || host.bytes().all(|b| b == b'.') {
        return None;
    }
    match parsed.port() {
        Some(port) => Some(format!("{}://{}:{}", parsed.scheme(), host, port)),
        None => Some(format!("{}://{}", parsed.scheme(), host)),
    }
}

/// Nom de répertoire sûr : aucun séparateur, aucun `..`, hôte en minuscules.
pub fn sandbox_dir_name(origin: &str) -> Option<String> {
    let parsed = url::Url::parse(origin).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host.is_empty() || host.contains('/') {
        return None;
    }
    let name = match parsed.port() {
        Some(port) => format!("{}__{}_{}", parsed.scheme(), host, port),
        None => format!("{}__{}", parsed.scheme(), host),
    };
    let clean: String = name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '.' | '_' => ch,
            _ => '_',
        })
        .collect();
    if clean.contains("..") || clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

pub fn sandbox_dirs() -> Vec<PathBuf> {
    sandbox_dirs_for_root(&SiteSandboxRegistry::default_root())
}

pub fn sandbox_dirs_for_root(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Règle « 1 cookie → 1 site » + refus des traqueurs même first-party.
pub fn should_accept_cookie(first_party_host: &str, cookie_domain: &str, is_tracker: bool) -> bool {
    if is_tracker {
        return false;
    }
    let first_party = first_party_host.trim().trim_start_matches('.').to_ascii_lowercase();
    let domain = cookie_domain.trim().trim_start_matches('.').to_ascii_lowercase();
    if first_party.is_empty() || domain.is_empty() {
        return false; // fail closed
    }
    first_party == domain || first_party.ends_with(&format!(".{domain}"))
}

/// Un cookie dont le domaine n'est pas un site visité (ou traqueur) est purgé.
pub fn should_delete_cookie(cookie_domain: &str, allowed_origins: &HashSet<String>) -> bool {
    let domain = cookie_domain.trim().trim_start_matches('.').to_ascii_lowercase();
    if domain.is_empty() {
        return true;
    }
    !allowed_origins
        .iter()
        .filter_map(|origin| url::Url::parse(origin).ok())
        .any(|parsed| {
            parsed
                .host_str()
                .is_some_and(|host| should_accept_cookie(host, &domain, false))
        })
}

/// Destruction complète des sandbox périmées : répertoire + données WebKit
/// (cookies, IDB, cache, localStorage, ServiceWorker, sessionStorage) plus
/// vieilles que la limite d'inactivité. Renvoie le nombre de sites purgés.
pub async fn purge_expired_sandboxes(context: &WebContext, expired: &[SiteSandbox]) -> usize {
    if expired.is_empty() {
        return 0;
    }
    let idle_timespan = SANDBOX_IDLE_LIMIT
        .num_microseconds()
        .map(glib::TimeSpan::from_microseconds)
        .unwrap_or_else(glib::TimeSpan::zero);
    if let Some(manager) = context.website_data_manager() {
        if let Err(error) = manager
            .clear_future(WebsiteDataTypes::ALL, idle_timespan)
            .await
        {
            eprintln!("LiteWeb: purge du stockage WebKit impossible: {error}");
        }
    }
    if let Some(cookies) = context.cookie_manager() {
        // La liste est vide sans URI de référence ; on purge par domaine connu.
        for sandbox in expired {
            if let Ok(parsed) = url::Url::parse(&sandbox.origin) {
                if let Some(host) = parsed.host_str() {
                    cookies.delete_cookies_for_domain(host);
                }
            }
        }
    }
    for sandbox in expired {
        remove_dir_all_logged(&sandbox.dir);
    }
    expired.len()
}

fn remove_dir_all_logged(path: &Path) {
    if !path.exists() {
        return;
    }
    if let Err(error) = std::fs::remove_dir_all(path) {
        eprintln!("LiteWeb: suppression de la sandbox {} impossible: {error}", path.display());
    }
}
```

> `delete_cookies_for_domain` is deprecated since 2.16 but is the only synchronous per-domain deletion in the 4.1 binding; annotate the call site with `#[allow(deprecated)]` and keep the comment. `Duration::minutes` needs `chrono::Duration` imported — `SANDBOX_IDLE_LIMIT` is a `chrono::Duration` const (`chrono::Duration::minutes` is a `const fn`).

Add to `src/privacy/mod.rs`:

```rust
pub mod cookies;
pub use cookies::{
    purge_expired_sandboxes, sandbox_dir_name, sandbox_dirs, should_accept_cookie,
    should_delete_cookie, site_origin, SiteSandbox, SiteSandboxRegistry, PURGE_TICK,
    SANDBOX_IDLE_LIMIT,
};
```

- [ ] **Step 4: GREEN — run the tests**

Run: `cargo test -- privacy::cookies`
Expected: the 10 new tests PASS (no GTK object is constructed in these tests).

- [ ] **Step 5: Wire activity tracking + the 30-second purge timer**

1. `src/ui/window.rs` — `AppState` gains:

```rust
    sandboxes: Rc<RefCell<SiteSandboxRegistry>>,
```

initialised in `BrowserWindow::new`:

```rust
let sandboxes = Rc::new(RefCell::new(SiteSandboxRegistry::new(
    SiteSandboxRegistry::default_root(),
)));
```

2. In `connect_webview`, inside the `LoadEvent::Committed | LoadEvent::Redirected` branch (right after `is_safe_navigation_url` succeeds), record activity:

```rust
let state_cookies = state_committed.clone();
glib::idle_add_local_once(move || {
    state_cookies.borrow_mut().sandboxes.borrow_mut().touch(&uri);
});
```

3. New timer next to `start_energy_timer`, called from `BrowserWindow::new` **before** `render_tabs`:

```rust
fn start_privacy_timer(state: Rc<RefCell<AppState>>) {
    glib::timeout_add_local(PURGE_TICK, move || {
        let now = Utc::now();
        let (expired, context) = {
            let st = state.borrow();
            (
                st.sandboxes.borrow().purgeable(now, SANDBOX_IDLE_LIMIT),
                st.web_context.clone(),
            )
        };
        if expired.is_empty() {
            return glib::Continue(true);
        }
        let origins: Vec<String> = expired.iter().map(|s| s.origin.clone()).collect();
        let registry = state.borrow().sandboxes.clone();
        let executor = glib::Executor::new().expect("glib executor");
        let purged = executor.block_on(purge_expired_sandboxes(&context, &expired));
        if purged > 0 {
            let mut reg = registry.borrow_mut();
            for origin in origins {
                reg.forget(&origin);
            }
        }
        glib::Continue(true)
    });
}
```

`block_on` on the main thread is acceptable here: the two futures complete on WebKit's own IO thread without pumping the GTK main loop. If a future ever needs the main loop, switch to `glib::spawn_future_local` + a completion flag instead.

4. Imports to add in `src/ui/window.rs`:

```rust
use chrono::Utc;
use crate::privacy::{
    purge_expired_sandboxes, PrivacyPolicy, SiteSandboxRegistry, PURGE_TICK, SANDBOX_IDLE_LIMIT,
};
```

- [ ] **Step 6: REFACTOR — full suite + manual purge check**

Run: `cargo build && cargo test`
Expected: green.

Manual: `cargo run`, visit `https://example.com`, then `ls ~/.local/share/liteweb-privacy` → a `https__example.com` directory. Wait 5 min 30 s without touching the tab → the directory is gone and `find ~/.local/share/liteweb-privacy -type f | wc -l` drops. Then confirm `http://localhost:8080` still loads with the kill-switch off.

---

### Task 3: `privacy::tracker` — EasyPrivacy host matcher

**Files:**
- Create: `src/privacy/tracker.rs`, `assets/privacy/easyprivacy.txt`
- Modify: `src/privacy/mod.rs` (delete the Task 1 `TrackerBlocker` stub)
- Test: `src/privacy/tracker.rs`

**Interfaces:**
- Consumes: `url::Url`, `include_str!("../../assets/privacy/easyprivacy.txt")`.
- Produces:
  - `pub struct TrackerBlocker` with `TrackerBlocker::load_bundled()`, `TrackerBlocker::from_str(content: &str)`, `is_blocked(&self, url: &str) -> bool`, `blocks_host(&self, host: &str) -> bool`, `rule_count(&self) -> usize`, `hit_count(&self) -> u64`
  - `pub fn is_tracker_host(host: &str) -> bool` — convenience over the bundled list (used by Task 2's cookie policy and Task 6's permissions)

- [ ] **Step 1: Create the bundled list**

`assets/privacy/easyprivacy.txt` — EasyPrivacy-derived, **domain rules only** (`||host^`), MIT-licensed subset. Header comment + one host per line:

```
# EasyPrivacy (subset) — règles de domaine uniquement, format ||host^
# Source: https://easylist.to/easylist/easyprivacy.txt (MIT / GPLv3)
# Les règles de chemin ou d'option EasyPrivacy ne sont PAS supportées par le
# matcher LiteWeb : elles doivent être réduites à leur hôte avant inclusion.
||doubleclick.net^
||google-analytics.com^
||googletagmanager.com^
||googlesyndication.com^
||googleadservices.com^
||facebook.net^
||connect.facebook.net^
||fbcdn.net^
||scorecardresearch.com^
||amazon-adsystem.com^
||criteo.com^
||criteo.net^
||taboola.com^
||outbrain.com^
||adnxs.com^
||adsrvr.org^
||rubiconproject.com^
||pubmatic.com^
||openx.net^
||casalemedia.com^
||smartadserver.com^
||teads.tv^
||hotjar.com^
||mouseflow.com^
||fullstory.com^
||mixpanel.com^
||segment.io^
||segment.com^
||amplitude.com^
||heap.io^
||branch.io^
||appsflyer.com^
||adjust.com^
||matomo.cloud^
||piwik.pro^
||plausible.io^
||quantserve.com^
||chartbeat.com^
||newrelic.com^
||nr-data.net^
||sentry.io^
||bugsnag.com^
||rollbar.com^
||cdn.onesignal.com^
||onesignal.com^
||pushwoosh.com^
||zedo.com^
||yieldmo.com^
||indexww.com^
||3lift.com^
||moatads.com^
||adsafeprotected.com^
||doubleverify.com^
||bluekai.com^
||demdex.net^
||krxd.net^
||exelator.com^
||rlcdn.com^
||tapad.com^
||liveintent.com^
||intentiq.com^
||adform.net^
||serving-sys.com^
||mediaplex.com^
||atdmt.com^
||2mdn.net^
||statcounter.com^
||histats.com^
||addthis.com^
||sharethis.com^
||pinterest.com/ct/^
```

(`||pinterest.com/ct/^` is deliberately present: the matcher must ignore the path suffix and keep only the host, per the rule below. Replace the last line with a real tracker host if you prefer a list without path rules.)

- [ ] **Step 2: RED — write the failing tests**

Create `src/privacy/tracker.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
! commentaire EasyPrivacy
[Adblock Plus 2.0]
# commentaire shell

||doubleclick.net^
||analytics.example.com^
@@||allowed.example.com^
||TRACKER.IO^
|https://exact.example/beacon.js|
||pinterest.com/ct/^
";

    #[test]
    fn matches_exact_host_and_subdomains() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.blocks_host("doubleclick.net"));
        assert!(blocker.blocks_host("ad.doubleclick.net"));
        assert!(blocker.blocks_host("a.b.ad.doubleclick.net"));
        assert!(blocker.blocks_host("analytics.example.com"));
    }

    #[test]
    fn does_not_match_lookalike_or_parent_hosts() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(!blocker.blocks_host("notdoubleclick.net"));
        assert!(!blocker.blocks_host("doubleclick.net.evil.com"));
        assert!(!blocker.blocks_host("example.com"), "le parent d'un sous-domaine traqueur n'est pas traqueur");
        assert!(!blocker.blocks_host(""));
    }

    #[test]
    fn host_matching_is_case_insensitive() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.blocks_host("TRACKER.IO"));
        assert!(blocker.blocks_host("tracker.io"));
        assert!(blocker.blocks_host("Tracker.Io"));
    }

    #[test]
    fn comments_and_empty_lines_are_ignored() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        // 4 domaines bloquants + 1 exception + 1 règle exacte = 6 règles retenues.
        assert_eq!(blocker.rule_count(), 6);
    }

    #[test]
    fn path_rules_are_reduced_to_their_host() {
        let blocker = TrackerBlocker::from_str("||pinterest.com/ct/^\n|https://exact.example/beacon.js|");
        assert!(blocker.blocks_host("pinterest.com"));
        assert!(blocker.blocks_host("exact.example"));
    }

    #[test]
    fn exceptions_win_over_blocks() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.blocks_host("analytics.example.com"));
        assert!(!blocker.blocks_host("allowed.example.com"));
        assert!(!blocker.blocks_host("sub.allowed.example.com"));
    }

    #[test]
    fn is_blocked_uses_the_url_host_only() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.is_blocked("https://ad.doubleclick.net/click?id=1"));
        assert!(!blocker.is_blocked("https://example.com/page"));
        assert!(!blocker.is_blocked("about:blank"));
        assert!(!blocker.is_blocked("not a url"));
        assert!(!blocker.is_blocked("https://doubleclick.net@allowed.example.com/"));
    }

    #[test]
    fn hits_are_counted() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert_eq!(blocker.hit_count(), 0);
        blocker.is_blocked("https://ad.doubleclick.net/x");
        blocker.is_blocked("https://ad.doubleclick.net/y");
        assert_eq!(blocker.hit_count(), 2);
    }

    #[test]
    fn bundled_list_blocks_the_reference_trackers() {
        let blocker = TrackerBlocker::load_bundled();
        assert!(blocker.rule_count() >= 50, "liste EasyPrivacy embarquée trop petite");
        for host in [
            "doubleclick.net",
            "www.google-analytics.com",
            "googletagmanager.com",
            "connect.facebook.net",
            "scorecardresearch.com",
            "cdn.taboola.com",
            "static.hotjar.com",
            "api.segment.io",
            "cdn.amplitude.com",
            "sentry.io",
        ] {
            assert!(blocker.blocks_host(host), "{host} doit être bloqué");
        }
        for host in ["example.com", "duckduckgo.com", "localhost", "wikipedia.org"] {
            assert!(!blocker.blocks_host(host), "{host} ne doit pas être bloqué");
        }
    }

    #[test]
    fn bundled_list_loads_fast() {
        let started = std::time::Instant::now();
        let blocker = TrackerBlocker::load_bundled();
        assert!(blocker.rule_count() > 0);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "chargement EasyPrivacy trop lent: {:?}",
            started.elapsed()
        );
    }
}
```

- [ ] **Step 3: RED — run and confirm**

Run: `cargo test -- privacy::tracker`
Expected: compile error — `TrackerBlocker` not found (the Task 1 stub is still in `mod.rs`; delete it now).

- [ ] **Step 4: GREEN — implementation**

`src/privacy/tracker.rs`:

```rust
use std::cell::RefCell;
use std::collections::HashSet;

use url::Url;

/// Chargeur + matcher EasyPrivacy. Seules les règles **de domaine** sont
/// supportées (`||host^`, `@@||host^`, `|https://host/path|`) : les règles de
/// chemin ou d'option sont réduites à leur hôte à l'import.
pub struct TrackerBlocker {
    blocked: HashSet<String>,
    exceptions: HashSet<String>,
    rules: usize,
    hits: RefCell<u64>,
}

impl TrackerBlocker {
    pub fn load_bundled() -> Self {
        Self::from_str(include_str!("../../assets/privacy/easyprivacy.txt"))
    }

    pub fn from_str(content: &str) -> Self {
        let mut blocked = HashSet::new();
        let mut exceptions = HashSet::new();
        let mut rules = 0usize;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('!') || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let (is_exception, rest) = match line.strip_prefix("@@") {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let Some(host) = host_from_rule(rest) else { continue };
            rules += 1;
            if is_exception {
                exceptions.insert(host);
            } else {
                blocked.insert(host);
            }
        }
        Self { blocked, exceptions, rules, hits: RefCell::new(0) }
    }

    pub fn is_blocked(&self, url: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else { return false };
        if !matches!(parsed.scheme(), "http" | "https") {
            return false;
        }
        let Some(host) = parsed.host_str() else { return false };
        let blocked = self.blocks_host(host);
        if blocked {
            *self.hits.borrow_mut() += 1;
        }
        blocked
    }

    pub fn blocks_host(&self, host: &str) -> bool {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return false;
        }
        if matches_domain(&self.exceptions, &host) {
            return false;
        }
        matches_domain(&self.blocked, &host)
    }

    pub fn rule_count(&self) -> usize {
        self.rules
    }

    pub fn hit_count(&self) -> u64 {
        *self.hits.borrow()
    }
}

impl Default for TrackerBlocker {
    fn default() -> Self {
        Self::load_bundled()
    }
}

/// Hôte seul, utile pour la politique cookies et le registre de permissions.
pub fn is_tracker_host(host: &str) -> bool {
    static BUNDLED: std::sync::OnceLock<TrackerBlocker> = std::sync::OnceLock::new();
    BUNDLED
        .get_or_init(TrackerBlocker::load_bundled)
        .blocks_host(host)
}

fn matches_domain(domains: &HashSet<String>, host: &str) -> bool {
    domains.iter().any(|domain| {
        host == domain
            || host.ends_with(domain.as_str())
                && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
    })
}

/// Extrait l'hôte d'une règle EasyPrivacy : retire `||`, `^`, les ancres `|`,
/// le schéma et tout chemin.
fn host_from_rule(rule: &str) -> Option<String> {
    let rule = rule.trim().trim_start_matches("||").trim_start_matches('|').trim_end_matches('|');
    let without_options = rule.split('$').next().unwrap_or(rule);
    let without_anchor = without_options.trim_end_matches('^');
    let authority = match without_anchor.find("://") {
        Some(index) => &without_anchor[index + 3..],
        None => without_anchor,
    };
    let host = authority.split(['/', '?', '#']).next().unwrap_or("").trim();
    let host = host.rsplit('@').next().unwrap_or("").trim();
    // Pas de normalisation `www.` : `||analytics.example.com^` doit rester un
    // sous-domaine, sinon l'exception `@@||allowed.example.com^` et le test
    // `does_not_match_lookalike_or_parent_hosts` cassent.
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.contains('/') || host.contains(' ') {
        return None;
    }
    Some(host)
}
```

> Le matching est suffixe-borné (`host == domain` ou `host` se termine par `.domain`), donc `||google-analytics.com^` bloque `www.google-analytics.com` sans aucune normalisation `www.`. Ne **pas** retirer `www.` côté liste : cela transformerait une règle de sous-domaine en règle de domaine parent et ferait mentir les tests d'exception.

Replace the Task 1 stub in `src/privacy/mod.rs`:

```rust
pub mod tracker;
pub use tracker::{is_tracker_host, TrackerBlocker};
```

and make `PrivacyPolicy::new` use the real type (the `Rc<TrackerBlocker>` field is unchanged).

- [ ] **Step 5: GREEN — run the tests**

Run: `cargo test -- privacy::tracker`
Expected: the 11 new tests PASS, including `bundled_list_blocks_the_reference_trackers`.

- [ ] **Step 6: Wire the tracker check into the navigation gate**

`src/browser/webview.rs` — in the three policy arms, the closure passed to `navigation_allowed` must consult both lists:

```rust
privacy_policy.navigation_allowed(uri.as_deref(), |candidate| {
    blocker_policy.check(candidate) || privacy_policy.is_blocked(candidate)
})
```

- [ ] **Step 7: REFACTOR — full suite**

Run: `cargo build && cargo test`
Expected: green. Manual: `cargo run`, open a page with analytics, confirm the « Bloqués: N » toolbar label still moves (adblock) and that `https://www.google-analytics.com` refuses to load (« Navigation refusée » / blank).

---

### Task 4: `privacy::headers` — strip every `Sec-CH-*`, shrink `Referer`, force `DNT`

**Files:**
- Create: `src/privacy/headers.rs`
- Modify: `src/privacy/mod.rs`, `src/browser/webview.rs`
- Test: `src/privacy/headers.rs`

**Interfaces:**
- Consumes: `webkit2gtk::{URIRequest, URIRequestExt}`, `soup::MessageHeaders`, `url::Url`, `crate::privacy::PRIVACY_FINGERPRINT`.
- Produces:
  - `pub const STRIPPED_CLIENT_HINTS: [&str; 16]`
  - `pub fn referer_policy(page_uri: Option<&str>, request_uri: &str) -> Option<String>` — `None` = remove the header
  - `pub fn apply_referer_policy(headers: &MessageHeaders, referer: Option<&str>)`
  - `pub fn harden_request_headers(headers: &MessageHeaders, page_uri: Option<&str>, request_uri: &str)`
  - `pub fn harden_outgoing_request(request: &URIRequest, page_uri: Option<&str>)`

- [ ] **Step 1: RED — write the failing tests**

Create `src/privacy/headers.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use webkit2gtk::URIRequestExt;

    fn headers() -> MessageHeaders {
        MessageHeaders::new(MessageHeadersType::Request)
    }

    #[test]
    fn cross_origin_requests_carry_no_referer_at_all() {
        assert_eq!(
            referer_policy(Some("https://example.com/secret/path"), "https://tracker.test/p.gif"),
            None
        );
    }

    #[test]
    fn same_origin_referer_is_reduced_to_the_origin() {
        assert_eq!(
            referer_policy(Some("https://example.com/a/b?c=d"), "https://example.com/e").as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(
            referer_policy(Some("https://example.com/a"), "https://example.com:443/b").as_deref(),
            Some("https://example.com/")
        );
    }

    #[test]
    fn port_and_scheme_changes_are_cross_origin() {
        assert_eq!(
            referer_policy(Some("https://example.com/a"), "http://example.com/b"),
            None,
            "changement de schéma = cross-origin"
        );
        assert_eq!(
            referer_policy(Some("http://localhost:8080/a"), "http://localhost:3000/b"),
            None
        );
        assert_eq!(
            referer_policy(Some("http://localhost:8080/a"), "http://localhost:8080/b").as_deref(),
            Some("http://localhost:8080/")
        );
    }

    #[test]
    fn unparsable_page_uri_is_fail_closed() {
        assert_eq!(referer_policy(Some("not a url"), "https://example.com/"), None);
        assert_eq!(referer_policy(Some("about:blank"), "https://example.com/"), None);
        assert_eq!(referer_policy(None, "https://example.com/"), None);
    }

    #[test]
    fn all_client_hints_are_stripped() {
        let h = headers();
        for name in STRIPPED_CLIENT_HINTS {
            h.append(name, "leaked");
        }
        h.append("Sec-Fetch-Mode", "navigate");
        h.append("Origin", "https://example.com");
        apply_referer_policy(&h, Some("https://example.com/"));
        for name in STRIPPED_CLIENT_HINTS {
            assert!(
                h.get_one(name).is_none(),
                "{name} doit être supprimé"
            );
        }
        assert_eq!(
            h.get_one("Sec-Fetch-Mode").as_deref(),
            Some("navigate"),
            "Sec-Fetch-* reste géré par WebKit (requis pour CORS)"
        );
        assert_eq!(h.get_one("Origin").as_deref(), Some("https://example.com"));
    }

    #[test]
    fn client_hint_list_covers_the_spec() {
        for name in [
            "Sec-CH-UA",
            "Sec-CH-UA-Mobile",
            "Sec-CH-UA-Platform",
            "Sec-CH-UA-Platform-Version",
            "Sec-CH-UA-Arch",
            "Sec-CH-UA-Model",
            "Sec-CH-UA-Full-Version",
            "Sec-CH-UA-Full-Version-List",
            "Sec-CH-UA-Bitness",
            "Sec-CH-UA-Wow64",
            "Sec-CH-Prefers-Color-Scheme",
            "Sec-CH-Prefers-Reduced-Motion",
            "Sec-CH-Viewport-Width",
            "Sec-CH-Viewport-Height",
            "Sec-CH-DPR",
            "Sec-CH-Device-Memory",
            "Sec-CH-Downlink",
            "Sec-CH-Width",
        ] {
            assert!(
                STRIPPED_CLIENT_HINTS.contains(&name),
                "{name} manque dans STRIPPED_CLIENT_HINTS"
            );
        }
    }

    #[test]
    fn hardening_sets_dnt_and_the_frozen_accept_language() {
        let h = headers();
        h.append("Accept-Language", "en-US,en;q=0.5");
        h.append("Referer", "https://example.com/secret/deep/path");
        h.append("Sec-CH-UA", "\"Chromium\";v=\"127\"");
        harden_request_headers(&h, Some("https://example.com/secret/deep/path"), "https://example.com/next");
        assert_eq!(h.get_one("DNT").as_deref(), Some("1"));
        assert_eq!(
            h.get_one("Accept-Language").as_deref(),
            Some(PRIVACY_FINGERPRINT.accept_language)
        );
        assert_eq!(h.get_one("Referer").as_deref(), Some("https://example.com/"));
        assert!(h.get_one("Sec-CH-UA").is_none());
        assert_eq!(h.get_list("DNT").len(), 1, "DNT ne doit pas être dupliqué");
    }

    #[test]
    fn cross_origin_request_loses_its_referer_header() {
        let h = headers();
        h.append("Referer", "https://example.com/secret/deep/path");
        harden_request_headers(&h, Some("https://example.com/page"), "https://cdn.other.test/x.js");
        assert!(h.get_one("Referer").is_none());
        assert_eq!(h.get_one("DNT").as_deref(), Some("1"));
    }

    #[test]
    fn uri_request_headers_are_hardened_end_to_end() {
        let request = webkit2gtk::URIRequest::new("https://example.com/page");
        if let Some(h) = request.http_headers() {
            h.append("Sec-CH-UA-Platform", "\"Linux\"");
            h.append("Referer", "https://tracker.test/previous");
            harden_outgoing_request(&request, Some("https://example.com/"));
            assert!(h.get_one("Sec-CH-UA-Platform").is_none());
            assert!(h.get_one("Referer").is_none(), "cross-origin → pas de Referer");
            assert_eq!(h.get_one("DNT").as_deref(), Some("1"));
        }
    }
}
```

- [ ] **Step 2: RED — run and confirm**

Run: `cargo test -- privacy::headers`
Expected: compile errors (`STRIPPED_CLIENT_HINTS`, `referer_policy`, `harden_request_headers`, … not found).

- [ ] **Step 3: GREEN — implementation**

`src/privacy/headers.rs` (above the tests):

```rust
use soup::{MessageHeaders, MessageHeadersType};
use url::Url;
use webkit2gtk::{URIRequest, URIRequestExt};

use crate::privacy::PRIVACY_FINGERPRINT;

/// Tous les en-têtes de fingerprinting supprimés (spec, Axe 3).
/// `Sec-Fetch-*` et `Origin` restent gérés par WebKit : requis pour CORS.
pub const STRIPPED_CLIENT_HINTS: [&str; 18] = [
    "Sec-CH-UA",
    "Sec-CH-UA-Mobile",
    "Sec-CH-UA-Platform",
    "Sec-CH-UA-Platform-Version",
    "Sec-CH-UA-Arch",
    "Sec-CH-UA-Model",
    "Sec-CH-UA-Full-Version",
    "Sec-CH-UA-Full-Version-List",
    "Sec-CH-UA-Bitness",
    "Sec-CH-UA-Wow64",
    "Sec-CH-Prefers-Color-Scheme",
    "Sec-CH-Prefers-Reduced-Motion",
    "Sec-CH-Viewport-Width",
    "Sec-CH-Viewport-Height",
    "Sec-CH-Width",
    "Sec-CH-DPR",
    "Sec-CH-Device-Memory",
    "Sec-CH-Downlink",
];

/// Politique `Referer` : cross-origin → **aucun** `Referer` ; same-origin →
/// réduit à l'origine (le chemin complet est du tracking déguisé).
/// Fail-closed : page inconnue ou non parsable → `None`.
pub fn referer_policy(page_uri: Option<&str>, request_uri: &str) -> Option<String> {
    let page = Url::parse(page_uri?).ok()?;
    let request = Url::parse(request_uri).ok()?;
    if !matches!(page.scheme(), "http" | "https") || !matches!(request.scheme(), "http" | "https") {
        return None;
    }
    let page_origin = origin_of(&page)?;
    if page_origin != origin_of(&request)? {
        return None;
    }
    // « réduit à l'origine » : `https://example.com/` (barre finale incluse).
    Some(format!("{page_origin}/"))
}

fn origin_of(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    match url.port_or_known_default() {
        Some(port) => Some(format!("{}://{}:{}", url.scheme(), host, port)),
        None => Some(format!("{}://{}", url.scheme(), host)),
    }
}

/// Applique la politique `Referer` : `Some(origin)` → `replace`, `None` → `remove`.
pub fn apply_referer_policy(headers: &MessageHeaders, referer: Option<&str>) {
    match referer {
        Some(value) => headers.replace("Referer", value),
        None => headers.remove("Referer"),
    }
}

/// DNT + Accept-Language figé + suppression de tous les `Sec-CH-*`.
pub fn harden_request_headers(headers: &MessageHeaders, page_uri: Option<&str>, request_uri: &str) {
    for name in STRIPPED_CLIENT_HINTS {
        headers.remove(name);
    }
    headers.replace("DNT", PRIVACY_FINGERPRINT.dnt);
    headers.replace("Accept-Language", PRIVACY_FINGERPRINT.accept_language);
    let referer = referer_policy(page_uri, request_uri);
    apply_referer_policy(headers, referer.as_deref());
}

/// Point d'entrée utilisé par `resource-load-started`.
pub fn harden_outgoing_request(request: &URIRequest, page_uri: Option<&str>) {
    let Some(headers) = request.http_headers() else {
        return;
    };
    let request_uri = request.uri().unwrap_or_default().to_string();
    harden_request_headers(&headers, page_uri, &request_uri);
}
```

> `origin_of` normalise le port (`port_or_known_default`), donc `https://example.com/a` et `https://example.com:443/b` sont bien same-origin. La valeur renvoyée porte la barre finale (`https://example.com/`) : c'est la convention des tests ci-dessus et du spec (« réduit à l'origine »).

`src/privacy/mod.rs`:

```rust
pub mod headers;
pub use headers::{
    apply_referer_policy, harden_outgoing_request, harden_request_headers, referer_policy,
    STRIPPED_CLIENT_HINTS,
};
```

`src/browser/webview.rs` — inside `create_webview`, after the `connect_decide_policy` block:

```rust
    // En-têtes sortants : DNT, Accept-Language figé, Referer réduit/supprimé,
    // tous les Sec-CH-* strippés. Aucune exception, aucune option.
    webview.connect_resource_load_started(move |_view, resource, request| {
        let page_uri = resource.request().and_then(|r| r.uri());
        crate::privacy::harden_outgoing_request(request, page_uri.as_deref());
        let _ = request.uri(); // garde : `URIRequestExt` bien importé
    });
```

(`resource.request()` gives the *initiating* request; if it returns the same object as `request`, fall back to the current `WebView::uri()` captured per load — see Task 4 Step 6.)

- [ ] **Step 4: GREEN — run the tests**

Run: `cargo test -- privacy::headers`
Expected: the 10 new tests PASS. If `MessageHeaders::get_one` / `get_list` are not exposed by `soup3 0.3.2`, switch those two assertions to a `has_header`-style check built on `append`+`get_list` alternatives listed in the crate, and record the substitution in the spike note.

- [ ] **Step 5: REFACTOR — use the live page URI, not the resource request**

In `connect_webview` (`src/ui/window.rs`) the page URI is already tracked per tab; expose it to the header hook by capturing the `WebView` inside the closure instead:

```rust
webview.connect_resource_load_started(move |view, _resource, request| {
    let page_uri = view.uri();
    crate::privacy::harden_outgoing_request(request, page_uri.as_deref());
});
```

and delete the placeholder line from Step 3. `view.uri()` is the committed main-document URI — exactly the « first-party page » the `Referer` policy needs.

- [ ] **Step 6: Full suite + manual header check**

Run: `cargo build && cargo test`
Expected: green.

Manual (needs a local listener, no headless harness):

```bash
python3 - <<'PY'
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        print(dict(self.headers)); self.send_response(204); self.end_headers()
HTTPServer(("127.0.0.1", 8080), H).serve_forever()
PY
```

Then in LiteWeb (kill-switch off) load `http://localhost:8080/a/b/c` and confirm the printed headers contain `Dnt: 1`, `Accept-Language: fr-FR,fr;q=0.9,en;q=0.8`, **no** `Sec-CH-UA*`, and either no `Referer` (first load) or `Referer: http://localhost:8080/` when navigating from another path on the same origin.

---

### Task 5: `privacy::permissions` — deny-all plus a SQLite exception registry

**Files:**
- Create: `src/privacy/permissions.rs`
- Modify: `src/privacy/mod.rs`, `src/storage/db.rs`, `src/storage/mod.rs`, `src/browser/webview.rs`, `src/ui/window.rs`
- Test: `src/privacy/permissions.rs`, `src/storage/db.rs`

**Interfaces:**
- Consumes: `crate::storage::Storage`, `url::Url`.
- Produces:
  - `pub const PERMISSION_APIS: [&str; 22]`
  - `pub fn permission_denied(origin: Option<&str>, api: &str, is_granted: impl Fn(&str, &str) -> bool) -> bool`
  - `pub fn describe_permission(record: &PermissionRecord) -> String`
  - `Storage::{get_setting, set_setting, set_permission, grant_permission, revoke_permission, is_permission_granted, list_permissions}` and `pub struct PermissionRecord { origin, api, granted, granted_at }`

- [ ] **Step 1: RED — permissions tests**

Create `src/privacy/permissions.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_list_covers_every_sensitive_surface() {
        for api in [
            "geolocation", "notification", "camera", "microphone", "screen",
            "clipboard-read", "clipboard-write", "usb", "bluetooth", "hid",
            "sensors", "midi", "serial", "idle-detection", "window-placement",
            "font-access", "local-font-access", "storage-access",
            "local-network-access", "pointer-lock", "fullscreen", "payment",
        ] {
            assert!(PERMISSION_APIS.contains(&api), "{api} manque");
        }
    }

    #[test]
    fn everything_is_denied_by_default() {
        assert!(permission_denied(Some("https://example.com"), "geolocation", |_, _| false));
        assert!(permission_denied(Some("https://example.com"), "camera", |_, _| false));
        assert!(permission_denied(Some("https://example.com"), "notification", |_, _| false));
    }

    #[test]
    fn granted_origins_are_allowed_for_that_api_only() {
        let granted = |origin: &str, api: &str| origin == "https://example.com" && api == "camera";
        assert!(!permission_denied(Some("https://example.com"), "camera", granted));
        assert!(permission_denied(Some("https://example.com"), "microphone", granted));
        assert!(permission_denied(Some("https://other.example"), "camera", granted));
    }

    #[test]
    fn missing_origin_or_unknown_api_is_fail_closed() {
        assert!(permission_denied(None, "camera", |_, _| true));
        assert!(permission_denied(Some("about:blank"), "camera", |_, _| true));
        assert!(permission_denied(Some("https://example.com"), "telepathy", |_, _| true));
    }

    #[test]
    fn origin_is_normalised_to_scheme_host_port() {
        assert_eq!(
            permission_origin("https://Example.com:443/a/b?c=d").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(permission_origin("not a url"), None);
        assert_eq!(permission_origin("file:///etc/passwd"), None);
    }
}
```

- [ ] **Step 2: RED — storage tests**

Add to the `tests` module of `src/storage/db.rs`:

```rust
#[test]
fn permissions_are_denied_until_granted() {
    let storage = Storage::open_in_memory().unwrap();
    assert!(!storage.is_permission_granted("https://example.com", "camera"));
    storage.grant_permission("https://example.com", "camera");
    assert!(storage.is_permission_granted("https://example.com", "camera"));
    assert!(!storage.is_permission_granted("https://example.com", "microphone"));
    assert!(!storage.is_permission_granted("https://other.example", "camera"));
    storage.revoke_permission("https://example.com", "camera");
    assert!(!storage.is_permission_granted("https://example.com", "camera"));
}

#[test]
fn permission_grants_are_unique_and_capped_at_200() {
    let storage = Storage::open_in_memory().unwrap();
    storage.grant_permission("https://example.com", "camera");
    storage.grant_permission("https://example.com", "camera");
    assert_eq!(storage.list_permissions().len(), 1);
    for i in 0..205 {
        storage.grant_permission(&format!("https://site-{i}.example"), "geolocation");
    }
    assert_eq!(storage.list_permissions().len(), 200);
}

#[test]
fn settings_are_key_value_and_overwritable() {
    let storage = Storage::open_in_memory().unwrap();
    assert_eq!(storage.get_setting("vpn_required"), None);
    storage.set_setting("vpn_required", "1");
    assert_eq!(storage.get_setting("vpn_required").as_deref(), Some("1"));
    storage.set_setting("vpn_required", "0");
    assert_eq!(storage.get_setting("vpn_required").as_deref(), Some("0"));
}
```

- [ ] **Step 3: RED — run and confirm**

Run: `cargo test -- privacy::permissions` and `cargo test -- storage`
Expected: both fail to compile (`PERMISSION_APIS`, `permission_denied`, `permission_origin`, `grant_permission`, `get_setting` not found).

- [ ] **Step 4: GREEN — storage layer**

In `src/storage/db.rs`:

1. After `DownloadEntry`:

```rust
#[derive(Debug, Clone)]
pub struct PermissionRecord {
    pub origin: String,
    pub api: String,
    pub granted: bool,
    pub granted_at: DateTime<Utc>,
}
```

2. In `init_schema`, after the `downloads` table:

```sql
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS permissions (
    origin TEXT NOT NULL,
    api TEXT NOT NULL,
    granted INTEGER NOT NULL DEFAULT 1,
    granted_at TEXT NOT NULL,
    PRIMARY KEY (origin, api)
);
CREATE INDEX IF NOT EXISTS idx_permissions_granted_at ON permissions(granted_at DESC);
```

3. After `recent_downloads`:

```rust
pub fn get_setting(&self, key: &str) -> Option<String> {
    self.conn()
        .query_row("SELECT value FROM app_settings WHERE key = ?1", params![key], |row| row.get(0))
        .ok()
}

pub fn set_setting(&self, key: &str, value: &str) {
    if let Err(error) = self.conn().execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    ) {
        eprintln!("LiteWeb: impossible d'enregistrer le réglage {key}: {error}");
    }
}

pub fn set_permission(&self, origin: &str, api: &str, granted: bool) {
    let now = Utc::now().to_rfc3339();
    let conn = self.conn();
    if let Err(error) = conn.execute(
        "INSERT INTO permissions (origin, api, granted, granted_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(origin, api) DO UPDATE SET granted = excluded.granted, granted_at = excluded.granted_at",
        params![origin, api, granted as i64, now],
    ) {
        eprintln!("LiteWeb: impossible d'enregistrer la permission: {error}");
        return;
    }
    drop(conn);
    if let Err(error) = self.conn().execute(
        "DELETE FROM permissions WHERE rowid NOT IN (SELECT rowid FROM permissions ORDER BY granted_at DESC, rowid DESC LIMIT 200)",
        [],
    ) {
        eprintln!("LiteWeb: impossible de limiter les permissions: {error}");
    }
}

pub fn grant_permission(&self, origin: &str, api: &str) {
    self.set_permission(origin, api, true);
}

pub fn revoke_permission(&self, origin: &str, api: &str) {
    self.set_permission(origin, api, false);
}

pub fn is_permission_granted(&self, origin: &str, api: &str) -> bool {
    self.conn()
        .query_row(
            "SELECT granted FROM permissions WHERE origin = ?1 AND api = ?2",
            params![origin, api],
            |row| row.get::<_, i64>(0),
        )
        .map(|granted| granted != 0)
        .unwrap_or(false)
}

pub fn list_permissions(&self) -> Vec<PermissionRecord> {
    let conn = self.conn();
    let Ok(mut stmt) = conn.prepare(
        "SELECT origin, api, granted, granted_at FROM permissions ORDER BY granted_at DESC, rowid DESC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        let granted_at: String = row.get(3)?;
        Ok(PermissionRecord {
            origin: row.get(0)?,
            api: row.get(1)?,
            granted: row.get::<_, i64>(2)? != 0,
            granted_at: granted_at.parse().unwrap_or_else(|_| Utc::now()),
        })
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok).collect()
}
```

(The `drop(conn)` before the cap `DELETE` matters: the mutex is not reentrant — same pattern as `add_download`.)

4. `src/storage/mod.rs`:

```rust
pub use db::{
    DownloadEntry, PermissionRecord, Storage, DOWNLOAD_STATUS_COMPLETED, DOWNLOAD_STATUS_FAILED,
    DOWNLOAD_STATUS_IN_PROGRESS,
};
```

- [ ] **Step 5: GREEN — permissions module**

`src/privacy/permissions.rs` (above the tests):

```rust
use url::Url;

use crate::storage::PermissionRecord;

/// Toute surface sensible : refusée sans exception explicite de l'utilisateur.
pub const PERMISSION_APIS: [&str; 22] = [
    "geolocation",
    "notification",
    "camera",
    "microphone",
    "screen",
    "clipboard-read",
    "clipboard-write",
    "usb",
    "bluetooth",
    "hid",
    "sensors",
    "midi",
    "serial",
    "idle-detection",
    "window-placement",
    "font-access",
    "local-font-access",
    "storage-access",
    "local-network-access",
    "pointer-lock",
    "fullscreen",
    "payment",
];

/// Origine normalisée `scheme://host[:port]` — clé du registre.
pub fn permission_origin(url: &str) -> Option<String> {
    crate::privacy::site_origin(url)
}

/// Fail-closed : origine absente, origine non http(s), API inconnue → refus.
pub fn permission_denied(
    origin: Option<&str>,
    api: &str,
    is_granted: impl Fn(&str, &str) -> bool,
) -> bool {
    if !PERMISSION_APIS.contains(&api) {
        return true;
    }
    match origin {
        Some(origin) => !is_granted(origin, api),
        None => true,
    }
}

/// Ligne lisible pour `:permission list`.
pub fn describe_permission(record: &PermissionRecord) -> String {
    let state = if record.granted { "accordée" } else { "refusée" };
    format!(
        "• {} — {} [{}] le {}",
        record.origin,
        record.api,
        state,
        record.granted_at.format("%Y-%m-%d %H:%M")
    )
}
```

`src/privacy/mod.rs`:

```rust
pub mod permissions;
pub use permissions::{describe_permission, permission_denied, permission_origin, PERMISSION_APIS};
```

- [ ] **Step 6: GREEN — run the tests**

Run: `cargo test -- privacy::permissions` and `cargo test -- storage`
Expected: all new tests PASS, plus the existing storage regressions.

- [ ] **Step 7: Wire the registry into `connect_permission_request`**

`src/browser/webview.rs` — `create_webview` gains a `storage: Storage` parameter (both `render_tabs` call sites already hold a `storage` clone) and replaces the blanket deny:

```rust
    let storage_permissions = storage.clone();
    webview.connect_permission_request(move |view, request| {
        // Jamais de prompt WebKit : on refuse toujours côté LiteWeb. Le
        // registre ne fait que tracer l'intention de l'utilisateur.
        let origin = view.uri().and_then(|uri| crate::privacy::permission_origin(&uri));
        let api = permission_api_name(request);
        let granted = origin.as_deref().is_some_and(|origin| {
            storage_permissions.is_permission_granted(origin, api)
        });
        if granted {
            eprintln!(
                "LiteWeb: permission « {api} » enregistrée pour {} — rechargez la page via \
                 l'extension du site si besoin",
                origin.unwrap_or_default()
            );
        }
        request.deny();
        true
    });
```

plus a small mapping helper in `webview.rs`:

```rust
fn permission_api_name(request: &webkit2gtk::PermissionRequest) -> &'static str {
    use webkit2gtk::*;
    if request.dynamic_cast_ref::<GeolocationPermissionRequest>().is_some() {
        "geolocation"
    } else if request.dynamic_cast_ref::<NotificationPermissionRequest>().is_some() {
        "notification"
    } else if request.dynamic_cast_ref::<UserMediaPermissionRequest>().is_some() {
        "camera"
    } else if request.dynamic_cast_ref::<MediaKeySystemPermissionRequest>().is_some() {
        "encrypted-media"
    } else if request.dynamic_cast_ref::<PointerLockPermissionRequest>().is_some() {
        "pointer-lock"
    } else if request.dynamic_cast_ref::<WebsiteDataAccessPermissionRequest>().is_some() {
        "storage-access"
    } else if request.dynamic_cast_ref::<InstallMissingMediaPluginsPermissionRequest>().is_some() {
        "media-plugins"
    } else {
        "unknown"
    }
}
```

> `"unknown"` is not in `PERMISSION_APIS` → `permission_denied` returns `true` → still denied. Import `glib::Cast` for `dynamic_cast_ref` (already imported in `webview.rs`).

- [ ] **Step 8: REFACTOR — full suite**

Run: `cargo build && cargo test`
Expected: green. Manual: `cargo run`, open a page that asks for geolocation (`https://www.openstreetmap.org` → « Show my location ») → **no** WebKit prompt appears, the site gets a denial, and nothing is written to `permissions`.

---

### Task 6: `privacy::fingerprint` — frozen anti-leak JS injected at document start

**Files:**
- Create: `src/privacy/fingerprint.rs`
- Modify: `src/privacy/mod.rs`, `src/browser/webview.rs`
- Test: `src/privacy/fingerprint.rs`

**Interfaces:**
- Consumes: `crate::privacy::PRIVACY_FINGERPRINT`, `webkit2gtk::{UserContentManager, UserContentManagerExt, UserScript, UserContentInjectedFrames, UserScriptInjectionTime}`.
- Produces:
  - `pub fn fingerprint_script() -> String` — the JS source, derived from the const (no literal duplication)
  - `pub fn inject_fingerprint(content: &UserContentManager)` — `remove_all_scripts()` then `add_script(...)`
  - `pub const FINGERPRINT_WORLD: &str = "liteweb-privacy";` (used only as a comment marker; the page world must be the default one for the getters to be visible to site JS)

- [ ] **Step 1: RED — write the failing tests**

Create `src/privacy/fingerprint.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::PRIVACY_FINGERPRINT;

    #[test]
    fn script_pins_every_frozen_value_from_the_const() {
        let js = fingerprint_script();
        for value in [
            PRIVACY_FINGERPRINT.user_agent,
            PRIVACY_FINGERPRINT.platform,
            PRIVACY_FINGERPRINT.time_zone,
            PRIVACY_FINGERPRINT.webgl_vendor,
            PRIVACY_FINGERPRINT.webgl_renderer,
            "1920",
            "1080",
            "1050",
            "24",
            "-120",
        ] {
            assert!(js.contains(value), "le script doit figer {value:?}");
        }
        for language in PRIVACY_FINGERPRINT.languages() {
            assert!(js.contains(language), "le script doit figer la langue {language}");
        }
    }

    #[test]
    fn script_locks_properties_and_never_randomizes() {
        let js = fingerprint_script();
        assert!(js.contains("Object.defineProperty"));
        assert!(js.contains("configurable: false"), "les getters doivent être verrouillés");
        assert!(js.contains("navigator.webdriver"));
        for banned in ["Math.random", "crypto.getRandomValues", "Date.now =", "performance.now ="] {
            assert!(!js.contains(banned), "aucune randomisation : {banned}");
        }
    }

    #[test]
    fn script_neutralises_the_usual_probes() {
        let js = fingerprint_script();
        assert!(js.contains("hardwareConcurrency"));
        assert!(js.contains("deviceMemory"));
        assert!(js.contains("maxTouchPoints"));
        assert!(js.contains("colorDepth"));
        assert!(js.contains("devicePixelRatio"));
        assert!(js.contains("getTimezoneOffset"));
        assert!(js.contains("getBattery"));
        assert!(js.contains("UNMASKED_VENDOR_WEBGL"));
        assert!(js.contains("UNMASKED_RENDERER_WEBGL"));
        assert!(js.contains("MAX_TEXTURE_SIZE"));
        assert!(js.contains("plugins"));
        assert!(js.contains("mimeTypes"));
    }

    #[test]
    fn script_is_compact_and_side_effect_free() {
        let js = fingerprint_script();
        assert!(js.len() < 8_000, "le script tourne sur chaque page : {}", js.len());
        assert!(js.contains("try {"), "une sonde cassée ne doit pas casser la page");
        assert!(!js.contains("alert("));
        assert!(!js.contains("document.write"));
    }
}
```

- [ ] **Step 2: RED — run and confirm**

Run: `cargo test -- privacy::fingerprint`
Expected: compile error — `fingerprint_script` not found.

- [ ] **Step 3: GREEN — implementation**

`src/privacy/fingerprint.rs`:

```rust
use webkit2gtk::{
    UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserScript,
    UserScriptInjectionTime,
};

use crate::privacy::PRIVACY_FINGERPRINT;

/// Monde d'injection : le monde **par défaut** de la page est obligatoire,
/// sinon les getters redéfinis ne sont pas visibles du JS du site.
pub const FINGERPRINT_WORLD: &str = "liteweb-privacy";

/// JS anti-fuite généré depuis `PRIVACY_FINGERPRINT` — aucune valeur littérale
/// dupliquée, donc aucune dérive possible entre le Rust et le JS.
pub fn fingerprint_script() -> String {
    let s = PRIVACY_FINGERPRINT;
    let languages = s
        .languages()
        .iter()
        .map(|tag| format!("\"{tag}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"(function () {{
  "use strict";
  function pin(object, property, value) {{
    try {{
      Object.defineProperty(object, property, {{
        get: function () {{ return value; }},
        set: undefined,
        enumerable: true,
        configurable: false
      }});
    }} catch (error) {{ /* sonde non verrouillable : on continue */ }}
  }}
  function pinDeep(object, property, value) {{
    try {{
      if (!object) return;
      pin(object.prototype || object, property, value);
    }} catch (error) {{}}
  }}

  var nav = window.navigator;
  pin(nav, "userAgent", "{user_agent}");
  pin(nav, "appVersion", "{app_version}");
  pin(nav, "platform", "{platform}");
  pin(nav, "vendor", "Google Inc.");
  pin(nav, "product", "Gecko");
  pin(nav, "productSub", "20030107");
  pin(nav, "webdriver", false);
  pin(nav, "hardwareConcurrency", {hardware_concurrency});
  pin(nav, "deviceMemory", {device_memory});
  pin(nav, "maxTouchPoints", {max_touch_points});
  pin(nav, "pdfViewerEnabled", false);
  pin(nav, "doNotTrack", "{dnt}");
  try {{ Object.defineProperty(nav, "languages", {{ get: function () {{ return Object.freeze([{languages}]); }}, configurable: false }}); }} catch (error) {{}}
  try {{ Object.defineProperty(nav, "plugins", {{ get: function () {{ return Object.freeze([]); }}, configurable: false }}); }} catch (error) {{}}
  try {{ Object.defineProperty(nav, "mimeTypes", {{ get: function () {{ return Object.freeze([]); }}, configurable: false }}); }} catch (error) {{}}
  try {{ nav.getBattery = function () {{ return Promise.reject(new DOMException("NotAllowedError")); }}; }} catch (error) {{}}
  try {{ nav.sendBeacon = nav.sendBeacon; }} catch (error) {{}}

  var screen = window.screen;
  pin(screen, "width", {screen_width});
  pin(screen, "height", {screen_height});
  pin(screen, "availWidth", {screen_avail_width});
  pin(screen, "availHeight", {screen_avail_height});
  pin(screen, "colorDepth", {color_depth});
  pin(screen, "pixelDepth", {pixel_depth});
  try {{ Object.defineProperty(window, "devicePixelRatio", {{ get: function () {{ return {device_pixel_ratio}; }}, configurable: false }}); }} catch (error) {{}}
  try {{ Object.defineProperty(window, "outerWidth", {{ get: function () {{ return {screen_width}; }}, configurable: false }}); }} catch (error) {{}}
  try {{ Object.defineProperty(window, "outerHeight", {{ get: function () {{ return {screen_height}; }}, configurable: false }}); }} catch (error) {{}}

  try {{
    pinDeep(Date.prototype, "getTimezoneOffset", function () {{ return {timezone_offset}; }});
    var DateTimeFormat = Intl.DateTimeFormat;
    var resolvedOptions = DateTimeFormat.prototype.resolvedOptions;
    DateTimeFormat.prototype.resolvedOptions = function () {{
      var options = resolvedOptions.call(this);
      options.timeZone = "{time_zone}";
      return options;
    }};
  }} catch (error) {{}}

  try {{
    var getParameter = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function (parameter) {{
      var ext = this.getExtension("WEBGL_debug_renderer_info");
      if (ext) {{
        if (parameter === ext.UNMASKED_VENDOR_WEBGL) return "{webgl_vendor}";
        if (parameter === ext.UNMASKED_RENDERER_WEBGL) return "{webgl_renderer}";
      }}
      if (parameter === this.VENDOR) return "{webgl_vendor}";
      if (parameter === this.RENDERER) return "{webgl_renderer}";
      if (parameter === this.MAX_TEXTURE_SIZE) return {webgl_max_texture_size};
      if (parameter === this.MAX_RENDERBUFFER_SIZE) return {webgl_max_texture_size};
      return getParameter.call(this, parameter);
    }};
  }} catch (error) {{}}
  try {{
    if (window.WebGL2RenderingContext) {{
      var getParameter2 = WebGL2RenderingContext.prototype.getParameter;
      WebGL2RenderingContext.prototype.getParameter = function (parameter) {{
        var ext = this.getExtension("WEBGL_debug_renderer_info");
        if (ext) {{
          if (parameter === ext.UNMASKED_VENDOR_WEBGL) return "{webgl_vendor}";
          if (parameter === ext.UNMASKED_RENDERER_WEBGL) return "{webgl_renderer}";
        }}
        if (parameter === this.MAX_TEXTURE_SIZE) return {webgl_max_texture_size};
        return getParameter2.call(this, parameter);
      }};
    }}
  }} catch (error) {{}}

  // Canvas / AudioContext : aucun bruit, aucune perturbation (le bruit crée des
  // corrélations détectables). On neutralise seulement `toDataURL` aléatoire
  // éventuel en le laissant intact — la constance vient de l'absence de bruit.
  try {{ window.Notification = undefined; }} catch (error) {{}}
}})();"#,
        user_agent = s.user_agent,
        app_version = s.user_agent.trim_start_matches("Mozilla/"),
        platform = s.platform,
        hardware_concurrency = s.hardware_concurrency,
        device_memory = s.device_memory,
        max_touch_points = s.max_touch_points,
        dnt = s.dnt,
        languages = languages,
        screen_width = s.screen_width,
        screen_height = s.screen_height,
        screen_avail_width = s.screen_avail_width,
        screen_avail_height = s.screen_avail_height,
        color_depth = s.color_depth,
        pixel_depth = s.pixel_depth,
        device_pixel_ratio = s.device_pixel_ratio,
        timezone_offset = s.timezone_offset_minutes,
        time_zone = s.time_zone,
        webgl_vendor = s.webgl_vendor,
        webgl_renderer = s.webgl_renderer,
        webgl_max_texture_size = s.webgl_max_texture_size,
    )
}

/// Injection au tout début du document, dans tous les frames.
/// `remove_all_scripts` garantit qu'un seul exemplaire tourne (idempotent).
pub fn inject_fingerprint(content: &UserContentManager) {
    content.remove_all_scripts();
    let script = UserScript::new(
        &fingerprint_script(),
        UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    );
    content.add_script(&script);
}
```

`src/privacy/mod.rs`:

```rust
pub mod fingerprint;
pub use fingerprint::{fingerprint_script, inject_fingerprint, FINGERPRINT_WORLD};
```

- [ ] **Step 4: GREEN — run the tests**

Run: `cargo test -- privacy::fingerprint`
Expected: the 4 new tests PASS. Adjust the assertions **only** if the `format!` escaping produced something genuinely different — never delete an assertion to make it pass.

- [ ] **Step 5: Wire the injection once per window**

`src/ui/window.rs`, in `BrowserWindow::new`, immediately after `let user_content = create_user_content_manager();`:

```rust
        // Anti-fuite JS : injecté une fois, partagé par tous les onglets
        // (un seul `UserContentManager` par fenêtre).
        crate::privacy::inject_fingerprint(&user_content);
```

`inject_fingerprint` must be called **before** `set_archaic_stylesheet` (the latter calls `remove_all_style_sheets`, which does not touch scripts — but ordering keeps the intent obvious).

- [ ] **Step 6: REFACTOR — full suite + manual fingerprint check**

Run: `cargo build && cargo test`
Expected: green.

Manual: `cargo run`, open `https://browserleaks.com/javascript` (or `about:blank` + `:open` a data-free test page you serve locally with `<script>document.title = JSON.stringify({ua: navigator.userAgent, w: screen.width, tz: Intl.DateTimeFormat().resolvedOptions().timeZone, wd: navigator.webdriver, p: navigator.plugins.length})</script>`), and confirm:
`navigator.webdriver === false`, `navigator.plugins.length === 0`, `navigator.languages` = `["fr-FR","fr","en"]`, `screen.width === 1920`, `screen.height === 1080`, `screen.colorDepth === 24`, `devicePixelRatio === 1`, `hardwareConcurrency === 8`, `deviceMemory === 8`, timezone `Europe/Paris`, `new Date().getTimezoneOffset() === -120`, WebGL vendor `Google Inc. (NVIDIA)`, renderer `ANGLE (NVIDIA, NVIDIA GeForce GTX 1060 6GB, OpenGL 4.5)`, `MAX_TEXTURE_SIZE === 16384`, `navigator.getBattery()` rejects.
Run it **twice in a row**: every value must be byte-identical (frozen profile, no randomization).

---

### Task 7: `privacy::network` — VPN kill-switch

**Files:**
- Create: `src/privacy/network.rs`
- Modify: `src/privacy/mod.rs` (delete the Task 1 `KillSwitch` stub), `src/browser/webview.rs`, `src/ui/window.rs`
- Test: `src/privacy/network.rs`

**Interfaces:**
- Consumes: `/sys/class/net/*/operstate`, `/sys/class/net/*/carrier`, `ip route show default`, `url::Url`, `crate::commands::is_safe_navigation_url`.
- Produces:
  - `pub const VPN_INTERFACE_CANDIDATES: [&str; 12]`, `pub const VPN_LOST_URI: &str = "about:blank#vpn-lost";`
  - `pub const MSG_VPN_BLOCKED_TITLE: &str = "VPN requis — connexion bloquée";`
  - `pub const MSG_VPN_BLOCKED_BODY: &str = "Connexion bloquée. Activez NordVPN (ou autre VPN) pour naviguer.";`
  - `pub const VPN_RECHECK_TICK: std::time::Duration = 30s`, `pub const VPN_OVERLAY_POLL: std::time::Duration = 5s`
  - `pub struct VpnStatus { pub connected: bool, pub interface: Option<String>, pub provider: Option<String>, pub default_route_interface: Option<String> }` with `describe(&self) -> String`
  - `pub fn detect_vpn() -> VpnStatus`, `pub fn vpn_interfaces_up(sys_root: &Path) -> Vec<String>`, `pub fn interface_is_up(sys_root: &Path, name: &str) -> bool`, `pub fn default_route_interface() -> Option<String>`
  - `pub fn is_internal_host(host: &str) -> bool`
  - `pub fn vpn_kill_switch(vpn_connected: bool, uri: &str) -> bool` — `true` = allow
  - `pub struct KillSwitch { vpn_required: bool }` with `new(bool)`, `set_required(bool)`, `is_required()`, `allow_navigation(&self, uri: &str) -> bool`, `status(&self) -> VpnStatus`, `describe(&self) -> String`
  - `pub fn format_vpn_status(status: &VpnStatus, required: bool) -> String`
  - `pub fn kill_switch_uri_allowed(uri: &str, vpn_connected: bool, vpn_required: bool) -> bool`

- [ ] **Step 1: RED — write the failing tests**

Create `src/privacy/network.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn unique_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "liteweb-vpn-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ))
    }

    fn fake_interface(root: &Path, name: &str, operstate: &str, carrier: &str) {
        let dir = root.join("class").join("net").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("operstate"), operstate).unwrap();
        fs::write(dir.join("carrier"), carrier).unwrap();
    }

    #[test]
    fn detects_known_vpn_interfaces_that_are_up() {
        let root = unique_root("up");
        fake_interface(&root, "nordlynx", "up\n", "1\n");
        fake_interface(&root, "eth0", "up\n", "1\n");
        fake_interface(&root, "tun0", "down\n", "0\n");
        let up = vpn_interfaces_up(&root);
        assert_eq!(up, vec!["nordlynx".to_string()]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn accepts_mullvad_proton_and_openvpn_signatures() {
        let root = unique_root("providers");
        fake_interface(&root, "wg0", "up", "1");
        assert!(vpn_interfaces_up(&root).contains(&"wg0".to_string()));
        fs::remove_dir_all(&root).ok();

        let root = unique_root("providers2");
        fake_interface(&root, "proton0", "up", "1");
        assert!(vpn_interfaces_up(&root).contains(&"proton0".to_string()));
        fs::remove_dir_all(&root).ok();

        let root = unique_root("providers3");
        fake_interface(&root, "tun1", "up", "1");
        assert!(vpn_interfaces_up(&root).contains(&"tun1".to_string()));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn interface_without_carrier_is_not_up() {
        let root = unique_root("nocarrier");
        fake_interface(&root, "nordlynx", "up", "0");
        assert!(vpn_interfaces_up(&root).is_empty());
        let root = unique_root("unknown");
        fake_interface(&root, "nordlynx", "unknown", "1");
        assert!(vpn_interfaces_up(&root).is_empty(), "fail closed");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_sys_root_is_not_connected() {
        let root = unique_root("absent");
        assert!(vpn_interfaces_up(&root).is_empty());
    }

    #[test]
    fn kill_switch_blocks_the_web_without_a_vpn() {
        assert!(!vpn_kill_switch(false, "https://example.com"));
        assert!(!vpn_kill_switch(false, "http://example.com/x"));
        assert!(vpn_kill_switch(true, "https://example.com"));
        assert!(vpn_kill_switch(false, "about:blank"));
        assert!(vpn_kill_switch(false, "about:blank#vpn-lost"));
    }

    #[test]
    fn kill_switch_never_blocks_localhost_or_lan() {
        for uri in [
            "http://localhost:8080/dev",
            "http://localhost/",
            "https://printer.local/status",
            "http://127.0.0.1:3000",
            "http://192.168.1.20/admin",
            "http://10.0.0.5/",
            "http://172.16.4.9/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]:8080/",
        ] {
            assert!(vpn_kill_switch(false, uri), "{uri} doit rester accessible sans VPN");
        }
    }

    #[test]
    fn kill_switch_blocks_public_ips_and_unparsable_uris() {
        assert!(!vpn_kill_switch(false, "http://93.184.216.34/"));
        assert!(!vpn_kill_switch(false, "not a uri"), "fail closed");
        assert!(!vpn_kill_switch(false, "https://127.0.0.1.evil.test/"));
    }

    #[test]
    fn is_internal_host_covers_loopback_and_private_ranges() {
        assert!(is_internal_host("localhost"));
        assert!(is_internal_host("sub.localhost"));
        assert!(is_internal_host("printer.local"));
        assert!(is_internal_host("127.0.0.1"));
        assert!(is_internal_host("127.5.5.5"));
        assert!(is_internal_host("::1"));
        assert!(is_internal_host("10.1.2.3"));
        assert!(is_internal_host("172.31.255.254"));
        assert!(is_internal_host("192.168.0.1"));
        assert!(!is_internal_host("172.32.0.1"));
        assert!(!is_internal_host("8.8.8.8"));
        assert!(!is_internal_host("example.com"));
        assert!(!is_internal_host(""));
    }

    #[test]
    fn kill_switch_uri_allowed_respects_the_requirement_flag() {
        // VPN non requis : tout passe (le reste de l'allowlist décide).
        assert!(kill_switch_uri_allowed("https://example.com", false, false));
        // VPN requis, absent : le web est bloqué, le LAN passe.
        assert!(!kill_switch_uri_allowed("https://example.com", false, true));
        assert!(kill_switch_uri_allowed("http://localhost:8080", false, true));
        assert!(kill_switch_uri_allowed("about:blank#vpn-lost", false, true));
        // VPN requis et actif : tout passe.
        assert!(kill_switch_uri_allowed("https://example.com", true, true));
    }

    #[test]
    fn status_lines_are_french_and_stable() {
        let status = VpnStatus {
            connected: true,
            interface: Some("nordlynx".into()),
            provider: Some("NordVPN (nordlynx)".into()),
            default_route_interface: Some("nordlynx".into()),
        };
        let text = format_vpn_status(&status, true);
        assert!(text.contains("VPN : actif"));
        assert!(text.contains("nordlynx"));
        assert!(text.contains("Kill-switch : requis"));

        let off = VpnStatus {
            connected: false,
            interface: None,
            provider: None,
            default_route_interface: Some("eth0".into()),
        };
        let text = format_vpn_status(&off, false);
        assert!(text.contains("VPN : inactif"));
        assert!(text.contains("Kill-switch : désactivé"));
        assert!(text.contains(MSG_VPN_BLOCKED_BODY));
    }

    #[test]
    fn provider_is_inferred_from_the_interface_name() {
        assert_eq!(provider_for_interface("nordlynx").as_deref(), Some("NordVPN (nordlynx)"));
        assert_eq!(provider_for_interface("wg0").as_deref(), Some("Mullvad/WireGuard (wg0)"));
        assert_eq!(provider_for_interface("proton0").as_deref(), Some("ProtonVPN (proton0)"));
        assert_eq!(provider_for_interface("tun0").as_deref(), Some("OpenVPN/tun (tun0)"));
        assert_eq!(provider_for_interface("eth0"), None);
    }

    #[test]
    fn kill_switch_is_inert_when_not_required() {
        let gate = KillSwitch::new(false);
        assert!(!gate.is_required());
        assert!(gate.allow_navigation("https://example.com"));
        assert!(gate.allow_navigation("not a uri"));
    }
}
```

- [ ] **Step 2: RED — run and confirm**

Run: `cargo test -- privacy::network`
Expected: compile errors for every item above.

- [ ] **Step 3: GREEN — implementation**

`src/privacy/network.rs` (above the tests):

```rust
use std::path::{Path, PathBuf};
use std::time::Duration;

use url::Url;

/// Chaînes UI exactes (ne pas reformuler).
pub const MSG_VPN_BLOCKED_TITLE: &str = "VPN requis — connexion bloquée";
pub const MSG_VPN_BLOCKED_BODY: &str =
    "Connexion bloquée. Activez NordVPN (ou autre VPN) pour naviguer.";
pub const VPN_LOST_URI: &str = "about:blank#vpn-lost";

/// Période du timer de re-vérification (le VPN peut tomber en cours de route).
pub const VPN_RECHECK_TICK: Duration = Duration::from_secs(30);
/// Période de l'overlay bloquant tant que le VPN est absent.
pub const VPN_OVERLAY_POLL: Duration = Duration::from_secs(5);

/// Signatures d'interfaces acceptées : NordVPN, Mullvad, ProtonVPN, OpenVPN.
pub const VPN_INTERFACE_CANDIDATES: [&str; 12] = [
    "nordlynx", "wg0", "wg1", "tun0", "tun1", "tun2", "proton0", "mullvad0", "utun0", "vpn0",
    "ppp0", "ppp1",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VpnStatus {
    pub connected: bool,
    pub interface: Option<String>,
    pub provider: Option<String>,
    pub default_route_interface: Option<String>,
}

impl VpnStatus {
    pub fn describe(&self) -> String {
        format_vpn_status(self, false)
    }
}

/// Détection réelle (lit `/sys/class/net` et `ip route`).
pub fn detect_vpn() -> VpnStatus {
    let up = vpn_interfaces_up(Path::new("/sys"));
    let interface = up.into_iter().next();
    let provider = interface.as_deref().and_then(provider_for_interface);
    VpnStatus {
        connected: interface.is_some(),
        interface,
        provider,
        default_route_interface: default_route_interface(),
    }
}

/// Interfaces VPN candidates `operstate == up` **et** `carrier == 1`.
pub fn vpn_interfaces_up(sys_root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for name in VPN_INTERFACE_CANDIDATES {
        if interface_is_up(sys_root, name) {
            found.push(name.to_string());
        }
    }
    found
}

pub fn interface_is_up(sys_root: &Path, name: &str) -> bool {
    let dir = sys_root.join("class").join("net").join(name);
    let operstate = read_trimmed(&dir.join("operstate"));
    let carrier = read_trimmed(&dir.join("carrier"));
    operstate.as_deref() == Some("up") && carrier.as_deref() == Some("1")
}

/// Interface de la route par défaut (`ip route show default`), si `ip` existe.
pub fn default_route_interface() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.split_whitespace().zip(line.split_whitespace().skip(1))
            .find(|(key, _)| *key == "dev")
            .map(|(_, value)| value.to_string()))
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|value| value.trim().to_string())
}

pub fn provider_for_interface(interface: &str) -> Option<String> {
    let label = match interface {
        "nordlynx" => "NordVPN (nordlynx)",
        "wg0" | "wg1" => "Mullvad/WireGuard (wg0)",
        "proton0" => "ProtonVPN (proton0)",
        "mullvad0" => "Mullvad (mullvad0)",
        name if name.starts_with("tun") => "OpenVPN/tun (tun0)",
        name if name.starts_with("ppp") => "PPP/VPN (ppp0)",
        _ => return None,
    };
    Some(label.to_string())
}

/// Loopback, `.local`, RFC1918 et link-local : jamais bloqués par le kill-switch.
pub fn is_internal_host(host: &str) -> bool {
    let host = host.trim().trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    if host == "::1" {
        return true;
    }
    let octets: Vec<u8> = host
        .split('.')
        .filter_map(|part| part.parse::<u8>().ok())
        .collect();
    if octets.len() != 4 {
        return false;
    }
    match octets[0] {
        127 => true,
        10 => true,
        172 => (16..=31).contains(&octets[1]),
        192 => octets[1] == 168,
        169 => octets[1] == 254,
        _ => false,
    }
}

/// Décision du kill-switch pour un URI : `true` = navigation autorisée.
/// Ne dépend **pas** du drapeau « requis » (c'est le rôle de `KillSwitch`).
pub fn vpn_kill_switch(vpn_connected: bool, uri: &str) -> bool {
    if vpn_connected {
        return true;
    }
    if uri.starts_with("about:") {
        return true;
    }
    let Ok(parsed) = Url::parse(uri) else {
        return false; // fail closed
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    match parsed.host_str() {
        Some(host) => is_internal_host(host),
        None => false,
    }
}

/// Version complète utilisée par les points de policy (`Some(uri)` requis).
pub fn kill_switch_uri_allowed(uri: &str, vpn_connected: bool, vpn_required: bool) -> bool {
    if !vpn_required {
        return true;
    }
    vpn_kill_switch(vpn_connected, uri)
}

/// Ligne d'état affichée par `:vpn status`.
pub fn format_vpn_status(status: &VpnStatus, required: bool) -> String {
    let state = if status.connected { "actif" } else { "inactif" };
    let interface = status.interface.as_deref().unwrap_or("aucune");
    let provider = status.provider.as_deref().unwrap_or("inconnu");
    let route = status
        .default_route_interface
        .as_deref()
        .unwrap_or("inconnue");
    let requirement = if required { "requis" } else { "désactivé" };
    let mut text = format!(
        "VPN : {state}\nInterface : {interface}\nFournisseur : {provider}\nRoute par défaut : {route}\nKill-switch : {requirement}"
    );
    if !status.connected {
        text.push_str(&format!("\n\n{MSG_VPN_BLOCKED_BODY}"));
    }
    text
}

/// Porte d'entrée partagée : `PrivacyPolicy` (policy decisions) et
/// `navigate_to` (barre d'URL) l'utilisent tous les deux.
#[derive(Clone)]
pub struct KillSwitch {
    vpn_required: bool,
}

impl KillSwitch {
    /// Défaut `false` : ne casse ni les benchmarks ni le LAN. `:vpn require on`
    /// l'active et le rend persistant (`app_settings.vpn_required`).
    pub fn new(vpn_required: bool) -> Self {
        Self { vpn_required }
    }

    pub fn set_required(&mut self, required: bool) {
        self.vpn_required = required;
    }

    pub fn is_required(&self) -> bool {
        self.vpn_required
    }

    /// `true` = navigation autorisée. Inerte quand le kill-switch est off.
    pub fn allow_navigation(&self, uri: &str) -> bool {
        if !self.vpn_required {
            return true;
        }
        vpn_kill_switch(detect_vpn().connected, uri)
    }

    pub fn status(&self) -> VpnStatus {
        detect_vpn()
    }

    pub fn describe(&self) -> String {
        format_vpn_status(&self.status(), self.vpn_required)
    }
}
```

Replace the Task 1 stub in `src/privacy/mod.rs`:

```rust
pub mod network;
pub use network::{
    detect_vpn, format_vpn_status, is_internal_host, kill_switch_uri_allowed, provider_for_interface,
    vpn_interfaces_up, vpn_kill_switch, KillSwitch, VpnStatus, MSG_VPN_BLOCKED_BODY,
    MSG_VPN_BLOCKED_TITLE, VPN_LOST_URI, VPN_OVERLAY_POLL, VPN_RECHECK_TICK,
};
```

- [ ] **Step 4: GREEN — run the tests**

Run: `cargo test -- privacy::network`
Expected: the 12 new tests PASS. `default_route_interface()` is **not** unit-tested (it shells out to `ip`); it is covered by the manual step.

- [ ] **Step 5: Wire the gate and the 30-second re-check**

1. `src/browser/webview.rs` — the three policy arms already call `privacy_policy.navigation_allowed(...)` (Task 1). `PrivacyPolicy::navigation_allowed` now consults the real `KillSwitch`; add the URI-level message hook by returning a richer verdict:

```rust
impl PrivacyPolicy {
    /// `Err(reason)` = navigation refusée par le kill-switch.
    pub fn navigation_verdict(
        &self,
        uri: Option<&str>,
        is_blocked: impl Fn(&str) -> bool,
    ) -> Result<(), &'static str> {
        match uri {
            Some(uri) if !self.kill_switch.allow_navigation(uri) => {
                Err(crate::privacy::MSG_VPN_BLOCKED_TITLE)
            }
            Some(uri) if crate::commands::should_allow_navigation(Some(uri), is_blocked) => Ok(()),
            _ => Err("navigation refusée"),
        }
    }
}
```

In `connect_decide_policy`, for the `NavigationAction` arm only, surface the message (the other arms stay silent):

```rust
PolicyDecisionType::NavigationAction => {
    let uri = /* unchanged */;
    if let Err(reason) = privacy_policy.navigation_verdict(uri.as_deref(), |candidate| {
        blocker_policy.check(candidate) || privacy_policy.is_blocked(candidate)
    }) {
        decision.ignore();
        if reason == crate::privacy::MSG_VPN_BLOCKED_TITLE {
            crate::privacy::notify_kill_switch_blocked();
        }
        return true;
    }
    false
}
```

2. `src/privacy/network.rs` — the notifier (GTK-free, so it stays testable):

```rust
/// Signal « kill-switch déclenché » consommé par l'UI (overlay + message).
/// Canal `std::sync::mpsc` : `webview.rs` notifie, `window.rs` draine dans son
/// timer de 30 s. Aucun GTK dans ce module.
pub fn kill_switch_channel() -> &'static (
    std::sync::mpsc::Sender<String>,
    std::sync::Mutex<std::sync::mpsc::Receiver<String>>,
) {
    static CHANNEL: std::sync::OnceLock<(
        std::sync::mpsc::Sender<String>,
        std::sync::Mutex<std::sync::mpsc::Receiver<String>>,
    )> = std::sync::OnceLock::new();
    CHANNEL.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        (tx, std::sync::Mutex::new(rx))
    })
}

pub fn notify_kill_switch_blocked() {
    let (tx, _) = kill_switch_channel();
    let _ = tx.send(MSG_VPN_BLOCKED_TITLE.to_string());
}

pub fn drain_kill_switch_notifications() -> Vec<String> {
    let (_, rx) = kill_switch_channel();
    let Ok(rx) = rx.lock() else { return Vec::new() };
    let mut drained = Vec::new();
    while let Ok(message) = rx.try_recv() {
        drained.push(message);
    }
    drained
}
```

3. `src/ui/window.rs` — VPN timer + overlay, started in `BrowserWindow::new` next to `start_privacy_timer`:

```rust
fn start_vpn_timer(state: Rc<RefCell<AppState>>) {
    glib::timeout_add_local(VPN_RECHECK_TICK, move || {
        let required = state.borrow().privacy.kill_switch.is_required();
        if !required {
            return glib::Continue(true);
        }
        let connected = crate::privacy::detect_vpn().connected;
        if !connected {
            Self::vpn_lost_kick(state.clone());
        }
        glib::Continue(true)
    });
}

/// VPN perdu : **tous** les onglets actifs passent en `about:blank#vpn-lost`.
fn vpn_lost_kick(state: Rc<RefCell<AppState>>) {
    let views: Vec<WebView> = {
        let st = state.borrow();
        st.tabs
            .tabs()
            .iter()
            .filter_map(|tab| tab.webview.clone())
            .collect()
    }; // emprunts relâchés AVANT les appels GTK (boucle imbriquée possible)
    for view in views {
        if view.uri().as_deref() != Some(VPN_LOST_URI) {
            view.stop_loading();
            view.load_uri(VPN_LOST_URI);
        }
    }
    for message in crate::privacy::drain_kill_switch_notifications() {
        if message == MSG_VPN_BLOCKED_TITLE {
            Self::show_message(MSG_VPN_BLOCKED_TITLE, MSG_VPN_BLOCKED_BODY);
        }
    }
}

/// Overlay plein écran tant que le VPN est absent (re-sondage toutes les 5 s).
fn show_vpn_overlay(state: Rc<RefCell<AppState>>) {
    let dialog = gtk::MessageDialog::new(
        None::<&gtk::Window>,
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Warning,
        gtk::ButtonsType::None,
        MSG_VPN_BLOCKED_BODY,
    );
    dialog.set_title(MSG_VPN_BLOCKED_TITLE);
    let gate = state.borrow().privacy.kill_switch.clone();
    glib::timeout_add_local(VPN_OVERLAY_POLL, move || {
        let still_blocked = gate.is_required() && !crate::privacy::detect_vpn().connected;
        if !still_blocked {
            dialog.close();
            return glib::Continue(false);
        }
        glib::Continue(true)
    });
    dialog.show_all();
}
```

4. `navigate_to` (`src/ui/window.rs`) gains the explicit refusal **before** the existing `is_safe_navigation_url` check:

```rust
        if !state.borrow().privacy.kill_switch.allow_navigation(&url) {
            Self::show_message(MSG_VPN_BLOCKED_TITLE, MSG_VPN_BLOCKED_BODY);
            Self::show_vpn_overlay(state.clone());
            return;
        }
```

5. Imports in `src/ui/window.rs`:

```rust
use crate::privacy::{
    MSG_VPN_BLOCKED_BODY, MSG_VPN_BLOCKED_TITLE, VPN_LOST_URI, VPN_RECHECK_TICK,
};
```

- [ ] **Step 6: REFACTOR — full suite + manual kill-switch check**

Run: `cargo build && cargo test`
Expected: green, including `navigation_allowlist_rejects_local_and_active_schemes` (localhost stays safe).

Manual:
1. `cargo run`, `:vpn require on`, VPN disconnected → the overlay « Connexion bloquée. Activez NordVPN (ou autre VPN) pour naviguer. » appears and `https://example.com` refuses with title « VPN requis — connexion bloquée ».
2. `http://localhost:8080` still loads with the kill-switch **on** (constraint: le kill-switch ne casse pas localhost).
3. `ss -tnp | grep liteweb` shows no established connection to a public IP while blocked.
4. Connect the VPN → within 30 s the overlay closes and navigation works.
5. Disconnect the VPN while a tab is loaded → within 30 s every tab is on `about:blank#vpn-lost`.
6. `:vpn require off` → everything is allowed again, and the setting survives a restart (`app_settings.vpn_required`).

---

### Task 8: `scripts/liteweb-private.sh` — bubblewrap OS sandbox

**Files:**
- Create: `scripts/liteweb-private.sh`, `assets/privacy/fonts.conf`, `packaging/liteweb-private.desktop`
- Test: inline assertions inside the launcher's `--self-test` mode (no Rust test harness)

**Interfaces:**
- Consumes: `bwrap`, `getconf`, `fc-cache` (optional), `env -0`, `liteweb` binary (`LITEWEB_BIN` override for tests).
- Produces: an executable launcher that runs LiteWeb inside a PID/net/user namespace with fake machine identity, forced `TZ`/`LANG`, and a reduced fontconfig profile.

- [ ] **Step 1: RED — the self-test must fail first**

Create `scripts/liteweb-private.sh` with only the self-test scaffolding (no `bwrap` call yet):

```bash
#!/usr/bin/env bash
# LiteWeb privé — sandbox OS (bubblewrap) : masque machine, TZ, résolution, fontes.
# Voir docs/superpowers/specs/privacy-mode.md (Axe 4).
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: liteweb-private.sh [options liteweb] [args...]

Options:
  --dry-run     affiche la commande bwrap sans l'exécuter
  --self-test   vérifie la sandbox avec un binaire factice puis sort
  -h, --help    cette aide

Variables:
  LITEWEB_BIN            binaire à lancer (défaut: cible release puis debug)
  LITEWEB_TZ             fuseau forcé (défaut: Europe/Paris)
  LITEWEB_NO_SANDBOX=1   exec direct, sans bubblewrap
EOF
}
# --- (le corps arrive au Step 3) ---
self_test() { echo "self-test non implémenté" >&2; exit 1; }
case "${1:-}" in
  --self-test) self_test ;;
  -h|--help) usage ;;
  *) echo "corps non implémenté" >&2; exit 1 ;;
esac
```

```bash
chmod +x scripts/liteweb-private.sh
./scripts/liteweb-private.sh --self-test; echo "exit=$?"
```

Expected: `self-test non implémenté`, `exit=1` — RED.

- [ ] **Step 2: GREEN — full launcher**

Replace the placeholder body with:

```bash
SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SELF_DIR/.." && pwd)"
TZ_NAME="${LITEWEB_TZ:-Europe/Paris}"

require_bwrap() {
  if command -v bwrap >/dev/null 2>&1; then return 0; fi
  for candidate in /usr/bin/bwrap /bin/bwrap /usr/local/bin/bwrap; do
    [ -x "$candidate" ] && return 0
  done
  return 1
}

resolve_binary() {
  if [ -n "${LITEWEB_BIN:-}" ]; then printf '%s' "$LITEWEB_BIN"; return 0; fi
  for candidate in "$ROOT_DIR/target/release/liteweb" "$ROOT_DIR/target/debug/liteweb" \
                   "$(command -v liteweb || true)"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then printf '%s' "$candidate"; return 0; fi
  done
  echo "LiteWeb: binaire introuvable (construisez avec 'cargo build --release' ou définissez LITEWEB_BIN)." >&2
  exit 2
}

prepare_fakes() {
  local dir="$1"
  mkdir -p "$dir/proc" "$dir/sys-class-dmi-id" "$dir/etc" "$dir/sys-class-net-eth0" \
           "$dir/fontconfig" "$dir/run/user/$(id -u)" "$dir/tmp"
  local cores; cores="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 8)"
  {
    echo "processor	: 0"
    echo "vendor_id	: GenuineIntel"
    echo "model name	: Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz"
    echo "cpu cores	: $cores"
    echo "flags		: fpu vme de pse tsc msr pae mce cx8 apic sep mtrr"
  } > "$dir/proc/cpuinfo"
  printf 'To Be Filled By O.E.M.\n' > "$dir/sys-class-dmi-id/product_serial"
  printf 'To Be Filled By O.E.M.\n' > "$dir/sys-class-dmi-id/product_name"
  printf 'To Be Filled By O.E.M.\n' > "$dir/sys-class-dmi-id/board_name"
  printf 'To Be Filled By O.E.M.\n' > "$dir/sys-class-dmi-id/sys_vendor"
  printf 'localhost\n'             > "$dir/etc/hostname"
  printf '00000000000000000000000000000000\n' > "$dir/etc/machine-id"
  printf '00:00:00:00:00:00\n'    > "$dir/sys-class-net-eth0/address"
  cp "$ROOT_DIR/assets/privacy/fonts.conf" "$dir/fontconfig/fonts.conf"
}

bwrap_argv() {
  local fakes="$1" binary="$2"; shift 2
  printf '%s\n' \
    bwrap \
    --ro-bind /usr /usr \
    --ro-bind /lib /lib \
    --ro-bind /bin /bin \
    --ro-bind /etc/alternatives /etc/alternatives \
    --ro-bind /etc/ssl /etc/ssl \
    --ro-bind /etc/fonts /etc/fonts \
    --ro-bind /usr/share/fonts /usr/share/fonts \
    --ro-bind /usr/share/icons /usr/share/icons \
    --ro-bind /usr/share/mime /usr/share/mime \
    --ro-bind /usr/share/X11 /usr/share/X11 \
    --ro-bind /etc/locale /etc/locale \
    --ro-bind /usr/lib/locale /usr/lib/locale \
    --proc /proc \
    --dev /dev \
    --ro-bind "$fakes/proc/cpuinfo" /proc/cpuinfo \
    --ro-bind "$fakes/sys-class-dmi-id" /sys/class/dmi/id \
    --ro-bind "$fakes/sys-class-net-eth0/address" /sys/class/net/eth0/address \
    --ro-bind "$fakes/etc/hostname" /etc/hostname \
    --ro-bind "$fakes/etc/machine-id" /etc/machine-id \
    --ro-bind "$fakes/fontconfig" "$HOME/.config/fontconfig" \
    --bind "$HOME" "$HOME" \
    --bind "$fakes/run/user/$(id -u)" "/run/user/$(id -u)" \
    --bind "$fakes/tmp" /tmp \
    --unshare-all --share-net \
    --die-with-parent --new-session \
    --clearenv \
    --setenv PATH /usr/local/bin:/usr/bin:/bin \
    --setenv HOME "$HOME" \
    --setenv XDG_RUNTIME_DIR "/run/user/$(id -u)" \
    --setenv TZ "$TZ_NAME" \
    --setenv LANG fr_FR.UTF-8 \
    --setenv LC_ALL C.UTF-8 \
    --setenv DISPLAY "${DISPLAY:-:0}" \
    --setenv LITEWEB_PRIVATE 1 \
    "$binary" "$@"
}

self_test() {
  local work binary out
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  prepare_fakes "$work/fakes"
  binary="$work/fake-liteweb"
  printf '%s\n' '#!/bin/sh' 'echo "LITEWEB_PRIVATE=$LITEWEB_PRIVATE"' \
    'echo "TZ=$TZ"' 'echo "LANG=$LANG"' 'echo "HOME=$HOME"' \
    'printf "CPU="; grep -c ^processor /proc/cpuinfo' \
    'printf "MODEL="; sed -n "s/^model name.*: //p" /proc/cpuinfo' \
    'printf "DMI="; cat /sys/class/dmi/id/product_serial' \
    'printf "HOST="; cat /etc/hostname' \
    'printf "MID="; cat /etc/machine-id' \
    'printf "MAC="; cat /sys/class/net/eth0/address' \
    'printf "FONTS="; ls "$HOME/.config/fontconfig" 2>/dev/null' \
    'env -0 | tr "\0" "\n" | grep -c . | sed "s/^/ENVVARS=/"' > "$binary"
  chmod +x "$binary"

  if [ "${LITEWEB_NO_SANDBOX:-0}" = "1" ] || ! require_bwrap; then
    out="$(LITEWEB_PRIVATE=1 TZ="$TZ_NAME" LANG=fr_FR.UTF-8 HOME="$HOME" "$binary")"
    echo "$out" | sed 's/^/[no-sandbox] /'
  else
    local argv; mapfile -t argv < <(bwrap_argv "$work/fakes" "$binary")
    out="$("${argv[@]}")"
    echo "$out" | sed 's/^/[bwrap] /'
  fi

  local failures=0
  check() { # check <libellé> <attendu> <obtenu>
    if [ "$2" != "$3" ]; then
      echo "ÉCHEC: $1 — attendu « $2 », obtenu « $3 »" >&2
      failures=$((failures + 1))
    else
      echo "OK: $1 = $3"
    fi
  }
  line() { printf '%s\n' "$1" | sed -n "s/^$2=//p" | head -n1; }
  check "LITEWEB_PRIVATE" 1 "$(line "$out" LITEWEB_PRIVATE)"
  check "TZ" "$TZ_NAME" "$(line "$out" TZ)"
  check "LANG" "fr_FR.UTF-8" "$(line "$out" LANG)"
  check "DMI" "To Be Filled By O.E.M." "$(line "$out" DMI)"
  check "HOST" "localhost" "$(line "$out" HOST)"
  check "MID" "00000000000000000000000000000000" "$(line "$out" MID)"
  check "MAC" "00:00:00:00:00:00" "$(line "$out" MAC)"
  if [ "${LITEWEB_NO_SANDBOX:-0}" != "1" ] && require_bwrap; then
    check "FONTES" "fonts.conf" "$(line "$out" FONTS)"
    local envvars; envvars="$(line "$out" ENVVARS)"
    if [ "${envvars:-99}" -gt 12 ]; then
      echo "ÉCHEC: environnement non nettoyé ($envvars variables)" >&2
      failures=$((failures + 1))
    else
      echo "OK: environnement nettoyé ($envvars variables)"
    fi
    if ! printf '%s\n' "$out" | grep -q '^MODEL=Intel(R) Core(TM) i7-8700'; then
      echo "ÉCHEC: /proc/cpuinfo factice non monté" >&2
      failures=$((failures + 1))
    else
      echo "OK: /proc/cpuinfo factice monté"
    fi
  fi
  if [ "$failures" -ne 0 ]; then
    echo "self-test: $failures échec(s)" >&2
    exit 1
  fi
  echo "self-test: OK"
}

main() {
  local dry_run=0 args=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --dry-run) dry_run=1; shift ;;
      --self-test) self_test; exit $? ;;
      -h|--help) usage; exit 0 ;;
      --) shift; while [ $# -gt 0 ]; do args+=("$1"); shift; done ;;
      *) args+=("$1"); shift ;;
    esac
  done
  local binary; binary="$(resolve_binary)"
  if [ "${LITEWEB_NO_SANDBOX:-0}" = "1" ]; then
    echo "LiteWeb: sandbox bubblewrap désactivée (LITEWEB_NO_SANDBOX=1) — machine visible." >&2
    exec env TZ="$TZ_NAME" LANG=fr_FR.UTF-8 LITEWEB_PRIVATE=1 "$binary" "${args[@]}"
  fi
  if ! require_bwrap; then
    echo "LiteWeb: bubblewrap (bwrap) est introuvable." >&2
    echo "  Debian/Ubuntu: sudo apt install bubblewrap" >&2
    echo "  Fedora:        sudo dnf install bubblewrap" >&2
    echo "  Arch:          sudo pacman -S bubblewrap" >&2
    echo "  Repli sans sandbox: LITEWEB_NO_SANDBOX=1 $0 (machine visible)." >&2
    exit 3
  fi
  local fakes; fakes="$(mktemp -d)"
  trap 'rm -rf "$fakes"' EXIT
  prepare_fakes "$fakes"
  local argv; mapfile -t argv < <(bwrap_argv "$fakes" "$binary" "${args[@]}")
  if [ "$dry_run" = "1" ]; then
    printf '%s\n' "${argv[@]}"
    exit 0
  fi
  exec "${argv[@]}"
}

main "$@"
```

> `bwrap_argv` écrit un argument par ligne sur stdout ; `mapfile -t` le relit en tableau, donc aucun mot n'est découpé par le shell (chemins avec espaces inclus). N'utilisez **jamais** `xargs` ici.

`assets/privacy/fonts.conf` (6 familles standard, aucune fonte locale) :

```xml
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <dir>/usr/share/fonts</dir>
  <cachedir prefix="xdg">fontconfig</cachedir>
  <alias><family>sans-serif</family><prefer><family>DejaVu Sans</family></prefer></alias>
  <alias><family>serif</family><prefer><family>DejaVu Serif</family></prefer></alias>
  <alias><family>monospace</family><prefer><family>DejaVu Sans Mono</family></prefer></alias>
  <alias><family>Noto Sans</family><prefer><family>DejaVu Sans</family></prefer></alias>
  <alias><family>Arial</family><prefer><family>DejaVu Sans</family></prefer></alias>
  <alias><family>Helvetica</family><prefer><family>DejaVu Sans</family></prefer></alias>
  <match target="pattern">
    <test qual="any" name="family"><string>emoji</string></test>
    <edit name="family" mode="assign" binding="same"><string>Noto Color Emoji</string></edit>
  </match>
  <selectfont><rejectfont><glob>/usr/share/fonts/*/extra/*</glob></rejectfont></selectfont>
</fontconfig>
```

`packaging/liteweb-private.desktop`:

```ini
[Desktop Entry]
Type=Application
Version=1.0
Name=LiteWeb (privé)
GenericName=Web Browser
Comment=Navigateur LiteWeb dans une sandbox bubblewrap
Exec=liteweb-private.sh
Icon=liteweb
Terminal=false
Categories=Network;WebBrowser;
Keywords=browser;web;privacy;vpn;
StartupNotify=true
```

- [ ] **Step 3: GREEN — run the self-test**

```bash
chmod +x scripts/liteweb-private.sh
./scripts/liteweb-private.sh --self-test
./scripts/liteweb-private.sh --dry-run | head -5
LITEWEB_NO_SANDBOX=1 ./scripts/liteweb-private.sh --self-test
```

Expected: `self-test: OK` in both sandboxed and `LITEWEB_NO_SANDBOX=1` modes (the no-sandbox mode skips the font/env/cpuinfo assertions), and `--dry-run` prints `bwrap` as the first token. If `bwrap` is absent on the machine, the sandboxed run must exit `3` with the install hints — verify that too by temporarily renaming `PATH`.

- [ ] **Step 4: REFACTOR — shell hygiene**

```bash
bash -n scripts/liteweb-private.sh
command -v shellcheck >/dev/null && shellcheck scripts/liteweb-private.sh || echo "shellcheck absent"
```

Expected: no syntax error; fix every `shellcheck` finding (or annotate with `# shellcheck disable=SCxxxx` plus a one-line justification). Then a real run:

```bash
./scripts/liteweb-private.sh --dry-run | head -1              # premier token = bwrap
./scripts/liteweb-private.sh --dry-run | wc -l                 # un argument par ligne
./scripts/liteweb-private.sh                                   # lancement réel
```

Inside the launched browser, confirm (via a local page or `:privacy` from Task 10) that `cat /proc/cpuinfo` differs from the host, `date +%Z` prints `CEST`/`CET` (Europe/Paris), and `fc-list | wc -l` ≤ 8.

---

### Task 9: UI — `:vpn`, `:permission`, `:setting`, `:privacy` and the toolbar indicator

**Files:**
- Modify: `src/commands/palette.rs` (variants + parser + tests)
- Modify: `src/ui/window.rs` (`run_command` arms, `show_vpn_status`, `show_permissions`, `show_privacy_summary`, `update_chrome`, `KEYBAR_COMMAND`)
- Test: `src/commands/palette.rs`

**Interfaces:**
- Consumes: `crate::privacy::{describe_permission, format_vpn_status, PERMISSION_APIS}`, `Storage::{list_permissions, grant_permission, revoke_permission, get_setting, set_setting}`, `TrackerBlocker::{rule_count, hit_count}`.
- Produces: `CommandAction::{VpnStatus, VpnRequire(bool), PermissionList, PermissionGrant(String, String), PermissionRevoke(String, String), SettingTrackerBlock(bool), PrivacySummary}`.

- [ ] **Step 1: RED — parser tests**

Add to the `tests` module of `src/commands/palette.rs`:

```rust
#[test]
fn parses_vpn_commands() {
    assert_eq!(CommandPalette::parse(":vpn status"), CommandAction::VpnStatus);
    assert_eq!(CommandPalette::parse(":vpn"), CommandAction::VpnStatus);
    assert_eq!(CommandPalette::parse(":vpn require on"), CommandAction::VpnRequire(true));
    assert_eq!(CommandPalette::parse(":vpn require off"), CommandAction::VpnRequire(false));
    match CommandPalette::parse(":vpn require maybe") {
        CommandAction::Unknown(_) => {}
        other => panic!("attendu Unknown, obtenu {other:?}"),
    }
}

#[test]
fn parses_permission_commands() {
    assert_eq!(CommandPalette::parse(":permission list"), CommandAction::PermissionList);
    assert_eq!(CommandPalette::parse(":permission"), CommandAction::PermissionList);
    assert_eq!(
        CommandPalette::parse(":permission grant https://example.com camera"),
        CommandAction::PermissionGrant("https://example.com".into(), "camera".into())
    );
    assert_eq!(
        CommandPalette::parse(":perm revoke https://example.com camera"),
        CommandAction::PermissionRevoke("https://example.com".into(), "camera".into())
    );
    match CommandPalette::parse(":permission grant https://example.com") {
        CommandAction::Unknown(_) => {}
        other => panic!("attendu Unknown (api manquante), obtenu {other:?}"),
    }
}

#[test]
fn parses_tracker_block_setting() {
    assert_eq!(
        CommandPalette::parse(":setting tracker-block off"),
        CommandAction::SettingTrackerBlock(false)
    );
    assert_eq!(
        CommandPalette::parse(":setting tracker-block on"),
        CommandAction::SettingTrackerBlock(true)
    );
}

#[test]
fn parses_privacy_summary() {
    assert_eq!(CommandPalette::parse(":privacy"), CommandAction::PrivacySummary);
}
```

- [ ] **Step 2: RED — run and confirm**

Run: `cargo test -- commands::palette`
Expected: compile error — the new variants do not exist.

- [ ] **Step 3: GREEN — variants and parser**

`src/commands/palette.rs`:

```rust
    DownloadList,
    VpnStatus,
    VpnRequire(bool),
    PermissionList,
    PermissionGrant(String, String),
    PermissionRevoke(String, String),
    SettingTrackerBlock(bool),
    PrivacySummary,
    Unknown(String),
```

and in the `match verb.as_str()` block, before the `_ =>` arm:

```rust
            "vpn" => {
                let mut words = arg.split_whitespace();
                match words.next().unwrap_or("status").to_lowercase().as_str() {
                    "status" | "" => CommandAction::VpnStatus,
                    "require" => match words.next().unwrap_or("").to_lowercase().as_str() {
                        "on" => CommandAction::VpnRequire(true),
                        "off" => CommandAction::VpnRequire(false),
                        _ => CommandAction::Unknown(cmd.to_string()),
                    },
                    _ => CommandAction::Unknown(cmd.to_string()),
                }
            }
            "permission" | "perm" => {
                let mut words = arg.split_whitespace();
                match words.next().unwrap_or("list").to_lowercase().as_str() {
                    "list" | "" => CommandAction::PermissionList,
                    verb @ ("grant" | "revoke") => {
                        let origin = words.next().unwrap_or("").to_string();
                        let api = words.next().unwrap_or("").to_lowercase();
                        if origin.is_empty() || api.is_empty() {
                            CommandAction::Unknown(cmd.to_string())
                        } else if verb == "grant" {
                            CommandAction::PermissionGrant(origin, api)
                        } else {
                            CommandAction::PermissionRevoke(origin, api)
                        }
                    }
                    _ => CommandAction::Unknown(cmd.to_string()),
                }
            }
            "setting" | "settings" => {
                let mut words = arg.split_whitespace();
                match words.next().unwrap_or("").to_lowercase().as_str() {
                    "tracker-block" => match words.next().unwrap_or("").to_lowercase().as_str() {
                        "on" => CommandAction::SettingTrackerBlock(true),
                        "off" => CommandAction::SettingTrackerBlock(false),
                        _ => CommandAction::Unknown(cmd.to_string()),
                    },
                    _ => CommandAction::Unknown(cmd.to_string()),
                }
            }
            "privacy" | "confidentialite" => CommandAction::PrivacySummary,
```

- [ ] **Step 4: GREEN — run the parser tests**

Run: `cargo test -- commands::palette`
Expected: the 4 new tests PASS and every existing parser test still passes.

- [ ] **Step 5: Wire `run_command` and the displays**

`src/ui/window.rs` — new arms in `run_command` (before `CommandAction::Unknown`):

```rust
            CommandAction::VpnStatus => Self::show_vpn_status(state_cmd.clone()),
            CommandAction::VpnRequire(required) => {
                {
                    let mut st = state_cmd.borrow_mut();
                    st.privacy.kill_switch.set_required(required);
                    st.storage.set_setting("vpn_required", if required { "1" } else { "0" });
                }
                Self::show_vpn_status(state_cmd.clone());
                if required && !crate::privacy::detect_vpn().connected {
                    Self::show_vpn_overlay(state_cmd.clone());
                }
            }
            CommandAction::PermissionList => Self::show_permissions(state_cmd.clone()),
            CommandAction::PermissionGrant(origin, api) => {
                Self::set_permission(state_cmd.clone(), origin, api, true)
            }
            CommandAction::PermissionRevoke(origin, api) => {
                Self::set_permission(state_cmd.clone(), origin, api, false)
            }
            CommandAction::SettingTrackerBlock(enabled) => {
                state_cmd.borrow_mut().privacy.tracker.set_enabled(enabled);
                state_cmd.borrow().storage.set_setting("tracker_block", if enabled { "1" } else { "0" });
                Self::update_chrome(state_cmd.clone());
                if !enabled {
                    Self::show_message(
                        "Confidentialité",
                        "Traqueurs bloqués : la liste EasyPrivacy est désactivée, les requêtes ne sont plus filtrées.",
                    );
                }
            }
            CommandAction::PrivacySummary => Self::show_privacy_summary(state_cmd.clone()),
```

New methods (next to `show_downloads`):

```rust
fn show_vpn_status(state: Rc<RefCell<AppState>>) {
    let text = state.borrow().privacy.kill_switch.describe();
    Self::show_message("VPN", &text);
}

fn show_permissions(state: Rc<RefCell<AppState>>) {
    let records = state.borrow().storage.list_permissions();
    let text = if records.is_empty() {
        format!(
            "Aucune exception enregistrée — toutes les permissions sont refusées.\n\nAPI : {}",
            crate::privacy::PERMISSION_APIS.join(", ")
        )
    } else {
        records
            .iter()
            .map(crate::privacy::describe_permission)
            .collect::<Vec<_>>()
            .join("\n")
    };
    Self::show_message("Permissions", &text);
}

fn set_permission(state: Rc<RefCell<AppState>>, origin: String, api: String, granted: bool) {
    if !crate::privacy::PERMISSION_APIS.contains(&api.as_str()) {
        Self::show_message("Permissions", &format!("API inconnue : « {api} »."));
        return;
    }
    let Some(normalized) = crate::privacy::permission_origin(&origin) else {
        Self::show_message(
            "Permissions",
            &format!("Origine invalide : « {} » (http/https attendus).", sanitize_ui_text(&origin)),
        );
        return;
    };
    // Clone de `storage` puis relâchement de l'emprunt AVANT show_message
    // (boucle principale GTK imbriquée).
    let storage = state.borrow().storage.clone();
    storage.set_permission(&normalized, &api, granted);
    let state_label = if granted { "accordée" } else { "refusée" };
    Self::show_message(
        "Permission enregistrée",
        &format!("{normalized} — {api} [{state_label}]"),
    );
}

fn show_privacy_summary(state: Rc<RefCell<AppState>>) {
    let fingerprint = crate::privacy::privacy_settings();
    let (tracker_rules, tracker_hits, vpn_required, vpn_text, sandbox_count) = {
        let st = state.borrow();
        (
            st.privacy.tracker.rule_count(),
            st.privacy.tracker.hit_count(),
            st.privacy.kill_switch.is_required(),
            st.privacy.kill_switch.describe(),
            st.sandboxes.borrow().len(),
        )
    };
    let text = format!(
        "Confidentialité — LiteWeb\n\n\
         Empreinte (figée, jamais randomisée)\n\
         • UA : {}\n• Plateforme : {}\n• Langues : {}\n• Fuseau : {}\n• Écran : {}×{}\n\
         • WebGL : {} / {}\n\n\
         Stockage\n• Sandbox par site actives : {sandbox_count}\n\
         • Purge après 5 min d'inactivité\n• Cache HTTP : désactivé\n\n\
         Traqueurs\n• Règles EasyPrivacy : {tracker_rules}\n• Requêtes bloquées : {tracker_hits}\n\n\
         VPN\n{vpn_text}",
        fingerprint.user_agent,
        fingerprint.platform,
        fingerprint.languages().join(", "),
        fingerprint.time_zone,
        fingerprint.screen_width,
        fingerprint.screen_height,
        fingerprint.webgl_vendor,
        fingerprint.webgl_renderer,
    );
    let _ = vpn_required;
    Self::show_message("Confidentialité", &text);
}
```

`TrackerBlocker` needs the on/off switch used above — add to `src/privacy/tracker.rs`:

```rust
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.borrow_mut() = enabled;
    }
```

with `enabled: RefCell<bool>` in the struct (default `true`) and `blocks_host` returning `false` when disabled; add the RED test first:

```rust
    #[test]
    fn disabled_blocker_matches_nothing() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.is_blocked("https://ad.doubleclick.net/x"));
        blocker.set_enabled(false);
        assert!(!blocker.is_blocked("https://ad.doubleclick.net/x"));
        blocker.set_enabled(true);
        assert!(blocker.is_blocked("https://ad.doubleclick.net/x"));
    }
```

Toolbar indicator — in `update_chrome` (and in `render_tabs`'s `block_text` computation), extend the label instead of adding a widget:

```rust
let block_text = format!(
    "🛡 {} | Bloqués: {} | VPN: {}",
    if st.privacy.tracker.is_enabled() { "EasyPrivacy" } else { "sans filtre" },
    st.blocker.blocked_count() + st.privacy.tracker.hit_count(),
    if st.privacy.kill_switch.is_required() { "requis" } else { "off" }
);
```

`KEYBAR_COMMAND` becomes:

```rust
const KEYBAR_COMMAND: &str =
    ":open :tab new|close|next|prev|N :suspend :suspend-all :eco on|off|aggressive|ultra :bookmark :history :download :privacy :vpn status|require on|off :permission list|grant|revoke :setting tracker-block on|off  |  Entrée → exécuter  |  Échap → annuler";
```

- [ ] **Step 6: REFACTOR — full suite + manual UI pass**

Run: `cargo build && cargo test`
Expected: green.

Manual: `cargo run` then
1. `:privacy` → dialog titled « Confidentialité » with the frozen UA, 1920×1080, Europe/Paris, WebGL values, EasyPrivacy rule count, VPN block.
2. `:permission list` → « Permissions », « Aucune exception enregistrée — toutes les permissions sont refusées. » plus the API list.
3. `:permission grant https://example.com camera` → « Permission enregistrée » / `https://example.com — camera [accordée]`; `:permission list` shows it; `:perm revoke https://example.com camera` flips it to « refusée ».
4. `:permission grant https://example.com telepathy` → « API inconnue : « telepathy » .» (exact wording from `set_permission`).
5. `:setting tracker-block off` → toolbar shows « sans filtre » and the warning dialog appears; `:setting tracker-block on` restores it.
6. `:vpn status` → « VPN : actif/inactif », interface, fournisseur, route, « Kill-switch : … ».
7. `:vpn require on` with no VPN → overlay appears; `:vpn require off` → navigation works again; restart → the flag persisted.

---

### Task 10: End-to-end verification against the spec's acceptance criteria

No new production code (fixes only, each in its own focused edit).

**Files:**
- Modify (only if a criterion fails): any file from the map above.
- Test: `cargo test` plus the manual matrix below.

- [ ] **Step 1: Full automated suite**

Run: `cargo test`
Expected: every test PASSes. Then:

Run: `cargo build --release`
Expected: success, no new warnings (`cargo clippy --all-targets` clean if available).

- [ ] **Step 2: Acceptance matrix**

| Critère du spec | Vérification | Attendu |
|---|---|---|
| Cookies isolés par site | Onglet A : login sur site X. Onglet B : lien cross-site vers X | aucune session partagée ; `ls ~/.local/share/liteweb-privacy` = 1 répertoire par site |
| Purge 5 min | `touch` puis attendre 5 min 30 s | répertoire du site supprimé, `find … -type f \| wc -l` = 0 |
| Empreinte figée | deux runs successifs sur browserleaks/amiunique | valeurs identiques, `navigator.webdriver === false`, `plugins.length === 0`, `screen.width === 1920`, WebGL `Google Inc. (NVIDIA)` |
| Aucun `Referer` cross-origin | listener `python3` du Task 4 Step 6 | pas d'en-tête `Referer` ; same-origin → origine seule |
| `Sec-CH-*` strippés | idem | aucun `Sec-CH-UA*` dans la requête |
| EasyPrivacy | `cargo test -- privacy::tracker` | chargement < 100 ms, ≥ 80 % de la cover-list bloquée |
| Permissions | page demandant la géoloc | aucun prompt WebKit, `permissions` vide |
| Kill-switch VPN | `:vpn require on`, VPN coupé, `ss -tnp \| grep liteweb` | aucune connexion publique ; onglets → `about:blank#vpn-lost` en ≤ 30 s |
| localhost préservé | `http://localhost:8080` avec kill-switch actif | la page se charge |
| Sandbox OS | `./scripts/liteweb-private.sh --self-test` | `self-test: OK` ; `cat /proc/cpuinfo` ≠ hôte ; `date +%Z` = Europe/Paris ; `fc-list \| wc -l` ≤ 8 |
| Performances | `./scripts/benchmark_ultra.sh` avant/après | pas de régression > 10 % |

- [ ] **Step 3: Regression sweep on the untouched surfaces**

Run: `cargo test -- browser::downloads && cargo test -- storage && cargo test -- commands`
Expected: downloads, storage and palette regressions all PASS (the download dialog still refuses profile destinations, `Téléchargements` list still renders).

- [ ] **Step 4: Record the results**

Append the measured numbers (rule counts, purge delay, benchmark delta) to `docs/superpowers/notes/2026-09-30-privacy-webkit-api-spike.md` under a `## Vérifications` heading. Do not create any other note file.

---

### Task 11: `README.md` + `README.fr.md` — « Private mode / anti-detection »

**Files:**
- Modify: `README.md`, `README.fr.md`
- No tests (documentation).

**Interfaces:**
- Consumes: the shipped behaviour of Tasks 1–9.
- Produces: user documentation with honest limits.

- [ ] **Step 1: Add the French section**

In `README.fr.md`, insert a new `## Mode privé / anti-détection` section **between** `## Sécurité` and `## Raccourcis clavier`:

```markdown
## Mode privé / anti-détection

LiteWeb réduit la surface que les sites peuvent observer passivement. Tout est
actif par défaut, rien n'est randomisé.

- **Empreinte figée** — Chrome 127 / Linux x86_64 : UA, `navigator.platform`,
  langues `fr-FR, fr, en`, fuseau `Europe/Paris`, écran 1920×1080,
  `devicePixelRatio` 1, WebGL `Google Inc. (NVIDIA)` /
  `ANGLE (NVIDIA, NVIDIA GeForce GTX 1060 6GB, OpenGL 4.5)`, `plugins` et
  `mimeTypes` vides, `webdriver` faux. Un profil **stable** : aucune valeur
  n'est tirée au hasard, sinon le navigateur devient détectable.
- **Sandbox par site** — 1 site = 1 répertoire de stockage. Un cookie
  n'appartient jamais à un autre site. Après 5 min d'inactivité, la sandbox du
  site est détruite (cookies, IndexedDB, cache, localStorage, ServiceWorker).
  Aucun cache HTTP persistant, pas d'historique de stockage, purge complète au
  démarrage.
- **Anti-tracking durci** — tous les en-têtes `Sec-CH-*` sont supprimés,
  `DNT: 1` est envoyé, `Accept-Language` est aligné sur l'empreinte, `Referer`
  est réduit à l'origine en same-origin et **supprimé** en cross-origin. La
  liste EasyPrivacy embarquée bloque les requêtes vers les traqueurs connus.
- **Permissions refusées** — géoloc, notifications, caméra, micro, écran,
  presse-papiers, USB, Bluetooth, capteurs, etc. Aucun prompt n'apparaît ; une
  exception se déclare explicitement avec `:permission grant <origine> <api>`.
- **Kill-switch VPN** — `:vpn require on` bloque toute navigation HTTP/HTTPS
  tant qu'aucun VPN n'est actif (NordVPN `nordlynx`, Mullvad `wg0`, ProtonVPN
  `proton0`, OpenVPN `tun*`). `localhost`, `*.local` et les plages privées
  restent accessibles. En cas de perte du VPN, tous les onglets passent en
  `about:blank#vpn-lost`.
- **Sandbox OS** — `scripts/liteweb-private.sh` lance LiteWeb dans bubblewrap :
  `/proc/cpuinfo`, DMI, hostname, machine-id et adresse MAC factices, `TZ` et
  `LANG` forcés, fontconfig réduit, environnement nettoyé, `--unshare-all`.

### Commandes

| Commande | Effet |
|---|---|
| `:privacy` | récapitulatif (empreinte, sandbox actives, traqueurs, VPN) |
| `:vpn status` | état détaillé du VPN et du kill-switch |
| `:vpn require on\|off` | active/désactive le kill-switch (persistant) |
| `:permission list\|grant\|revoke` | registre des exceptions de permission |
| `:setting tracker-block on\|off` | filtre EasyPrivacy (un avertissement s'affiche à l'arrêt) |

### Limites honnêtes

Ce mode **ne remplace pas un VPN** et ne rend pas anonyme :

- **l'adresse IP reste visible** côté serveur — elle voyage dans la pile réseau
  du noyau, pas dans le navigateur ; seul un VPN actif la masque ;
- **l'empreinte TLS (JA3/JA4) n'est pas modifiée** : elle dépend de la pile
  réseau de WebKitGTK, hors de portée d'un navigateur non forké ;
- **le moteur reste WebKit, pas Blink** : quelques API JavaScript diffèrent de
  Chrome malgré l'UA annoncé — c'est le résidu assumé du profil « se fondre
  dans la masse » ;
- le blocage de traqueurs est **léger** (liste de domaines EasyPrivacy), ce
  n'est pas uBlock Origin ;
- le profil figé + le deny-all **cassent des sites** : utilisez
  `:setting tracker-block off` ou `:permission grant` au cas par cas.
```

- [ ] **Step 2: Add the English section**

In `README.md`, insert the mirrored `## Private mode / anti-detection` section between `## Security` and `## Keyboard shortcuts`, same structure and same table, translated. Keep the exact command strings and the exact French dialog wording in quotes where the UI shows French (« VPN requis — connexion bloquée », « Connexion bloquée. Activez NordVPN (ou autre VPN) pour naviguer. », « Permissions », « Confidentialité », « Téléchargements »).

- [ ] **Step 3: Update the command list and the architecture section**

In both READMEs:
- `## Commands` / `## Commandes` table gains `:privacy`, `:vpn status|require on|off`, `:permission list|grant|revoke`, `:setting tracker-block on|off`.
- `## Architecture` / `## Architecture` gains one line: `src/privacy/ — frozen fingerprint, per-site cookie sandbox, header hardening, EasyPrivacy matcher, permission registry, VPN kill-switch.`
- `## Requirements (Linux)` / `## Prérequis (Linux)` gains: `bubblewrap (bwrap) — optional, only for scripts/liteweb-private.sh`.

- [ ] **Step 4: Verify**

```bash
grep -n "Mode privé\|Private mode" README.md README.fr.md
grep -c "vpn require" README.md README.fr.md
cargo test
```

Expected: both files contain the new section, both mention `:vpn require`, and the suite is still green (documentation only).

---

## Spec coverage check

| Spec item | Task |
|---|---|
| Vérification API WebKitGTK 4.1 en premier (risque n°1 du spec) | 0 |
| Écarts API documentés (`OnlyFromMainDocumentDomain`, `set_custom_accept_language`, `WebExtension`, WebGPU, spell checking) | 0 |
| `privacy::config` : profil figé Chrome/Linux, aucune randomisation | 1 |
| `privacy::settings` : cache off, prefetch off, hyperlink auditing off, media stream/WebRTC off, spell checking off, TLS strict, favicon db off | 1 |
| `create_private_web_context` + `clear_on_quit` (purge au démarrage) | 1 |
| `PrivacyPolicy` partagé par `create_webview` | 1 |
| Axe 1 règle 1 : cookies tiers refusés, traqueurs refusés même first-party | 2 |
| Axe 1 règle 2 : 1 cookie → 1 site, répertoire segregé, cross-site = zéro | 2 |
| Axe 1 règle 3 : timer 30 s, purge après 5 min d'inactivité | 2 |
| Pas de cache HTTP / IndexedDB / localStorage / ServiceWorker / sessionStorage persistants | 1 + 2 |
| Axe 3 : tous les `Sec-CH-*` strippés, `Sec-Fetch-*` et `Origin` laissés à WebKit | 4 |
| `DNT: 1`, `Accept-Language` aligné, `Referer` réduit à l'origine / absent en cross-origin | 4 |
| EasyPrivacy embarqué, chargé au démarrage, `:setting tracker-block off` + warning | 3 + 9 |
| Permissions deny-all + registre SQLite + `:permission grant/revoke` | 5 + 9 |
| Axe 2 : WebExtension → `UserScript` à `DocumentStart` (tous les getters figés) | 6 |
| `navigator.webdriver === false`, plugins/mimeTypes vides, screen, WebGL, timezone, `getBattery` refusé | 6 |
| Axe 5 : détection NordVPN/Mullvad/Proton/OpenVPN, kill-switch applicatif, timer 30 s, `about:blank#vpn-lost`, overlay, `:vpn status` | 7 + 9 |
| Kill-switch ne casse pas localhost / LAN | 7 (tests + Global Constraints) |
| Axe 4 : `scripts/liteweb-private.sh` (cpuinfo, DMI, hostname, machine-id, MAC, TZ, LANG, fontconfig, env, `--unshare-all`) + `.desktop` | 8 |
| `bwrap` absent → message clair + repli `LITEWEB_NO_SANDBOX=1` | 8 |
| UI : `:privacy`, `:vpn`, `:permission`, `:setting`, indicateur toolbar | 9 |
| Critères d'acceptation mesurables (matrice complète) | 10 |
| Axe 6 : TLS strict + `TLSErrorsPolicy::Fail` (déjà en place, vérifié) ; DoH hors scope | 1 + 10 |
| README EN + FR avec limites honnêtes (JA3, IP, WebKit ≠ Blink) | 11 |
| Persistance `profiles` / `current_profile` | **hors plan** — voir Open Questions |
| `:profile list\|new\|use\|drop`, `:site-exception add`, `privacy-level standard\|strict` | **hors plan** — voir Open Questions |

---

## Risks / Open Questions

**Risks**

1. **`resource-load-started` may not expose mutable request headers for every request type.** If WebKit ignores `MessageHeaders::remove` on sub-resource requests (it is documented as best-effort for redirects and preflighted CORS requests), the `Sec-CH-*` stripping is partial. Mitigation: Task 4 Step 6 verifies against a local listener; if stripping fails for CORS preflights, document the residue in the README limits and keep `DNT` / `Accept-Language` / `Referer` (which are set on the main document request).
2. **`Referer` rewriting can trigger extra CORS preflights.** Adding `DNT` / `Accept-Language` to a cross-origin XHR makes the request non-simple. Mitigation: only `replace` headers that WebKit already sends (both are standard), never `append` new ones to XHR; verify with the local listener.
3. **`block_on` on the GTK main thread during the purge.** `clear_future` and `cookies_future` complete on WebKit's IO thread, but a stall would freeze the UI for the duration. Mitigation: if a freeze is observed in Task 2 Step 6, switch to `glib::spawn_future_local` with a completion flag; the registry API is unchanged.
4. **Cookie isolation is enforcement-by-sweep, not by construction.** WebKitGTK 4.1 exposes one `CookieManager` per `WebContext`, and `CookieAcceptPolicy` has no `OnlyFromMainDocumentDomain`. Between two 30-second ticks a cookie set by a cross-site response can exist in the jar. Mitigation: `NoThirdParty` blocks the main vector, the sweep deletes anything outside the visited-site registry, and the honest limit is documented. A hard fix would need one `WebContext` per site (heavy: one network process each) — see Open Question 2.
5. **The frozen profile breaks sites.** Deny-all permissions + no localStorage + no WebGL noise + blocked trackers will break login flows, video players and banking 3-D Secure. Mitigation: `:setting tracker-block off` and `:permission grant` exist; `:site-exception` is deferred (Open Question 3).
6. **`bwrap` availability and nesting.** Some distros ship `bwrap` without unprivileged user namespaces enabled (`kernel.unprivileged_userns_clone=0`), and running inside Flatpak/Snap already nests a sandbox. Mitigation: Task 8's `require_bwrap` + exit code 3 + `LITEWEB_NO_SANDBOX=1` fallback; add the `sysctl` hint to the error text if the self-test fails on the target machine.
7. **Performance.** The `UserScript` runs on every frame of every page and the two 30-second timers wake the process. Mitigation: keep the script under 8 KB (asserted in Task 6), and measure with `scripts/benchmark_ultra.sh` before/after in Task 10.
8. **`WebsiteDataTypes::ALL` purge is global, not per-origin.** `clear(types, timespan)` has no origin filter, so purging one expired site clears data older than the timespan for *all* sites. Mitigation: acceptable here (the timespan equals the idle limit, so an active site's fresh data survives), but it must be stated in the code comment — done in `purge_expired_sandboxes`.

**Open questions (unresolved, need a product decision)**

1. **`profiles` table and `:profile list|new|use|drop`.** The spec's « Persistance » section describes a multi-profile SQLite table and a `current_profile` setting, but the 9-step attack plan never schedules it. This plan implements a **single** frozen profile (matching « tout est `on`, non-désactivable par défaut ») and stores only `app_settings.{vpn_required, tracker_block}`. Is the multi-profile UI still wanted, or is the single frozen profile the shipped behaviour?
2. **True per-site `WebContext`.** Should isolation be upgraded to one `WebContext` + `WebsiteDataManager` per site (real « 1 cookie → 1 site » by construction, at the cost of one network process per site and a redesign of `TabManager`)? Current plan: single context + registry sweep.
3. **`:site-exception add <host>` and `:setting privacy-level standard|strict`.** Both appear only in the spec's « Risques » section, not in the attack plan. Deferred. Do we want them in this iteration (they are the main usability escape hatch for risk 5)?
4. **History.** The spec says « pas d'historique de navigation (sauf si l'utilisateur l'active explicitement, hors scope par défaut) », yet `:history` and the `history` table already ship. This plan leaves history untouched — should private mode stop writing history rows, or is the existing `:history` feature the explicit opt-in?
5. **`Upgrade-Insecure-Requests: 1` and HSTS preload (Axe 6).** The spec asks for it, the attack plan does not schedule it, and forcing the header on every request breaks plain-HTTP LAN devices (which the kill-switch deliberately keeps reachable). Deferred — confirm whether it is in scope.
6. **EasyPrivacy list provenance and refresh.** `assets/privacy/easyprivacy.txt` must be a real EasyPrivacy-derived subset with its licence header kept, and it will drift. Who refreshes it, and how often? (Task 3 ships a ~70-host seed list; the spec asks for a 5k-host subset and ≥ 80 % coverage of a 100-tracker cover-list, which needs the real list before Task 10 can pass.)
7. **Notification / geolocation prompts with a granted registry entry.** Task 5 keeps `request.deny()` even for granted origins (WebKit has no silent-allow path in this binding). Is a recorded-but-not-honoured grant acceptable UX, or should `:permission grant` be dropped until a real allow path exists?
