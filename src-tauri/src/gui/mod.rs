//! Desktop window (Tauri) that configures and launches the console bots.

mod launcher;
mod updater;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::{accounts, chromium, paths, sysenv};

#[derive(Clone, serde::Serialize)]
struct SetupProgress {
    fraction: f64,
    text: String,
    done: bool,
    error: Option<String>,
}

fn cfg_str(config: &Value, key: &str, default: &str) -> String {
    match &config[key] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => default.to_owned(),
    }
}

fn cfg_bool(config: &Value, key: &str, default: bool) -> bool {
    match &config[key] {
        Value::Bool(b) => *b,
        Value::String(s) => s.eq_ignore_ascii_case("true"),
        _ => default,
    }
}

/// Translate a GUI card id + its saved settings into `bot <command> --key=value ...`.
fn bot_args(id: &str, config: &Value) -> Option<(Vec<String>, bool)> {
    let s = |k: &str, d: &str| cfg_str(config, k, d);
    let b = |k: &str, d: bool| cfg_bool(config, k, d).to_string();
    let (command, opts): (&str, Vec<(&str, String)>) = match id {
        "create-account" => (
            "create",
            vec![
                ("username-prefix", s("usernamePrefix", "Cerolingo")),
                ("email-prefix", s("emailPrefix", "cero")),
                ("email-domain", s("emailDomain", "lingo.com")),
                ("qualify", b("qualify", true)),
                ("loop", b("loop", false)),
                ("hidden", b("hidden", false)),
            ],
        ),
        "farm" => (
            "farm",
            vec![
                ("lessons", s("lessonsPerAccount", "9")),
                ("legit", b("legit", false)),
                ("loop", b("loop", false)),
                ("hidden", b("hidden", false)),
            ],
        ),
        "solver" => (
            "solve",
            vec![
                ("account", s("accountIndex", "1")),
                ("auto-solver", b("autoSolver", true)),
                ("bot-highlight", b("botHighlight", true)),
                ("audio-block", b("audioBlock", true)),
                ("story-highlight", b("storyHighlight", true)),
                ("legendary-bypass", b("legendaryBypass", true)),
                ("lessons", s("lessons", "5")),
                ("hidden", b("hidden", false)),
            ],
        ),
        _ => return None,
    };
    let mut args = vec![command.to_owned()];
    args.extend(opts.into_iter().map(|(k, v)| format!("--{k}={}", v.trim())));
    let needs_browser = id != "farm" || cfg_bool(config, "legit", false);
    Some((args, needs_browser))
}

#[tauri::command]
async fn run_bot(app: AppHandle, id: String, config: Value) -> Result<(), String> {
    if id == "manage-accounts" {
        let path = accounts::ensure_file().map_err(|e| format!("Cannot create accounts.txt: {e}"))?;
        return sysenv::open_detached(&path).map_err(|e| format!("Cannot open {}: {e}", path.display()));
    }
    let (mut args, needs_browser) = bot_args(&id, &config).ok_or_else(|| format!("Unknown action: {id}"))?;
    if needs_browser {
        let emit = |p: SetupProgress| {
            let _ = app.emit("setup-progress", p);
        };
        let exe = match chromium::find_installed() {
            Some(exe) => exe,
            None => {
                let result = chromium::ensure(&|fraction, text| {
                    emit(SetupProgress { fraction, text: text.to_owned(), done: false, error: None })
                })
                .await;
                match result {
                    Ok(exe) => {
                        emit(SetupProgress { fraction: 1.0, text: "Browser ready".into(), done: true, error: None });
                        exe
                    }
                    Err(e) => {
                        let msg = format!("Could not set up a browser: {e:#}");
                        emit(SetupProgress { fraction: 0.0, text: msg.clone(), done: true, error: Some(msg.clone()) });
                        return Err(msg);
                    }
                }
            }
        };
        args.push(format!("--chrome={}", exe.display()));
    }
    launcher::spawn_bot(&args, cfg_bool(&config, "console", true)).map_err(|e| format!("Could not start the bot: {e}"))
}

#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only http(s) links can be opened".into());
    }
    sysenv::open_detached(&url).map_err(|e| e.to_string())
}

#[tauri::command]
fn app_version() -> &'static str {
    paths::APP_VERSION
}

#[tauri::command]
async fn install_update(app: AppHandle) -> String {
    updater::install(app).await
}

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![run_bot, open_external, app_version, install_update])
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_title(&format!("CerolingoBM v{}", paths::APP_VERSION));
            }
            updater::start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("{} failed to start: {e}", paths::APP_NAME);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_cards_to_bot_args() {
        let (args, browser) = bot_args("farm", &json!({"lessonsPerAccount": 3, "legit": false, "loop": "true"})).unwrap();
        assert_eq!(args, ["farm", "--lessons=3", "--legit=false", "--loop=true", "--hidden=false"]);
        assert!(!browser);
        let (args, browser) = bot_args("solver", &json!({"accountIndex": "0"})).unwrap();
        assert_eq!(args[0], "solve");
        assert!(args.contains(&"--account=0".to_owned()));
        assert!(browser);
        assert!(bot_args("nope", &json!({})).is_none());
    }
}
