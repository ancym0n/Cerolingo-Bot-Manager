//! The /lesson runner used by every bot: the Cerolingo mod menu answers the challenges, this
//! module keeps lessons moving (Continue / skip speaking / dismiss pop-ups) and counts them.

use std::{cell::Cell, time::Instant};

use anyhow::Result;
use serde_json::Value;

use super::{
    cdp::{Browser, StepError, sleep, wait_until},
    ui,
};

pub fn base_url() -> String {
    std::env::var("CBM_BASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "https://www.duolingo.com".into())
        .trim_end_matches('/')
        .to_owned()
}

pub fn lesson_url() -> String {
    format!("{}/lesson", base_url())
}

const CHEST: &str = r#"button[aria-label="Open chest"]"#;

const LESSON_INSPECT_JS: &str = r#"() => {
    const isVis = el => { if (!el) return false; const st = getComputedStyle(el);
        if (st.display === 'none' || st.visibility === 'hidden' || parseFloat(st.opacity) === 0) return false;
        const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
    const isEnabled = el => Boolean(el && !el.disabled && el.getAttribute('aria-disabled') !== 'true');
    const dismissTexts = new Set(['no thanks','not now','maybe later','got it','claim','claim xp',"let's go","i'll do it later"]);
    for (const sel of ['#onetrust-accept-btn-handler','#onetrust-reject-all-handler','[data-test="plus-no-thanks"]',
                       '[data-test="notification-drawer-no-thanks-button"]','[data-test="secondary-continue-button"]']) {
        const el = document.querySelector(sel);
        if (isVis(el) && isEnabled(el)) { el.click(); break; }
    }
    for (const btn of document.querySelectorAll('button')) {
        if (!isVis(btn) || !isEnabled(btn)) continue;
        if (btn.closest('#cerolingoModMenu, [data-test^="challenge"], [data-test="word-bank"]')) continue;
        if ((btn.getAttribute('data-test') || '') === 'quit-button') continue;
        if (dismissTexts.has((btn.innerText || '').trim().toLowerCase())) { btn.click(); break; }
    }
    const url = location.href;
    const inLesson = url.includes('/lesson') || url.includes('/story') || url.includes('/stories') || url.includes('/skill/');
    const find = sel => [...document.querySelectorAll(sel)].find(isVis) || null;
    const challenge = find('[data-test^="challenge"]');
    const playerNext = find('[data-test="player-next"]');
    const playerSkip = find('[data-test="player-skip"]');
    const storyDone = find('[data-test="stories-player-done"]');
    const storyEl = find('[data-test="stories-element"], [data-test="story-start"]');
    const progressEl = document.querySelector('[role="progressbar"]');
    const progress = progressEl ? (progressEl.getAttribute('aria-valuenow') || '') : '';
    const hasSkip = Boolean(playerSkip && isEnabled(playerSkip));
    const hasSpeak = [...document.querySelectorAll('[data-test*="challenge-speak"]')].some(isVis) ||
        (hasSkip && /can.?t speak/i.test(playerSkip.innerText || ''));
    const nextEnabled = Boolean(playerNext && isEnabled(playerNext));
    return {
        solverReady: Boolean(window.__cerolingoSolverInjected), url, inLesson,
        hasLessonUi: Boolean(challenge || playerNext || storyEl || storyDone),
        hasSpeak, hasSkip, hasPlayerNext: Boolean(playerNext), nextEnabled,
        sig: `${url}|${progress}|${challenge ? challenge.getAttribute('data-test') : ''}|${playerNext ? (nextEnabled ? 1 : 0) : 'x'}`,
    };
}"#;

const LESSON_READY_JS: &str = r#"() => Boolean(document.querySelector(
    '[data-test^="challenge"], [data-test="player-next"], [data-test="story-start"], [data-test="stories-element"]'))"#;

/// Get to a running lesson. /lesson bounces back to /learn while an unopened treasure chest
/// is the next item on the path, so chests are opened first.
pub async fn ensure_on_lesson(page: &Browser) -> Result<bool> {
    let lesson = lesson_url();
    if !page.url().await.contains("/lesson") {
        page.goto(&lesson).await?;
    }
    let mut tried_learn = false;
    let end = Instant::now() + std::time::Duration::from_secs(20);
    while Instant::now() < end {
        page.ensure_open()?;
        page.dismiss_overlays().await;
        let url = page.url().await;
        if url.contains("/lesson") {
            if page.eval_bool(LESSON_READY_JS).await {
                return Ok(true);
            }
        } else if url.contains("/learn") && wait_until(1.5, 0.15, async || page.visible(CHEST).await).await {
            page.click(CHEST).await;
            ui::dim("Opened a treasure chest");
            sleep(1.5).await;
            page.goto(&lesson).await?;
        } else if url.contains("/learn") && !tried_learn {
            tried_learn = true;
            let link = r#"a[href*="/lesson"]:not([href*="test"])"#;
            let node = r#"[data-test*="skill-path-level"]"#;
            if page.visible(link).await && page.click(link).await {
                continue;
            }
            if page.visible(node).await && page.click(node).await {
                sleep(0.25).await;
                if page.visible(link).await && page.click(link).await {
                    continue;
                }
            }
            page.goto(&lesson).await?;
        }
        sleep(0.15).await;
    }
    Ok(page.url().await.contains("/lesson"))
}

