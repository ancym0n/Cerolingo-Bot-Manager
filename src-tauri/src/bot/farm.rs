//! Farm XP on every saved account.
//!
//! Legit mode: opens duolingo.com/lesson logged in as each account, lets the Cerolingo mod menu
//! solve real lessons and tracks lessons + XP (read before/after from Duolingo).
//! Fast mode (Legit off): completes practice sessions directly through the API.

use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};

use super::{
    Opts, browser_exe,
    cdp::{Browser, Options, StepError, WindowClosed, is_window_closed, sleep, wait_until},
    interruptible,
    lessons::{base_url, run_lessons},
    solver, ui,
};
use crate::{accounts, accounts::Account, net};

const API: &str = "https://www.duolingo.com/2017-06-30";
const XP_PER_LESSON: u64 = 13;
const CHALLENGE_TYPES: [&str; 57] = [
    "assist", "characterIntro", "characterMatch", "characterPuzzle", "characterSelect", "characterTrace",
    "characterWrite", "completeReverseTranslation", "definition", "dialogue", "extendedMatch",
    "extendedListenMatch", "form", "freeResponse", "gapFill", "judge", "listen", "listenComplete", "listenMatch",
    "match", "name", "listenComprehension", "listenIsolation", "listenSpeak", "listenTap", "orderTapComplete",
    "partialListen", "partialReverseTranslate", "patternTapComplete", "radioBinary", "radioImageSelect",
    "radioListenMatch", "radioListenRecognize", "radioSelect", "readComprehension", "reverseAssist",
    "sameDifferent", "select", "selectPronunciation", "selectTranscription", "svgPuzzle", "syllableTap",
    "syllableListenTap", "speak", "tapCloze", "tapClozeTable", "tapComplete", "tapCompleteTable", "tapDescribe",
    "translate", "transliterate", "transliterationAssist", "typeCloze", "typeClozeTable", "typeComplete",
    "typeCompleteTable", "writeComprehension",
];
const TOTAL_XP_JS: &str = r#"async ([id, jwt]) => {
    try {
        const r = await fetch(`/2017-06-30/users/${id}?fields=totalXp`, {
            headers: { Authorization: `Bearer ${jwt}` }, credentials: 'include' });
        return r.ok ? ((await r.json()).totalXp ?? null) : null;
    } catch (_) { return null; }
}"#;

fn lessons_word(n: usize) -> String {
    format!("{n} lesson{}", if n == 1 { "" } else { "s" })
}

struct Settings {
    lessons: usize,
    legit: bool,
    looping: bool,
    hidden: bool,
}

// -- fast mode (API) ------------------------------------------------------------------------

/// Duolingo refused the API request (HTTP 401/403).
struct Blocked(u16);

enum ApiError {
    Blocked(Blocked),
    Net(reqwest::Error),
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Net(e)
    }
}

