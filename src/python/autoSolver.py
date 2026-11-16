"""Log in with a saved account and let the Cerolingo mod menu solve lessons on /lesson.

Args: account# (0 = log in yourself), autoSolver, botHighlight, audioBlock, storyHighlight,
      legendaryBypass, lessons (0 = until the window is closed), hidden
"""

import sys
import time

import env
import solver
import ui
from lessons import StepError, WindowClosed, debug_dump, goto, run_lessons, wait_until

sync_playwright = env.ensure_playwright()

account_index = max(env.arg(1, 1, int), 0)
settings = {
    "autoSolver": env.arg(2, True, bool),
    "botHighlight": env.arg(3, True, bool),
    "audioBlock": env.arg(4, True, bool),
    "storyHighlight": env.arg(5, True, bool),
    "legendaryBypass": env.arg(6, True, bool),
    "fastMode": True,
}
lessons_target = max(env.arg(7, 5, int), 0)
hidden = env.arg(8, False, bool)

LEARN_URL = f"{env.BASE_URL}/learn"


def pick_account():
    if account_index == 0:
        return None
    accounts = env.load_accounts()
    if not accounts:
        ui.warn("No saved accounts with a token found - log in yourself in the window")
        return None
    if account_index > len(accounts):
        ui.warn(f"Only {len(accounts)} saved account(s); using #{len(accounts)}")
    return accounts[min(account_index, len(accounts)) - 1]


def wait_for_manual_login(window):
    ui.info("Log in to Duolingo in the browser window - solving starts automatically")
    while not env.logged_in(window):
        if window.page.is_closed() or not window.context.pages:
            raise WindowClosed()
        time.sleep(0.5)
    ui.ok("Logged in")


def main():
    ui.banner("Auto Solver", "Log in -> /lesson -> solved by the Cerolingo mod menu")
    ui.config(
        [
            ("Account", f"#{account_index}" if account_index else "log in yourself"),
            ("Lessons", str(lessons_target) if lessons_target else "until the window is closed"),
            ("Auto solver", settings["autoSolver"]),
            ("Blue animations", settings["botHighlight"]),
            ("Audio blocking", settings["audioBlock"]),
            ("Lessons highlighter", settings["storyHighlight"]),
            ("Legendary bypass", settings["legendaryBypass"]),
            ("Hidden browser", hidden),
        ]
    )

    ui.section("Mod menu")
    try:
        init_script = solver.build_init_script(settings)
    except RuntimeError as exc:
        ui.error(str(exc))
        return 1

    account = pick_account()
    headless = hidden
    if headless and (not account or not settings["autoSolver"]):
        ui.warn("Hidden browser needs a saved account and the auto solver on - showing the window")
        headless = False

    done, code = 0, 0
    ui.section("Session")
    with sync_playwright() as playwright:
        with env.AppWindow(
            playwright, size=(1280, 820), mute=settings["audioBlock"], headless=headless, init_script=init_script
        ) as window:
            page = window.page
            try:
                if account:
                    window.context.add_cookies(env.account_cookies(account))
                goto(page, LEARN_URL)
                if account:
                    if wait_until(lambda: env.logged_in(window), 10, 0.25):
                        ui.ok(f"Logged in as {account['email']}")
                    elif headless:
                        raise StepError(f"The saved token for {account['email']} no longer works")
                    else:
                        ui.warn(f"The saved token for {account['email']} no longer works")
                        wait_for_manual_login(window)
                else:
                    wait_for_manual_login(window)

                if not settings["autoSolver"]:
                    ui.info("Auto solver is off: the mod menu is ready, solve lessons yourself")
                    ui.dim("Close the browser window to exit")
                    window.wait_closed()
                else:
                    ui.section("Lessons")
                    done = run_lessons(page, init_script, lessons_target)
                    ui.ok(f"Finished {done} lesson(s)")
                    if not headless:
                        ui.dim("Close the browser window to exit")
                        window.wait_closed()
            except WindowClosed:
                ui.info("Browser window closed")
            except KeyboardInterrupt:
                ui.warn("Interrupted")
            except Exception as exc:
                done = getattr(exc, "done", done)
                if page.is_closed():
                    ui.info("Browser window closed")
                else:
                    ui.error((str(exc).splitlines() or [exc.__class__.__name__])[0])
                    debug_dump(page, "solver_failed")
                    code = 1

    ui.summary("Summary", [("Lessons solved", str(done))])
    ui.farewell("Solver session ended")
    return code


if __name__ == "__main__":
    sys.exit(main())
