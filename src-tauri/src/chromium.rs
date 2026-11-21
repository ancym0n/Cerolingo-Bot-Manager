//! Chromium for the bots: reuse an installed copy or download Chrome for Testing once.

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::{net, paths};

const CFT_INDEX: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";
const MARKER: &str = "ready.json";

#[derive(Serialize, Deserialize)]
struct Marker {
    version: String,
    /// Executable path relative to the browsers dir.
    exe: PathBuf,
}

/// Serialises downloads inside one process (the GUI may be asked twice in a row).
static INSTALL_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

fn cft_platform() -> Option<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux64"),
        ("windows", "x86_64") => Some("win64"),
        ("windows", "x86") => Some("win32"),
        ("macos", "aarch64") => Some("mac-arm64"),
        ("macos", "x86_64") => Some("mac-x64"),
        _ => None,
    }
}

fn cft_exe(platform: &str) -> PathBuf {
    let dir = PathBuf::from(format!("chrome-{platform}"));
    match platform {
        p if p.starts_with("win") => dir.join("chrome.exe"),
        p if p.starts_with("mac") => dir
            .join("Google Chrome for Testing.app")
            .join("Contents/MacOS/Google Chrome for Testing"),
        _ => dir.join("chrome"),
    }
}

fn managed() -> Option<PathBuf> {
    let root = paths::browsers_dir();
    let marker: Marker = serde_json::from_slice(&fs::read(root.join(MARKER)).ok()?).ok()?;
    Some(root.join(marker.exe)).filter(|p| p.is_file())
}

/// Sub-directories of `base` whose name starts with `prefix`, newest (highest name) first.
fn dirs_with_prefix(base: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort_by(|a, b| b.cmp(a));
    found
}

/// Chromium builds downloaded by Playwright (older app versions, or a dev install).
fn playwright_candidates() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(p) = env::var_os("PLAYWRIGHT_BROWSERS_PATH").filter(|v| !v.is_empty() && v != "0") {
        roots.push(p.into());
    }
    roots.push(paths::data_dir().join("runtime").join("browsers"));
    roots.extend(paths::legacy_dirs().into_iter().map(|d| d.join("runtime").join("browsers")));
    if cfg!(windows) {
        if let Some(d) = dirs::data_local_dir() {
            roots.push(d.join("ms-playwright"));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(h) = dirs::home_dir() {
            roots.push(h.join("Library/Caches/ms-playwright"));
        }
    } else if let Some(h) = dirs::home_dir() {
        roots.push(h.join(".cache/ms-playwright"));
    }

    let (sub_prefix, exe): (&str, &str) = if cfg!(windows) {
        ("chrome-win", "chrome.exe")
    } else if cfg!(target_os = "macos") {
        ("chrome-mac", "Chromium.app/Contents/MacOS/Chromium")
    } else {
        ("chrome-linux", "chrome")
    };
    roots
        .iter()
        .flat_map(|root| dirs_with_prefix(root, "chromium-"))
        .flat_map(|build| dirs_with_prefix(&build, sub_prefix))
        .map(|dir| dir.join(exe))
        .collect()
}

