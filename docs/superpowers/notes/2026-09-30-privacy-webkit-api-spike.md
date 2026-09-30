# Spike — surface WebKitGTK 4.1 pour le mode privé

Date: 2026-09-30. Binding: `webkit2gtk 0.19.2` (feature `v2_38`) → `webkit2gtk-4.1`.
Runtime locale: `libwebkit2gtk-4.1.so.0.21.10` (WebKitGTK 2.50.x).
Source: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/webkit2gtk-0.19.2/`.

## Disponible

| Besoin | API retenue | Localisation |
|---|---|---|
| UA figé | `SettingsExt::set_user_agent(Some(UA))` | `src/auto/settings.rs:1316` |
| Fuseau figé | `WebContext::builder().time_zone_override("Europe/Paris")` | `src/auto/web_context.rs:221` |
| Langues | `WebContextExt::set_preferred_languages(&["fr-FR","fr","en"])` | `src/auto/web_context.rs:405` |
| Cache HTTP off | `WebContextExt::set_cache_model(CacheModel::DocumentViewer)` | — |
| Spell checking off | `WebContextExt::set_spell_checking_enabled(false)` | `src/auto/web_context.rs:418` |
| Pas de cache disque / SW / IDB | `WebsiteDataManager` builder (`disk_cache_directory`, `service_worker_registrations_directory`, `indexeddb_directory`) + `WebsiteDataTypes::{MEMORY_CACHE, DISK_CACHE, OFFLINE_APPLICATION_CACHE, SESSION_STORAGE, LOCAL_STORAGE, WEBSQL_DATABASES, INDEXEDDB_DATABASES, PLUGIN_DATA, COOKIES, DEVICE_ID_HASH_SALT, HSTS_CACHE, ITP, SERVICE_WORKER_REGISTRATIONS, DOM_CACHE, ALL}` | `src/auto/flags.rs:415-446` |
| Purge par tranche de temps | `WebsiteDataManagerExtManual::clear(types, timespan, None, cb)` (v2_16+) | `src/website_data_manager.rs:18,41` |
| Lecture / suppression de cookies | `CookieManagerExt::cookies_future(uri)`, `delete_cookie_future(&mut cookie)` — `soup::Cookie::domain()` / `name()` prennent `&mut self` | `soup3-0.3.2` |
| En-têtes sortants | `URIRequestExt::http_headers()` → `soup::MessageHeaders::{remove, replace, append}` | `soup3-0.3.2/src/auto/message_headers.rs:31,196,203` |
| JS anti-fuite | `UserContentManagerExt::add_script(&UserScript::new(src, AllFrames, Start, &[], &[]))` | — |
| Permissions | `WebView::connect_permission_request` (déjà `deny()` global) | — |
| WebRTC / MediaStream off | `Settings::set_enable_webrtc(false)`, `set_enable_media_stream(false)` | `src/auto/settings.rs:1258,1206` |
| Smooth scrolling off | `Settings::set_enable_smooth_scrolling(false)` | `src/auto/settings.rs:1239` |
| LocalStorage off | `Settings::set_enable_html5_local_storage(false)` | `src/auto/settings.rs:1176` |
| WebGL off (optionnel) | `Settings::set_enable_webgl(false)` | `src/auto/settings.rs:1253` |
| Sortie sandbox WebKit | `WebContextExt::add_path_to_sandbox`, `is_sandbox_enabled` | — |
| Répertoire extensions | `WebContextExt::set_web_extensions_directory` (existe mais inutile ici, cf. ci-dessous) | `src/auto/web_context.rs:433` |

## Indisponible (écart au spec)

| Le spec demande | Réalité 4.1 | Décision |
|---|---|---|
| `CookieAcceptPolicy::OnlyFromMainDocumentDomain` | enum = `Always` / `Never` / `NoThirdParty` **seulement** (`src/auto/enums.rs:782-790`) | garder `NoThirdParty` + purge par registre de sites (Task 2) |
| `Settings.set_custom_accept_language` | inexistant | `set_preferred_languages` sur le contexte + `replace("Accept-Language", …)` par requête (Task 4) |
| `Settings.set_enable_spell_checking` | inexistant sur `Settings` | `WebContextExt::set_spell_checking_enabled(false)` |
| `Settings.set_enable_webgpu` | inexistant | rien à faire : WebGPU n'est pas exposé par WebKitGTK 4.1 |
| `set_webrtc_ip_handling_policy` | inexistant | `set_enable_webrtc(false)` + `set_enable_media_stream(false)` |
| `WebExtension` (`connect_web_extension`, `manifest.json` + `content.js`) | le binding ne fournit ni `WebExtensionContext` ni crate d'extension ; l'API d'extension reste du C (`libwebkit2gtk-web-extension`) | `UserScript` injecté à `DocumentStart` = le « fallback » du spec, mais en fait **le** mécanisme retenu. Aucun `.so` C à construire. |
| `Settings.set_enable_form_automation` | inexistant | déjà couvert : `set_javascript_can_access_clipboard(false)`, `set_allow_modal_dialogs(false)` |

## Conséquences pour le plan

1. **Pas de fichier `manifest.json`** ni de répertoire d'extensions WebKit : le JS vit dans `src/privacy/fingerprint.rs` (inline, pas de `include_str!`).
2. **`Accept-Language` est posé par requête** (Task 4), sinon WebKit envoie les langues système.
3. **La politique cookies reste `NoThirdParty`** ; l'isolation « 1 cookie → 1 site » est obtenue par le registre + le sweep (Task 2), pas par l'enum.
4. `set_web_extensions_directory` existe mais ne sert pas — on utilise `UserScript`, pas les extensions WebKit.

## Vérifications effectuées

```bash
$ ls /usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so*
/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0
/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0.21.10

$ grep 'name = "webkit2gtk"' -A2 Cargo.lock
name = "webkit2gtk"
version = "0.19.2"

$ grep webkit2gtk Cargo.toml
webkit2gtk = { version = "0.19", features = ["v2_38"] }
```

Toutes les APIs « Disponible » ont été retrouvées par `grep` dans les sources du binding.
Toutes les APIs « Indisponible » ont été cherchées et **absentes** desdites sources.
