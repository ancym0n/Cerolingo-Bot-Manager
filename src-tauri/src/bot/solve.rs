//! Log in with a saved account and let the Cerolingo mod menu solve lessons on /lesson.

use std::cell::Cell;

use anyhow::Result;
use serde_json::json;

use super::{
    Opts, browser_exe,
    cdp::{Browser, Options, StepError, WindowClosed, is_window_closed, sleep, step_error, wait_until},
    interruptible,
    lessons::{base_url, run_lessons_counted},
    solver, ui,
};
use crate::accounts::{self, Account};

fn pick_account(index: usize) -> Option<Account> {
    if index == 0 {
        return None;
    }
    let list = accounts::load();
    if list.is_empty() {
        ui::warn("No saved accounts with a token found - log in yourself in the window");
        return None;
    }
    if index > list.len() {
        ui::warn(&format!("Only {0} saved account(s); using #{0}", list.len()));
    }
    let i = index.min(list.len()) - 1;
    list.into_iter().nth(i)
}

async fn wait_for_manual_login(page: &Browser) -> Result<()> {
    ui::info("Log in to Duolingo in the browser window - solving starts automatically");
    while !page.logged_in().await {
        if page.is_closed() {
            return Err(WindowClosed.into());
        }
        sleep(0.5).await;
    }
    ui::ok("Logged in");
    Ok(())
}

struct Session<'a> {
    page: &'a Browser,
    account: Option<&'a Account>,
    init: &'a str,
    auto_solver: bool,
    headless: bool,
    lessons: usize,
    done: &'a Cell<usize>,
}

async fn session(s: Session<'_>) -> Result<()> {
    let page = s.page;
    if let Some(account) = s.account {
        page.set_cookies(accounts::browser_cookies(account)).await?;
    }
    page.goto(&format!("{}/learn", base_url())).await?;
    match s.account {
        Some(account) => {
            if wait_until(10.0, 0.25, async || page.is_closed() || page.logged_in().await).await && !page.is_closed() {
                ui::ok(&format!("Logged in as {}", account.email));
            } else if page.is_closed() {
                return Err(WindowClosed.into());
            } else if s.headless {
                return Err(step_error(format!("The saved token for {} no longer works", account.email)));
            } else {
                ui::warn(&format!("The saved token for {} no longer works", account.email));
                wait_for_manual_login(page).await?;
            }
        }
        None => wait_for_manual_login(page).await?,
    }

    if !s.auto_solver {
        ui::info("Auto solver is off: the mod menu is ready, solve lessons yourself");
        ui::dim("Close the browser window to exit");
        page.wait_closed().await;
        return Ok(());
    }
    ui::section("Lessons");
    let done = run_lessons_counted(page, s.init, s.lessons, "Lessons", s.done).await?;
    ui::ok(&format!("Finished {done} lesson(s)"));
    if !s.headless {
        ui::dim("Close the browser window to exit");
        page.wait_closed().await;
    }
    Ok(())
}

pub async fn run(opts: &Opts) -> i32 {
    let account_index = opts.int("account", 1).max(0) as usize;
    let auto_solver = opts.bool("auto-solver", true);
    let settings = json!({
        "autoSolver": auto_solver,
        "botHighlight": opts.bool("bot-highlight", true),
        "audioBlock": opts.bool("audio-block", true),
        "storyHighlight": opts.bool("story-highlight", true),
        "legendaryBypass": opts.bool("legendary-bypass", true),
        "fastMode": true,
    });
    let lessons = opts.int("lessons", 5).max(0) as usize;
    let hidden = opts.bool("hidden", false);
    let flag = |k: &str| settings[k].as_bool().unwrap_or(true);

    ui::set_title("Auto Solver");
    ui::banner("Auto Solver", "Log in -> /lesson -> solved by the Cerolingo mod menu");
    ui::config(&[
        ("Account", if account_index > 0 { format!("#{account_index}") } else { "log in yourself".into() }.into()),
        ("Lessons", if lessons > 0 { lessons.to_string() } else { "until the window is closed".into() }.into()),
        ("Auto solver", auto_solver.into()),
        ("Blue animations", flag("botHighlight").into()),
        ("Audio blocking", flag("audioBlock").into()),
        ("Lessons highlighter", flag("storyHighlight").into()),
        ("Legendary bypass", flag("legendaryBypass").into()),
        ("Hidden browser", hidden.into()),
    ]);

    ui::section("Mod menu");
    let init = match solver::build_init_script(&settings, false).await {
        Ok(v) => v,
        Err(e) => {
            ui::error(&format!("{e:#}"));
            return 1;
        }
    };
    let account = pick_account(account_index);
    let mut headless = hidden;
    if headless && (account.is_none() || !auto_solver) {
        ui::warn("Hidden browser needs a saved account and the auto solver on - showing the window");
        headless = false;
    }
    let exe = match browser_exe(opts).await {
        Ok(p) => p,
        Err(e) => {
            ui::error(&format!("No browser available: {e:#}"));
            return 1;
        }
    };

    let done = Cell::new(0);
    let mut code = 0;
    ui::section("Session");
    let opts = Options { size: (1280, 820), mute: flag("audioBlock"), headless, init_script: Some(&init) };
    match Browser::launch(&exe, &opts).await {
        Err(e) => {
            ui::error(&format!("Could not start the browser: {}", ui::first_line(&e)));
            code = 1;
        }
        Ok(page) => {
            let work = session(Session {
                page: &page,
                account: account.as_ref(),
                init: &init,
                auto_solver,
                headless,
                lessons,
                done: &done,
            });
            match interruptible(work).await {
                None => {
                    ui::progress_done();
                    ui::warn("Interrupted");
                }
                Some(Ok(())) => {}
                Some(Err(e)) if is_window_closed(&e) || page.is_closed() => ui::info("Browser window closed"),
                Some(Err(e)) => {
                    if let Some(step) = e.downcast_ref::<StepError>() {
                        done.set(done.get().max(step.done));
                    }
                    ui::error(&ui::first_line(&e));
                    page.debug_dump("solver_failed").await;
                    code = 1;
                }
            }
            page.close().await;
        }
    }

    ui::summary("Summary", &[("Lessons solved", done.get().to_string().into())]);
    ui::farewell("Solver session ended");
    code
}
