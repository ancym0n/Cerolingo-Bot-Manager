//! Mod menu / auto solver bundle, built from the Cerolingo extension repository.
//!
//! The extension sources are downloaded from GitHub on every run (a few dozen KB), glued onto
//! a tiny `chrome.*` bridge (bridge.js) and cached on disk, so the bot always runs the newest
//! mod menu and still works offline from the last good copy.

use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};

use super::ui;
use crate::{net, paths};

/// Order matters: audio hooks first, menu last (matches the extension's runtime behaviour).
const MODULES: [&str; 8] = ["audio", "main", "solver", "challenges", "bypass", "highlight", "xp", "menu"];
const CSS_PATH: &str = "src/styles/content.css";
const ASSET_PATHS: [&str; 1] = ["assets/title.svg"];
const MANIFEST_PATH: &str = "manifest.json";
const BRIDGE: &str = include_str!("bridge.js");
/// Bot Manager tweaks layered on top of the upstream modules. Each one is optional: if upstream
/// changes the matched text the patch is skipped and the stock code runs.
const PATCHES_JSON: &str = include_str!("patches.json");

#[derive(Deserialize)]
#[serde(untagged)]
enum Patch {
    Literal { old: String, new: String },
    /// `await sleep(N);` -> `await stepSleep(N);` for each N.
    SleepToStepSleep { sleep_to_step_sleep: Vec<u32> },
}

pub fn repo() -> String {
    std::env::var("CBM_SOLVER_REPO").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "ancym0n/Cerolingo".into())
}

fn raw_base() -> String {
    let git_ref = std::env::var("CBM_SOLVER_REF").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "HEAD".into());
    format!("https://raw.githubusercontent.com/{}/{git_ref}/", repo())
}

/// Apply the tweaks for module `name`; returns the new text and how many were skipped.
fn apply_patches(name: &str, mut text: String) -> (String, usize) {
    let all: HashMap<String, Vec<Patch>> = serde_json::from_str(PATCHES_JSON).expect("valid patches.json");
    let mut skipped = 0;
    for patch in all.get(name).into_iter().flatten() {
        match patch {
            Patch::Literal { old, new } => {
                if !text.contains(old.as_str()) {
                    skipped += usize::from(!text.contains(new.as_str())); // already applied upstream -> no-op
                    continue;
                }
                text = text.replace(old.as_str(), new);
            }
            Patch::SleepToStepSleep { sleep_to_step_sleep } => {
                let mut count = 0;
                for n in sleep_to_step_sleep {
                    let old = format!("await sleep({n});");
                    count += text.matches(&old).count();
                    text = text.replace(&old, &format!("await stepSleep({n});"));
                }
                skipped += usize::from(count == 0);
            }
        }
    }
    (text, skipped)
}

async fn fetch(base: &str, path: &str) -> Result<Vec<u8>> {
    let url = format!("{base}{path}");
    let mut last = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        match net::client().get(&url).timeout(Duration::from_secs(15)).send().await {
            Ok(res) if res.status().is_success() => match res.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last = Some(anyhow!(e)),
            },
            Ok(res) => {
                let status = res.status().as_u16();
                last = Some(anyhow!("HTTP {status} for {path}"));
                if (400..500).contains(&status) {
                    break; // not found / forbidden: retrying won't help
                }
            }
            Err(e) => last = Some(anyhow!(e)),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("download failed")))
}

async fn download() -> Result<HashMap<String, Vec<u8>>> {
    let base = raw_base();
    let paths: Vec<String> = MODULES
        .iter()
        .map(|m| format!("src/content/{m}.js"))
        .chain([CSS_PATH, MANIFEST_PATH].map(String::from))
        .chain(ASSET_PATHS.map(String::from))
        .collect();
    let blobs = futures_util::future::try_join_all(paths.iter().map(|p| fetch(&base, p))).await?;
    Ok(paths.into_iter().zip(blobs).collect())
}

fn text(files: &HashMap<String, Vec<u8>>, path: &str) -> Result<String> {
    let bytes = files.get(path).ok_or_else(|| anyhow!("missing {path}"))?;
    String::from_utf8(bytes.clone()).with_context(|| format!("{path} is not UTF-8"))
}

/// Returns (bundle, version, warnings).
fn build_bundle(files: &HashMap<String, Vec<u8>>) -> Result<(String, String, Vec<String>)> {
    let manifest: Value = serde_json::from_str(&text(files, MANIFEST_PATH)?).context("bad manifest.json")?;
    let version = match &manifest["version"] {
        Value::String(s) => s.clone(),
        Value::Null => "0".into(),
        other => other.to_string(),
    };
    let css = text(files, CSS_PATH)?;
    let mut assets = serde_json::Map::new();
    for path in ASSET_PATHS {
        let mime = if path.ends_with(".svg") { "image/svg+xml" } else { "image/png" };
        let data = base64::engine::general_purpose::STANDARD.encode(files.get(path).ok_or_else(|| anyhow!("missing {path}"))?);
        assets.insert(path.into(), Value::String(format!("data:{mime};base64,{data}")));
    }

    let mut warnings = Vec::new();
    let mut modules = Vec::with_capacity(MODULES.len());
    for name in MODULES {
        let src = text(files, &format!("src/content/{name}.js"))?.replace("\r\n", "\n");
        let (patched, skipped) = apply_patches(name, src.trim_end().to_owned());
        if skipped > 0 {
            warnings.push(format!("{name}.js: {skipped} tweak(s) no longer match upstream and were skipped"));
        }
        modules.push(format!("// --- src/content/{name}.js ---\n{patched}\n"));
    }
    let modules = modules.join("\n");

    for marker in ["/*__ASSETS__*/", "/*__CSS__*/", "/*__VERSION__*/", "/*__MODULES__*/"] {
        if !BRIDGE.contains(marker) {
            bail!("bridge.js is missing placeholder {marker}");
        }
    }
    let bundle = BRIDGE
        .replacen("/*__ASSETS__*/ {}", &Value::Object(assets).to_string(), 1)
        .replacen("/*__CSS__*/ \"\"", &json!(css).to_string(), 1)
        .replacen("/*__VERSION__*/ \"0\"", &json!(version).to_string(), 1)
        .replacen("/*__MODULES__*/", &modules, 1);
    Ok((bundle, version, warnings))
}

