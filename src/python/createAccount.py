"""Create Duolingo accounts, finish onboarding and (optionally) qualify for leaderboards.

Flow (mirrors the real site, see tmp/ref):
  1. duolingo.com -> "I already have an account" -> "Sign up" -> "How old are you?" (age -> Next)
  2. "Create your profile"  (username / email / password -> Create account)
  3. "What would you like to learn?"   (pick a course -> Continue)
  4. "How much do you know?"            (skipped: as soon as it shows up we leave it)
  5. duolingo.com/lesson   -> the Cerolingo mod menu solves the lessons
"""

import os
import random
import re
import sys
import time

import env
import solver
import ui
from lessons import (
    StepError,
    click,
    debug_dump,
    dismiss_overlays,
    fill,
    first,
    goto,
    is_visible,
    run_lessons,
    wait_enabled,
    wait_until,
)

sync_playwright = env.ensure_playwright()

BASE = env.BASE_URL
HOME_URL = f"{BASE}/"

username_prefix = re.sub(r"[^A-Za-z0-9_]", "", env.arg(1, "Cerolingo"))[:14] or "Cerolingo"
email_prefix = env.arg(2, "cero").strip() or "cero"
email_domain = env.arg(3, "lingo.com").strip().lstrip("@") or "lingo.com"
qualify = env.arg(4, True, bool)
loop = env.arg(5, False, bool)
hidden = env.arg(6, False, bool)

SOLVER_SETTINGS = {
    "autoSolver": True,
    "botHighlight": True,
    "audioBlock": True,
    "storyHighlight": True,
    "legendaryBypass": True,
    "fastMode": True,
}

# -- selectors taken from the saved pages -------------------------------------------------
AGE = '[data-test="age-input"]'
NEXT = '[data-test="continue-button"]'
USERNAME = '[data-test="username-input"]'
FULL_NAME = '[data-test="full-name-input"]'
EMAIL = '[data-test="email-input"]'
PASSWORD = '[data-test="password-input"]'
REGISTER = '[data-test="register-button"]'
SIGN_UP_TOGGLE = '[data-test="sign-up-button"]'
HAVE_ACCOUNT = '[data-test="have-account"]'
FUNBOARDING = '[data-test="funboarding-continue-button"]'
POST_SIGNUP_URL = re.compile(r"/(welcome|onboarding|learn|lesson|practice|stories)(\?|/|$)")
AFTER_ONBOARDING_URL = re.compile(r"/(learn|lesson|practice|stories)(\?|/|$)")
ON_ONBOARDING = f'{FUNBOARDING}, [role="radiogroup"]'
_EMAIL_OPTION = re.compile(r"^\s*continue with e-?mail\s*$", re.I)
PAGE3 = '[data-test="prior-proficiency-card"], [data-test="start-basics"], [data-test="start-placement"]'


# -- steps --------------------------------------------------------------------------------
def step_open_signup(page):
    """Page 1: landing -> "I already have an account" (login modal) -> "Sign up" -> age form.
    ("Get started" / /register lead to the course picker instead, so they are not used.)"""
    goto(page, HOME_URL)
    last = 0.0

    def ready():
        nonlocal last
        dismiss_overlays(page)
        if is_visible(page, AGE):
            return True
        if time.time() - last < 0.8:  # let the modal animate before clicking again
            return False
        toggle = first(page, SIGN_UP_TOGGLE)
        # The toggle reads "Sign up" on the login form and "Login" on the signup form.
        if toggle.is_visible() and re.search(r"sign\s*up", toggle.inner_text(), re.I):
            last = time.time() if click(toggle) else last
        elif is_visible(page, HAVE_ACCOUNT) and not is_visible(page, EMAIL):
            last = time.time() if click(first(page, HAVE_ACCOUNT)) else last
        elif "/register" in page.url or "/learn" in page.url:
            goto(page, HOME_URL)  # landed somewhere unexpected: start over from the landing page
            last = time.time()
        return False

    if not wait_until(ready, 30, 0.15):
        raise StepError("Signup form did not appear")


def step_age(page):
    fill(page, AGE, random.randint(19, 45))
    if not wait_enabled(page, NEXT):
        raise StepError("Age was rejected (Next stayed disabled)")
    click(first(page, NEXT))


