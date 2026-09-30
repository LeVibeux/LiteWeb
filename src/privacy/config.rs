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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_the_frozen_chrome_linux_profile() {
        let s = privacy_settings();
        assert_eq!(
            s.user_agent,
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36"
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
        assert_eq!(privacy_settings(), &PRIVACY_FINGERPRINT);
        assert_eq!(
            privacy_settings().user_agent,
            PRIVACY_FINGERPRINT.user_agent
        );
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