fn request(method: reqwest::Method, url: &str, jwt: &str) -> reqwest::RequestBuilder {
    net::client()
        .request(method, url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {jwt}"))
        .header("User-Agent", "Mozilla/5.0")
        .timeout(Duration::from_secs(30))
}

fn check_blocked(status: reqwest::StatusCode) -> Result<(), ApiError> {
    match status.as_u16() {
        code @ (401 | 403) => Err(ApiError::Blocked(Blocked(code))),
        _ => Ok(()),
    }
}

async fn user_info(jwt: &str) -> Result<(String, String), ApiError> {
    let uid = accounts::user_id(jwt).unwrap_or_default();
    let res =
        request(reqwest::Method::GET, &format!("{API}/users/{uid}?fields=fromLanguage,learningLanguage"), jwt).send().await?;
    check_blocked(res.status())?;
    let data: Value = if res.status().is_success() { res.json().await.unwrap_or(Value::Null) } else { Value::Null };
    let pick = |k: &str, d: &str| data[k].as_str().filter(|s| !s.is_empty()).unwrap_or(d).to_owned();
    Ok((pick("fromLanguage", "en"), pick("learningLanguage", "es")))
}

async fn run_api_lesson(jwt: &str, from: &str, learn: &str) -> Result<bool, ApiError> {
    let body = json!({
        "challengeTypes": CHALLENGE_TYPES.as_slice(), "fromLanguage": from, "isFinalLevel": false, "isV2": true,
        "juicy": true, "learningLanguage": learn, "smartTipsVersion": 2, "type": "GLOBAL_PRACTICE",
    });
    let res = request(reqwest::Method::POST, &format!("{API}/sessions"), jwt).body(body.to_string()).send().await?;
    check_blocked(res.status())?;
    if res.status() != reqwest::StatusCode::OK {
        return Ok(false);
    }
    let Ok(mut session) = res.json::<Value>().await else { return Ok(false) };
    let id = match &session["id"] {
        Value::String(s) if !s.is_empty() => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Ok(false),
    };
    let now = chrono::Utc::now().timestamp();
    if let Some(obj) = session.as_object_mut() {
        obj.insert("heartsLeft".into(), json!(0));
        obj.insert("startTime".into(), json!(now - 60));
        obj.insert("enableBonusPoints".into(), json!(true));
        obj.insert("endTime".into(), json!(now));
        obj.insert("failed".into(), json!(false));
        obj.insert("maxInLessonStreak".into(), json!(9));
        obj.insert("shouldLearnThings".into(), json!(true));
    }
    let res = request(reqwest::Method::PUT, &format!("{API}/sessions/{id}"), jwt).body(session.to_string()).send().await?;
    Ok(res.status().is_success())
}

async fn farm_api(account: &Account, s: &Settings) -> (usize, u64) {
    let jwt = &account.jwt_token;
    let result: Result<usize, Blocked> = async {
        let (from, learn) = match user_info(jwt).await {
            Ok(v) => v,
            Err(ApiError::Blocked(b)) => return Err(b),
            Err(ApiError::Net(e)) => {
                ui::warn(&format!("network error: {}", short_net(&e)));
                ("en".into(), "es".into())
            }
        };
        ui::dim(&format!("course: {learn} (from {from})"));
        let mut ok = 0;
        for n in 1..=s.lessons {
            match run_api_lesson(jwt, &from, &learn).await {
                Ok(true) => ok += 1,
                Ok(false) => {}
                Err(ApiError::Blocked(b)) => return Err(b),
                Err(ApiError::Net(e)) => {
                    ui::progress_done();
                    ui::warn(&format!("network error: {}", short_net(&e)));
                }
            }
            ui::progress("XP", n, s.lessons, &format!("+{} xp", ok as u64 * XP_PER_LESSON));
        }
        Ok(ok)
    }
    .await;
    ui::progress_done();
    match result {
        Err(Blocked(code)) => {
            ui::error(&format!("Duolingo blocked the API request (HTTP {code}) - turn on Legit mode instead"));
            (0, 0)
        }
        Ok(ok) => {
            let xp = ok as u64 * XP_PER_LESSON;
            if ok > 0 {
                ui::ok(&format!("{ok}/{} completed (~{xp} xp)", lessons_word(s.lessons)));
            } else {
                ui::error("No lesson was accepted - is the token expired?");
            }
            (ok, xp)
        }
    }
}

fn short_net(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connection failed"
    } else if e.is_decode() || e.is_body() {
        "bad response"
    } else {
        "request failed"
    }
}

// -- legit mode (real lessons in the browser) -----------------------------------------------

async fn total_xp(page: &Browser, account: &Account) -> Option<i64> {
    let uid = accounts::user_id(&account.jwt_token)?;
    page.call(TOTAL_XP_JS, &[json!([uid, account.jwt_token])]).await.ok()?.as_i64()
}

async fn farm_legit(exe: &std::path::Path, account: &Account, init: &str, s: &Settings) -> Result<(usize, u64)> {
    let page =
        Browser::launch(exe, &Options { size: (1280, 820), mute: true, headless: s.hidden, init_script: Some(init) })
            .await?;
    let result = farm_legit_in(&page, account, init, s).await;
    page.close().await;
    result
}