fn read_cache() -> Option<(String, Value)> {
    let dir = paths::cache_dir();
    let bundle = std::fs::read_to_string(dir.join("solver-bundle.js")).ok()?;
    let meta = std::fs::read(dir.join("solver-meta.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    Some((bundle, meta))
}

fn write_cache(bundle: &str, meta: &Value) -> std::io::Result<()> {
    let dir = paths::cache_dir();
    paths::write_atomic(&dir.join("solver-bundle.js"), bundle.as_bytes())?;
    paths::write_atomic(&dir.join("solver-meta.json"), meta.to_string().as_bytes())
}

/// The solver bundle source, downloading the newest extension code when online.
pub async fn load_bundle(quiet: bool) -> Result<String> {
    let cached = read_cache();
    let started = std::time::Instant::now();
    let fresh = async { build_bundle(&download().await?) }.await;
    match fresh {
        Ok((bundle, version, warnings)) => {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let meta = json!({ "version": version, "repo": repo(), "fetched": now });
            if let Err(e) = write_cache(&bundle, &meta)
                && !quiet
            {
                ui::warn(&format!("Could not cache the mod menu: {e}"));
            }
            if !quiet {
                for w in &warnings {
                    ui::warn(w);
                }
                match &cached {
                    None => ui::ok(&format!(
                        "Mod menu v{version} downloaded from {} ({:.1}s)",
                        repo(),
                        started.elapsed().as_secs_f64()
                    )),
                    Some((old, meta)) if *old != bundle => {
                        let old_v = meta["version"].as_str().unwrap_or("?");
                        ui::ok(&format!("Mod menu updated: v{old_v} -> v{version}"));
                    }
                    Some(_) => ui::ok(&format!("Mod menu v{version} is up to date")),
                }
            }
            Ok(bundle)
        }
        Err(err) => match cached {
            Some((bundle, meta)) if !bundle.is_empty() => {
                if !quiet {
                    ui::warn(&format!(
                        "Could not reach {} ({}); using cached mod menu v{}",
                        repo(),
                        ui::first_line(&err),
                        meta["version"].as_str().unwrap_or("?")
                    ));
                }
                Ok(bundle)
            }
            _ => Err(anyhow!(
                "Could not download the mod menu from github.com/{} and no cached copy exists: {err:#}",
                repo()
            )),
        },
    }
}

/// Bundle prefixed with the settings the Bot Manager wants the mod menu to start with.
pub async fn build_init_script(settings: &Value, quiet: bool) -> Result<String> {
    Ok(format!("window.__CEROLINGO_CONFIG__ = {settings};\n{}", load_bundle(quiet).await?))
}

pub fn default_settings() -> Value {
    json!({
        "autoSolver": true, "botHighlight": true, "audioBlock": true,
        "storyHighlight": true, "legendaryBypass": true, "fastMode": true,
    })
}

pub async fn update_cli() -> i32 {
    ui::banner("Mod menu update", "");
    match load_bundle(false).await {
        Ok(text) => {
            let file = paths::cache_dir().join("solver-bundle.js");
            ui::info(&format!("Bundle size: {:.1} KB  ({})", text.len() as f64 / 1024.0, file.display()));
            0
        }
        Err(e) => {
            ui::error(&format!("{e:#}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_parse_and_apply() {
        let src = "const sleep = (ms) => new Promise((r) => setTimeout(r, ms));\nawait sleep(55);\nawait sleep(100);\n";
        let (out, skipped) = apply_patches("challenges", src.to_owned());
        assert!(out.contains("const stepSleep"));
        assert!(out.contains("await stepSleep(55);") && out.contains("await stepSleep(100);"));
        assert!(!out.contains("await sleep(55);"));
        assert!(skipped > 0, "tweaks that don't match this tiny source are reported");
    }

    #[test]
    fn bundle_fills_bridge() {
        let mut files: HashMap<String, Vec<u8>> = MODULES
            .iter()
            .map(|m| (format!("src/content/{m}.js"), format!("/*{m}*/\r\n").into_bytes()))
            .collect();
        files.insert(CSS_PATH.into(), b"a{color:\"red\"}".to_vec());
        files.insert(MANIFEST_PATH.into(), br#"{"version":"2.3.4"}"#.to_vec());
        files.insert("assets/title.svg".into(), b"<svg/>".to_vec());
        let (bundle, version, _) = build_bundle(&files).unwrap();
        assert_eq!(version, "2.3.4");
        assert!(bundle.contains(r#"version: "2.3.4""#));
        assert!(bundle.contains(r#""a{color:\"red\"}""#));
        assert!(bundle.contains("data:image/svg+xml;base64,"));
        assert!(bundle.contains("// --- src/content/menu.js ---\n/*menu*/\n"));
        assert!(!bundle.contains("/*__MODULES__*/"));
    }
}
