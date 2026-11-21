//! Create Duolingo accounts, finish onboarding and (optionally) qualify for leaderboards.
//!
//! 1. duolingo.com -> "I already have an account" -> "Sign up" -> "How old are you?"
//! 2. "Create your profile" (username / email / password -> Create account)
//! 3. "What would you like to learn?" (pick a course -> Continue)
//! 4. "How much do you know?" (skipped: as soon as it shows up we leave it)
//! 5. duolingo.com/lesson -> the Cerolingo mod menu solves the lessons

use std::time::{Duration, Instant};

use anyhow::Result;

use super::{
    Opts, browser_exe,
    cdp::{Browser, Options, is_window_closed, sleep, step_error, wait_until},
    interruptible,
    lessons::{base_url, run_lessons},
    solver, ui,
};
use crate::{accounts, paths};

const AGE: &str = r#"[data-test="age-input"]"#;
const NEXT: &str = r#"[data-test="continue-button"]"#;
const USERNAME: &str = r#"[data-test="username-input"]"#;
const FULL_NAME: &str = r#"[data-test="full-name-input"]"#;
const EMAIL: &str = r#"[data-test="email-input"]"#;
const PASSWORD: &str = r#"[data-test="password-input"]"#;
const REGISTER: &str = r#"[data-test="register-button"]"#;
const SIGN_UP_TOGGLE: &str = r#"[data-test="sign-up-button"]"#;
const HAVE_ACCOUNT: &str = r#"[data-test="have-account"]"#;
const FUNBOARDING: &str = r#"[data-test="funboarding-continue-button"]"#;
const ON_ONBOARDING: &str = r#"[data-test="funboarding-continue-button"], [role="radiogroup"]"#;
const PAGE3: &str = r#"[data-test="prior-proficiency-card"], [data-test="start-basics"], [data-test="start-placement"]"#;
const EMAIL_OPTION: &str = r"^\s*continue with e-?mail\s*$";
const SKIP_BUTTONS: &str = r"^\s*(continue|skip|not now|no thanks|maybe later|i'll do it later|let's go|start learning)\s*$";
const FORM_ERROR_JS: &str = r#"() => [...document.querySelectorAll('form [data-test*="error"], form [role="alert"], form [aria-live]')]
    .map(e => (e.innerText || '').trim()).filter(Boolean).join(' | ')"#;
/// Pick the course in the first unanswered radio group; true when something was clicked.
const PICK_RADIO_JS: &str = r#"() => {
    const vis = el => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el);
        return r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && s.display !== 'none'; };
    for (const g of document.querySelectorAll('[role="radiogroup"]')) {
        if (!vis(g)) continue;
        if (g.querySelector('[role="radio"][aria-checked="true"]')) continue;
        const t = g.querySelector('[data-test="start-basics"]') || g.querySelector('[role="radio"]:not([aria-disabled="true"])');
        if (!t) return false;
        t.scrollIntoView({ block: 'center' }); t.click(); return true;
    }
    return false;
}"#;

/// `url` has a path segment from `names` followed by `?`, `/` or the end.
fn url_has_segment(url: &str, names: &[&str]) -> bool {
    names.iter().any(|n| {
        let pat = format!("/{n}");
        url.match_indices(&pat).any(|(i, _)| matches!(url[i + pat.len()..].chars().next(), None | Some('?' | '/')))
    })
}

fn post_signup(url: &str) -> bool {
    url_has_segment(url, &["welcome", "onboarding", "learn", "lesson", "practice", "stories"])
}

fn after_onboarding(url: &str) -> bool {
    url_has_segment(url, &["learn", "lesson", "practice", "stories"])
}

struct Settings {
    username_prefix: String,
    email_prefix: String,
    email_domain: String,
    qualify: bool,
    looping: bool,
    hidden: bool,
}

impl Settings {
    fn from(opts: &Opts) -> Self {
        let mut username_prefix: String =
            opts.str("username-prefix", "").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(14).collect();
        if username_prefix.is_empty() {
            username_prefix = "Cerolingo".into();
        }
        let email_prefix = Some(opts.str("email-prefix", "").trim().to_owned()).filter(|s| !s.is_empty());
        let email_domain =
            Some(opts.str("email-domain", "").trim().trim_start_matches('@').to_owned()).filter(|s| !s.is_empty());
        Settings {
            username_prefix,
            email_prefix: email_prefix.unwrap_or_else(|| "cero".into()),
            email_domain: email_domain.unwrap_or_else(|| "lingo.com".into()),
            qualify: opts.bool("qualify", true),
            looping: opts.bool("loop", false),
            hidden: opts.bool("hidden", false),
        }
    }
}

