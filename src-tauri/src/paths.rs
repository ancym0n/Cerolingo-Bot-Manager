//! Where the app keeps its data. Everything can be redirected with `CBM_DATA_DIR`.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const APP_NAME: &str = "Cerolingo Bot Manager";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const LEGACY_DIR_NAMES: [&str; 2] = ["Cerolingo Bot Manager", "cerolingo-bot-manager"];

fn base(dir: Option<PathBuf>) -> PathBuf {
    dir.or_else(dirs::home_dir).unwrap_or_else(env::temp_dir)
}

/// Settings-like data (accounts, mod-menu cache). Same folder the Electron build used,
/// so existing `logs/accounts.txt` files keep working after the upgrade.
pub fn data_dir() -> PathBuf {
    match env::var_os("CBM_DATA_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => base(dirs::config_dir()).join(APP_NAME),
    }
}

/// Large machine-local downloads (Chromium). Kept out of roaming profiles on Windows.
pub fn browsers_dir() -> PathBuf {
    if let Some(dir) = env::var_os("CBM_BROWSERS_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if env::var_os("CBM_DATA_DIR").is_some_and(|v| !v.is_empty()) {
        return data_dir().join("browser");
    }
    base(dirs::data_local_dir()).join("cerolingo-bot-manager").join("browser")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn accounts_path() -> PathBuf {
    logs_dir().join("accounts.txt")
}

pub fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}

/// Data folders written by the Electron/Python versions (possibly under another name).
pub fn legacy_dirs() -> Vec<PathBuf> {
    let config = base(dirs::config_dir());
    LEGACY_DIR_NAMES.iter().map(|name| config.join(name)).collect()
}

/// Copy `accounts.txt` from an older install location if the current one has none.
pub fn migrate_legacy_data() {
    let target = accounts_path();
    if target.exists() {
        return;
    }
    for dir in legacy_dirs() {
        let old = dir.join("logs").join("accounts.txt");
        if old != target && old.is_file() {
            if fs::create_dir_all(logs_dir()).is_ok() {
                let _ = fs::copy(&old, &target);
            }
            return;
        }
    }
}

/// Write a file atomically (temp file + rename) so readers never see half a file.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}
