use webkit2gtk::{
    CacheModel, CookieAcceptPolicy, CookieManagerExt, ProcessModel, Settings, SettingsExt,
    TLSErrorsPolicy, WebContext, WebContextExt, WebsiteDataManager, WebsiteDataManagerExt,
    WebsiteDataManagerExtManual, WebsiteDataTypes,
};

use super::config::{PRIVACY_DATA_DIR, PRIVACY_FINGERPRINT};

/// Durcissement des `WebKitSettings` : empreinte figée + fuites désactivées.
pub fn harden_settings(settings: &Settings) {
    settings.set_user_agent(Some(PRIVACY_FINGERPRINT.user_agent));
    settings.set_enable_webrtc(false);
    settings.set_enable_media_stream(false);
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
    settings.set_enable_developer_extras(false);
    settings.set_enable_site_specific_quirks(false);
    settings.set_allow_file_access_from_file_urls(false);
    settings.set_allow_universal_access_from_file_urls(false);
    settings.set_allow_top_navigation_to_data_urls(false);
    settings.set_allow_modal_dialogs(false);
    settings.set_javascript_can_access_clipboard(false);
    settings.set_javascript_can_open_windows_automatically(false);
    settings.set_media_playback_requires_user_gesture(true);
}

/// Durcissement du contexte : pas de cache HTTP, pas de favicon, TLS strict.
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
        cookies.set_accept_policy(CookieAcceptPolicy::NoThirdParty);
    }
}

/// `WebContext` dédié au mode privé : répertoires segregés, droits 0700.
pub fn create_private_web_context() -> WebContext {
    let manager = match private_data_manager() {
        Some(manager) => manager,
        None => {
            eprintln!(
                "LiteWeb: profil privé indisponible; repli sur WebsiteDataManager par défaut"
            );
            WebsiteDataManager::builder().build()
        }
    };
    let context = WebContext::with_website_data_manager(&manager);
    harden_web_context(&context);
    clear_previous_session_blocking(&context);
    context
}

/// Purge le stockage de la session précédente au démarrage.
pub fn clear_previous_session_blocking(context: &WebContext) {
    let Some(manager) = context.website_data_manager() else {
        return;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    manager.clear(
        WebsiteDataTypes::ALL,
        glib::TimeSpan(0),
        None::<&gio::Cancellable>,
        move |result| {
            let _ = tx.send(result);
        },
    );
    if let Ok(result) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
        if let Err(error) = result {
            eprintln!("LiteWeb: purge du stockage précédente impossible: {error}");
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use webkit2gtk::{CacheModel, SettingsExt, TLSErrorsPolicy, WebContextExt};

    fn init_gtk() {
        gtk::init().expect("gtk init");
    }

    // NOTE: ces tests créent des objets GTK/WebKit. Sans main loop GLib,
    // la destruction cause une corruption heap (SIGABRT). Exécution manuelle:
    //   cargo test -- privacy:: -- --ignored --test-threads=1
    #[test]
    #[ignore = "nécessite un main loop GTK; corruption heap en test unitaire"]
    fn hardened_settings_carry_the_frozen_fingerprint() {
        init_gtk();
        let settings = webkit2gtk::Settings::new();
        harden_settings(&settings);
        assert_eq!(
            settings.user_agent().unwrap(),
            PRIVACY_FINGERPRINT.user_agent
        );
    }

    #[test]
    #[ignore = "nécessite un main loop GTK; corruption heap en test unitaire"]
    fn hardened_settings_disable_every_leaky_feature() {
        init_gtk();
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
        assert!(!settings.enables_developer_extras());
        assert!(!settings.enables_site_specific_quirks());
        assert!(!settings.allows_file_access_from_file_urls());
        assert!(!settings.allows_universal_access_from_file_urls());
        assert!(!settings.allows_top_navigation_to_data_urls());
        assert!(!settings.allows_modal_dialogs());
    }

    #[test]
    #[ignore = "nécessite un main loop GTK; corruption heap en test unitaire"]
    fn hardened_context_is_private_and_fail_closed_on_tls() {
        init_gtk();
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
    #[ignore = "nécessite un main loop GTK; corruption heap en test unitaire"]
    fn private_data_manager_is_on_disk_and_never_ephemeral() {
        let context = create_private_web_context();
        let manager = context.website_data_manager().expect("data manager");
        assert!(
            !manager.is_ephemeral(),
            "le repli éphémère perdrait l'isolation par site"
        );
    }
}