/// System-wide Chrome / Chromium / Edge / Brave (fallback when downloading is impossible).
pub fn find_system() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for key in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(base) = env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from) {
                candidates.push(base.join(r"Google\Chrome\Application\chrome.exe"));
                candidates.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
            }
        }
    } else if cfg!(target_os = "macos") {
        candidates.push("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into());
        candidates.push("/Applications/Chromium.app/Contents/MacOS/Chromium".into());
    } else {
        for name in ["google-chrome-stable", "google-chrome", "chromium", "chromium-browser", "brave-browser"] {
            if let Some(path) = crate::sysenv::find_executable(name) {
                // Snap-confined browsers cannot read our profile / DevTools port file.
                let real = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if !real.starts_with("/snap") && !path.starts_with("/snap") {
                    candidates.push(path);
                }
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// A browser that is ready to use without downloading anything (system browsers excluded).
pub fn find_installed() -> Option<PathBuf> {
    if let Some(p) = env::var_os("CBM_CHROME").filter(|v| !v.is_empty()).map(PathBuf::from)
        && p.is_file()
    {
        return Some(p);
    }
    managed().or_else(|| playwright_candidates().into_iter().find(|p| p.is_file()))
}

/// Return a usable browser, downloading Chrome for Testing when none is installed.
/// `progress(fraction, text)` reports download / extraction status.
pub async fn ensure(progress: &(dyn Fn(f64, &str) + Send + Sync)) -> Result<PathBuf> {
    if let Some(p) = find_installed() {
        return Ok(p);
    }
    let _guard = INSTALL_LOCK.lock().await;
    if let Some(p) = find_installed() {
        return Ok(p);
    }
    match install(progress).await {
        Ok(p) => Ok(p),
        Err(err) => find_system().ok_or_else(|| anyhow!("{err:#}")).inspect(|_| {
            progress(1.0, "Using the browser installed on this computer");
        }),
    }
}

async fn install(progress: &(dyn Fn(f64, &str) + Send + Sync)) -> Result<PathBuf> {
    let platform = cft_platform().ok_or_else(|| anyhow!("no Chromium build for {}", env::consts::ARCH))?;
    progress(0.01, "Looking up the newest Chromium...");
    let index = net::get_json(CFT_INDEX).await.context("cannot reach Chrome for Testing")?;
    let stable = &index["channels"]["Stable"];
    let version = stable["version"].as_str().ok_or_else(|| anyhow!("bad Chrome for Testing index"))?.to_owned();
    let url = stable["downloads"]["chrome"]
        .as_array()
        .and_then(|list| list.iter().find(|d| d["platform"] == platform))
        .and_then(|d| d["url"].as_str())
        .ok_or_else(|| anyhow!("no Chromium download for {platform}"))?
        .to_owned();

    let root = paths::browsers_dir();
    fs::create_dir_all(&root).with_context(|| format!("cannot create {}", root.display()))?;
    let pid = std::process::id();
    let archive = root.join(format!(".download-{pid}.zip"));
    let staging = root.join(format!(".extract-{pid}"));
    let _ = fs::remove_dir_all(&staging);

    let text = format!("Downloading Chromium {version}");
    let mut last = -1i64;
    let result = async {
        net::download(&url, &archive, |f| {
            let pct = (f * 100.0) as i64;
            if pct != last {
                last = pct;
                progress(0.02 + f * 0.83, &format!("{text}... {pct}%"));
            }
        })
        .await?;
        progress(0.87, "Extracting Chromium...");
        let (a, s) = (archive.clone(), staging.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut zip = zip::ZipArchive::new(fs::File::open(&a)?).context("corrupt Chromium archive")?;
            zip.extract(&s).context("cannot extract Chromium")?;
            Ok(())
        })
        .await??;
        let rel_dir = PathBuf::from(&version);
        let final_dir = root.join(&rel_dir);
        if !final_dir.join(cft_exe(platform)).is_file() {
            let _ = fs::remove_dir_all(&final_dir);
            fs::rename(&staging, &final_dir).context("cannot move Chromium into place")?;
        }
        let exe = rel_dir.join(cft_exe(platform));
        if !root.join(&exe).is_file() {
            bail!("Chromium executable missing after extraction");
        }
        let marker = serde_json::to_vec(&Marker { version: version.clone(), exe: exe.clone() })?;
        paths::write_atomic(&root.join(MARKER), &marker)?;
        Ok(root.join(exe))
    }
    .await;

    let _ = fs::remove_file(&archive);
    let _ = fs::remove_dir_all(&staging);
    let exe = result?;
    cleanup_old(&root, &version);
    progress(1.0, "Chromium is ready");
    Ok(exe)
}

/// Remove older Chromium versions and the Python/Playwright runtime of the Electron build.
fn cleanup_old(root: &Path, keep: &str) {
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_version = name.chars().next().is_some_and(|c| c.is_ascii_digit());
        if entry.path().is_dir() && is_version && name != keep {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    for dir in std::iter::once(paths::data_dir()).chain(paths::legacy_dirs()) {
        let runtime = dir.join("runtime");
        if runtime.join("ready.json").is_file() {
            let _ = fs::remove_dir_all(runtime);
        }
    }
}
