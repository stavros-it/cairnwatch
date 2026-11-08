//! Portable path resolution: everything lives next to the exe.
//! No registry, no user-profile dirs, no external state.

use std::path::PathBuf;

/// The folder containing the running exe. All app data derives from here.
pub fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_file() -> PathBuf {
    app_dir().join("config.toml")
}

pub fn repos_file() -> PathBuf {
    app_dir().join("repos.json")
}

/// In-flight download directory (temp `.part` files live here).
pub fn tmp_dir() -> PathBuf {
    app_dir().join("tmp")
}

/// Delete stale files left in `dir` by interrupted downloads. Regular
/// files only; subfolders are left alone. Returns the number removed.
pub fn cleanup_tmp(dir: &std::path::Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

/// Default download destination (the user may override in config).
pub fn default_download_dir() -> PathBuf {
    app_dir().join("downloads")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_paths_live_under_app_dir() {
        let base = app_dir();
        assert!(config_file().starts_with(&base));
        assert!(repos_file().starts_with(&base));
        assert!(tmp_dir().starts_with(&base));
        assert!(default_download_dir().starts_with(&base));
    }

    #[test]
    fn cleanup_tmp_removes_files_keeps_dirs() {
        let d = std::env::temp_dir().join(format!("grs_tmp_test_{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.part"), b"x").unwrap();
        std::fs::write(d.join("b.zip"), b"x").unwrap();
        assert_eq!(cleanup_tmp(&d), 2);
        assert!(d.join("sub").is_dir());
        assert_eq!(cleanup_tmp(&d.join("missing")), 0);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn config_file_name_is_exact() {
        assert_eq!(config_file().file_name().unwrap(), "config.toml");
        assert_eq!(repos_file().file_name().unwrap(), "repos.json");
        assert_eq!(tmp_dir().file_name().unwrap(), "tmp");
        assert_eq!(default_download_dir().file_name().unwrap(), "downloads");
    }
}