struct Inspect {
    solver_ready: bool,
    in_lesson: bool,
    has_lesson_ui: bool,
    has_speak: bool,
    has_skip: bool,
    has_player_next: bool,
    next_enabled: bool,
    sig: String,
}

impl Inspect {
    fn parse(v: &Value) -> Option<Self> {
        let b = |k: &str| v[k].as_bool().unwrap_or(false);
        Some(Inspect {
            solver_ready: b("solverReady"),
            in_lesson: b("inLesson"),
            has_lesson_ui: b("hasLessonUi"),
            has_speak: b("hasSpeak"),
            has_skip: b("hasSkip"),
            has_player_next: b("hasPlayerNext"),
            next_enabled: b("nextEnabled"),
            sig: v["sig"].as_str()?.to_owned(),
        })
    }
}

pub async fn wait_lesson_complete(page: &Browser, init_script: &str, timeout: f64) -> Result<bool> {
    let start = Instant::now();
    let secs = || start.elapsed().as_secs_f64();
    let end = timeout;
    let mut saw_ui = false;
    let (mut last_sig, mut last_change) = (String::new(), secs());
    let (mut last_skip, mut last_next, mut last_fallback) = (f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);

    while secs() < end {
        page.ensure_open()?;
        let Some(st) = page.call(LESSON_INSPECT_JS, &[]).await.ok().as_ref().and_then(Inspect::parse) else {
            sleep(0.2).await;
            continue;
        };
        if !st.solver_ready && !init_script.is_empty() {
            let _ = page.eval(init_script).await;
        }
        saw_ui = saw_ui || st.has_lesson_ui;
        if saw_ui && !st.in_lesson {
            return Ok(true);
        }

        let now = secs();
        if st.sig != last_sig {
            last_sig = st.sig;
            last_change = now;
        }
        let stalled = now - last_change;

        // Speaking exercises are not solvable: skip straight away.
        if st.has_speak && st.has_skip && now - last_skip > 0.8 && page.click(r#"[data-test="player-skip"]"#).await {
            last_skip = now;
            sleep(0.2).await;
            continue;
        }
        if st.has_player_next
            && st.next_enabled
            && stalled >= 1.0
            && now - last_next > 0.8
            && page.click(r#"[data-test="player-next"]"#).await
        {
            last_next = now;
            sleep(0.2).await;
            continue;
        }
        if stalled >= 2.5 && st.has_skip && now - last_skip > 1.5 && page.click(r#"[data-test="player-skip"]"#).await {
            last_skip = now;
            sleep(0.2).await;
            continue;
        }
        if stalled >= 6.0 && now - last_fallback > 2.0 {
            for sel in [
                r#"[data-test="challenge-choice"]"#,
                r#"[role="radio"]"#,
                r#"button[data-test*="-challenge-tap-token"][aria-disabled="false"]"#,
            ] {
                if page.visible(sel).await {
                    page.click(sel).await;
                    last_fallback = now;
                    break;
                }
            }
        }
        sleep(0.2).await;
    }
    Ok(false)
}

/// Complete `total` lessons on /lesson (0 = keep going until the window is closed).
/// Returns the number of finished lessons; fails with `WindowClosed` / `StepError`.
pub async fn run_lessons(page: &Browser, init_script: &str, total: usize, label: &str) -> Result<usize> {
    run_lessons_counted(page, init_script, total, label, &Cell::new(0)).await
}

/// Like `run_lessons`, but `counter` stays readable when the future is dropped (Ctrl+C) or fails.
pub async fn run_lessons_counted(
    page: &Browser,
    init_script: &str,
    total: usize,
    label: &str,
    counter: &Cell<usize>,
) -> Result<usize> {
    let result = run_lessons_inner(page, init_script, total, label, counter).await;
    ui::progress_done();
    result
}

async fn run_lessons_inner(
    page: &Browser,
    init_script: &str,
    total: usize,
    label: &str,
    counter: &Cell<usize>,
) -> Result<usize> {
    let mut done = 0;
    let shown = if total == 0 { "∞".to_owned() } else { total.to_string() };
    while total == 0 || done < total {
        let mut finished = false;
        for attempt in 1..=2 {
            if ensure_on_lesson(page).await? && wait_lesson_complete(page, init_script, 90.0).await? {
                done += 1;
                counter.set(done);
                if total > 0 {
                    ui::progress(label, done, total, "");
                } else {
                    ui::progress(label, done, done, "(close the window to stop)");
                }
                finished = true;
                break;
            }
            ui::progress_done();
            ui::warn(&format!("Lesson {}/{shown} timed out (attempt {attempt}/2), retrying", done + 1));
            page.debug_dump("lesson_stuck").await;
            page.goto(&lesson_url()).await?;
        }
        if !finished {
            return Err(StepError { msg: format!("Timed out completing lesson {}/{shown}", done + 1), done }.into());
        }
    }
    Ok(done)
}