def _form_error(page):
    try:
        return page.evaluate(
            """() => [...document.querySelectorAll('form [data-test*="error"], form [role="alert"], form [aria-live]')]
                .map(e => (e.innerText || '').trim()).filter(Boolean).join(' | ')"""
        )
    except Exception:
        return ""


def _profile_form_ready(page):
    if is_visible(page, USERNAME) or is_visible(page, EMAIL):
        return True
    # A/B variant: "Choose a sign-up option" (Google / Apple / Email) before the profile form.
    button = page.locator("button:visible").filter(has_text=_EMAIL_OPTION).first
    if button.count():
        click(button)
        time.sleep(0.3)
    return False


def step_profile(page, username, email, password):
    if not wait_until(lambda: _profile_form_ready(page), 15, 0.1):
        raise StepError("Profile form did not appear after the age step")
    if is_visible(page, FULL_NAME):
        fill(page, FULL_NAME, username)
    if is_visible(page, USERNAME):
        fill(page, USERNAME, username)
    fill(page, EMAIL, email)
    fill(page, PASSWORD, password)
    if not wait_enabled(page, REGISTER, 10):
        raise StepError(f"Create account button stayed disabled ({_form_error(page) or 'no message'})")
    dismiss_overlays(page)
    accepted = lambda: POST_SIGNUP_URL.search(page.url) or is_visible(page, ON_ONBOARDING)  # noqa: E731
    # The first press is sometimes swallowed while reCAPTCHA is still loading: press again.
    # While the button is disabled a submit is in flight, so only re-press when it is enabled again.
    click(first(page, REGISTER))
    pressed, deadline = time.time(), time.time() + 35
    while time.time() < deadline:
        if wait_until(accepted, 0.5, 0.1):
            return
        if _form_error(page):
            break
        if time.time() - pressed > 7 and wait_enabled(page, REGISTER, 0.3):
            ui.dim("No response yet, pressing Create account again")
            click(first(page, REGISTER))
            pressed = time.time()
    raise StepError(f"Registration was not accepted: {_form_error(page) or 'no response from the site'}")


_SKIP_BUTTONS = re.compile(
    r"^\s*(continue|skip|not now|no thanks|maybe later|i'll do it later|let's go|start learning)\s*$", re.I
)


def step_onboarding(page):
    """Page 2: pick a course and press Continue until page 3 ("How much do you know?") or the
    home page shows up. Page 3 is skipped on purpose: the caller goes straight to /lesson."""
    deadline = time.time() + 60
    last_click = 0.0
    seen = False
    while time.time() < deadline:
        dismiss_overlays(page)
        if is_visible(page, PAGE3):
            ui.dim("Skipping 'How much do you know?'")
            return
        on_screen = is_visible(page, ON_ONBOARDING)
        seen = seen or on_screen
        if not on_screen and AFTER_ONBOARDING_URL.search(page.url):
            time.sleep(0.3)  # settle: not mid-redirect between two onboarding screens
            if not is_visible(page, ON_ONBOARDING):
                return
            continue

        acted = False
        groups = page.locator('[role="radiogroup"]').locator("visible=true")
        for i in range(groups.count()):
            group = groups.nth(i)
            if group.locator('[role="radio"][aria-checked="true"]').count():
                continue
            preferred = group.locator('[data-test="start-basics"]')
            target = preferred.first if preferred.count() else group.locator('[role="radio"]:not([aria-disabled="true"])').first
            if click(target):
                acted = True
                time.sleep(0.12)
            break

        if not acted and time.time() - last_click > 0.4 and is_visible(page, FUNBOARDING):
            if wait_enabled(page, FUNBOARDING, 1.5) and click(first(page, FUNBOARDING)):
                last_click = time.time()
                acted = True
                ui.dim("Continue")
                time.sleep(0.25)

        if not acted and not on_screen:
            button = page.locator("button:visible").filter(has_text=_SKIP_BUTTONS).first
            if button.count() and click(button):
                acted = True
                time.sleep(0.25)

        if not acted:
            time.sleep(0.12)
    debug_dump(page, "onboarding_timeout")
    raise StepError("Onboarding did not finish in time" if seen else "Onboarding screen never appeared")


