use std::path::{Path, PathBuf};

use crate::commands::is_bidi_control;

pub const FALLBACK_DOWNLOAD_FILENAME: &str = "telechargement";

/// Garde uniquement le dernier composant de chemin (aucun `/` ni `\`),
/// retire les caractères de contrôle et bidi, retire les points de tête
/// (pas de fichier caché), tronque à 128 caractères.
/// Résultat vide → FALLBACK_DOWNLOAD_FILENAME.
pub fn sanitize_download_filename(input: &str) -> String {
    let last_component = input.rsplit(|c| c == '/' || c == '\\').next().unwrap_or("");
    let cleaned: String = last_component
        .chars()
        .filter(|ch| !ch.is_control() && !is_bidi_control(*ch))
        .collect();
    let visible = cleaned.trim_start_matches('.');
    let truncated: String = visible.chars().take(128).collect();
    if truncated.is_empty() {
        FALLBACK_DOWNLOAD_FILENAME.to_string()
    } else {
        truncated
    }
}

/// dirs::download_dir() (XDG), sinon ~/Downloads, sinon ".".
pub fn default_download_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// true si le chemin choisi est sous ~/.config/liteweb, ~/.local/share/liteweb
/// ou ~/.cache/liteweb (profil, base SQLite, données WebKit).
pub fn is_forbidden_download_destination(path: &Path) -> bool {
    let mut roots = Vec::new();
    if let Some(dir) = dirs::config_dir() {
        roots.push(dir.join("liteweb"));
    }
    if let Some(dir) = dirs::data_dir() {
        roots.push(dir.join("liteweb"));
    }
    if let Some(dir) = dirs::cache_dir() {
        roots.push(dir.join("liteweb"));
    }
    destination_is_under(path, &roots)
}

/// Le parent est canonicalisé pour résister aux symlinks ; si la
/// canonicalisation échoue (répertoire pas encore créé), on compare le
/// chemin brut — fail-closed pour la détection.
fn destination_is_under(path: &Path, roots: &[PathBuf]) -> bool {
    let parent = path.parent().unwrap_or(path);
    let canonical_parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    roots.iter().any(|root| {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        canonical_parent.starts_with(&canonical_root)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "liteweb-downloads-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn strips_path_traversal_to_last_component() {
        assert_eq!(sanitize_download_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_download_filename("/etc/passwd"), "passwd");
        assert_eq!(
            sanitize_download_filename("..\\..\\Windows\\evil.exe"),
            "evil.exe"
        );
    }

    #[test]
    fn strips_leading_dots_no_hidden_files() {
        assert_eq!(sanitize_download_filename(".bashrc"), "bashrc");
        assert_eq!(sanitize_download_filename("..config"), "config");
    }

    #[test]
    fn empty_dots_and_slashes_fall_back() {
        assert_eq!(sanitize_download_filename(""), FALLBACK_DOWNLOAD_FILENAME);
        assert_eq!(
            sanitize_download_filename("..."),
            FALLBACK_DOWNLOAD_FILENAME
        );
        assert_eq!(
            sanitize_download_filename("///"),
            FALLBACK_DOWNLOAD_FILENAME
        );
    }

    #[test]
    fn removes_control_and_bidi_characters() {
        assert_eq!(
            sanitize_download_filename("fi\u{202E}le\u{0007}\u{000A}.txt"),
            "file.txt"
        );
    }

    #[test]
    fn truncates_to_128_chars() {
        let long = "a".repeat(200);
        assert_eq!(sanitize_download_filename(&long).chars().count(), 128);
    }

    #[test]
    fn default_download_dir_is_not_empty() {
        assert!(!default_download_dir().as_os_str().is_empty());
    }

    #[test]
    fn destination_is_under_detects_profile_roots() {
        let root = unique_temp_root("roots");
        let profile = root.join("config").join("liteweb");
        std::fs::create_dir_all(&profile).unwrap();
        let elsewhere = root.join("downloads");
        std::fs::create_dir_all(&elsewhere).unwrap();

        let roots = vec![profile.clone()];
        assert!(destination_is_under(&profile.join("liteweb.db"), &roots));
        assert!(destination_is_under(
            &profile.join("sub").join("x.bin"),
            &roots
        ));
        assert!(!destination_is_under(&elsewhere.join("file.bin"), &roots));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn destination_is_under_resists_symlinked_parents() {
        let root = unique_temp_root("symlink");
        let profile = root.join("liteweb");
        std::fs::create_dir_all(&profile).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&profile, &alias).unwrap();

        let roots = vec![profile];
        assert!(destination_is_under(&alias.join("liteweb.db"), &roots));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plain_tmp_path_is_not_forbidden() {
        assert!(!is_forbidden_download_destination(Path::new(
            "/tmp/liteweb-plain-download.bin"
        )));
    }
}
