use std::path::Path;
use std::time::Duration;

use url::Url;

/// Chaînes UI exactes (ne pas reformuler).
pub const MSG_VPN_BLOCKED_TITLE: &str = "VPN requis — connexion bloquée";
pub const MSG_VPN_BLOCKED_BODY: &str =
    "Connexion bloquée. Activez NordVPN (ou autre VPN) pour naviguer.";
pub const VPN_LOST_URI: &str = "about:blank#vpn-lost";

pub const VPN_RECHECK_TICK: Duration = Duration::from_secs(30);
pub const VPN_OVERLAY_POLL: Duration = Duration::from_secs(5);

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

pub fn default_route_interface() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        line.split_whitespace()
            .zip(line.split_whitespace().skip(1))
            .find(|(key, _)| *key == "dev")
            .map(|(_, value)| value.to_string())
    })
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
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

pub fn is_internal_host(host: &str) -> bool {
    let host = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    if host == "::1" {
        return true;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let Ok(octets) = parts
        .iter()
        .map(|part| part.parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    match octets[0] {
        127 => true,
        10 => true,
        172 => (16..=31).contains(&octets[1]),
        192 => octets[1] == 168,
        169 => octets[1] == 254,
        _ => false,
    }
}

pub fn vpn_kill_switch(vpn_connected: bool, uri: &str) -> bool {
    if vpn_connected {
        return true;
    }
    if uri.starts_with("about:") {
        return true;
    }
    let Ok(parsed) = Url::parse(uri) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    match parsed.host_str() {
        Some(host) => is_internal_host(host),
        None => false,
    }
}

pub fn kill_switch_uri_allowed(uri: &str, vpn_connected: bool, vpn_required: bool) -> bool {
    if !vpn_required {
        return true;
    }
    vpn_kill_switch(vpn_connected, uri)
}

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

#[derive(Clone)]
pub struct KillSwitch {
    vpn_required: bool,
}

impl KillSwitch {
    pub fn new(vpn_required: bool) -> Self {
        Self { vpn_required }
    }

    pub fn set_required(&mut self, required: bool) {
        self.vpn_required = required;
    }

    pub fn is_required(&self) -> bool {
        self.vpn_required
    }

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

pub fn notify_kill_switch_blocked() {
    let (tx, _) = kill_switch_channel();
    let _ = tx.send(MSG_VPN_BLOCKED_TITLE.to_string());
}

pub fn drain_kill_switch_notifications() -> Vec<String> {
    let (_, rx) = kill_switch_channel();
    let Ok(rx) = rx.lock() else {
        return Vec::new();
    };
    let mut drained = Vec::new();
    while let Ok(message) = rx.try_recv() {
        drained.push(message);
    }
    drained
}

fn kill_switch_channel() -> &'static (
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
        fs::remove_dir_all(&root).ok();
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
            assert!(
                vpn_kill_switch(false, uri),
                "{uri} doit rester accessible sans VPN"
            );
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
        assert!(kill_switch_uri_allowed("https://example.com", false, false));
        assert!(!kill_switch_uri_allowed("https://example.com", false, true));
        assert!(kill_switch_uri_allowed(
            "http://localhost:8080",
            false,
            true
        ));
        assert!(kill_switch_uri_allowed("about:blank#vpn-lost", false, true));
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
        assert_eq!(
            provider_for_interface("nordlynx").as_deref(),
            Some("NordVPN (nordlynx)")
        );
        assert_eq!(
            provider_for_interface("wg0").as_deref(),
            Some("Mullvad/WireGuard (wg0)")
        );
        assert_eq!(
            provider_for_interface("proton0").as_deref(),
            Some("ProtonVPN (proton0)")
        );
        assert_eq!(
            provider_for_interface("tun0").as_deref(),
            Some("OpenVPN/tun (tun0)")
        );
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
