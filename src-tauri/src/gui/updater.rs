//! Optional auto-update: checks the newest GitHub release and offers it. Never forced.

use std::{sync::Mutex, time::Duration};

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::{net, paths, sysenv};

const REPO: &str = "ancym0n/Cerolingo-Bot-Manager";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone)]
struct Asset {
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    name: String,
    url: String,
    digest: String,
}

#[derive(Clone)]
struct Release {
    version: String,
    page: String,
    asset: Option<Asset>,
}

static LATEST: Mutex<Option<Release>> = Mutex::new(None);
static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn strip_prefix(v: &str) -> &str {
    let v = v.trim();
    let v = if v.len() >= 5 && v[..5].eq_ignore_ascii_case("beta-") { &v[5..] } else { v };
    v.strip_prefix(['v', 'V']).unwrap_or(v)
}

/// `1.2.3[-pre]` (optionally prefixed with `v` / `beta-v`) -> ([1,2,3], "pre").
fn parse(v: &str) -> Option<([u64; 3], String)> {
    let v = strip_prefix(v);
    let mut nums = [0u64; 3];
    let mut rest = v;
    for (i, n) in nums.iter_mut().enumerate() {
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        *n = rest[..end].parse().ok()?;
        rest = &rest[end..];
        if i < 2 {
            rest = rest.strip_prefix('.')?;
        }
    }
    let pre = rest
        .strip_prefix('-')
        .map(|p| p.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.').collect())
        .unwrap_or_default();
    Some((nums, pre))
}

/// True when `a` is newer than `b` (pre-releases rank below their final release).
fn is_newer(a: &str, b: &str) -> bool {
    let (Some((x, xp)), Some((y, yp))) = (parse(a), parse(b)) else { return false };
    if x != y {
        return x > y;
    }
    xp.is_empty() && !yp.is_empty()
}

fn pick_asset(assets: &[Value]) -> Option<Asset> {
    let wanted: fn(&str) -> bool = if cfg!(windows) {
        |n| n.to_ascii_lowercase().ends_with("-setup.exe")
    } else if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some_and(|v| !v.is_empty()) {
        |n| n.to_ascii_lowercase().ends_with(".appimage")
    } else {
        return None; // dev / macOS builds: just open the release page
    };
    let a = assets.iter().find(|a| a["name"].as_str().is_some_and(wanted))?;
    Some(Asset {
        name: a["name"].as_str()?.to_owned(),
        url: a["browser_download_url"].as_str()?.to_owned(),
        digest: a["digest"].as_str().unwrap_or("").trim_start_matches("sha256:").to_owned(),
    })
}

async fn check() -> Option<Release> {
    let rel = net::get_json(&format!("https://api.github.com/repos/{REPO}/releases/latest")).await.ok()?;
    let tag = rel["tag_name"].as_str()?;
    if rel["draft"].as_bool() == Some(true)
        || rel["prerelease"].as_bool() == Some(true)
        || !is_newer(tag, paths::APP_VERSION)
    {
        return None;
    }
    Some(Release {
        version: strip_prefix(tag).to_owned(),
        page: rel["html_url"].as_str().unwrap_or("").to_owned(),
        asset: pick_asset(rel["assets"].as_array().map(Vec::as_slice).unwrap_or(&[])),
    })
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        loop {
            let found = check().await;
            if let Some(rel) = &found {
                let _ = app.emit("update-available", serde_json::json!({ "version": rel.version, "direct": rel.asset.is_some() }));
            }
            *LATEST.lock().unwrap() = found;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

fn open_page(page: &str) -> String {
    if page.starts_with("https://") {
        let _ = sysenv::open_detached(page);
    }
    "opened".into()
}

/// "opened" (release page shown) or "installing" (the app exits / restarts right after).
pub async fn install(app: AppHandle) -> String {
    use std::sync::atomic::Ordering;
    let Some(rel) = LATEST.lock().unwrap().clone() else { return "none".into() };
    let Some(asset) = rel.asset.clone().filter(|_| !cfg!(debug_assertions)) else { return open_page(&rel.page) };
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return "none".into();
    }
    match download_and_apply(&app, &asset).await {
        Ok(()) => "installing".into(),
        Err(_) => {
            INSTALLING.store(false, Ordering::SeqCst);
            open_page(&rel.page) // fall back to a manual download
        }
    }
}

async fn download_and_apply(app: &AppHandle, asset: &Asset) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    let (dest, target) = {
        let appimage = std::path::PathBuf::from(std::env::var_os("APPIMAGE").ok_or_else(|| anyhow::anyhow!("no APPIMAGE"))?);
        let mut dest = appimage.clone().into_os_string();
        dest.push(".download");
        (std::path::PathBuf::from(dest), appimage)
    };
    #[cfg(not(target_os = "linux"))]
    let dest = std::env::temp_dir().join(&asset.name);

    let mut last = -1i64;
    let result = net::download(&asset.url, &dest, |f| {
        let pct = (f * 100.0) as i64;
        if pct != last {
            last = pct;
            let _ = app.emit("update-progress", f);
        }
    })
    .await;
    let check = result.and_then(|sha| {
        if !asset.digest.is_empty() && !asset.digest.eq_ignore_ascii_case(&sha) {
            anyhow::bail!("checksum mismatch");
        }
        Ok(())
    });
    if let Err(e) = check {
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        let applied = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .and_then(|()| std::fs::rename(&dest, &target));
        if let Err(e) = applied {
            let _ = std::fs::remove_file(&dest);
            return Err(e.into());
        }
        // Relaunch after we're gone (avoids the single-instance check handing focus back to us).
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "sleep 1.5; exec \"$0\""]).arg(&target);
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        sysenv::clean_env(&mut cmd);
        cmd.spawn()?;
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::process::Command::new(&dest).spawn()?;
    }
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse("beta-v1.2.3-rc.1"), Some(([1, 2, 3], "rc.1".into())));
        assert_eq!(parse("v10.0.1"), Some(([10, 0, 1], String::new())));
        assert_eq!(parse("1.2"), None);
        assert!(is_newer("v1.1.2", "1.1.1"));
        assert!(is_newer("1.2.0", "1.1.9"));
        assert!(!is_newer("1.1.1", "1.1.1"));
        assert!(is_newer("1.1.1", "1.1.1-beta"));
        assert!(!is_newer("1.1.1-beta", "1.1.1"));
        assert!(parse("1.9.x").is_none());
        assert!(!is_newer("garbage", "1.0.0"));
    }
}