async fn step_open_signup(page: &Browser) -> Result<()> {
    let home = format!("{}/", base_url());
    page.goto(&home).await?;
    let mut last: Option<Instant> = None;
    let end = Instant::now() + Duration::from_secs(30);
    while Instant::now() < end {
        page.ensure_open()?;
        page.dismiss_overlays().await;
        if page.visible(AGE).await {
            return Ok(());
        }
        // let the modal animate before clicking again
        if last.is_none_or(|t| t.elapsed().as_secs_f64() >= 0.8) {
            // The toggle reads "Sign up" on the login form and "Login" on the signup form.
            if page.visible_text(SIGN_UP_TOGGLE, r"sign\s*up").await {
                if page.click_text(SIGN_UP_TOGGLE, r"sign\s*up").await {
                    last = Some(Instant::now());
                }
            } else if page.visible(HAVE_ACCOUNT).await && !page.visible(EMAIL).await {
                if page.click(HAVE_ACCOUNT).await {
                    last = Some(Instant::now());
                }
            } else {
                let url = page.url().await;
                if url.contains("/register") || url.contains("/learn") {
                    page.goto(&home).await?;
                    last = Some(Instant::now());
                }
            }
        }
        sleep(0.15).await;
    }
    Err(step_error("Signup form did not appear"))
}

async fn step_age(page: &Browser) -> Result<()> {
    page.fill(AGE, &fastrand::u32(19..=45).to_string()).await?;
    if !page.wait_enabled(NEXT, 8.0).await {
        return Err(step_error("Age was rejected (Next stayed disabled)"));
    }
    page.click(NEXT).await;
    Ok(())
}

async fn form_error(page: &Browser) -> String {
    page.call(FORM_ERROR_JS, &[]).await.ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

async fn step_profile(page: &Browser, username: &str, email: &str, password: &str) -> Result<()> {
    let ready = wait_until(15.0, 0.1, async || {
        if page.visible(USERNAME).await || page.visible(EMAIL).await {
            return true;
        }
        // A/B variant: "Choose a sign-up option" (Google / Apple / Email) before the form.
        if page.click_text("button", EMAIL_OPTION).await {
            sleep(0.3).await;
        }
        false
    })
    .await;
    page.ensure_open()?;
    if !ready {
        return Err(step_error("Profile form did not appear after the age step"));
    }
    if page.visible(FULL_NAME).await {
        page.fill(FULL_NAME, username).await?;
    }
    if page.visible(USERNAME).await {
        page.fill(USERNAME, username).await?;
    }
    page.fill(EMAIL, email).await?;
    page.fill(PASSWORD, password).await?;
    if !page.wait_enabled(REGISTER, 10.0).await {
        let err = form_error(page).await;
        let err = if err.is_empty() { "no message".into() } else { err };
        return Err(step_error(format!("Create account button stayed disabled ({err})")));
    }
    page.dismiss_overlays().await;
    // The first press is sometimes swallowed while reCAPTCHA is still loading: press again,
    // but only once the button is enabled again (disabled = a submit is in flight).
    page.click(REGISTER).await;
    let mut pressed = Instant::now();
    let end = Instant::now() + Duration::from_secs(35);
    while Instant::now() < end {
        page.ensure_open()?;
        if wait_until(0.5, 0.1, async || post_signup(&page.url().await) || page.visible(ON_ONBOARDING).await).await {
            return Ok(());
        }
        if !form_error(page).await.is_empty() {
            break;
        }
        if pressed.elapsed().as_secs_f64() > 7.0 && page.wait_enabled(REGISTER, 0.3).await {
            ui::dim("No response yet, pressing Create account again");
            page.click(REGISTER).await;
            pressed = Instant::now();
        }
    }
    page.ensure_open()?;
    let err = form_error(page).await;
    let err = if err.is_empty() { "no response from the site".into() } else { err };
    Err(step_error(format!("Registration was not accepted: {err}")))
}

/// Pick a course and press Continue until "How much do you know?" or the home page shows up.
async fn step_onboarding(page: &Browser) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(60);
    let mut last_click: Option<Instant> = None;
    let mut seen = false;
    while Instant::now() < end {
        page.ensure_open()?;
        page.dismiss_overlays().await;
        if page.visible(PAGE3).await {
            ui::dim("Skipping 'How much do you know?'");
            return Ok(());
        }
        let on_screen = page.visible(ON_ONBOARDING).await;
        seen |= on_screen;
        if !on_screen && after_onboarding(&page.url().await) {
            sleep(0.3).await; // not mid-redirect between two onboarding screens
            if !page.visible(ON_ONBOARDING).await {
                return Ok(());
            }
            continue;
        }

        let mut acted = page.eval_bool(PICK_RADIO_JS).await;
        if acted {
            sleep(0.12).await;
        }
        if !acted
            && last_click.is_none_or(|t| t.elapsed().as_secs_f64() > 0.4)
            && page.visible(FUNBOARDING).await
            && page.wait_enabled(FUNBOARDING, 1.5).await
            && page.click(FUNBOARDING).await
        {
            last_click = Some(Instant::now());
            acted = true;
            ui::dim("Continue");
            sleep(0.25).await;
        }
        if !acted && !on_screen && page.click_text("button", SKIP_BUTTONS).await {
            acted = true;
            sleep(0.25).await;
        }
        if !acted {
            sleep(0.12).await;
        }
    }
    page.debug_dump("onboarding_timeout").await;
    Err(step_error(if seen { "Onboarding did not finish in time" } else { "Onboarding screen never appeared" }))
}