async fn farm_legit_in(page: &Browser, account: &Account, init: &str, s: &Settings) -> Result<(usize, u64)> {
    page.set_cookies(accounts::browser_cookies(account)).await?;
    let learn = format!("{}/learn", base_url());
    page.goto(&learn).await?;
    if !wait_until(10.0, 0.25, async || page.is_closed() || page.logged_in().await).await || page.is_closed() {
        if page.is_closed() {
            return Err(WindowClosed.into());
        }
        ui::error("Saved token no longer works - skipping this account");
        return Ok((0, 0));
    }
    let before = total_xp(page, account).await;
    ui::dim(&format!("total xp: {}", before.map_or("?".into(), |v| v.to_string())));
    let done = match run_lessons(page, init, s.lessons, "Lessons").await {
        Ok(n) => n,
        Err(e) => match e.downcast::<StepError>() {
            Ok(step) => {
                ui::error(&step.msg);
                step.done
            }
            Err(e) => return Err(e),
        },
    };
    page.goto(&learn).await?;
    let after = total_xp(page, account).await;
    let gained = before.zip(after).map(|(b, a)| a - b);
    let xp_text = match (gained, after) {
        (Some(g), Some(a)) => format!("+{g} xp, total {a}"),
        _ => "xp unknown".into(),
    };
    let msg = format!("{done}/{} on /lesson ({xp_text})", lessons_word(s.lessons));
    if done > 0 { ui::ok(&msg) } else { ui::error(&msg) }
    Ok((done, gained.unwrap_or(0).max(0) as u64))
}

// -- main -----------------------------------------------------------------------------------

pub async fn run(opts: &Opts) -> i32 {
    let s = Settings {
        lessons: opts.int("lessons", 9).max(1) as usize,
        legit: opts.bool("legit", false),
        looping: opts.bool("loop", false),
        hidden: opts.bool("hidden", false),
    };
    ui::set_title("XP Farm");
    let mode = if s.legit { "Legit: real lessons on duolingo.com/lesson" } else { "Fast: practice sessions via the API" };
    ui::banner("XP Farm", mode);
    let mut rows = vec![
        ("Lessons per account", s.lessons.to_string().into()),
        ("Legit mode", s.legit.into()),
        ("Loop", s.looping.into()),
    ];
    if s.legit {
        rows.push(("Hidden browser", s.hidden.into()));
    }
    ui::config(&rows);

    let mut legit_env = None;
    if s.legit {
        ui::section("Mod menu");
        let init = match solver::build_init_script(&solver::default_settings(), false).await {
            Ok(v) => v,
            Err(e) => {
                ui::error(&format!("{e:#}"));
                return 1;
            }
        };
        let exe = match browser_exe(opts).await {
            Ok(p) => p,
            Err(e) => {
                ui::error(&format!("No browser available: {e:#}"));
                return 1;
            }
        };
        legit_env = Some((exe, init));
    }

    let (mut cycles, mut lessons, mut xp) = (0usize, 0usize, 0u64);
    let work = async {
        loop {
            let list = accounts::load();
            if list.is_empty() {
                ui::warn("No valid accounts found in accounts.txt (create some first)");
                break;
            }
            ui::section(&format!("Cycle {}: {} account(s)", cycles + 1, list.len()));
            let mut errors = 0;
            for (i, account) in list.iter().enumerate() {
                ui::step(i + 1, list.len(), &account.email);
                let (done, gained) = match &legit_env {
                    None => farm_api(account, &s).await,
                    Some((exe, init)) => match farm_legit(exe, account, init, &s).await {
                        Ok(v) => v,
                        Err(e) if is_window_closed(&e) => {
                            ui::progress_done();
                            ui::warn("Browser window closed - stopping");
                            return;
                        }
                        Err(e) => {
                            ui::progress_done();
                            ui::error(&ui::first_line(&e));
                            errors += 1;
                            sleep(2.0).await;
                            (0, 0)
                        }
                    },
                };
                lessons += done;
                xp += gained;
            }
            cycles += 1;
            if !s.looping {
                break;
            }
            if errors == list.len() {
                ui::error("Every account failed in this cycle - stopping");
                break;
            }
        }
    };
    if interruptible(work).await.is_none() {
        ui::progress_done();
        ui::warn("Interrupted");
    }

    let xp_text = if s.legit { xp.to_string() } else { format!("~{xp}") };
    ui::summary(
        "Summary",
        &[("Cycles", cycles.to_string().into()), ("Lessons", lessons.to_string().into()), ("XP gained", xp_text.into())],
    );
    ui::farewell("XP farm finished");
    0
}
