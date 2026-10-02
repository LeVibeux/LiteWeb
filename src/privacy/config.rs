/// Trois façons de mentir. `Blend` se fond dans un profil courant et cohérent.
/// `Blank` retire tout ce qui est retirable. `Rotate` tire un profil par
/// session, mais seulement derrière NordVPN : sans nouvelle IP, le tirage
/// ne sert à rien.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyMode {
    Off,
    Blend,
    Blank,
    Rotate,
}

impl PrivacyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacyMode::Off => "off",
            PrivacyMode::Blend => "blend",
            PrivacyMode::Blank => "blank",
            PrivacyMode::Rotate => "rotate",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" | "0" => Some(PrivacyMode::Off),
            "blend" | "masse" | "on" | "1" => Some(PrivacyMode::Blend),
            "blank" | "vide" => Some(PrivacyMode::Blank),
            "rotate" | "random" | "aleatoire" => Some(PrivacyMode::Rotate),
            _ => None,
        }
    }
}

/// Profil figé. `Blend` l'utilise tel quel. `Rotate` en tire un par session
/// depuis `rotate_profile`, en gardant UA, plateforme et renderer cohérents.
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
    // Chrome stable Linux, gelé (UA reduction). 127 était à 0,00 % en
    // septembre 2026 ; 153 est la version stable courante.
    user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36",
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
    // Renderer ANGLE générique. Une carte nommée (GTX 1060) est à 0,00 %.
    webgl_vendor: "Google Inc. (NVIDIA)",
    webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 Direct3D11 vs_5_0 ps_5_0, D3D11)",
    webgl_max_texture_size: 16384,
};

/// Sous-répertoire du profil LiteWeb qui porte les sandbox par site.
pub const PRIVACY_DATA_DIR: &str = "liteweb-privacy";

/// Profils cohérents pour `Rotate`. Chaque entrée aligne UA, plateforme,
/// langue, timezone et renderer : un Chrome/Windows qui annonce un GPU
/// Apple est un signal, pas un camouflage.
const ROTATE_PROFILES: &[PrivacySettings] = &[
    PRIVACY_FINGERPRINT,
    PrivacySettings {
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36",
        platform: "Win32",
        languages: &["en-US", "en"],
        accept_language: "en-US,en;q=0.9",
        time_zone: "America/New_York",
        timezone_offset_minutes: 240,
        dnt: "1",
        tracker_block: true,
        ephemeral: true,
        clear_on_quit: true,
        screen_width: 1920,
        screen_height: 1080,
        screen_avail_width: 1920,
        screen_avail_height: 1040,
        color_depth: 24,
        pixel_depth: 24,
        device_pixel_ratio: 1.0,
        hardware_concurrency: 8,
        device_memory: 8,
        max_touch_points: 0,
        webgl_vendor: "Google Inc. (NVIDIA)",
        webgl_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        webgl_max_texture_size: 16384,
    },
    PrivacySettings {
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36",
        platform: "MacIntel",
        languages: &["en-US", "en"],
        accept_language: "en-US,en;q=0.9",
        time_zone: "America/Los_Angeles",
        timezone_offset_minutes: 420,
        dnt: "1",
        tracker_block: true,
        ephemeral: true,
        clear_on_quit: true,
        screen_width: 1440,
        screen_height: 900,
        screen_avail_width: 1440,
        screen_avail_height: 875,
        color_depth: 24,
        pixel_depth: 24,
        device_pixel_ratio: 2.0,
        hardware_concurrency: 8,
        device_memory: 8,
        max_touch_points: 0,
        webgl_vendor: "Google Inc. (Apple)",
        webgl_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M1, Unspecified Version)",
        webgl_max_texture_size: 16384,
    },
];

/// Tire un profil pour la session. Le seed vient de l'horloge : deux sessions
/// d'affilée ont des seeds différents, donc des profils différents la plupart
/// du temps. Trois profils seulement, donc une collision reste possible.
pub fn rotate_profile(seed: u64) -> &'static PrivacySettings {
    &ROTATE_PROFILES[(seed as usize) % ROTATE_PROFILES.len()]
}

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
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36"
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
            "ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 Direct3D11 vs_5_0 ps_5_0, D3D11)"
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

    #[test]
    fn mode_parse_accepts_the_three_modes_and_rejects_the_rest() {
        assert_eq!(PrivacyMode::parse("blend"), Some(PrivacyMode::Blend));
        assert_eq!(PrivacyMode::parse("masse"), Some(PrivacyMode::Blend));
        assert_eq!(PrivacyMode::parse("vide"), Some(PrivacyMode::Blank));
        assert_eq!(PrivacyMode::parse("aleatoire"), Some(PrivacyMode::Rotate));
        assert_eq!(PrivacyMode::parse("off"), Some(PrivacyMode::Off));
        assert_eq!(PrivacyMode::parse("nope"), None);
    }

    #[test]
    fn rotated_profiles_stay_internally_consistent() {
        for seed in 0..ROTATE_PROFILES.len() as u64 {
            let profile = rotate_profile(seed);
            let windows = profile.user_agent.contains("Windows");
            let mac = profile.user_agent.contains("Macintosh");
            if windows {
                assert_eq!(profile.platform, "Win32");
                assert!(profile.webgl_renderer.contains("Direct3D11"));
            }
            if mac {
                assert_eq!(profile.platform, "MacIntel");
                assert!(profile.webgl_vendor.contains("Apple"));
            }
            for tag in profile.languages() {
                assert!(
                    profile.accept_language.contains(*tag),
                    "{tag} absent de {}",
                    profile.accept_language
                );
            }
        }
    }
}