/// (jwt, cookie header) - the header always includes the jwt.
async fn extract_cookies(page: &Browser, timeout: f64) -> (String, String) {
    let mut jwt = String::new();
    wait_until(timeout, 0.15, async || {
        jwt = page.jwt_cookie().await.unwrap_or_default();
        !jwt.is_empty()
    })
    .await;
    let mut header = page.eval("document.cookie").await.ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
    if !jwt.is_empty() && !header.contains("jwt_token=") {
        header = format!("jwt_token={jwt}; {header}").trim_matches(|c| c == ';' || c == ' ').to_owned();
    }
    (jwt, header)
}

async fn create_account(exe: &std::path::Path, s: &Settings, init: &str, number: usize) -> Result<String> {
    let username = format!("{}{}", s.username_prefix, fastrand::u32(100_000..=999_999));
    let email = format!("{}{}@{}", s.email_prefix, fastrand::u32(1_000_000..=9_999_999), s.email_domain);
    let password = format!("{}{username}{}", fastrand::u32(1000..=9999), fastrand::u32(1000..=9999));
    const TOTAL: usize = 6;
    ui::section(&format!("Account #{number}: {username}"));

    let page = Browser::launch(
        exe,
        &Options { size: (1180, 780), mute: true, headless: s.hidden, init_script: Some(init) },
    )
    .await?;
    let result: Result<()> = async {
        ui::step(1, TOTAL, "Opening signup");
        step_open_signup(&page).await?;
        ui::step(2, TOTAL, "Age");
        step_age(&page).await?;
        ui::step(3, TOTAL, "Creating profile");
        step_profile(&page, &username, &email, &password).await?;
        ui::ok(&format!("Registered {email}"));
        ui::step(4, TOTAL, "Course and level selection");
        step_onboarding(&page).await?;
        ui::step(5, TOTAL, "Saving token");
        let (jwt, cookies) = extract_cookies(&page, 6.0).await;
        if jwt.is_empty() {
            return Err(step_error("No jwt_token cookie found after signup"));
        }
        accounts::append(&email, &password, &cookies)?;
        ui::ok("Account saved to accounts.txt");
        ui::step(6, TOTAL, if s.qualify { "Qualifying lessons on /lesson (10x)" } else { "First lesson on /lesson" });
        run_lessons(&page, init, if s.qualify { 10 } else { 1 }, "Lessons").await?;
        let (_, fresh) = extract_cookies(&page, 2.0).await;
        if !fresh.is_empty() && fresh != cookies {
            let _ = accounts::update(&email, &password, &fresh);
        }
        Ok(())
    }
    .await;
    if let Err(e) = &result
        && !is_window_closed(e)
    {
        page.debug_dump("register_failed").await;
    }
    page.close().await;
    result.map(|()| email)
}

pub async fn run(opts: &Opts) -> i32 {
    let s = Settings::from(opts);
    ui::set_title("Account Creator");
    ui::banner("Account Creator", "Signup -> onboarding -> /lesson");
    ui::config(&[
        ("Username prefix", s.username_prefix.as_str().into()),
        ("Email", format!("{}0000000@{}", s.email_prefix, s.email_domain).into()),
        ("Qualify (10 lessons)", s.qualify.into()),
        ("Loop", s.looping.into()),
        ("Hidden browser", s.hidden.into()),
    ]);
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

    let (mut created, mut failed, mut number) = (0usize, 0usize, 0usize);
    let work = async {
        'outer: loop {
            number += 1;
            for attempt in 1..=3 {
                match create_account(&exe, &s, &init, number).await {
                    Ok(email) => {
                        created += 1;
                        ui::farewell(&format!("Created {email}"));
                        break;
                    }
                    Err(e) if is_window_closed(&e) => {
                        ui::warn("Browser window closed - stopping");
                        break 'outer;
                    }
                    Err(e) => {
                        ui::error(&format!("Attempt {attempt}/3 failed: {}", ui::first_line(&e)));
                        if attempt == 3 {
                            failed += 1;
                        } else {
                            sleep(1.0).await;
                        }
                    }
                }
            }
            if !s.looping {
                break;
            }
        }
    };
    if interruptible(work).await.is_none() {
        ui::warn("Interrupted");
    }

    ui::summary(
        "Summary",
        &[
            ("Created", created.to_string().into()),
            ("Failed", failed.to_string().into()),
            ("Accounts file", paths::accounts_path().display().to_string().into()),
        ],
    );
    if created > 0 || failed == 0 { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_patterns() {
        assert!(post_signup("https://www.duolingo.com/welcome"));
        assert!(post_signup("https://www.duolingo.com/learn?x=1"));
        assert!(post_signup("https://www.duolingo.com/onboarding/step"));
        assert!(!post_signup("https://www.duolingo.com/learners"));
        assert!(!post_signup("https://www.duolingo.com/register"));
        assert!(after_onboarding("https://www.duolingo.com/lesson"));
        assert!(!after_onboarding("https://www.duolingo.com/welcome"));
    }
}