def extract_cookies(page, context, timeout=6.0):
    jwt = (
        wait_until(lambda: next((c["value"] for c in context.cookies() if c.get("name") == "jwt_token"), ""), timeout, 0.15)
        or ""
    )
    header = ""
    try:
        header = page.evaluate("() => document.cookie")
    except Exception:
        pass
    if jwt and "jwt_token=" not in header:
        header = f"jwt_token={jwt}; {header}".strip("; ")
    return jwt, header


# -- orchestration ------------------------------------------------------------------------
def save_account(email, password, cookies):
    os.makedirs(os.path.dirname(env.ACCOUNTS_PATH), exist_ok=True)
    with open(env.ACCOUNTS_PATH, "a", encoding="utf-8") as f:
        f.write(f"{email}:{password}:{cookies}\n")


def update_account(email, password, cookies):
    try:
        with open(env.ACCOUNTS_PATH, "r", encoding="utf-8") as f:
            lines = f.read().splitlines()
        prefix = f"{email}:{password}:"
        lines = [f"{prefix}{cookies}" if line.startswith(prefix) else line for line in lines]
        with open(env.ACCOUNTS_PATH, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    except Exception:
        pass


def create_account(playwright, init_script, number):
    username = f"{username_prefix}{random.randint(100000, 999999)}"
    email = f"{email_prefix}{random.randint(1000000, 9999999)}@{email_domain}"
    password = f"{random.randint(1000, 9999)}{username}{random.randint(1000, 9999)}"
    total = 6
    ui.section(f"Account #{number}: {username}")

    with env.AppWindow(playwright, size=(1180, 780), mute=True, headless=hidden, init_script=init_script) as window:
        page = window.page
        try:
            ui.step(1, total, "Opening signup")
            step_open_signup(page)
            ui.step(2, total, "Age")
            step_age(page)
            ui.step(3, total, "Creating profile")
            step_profile(page, username, email, password)
            ui.ok(f"Registered {email}")
            ui.step(4, total, "Course and level selection")
            step_onboarding(page)
            ui.step(5, total, "Saving token")
            jwt, cookies = extract_cookies(page, window.context)
            if not jwt:
                raise StepError("No jwt_token cookie found after signup")
            save_account(email, password, cookies)
            ui.ok("Account saved to accounts.txt")
            ui.step(6, total, "Qualifying lessons on /lesson (10x)" if qualify else "First lesson on /lesson")
            run_lessons(page, init_script, 10 if qualify else 1)
            _, fresh = extract_cookies(page, window.context, timeout=2.0)
            if fresh and fresh != cookies:
                update_account(email, password, fresh)
        except Exception:
            debug_dump(page, "register_failed")
            raise
    return email


def main():
    ui.banner("Account Creator", "Signup -> onboarding -> /lesson")
    ui.config(
        [
            ("Username prefix", username_prefix),
            ("Email", f"{email_prefix}0000000@{email_domain}"),
            ("Qualify (10 lessons)", qualify),
            ("Loop", loop),
            ("Hidden browser", hidden),
        ]
    )
    ui.section("Mod menu")
    try:
        init_script = solver.build_init_script(SOLVER_SETTINGS)
    except RuntimeError as exc:
        ui.error(str(exc))
        return 1

    created = failed = number = 0
    with sync_playwright() as playwright:
        try:
            while True:
                number += 1
                for attempt in range(1, 4):
                    try:
                        email = create_account(playwright, init_script, number)
                        created += 1
                        ui.farewell(f"Created {email}")
                        break
                    except KeyboardInterrupt:
                        raise
                    except Exception as exc:
                        ui.error(f"Attempt {attempt}/3 failed: {(str(exc).splitlines() or [exc.__class__.__name__])[0]}")
                        if attempt == 3:
                            failed += 1
                        else:
                            time.sleep(1)
                if not loop:
                    break
        except KeyboardInterrupt:
            ui.warn("Interrupted")

    ui.summary("Summary", [("Created", str(created)), ("Failed", str(failed)), ("Accounts file", env.ACCOUNTS_PATH)])
    return 0 if created or not failed else 1


if __name__ == "__main__":
    sys.exit(main())
