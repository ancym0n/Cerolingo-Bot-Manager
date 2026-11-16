"""Shared Playwright page helpers and the /lesson runner used by the Account Creator and the
Auto Solver. The Cerolingo mod menu answers the challenges; this module keeps lessons moving
(Continue / skip speaking / dismiss pop-ups) and reports when each lesson is finished."""

import os
import time

import env
import ui

LESSON_URL = f"{env.BASE_URL}/lesson"


class StepError(RuntimeError):
    pass


class WindowClosed(Exception):
    """The user closed the browser window."""


# -- small helpers ------------------------------------------------------------------------
def wait_until(fn, timeout, interval=0.1):
    end = time.time() + timeout
    while time.time() < end:
        try:
            value = fn()
            if value:
                return value
        except Exception:
            pass
        time.sleep(interval)
    return None


def is_visible(page, selector):
    try:
        return page.locator(selector).first.is_visible()
    except Exception:
        return False


def first(page, selector):
    return page.locator(selector).locator("visible=true").first


_COVERED_JS = """el => {
    const r = el.getBoundingClientRect();
    const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !top || !(el === top || el.contains(top));
}"""


def click(loc):
    """Real mouse click when the element is reachable, DOM click when something (e.g. the mod
    menu) covers it. Never a forced coordinate click: that would hit the overlay instead."""
    try:
        loc.scroll_into_view_if_needed(timeout=1500)
        if not loc.evaluate(_COVERED_JS):
            loc.click(timeout=1500)
            return True
    except Exception:
        pass
    try:
        loc.evaluate("el => el.click()")
        return True
    except Exception:
        return False


def wait_enabled(page, selector, timeout=8.0):
    return wait_until(
        lambda: page.evaluate(
            """sel => {
                const el = [...document.querySelectorAll(sel)].find(e => e.getClientRects().length);
                return Boolean(el && !el.disabled && el.getAttribute('aria-disabled') !== 'true');
            }""",
            selector,
        ),
        timeout,
        0.08,
    )


def fill(page, selector, value):
    value = str(value)
    loc = first(page, selector)
    loc.click(timeout=3000)
    loc.fill(value)
    if loc.input_value() != value:  # controlled inputs occasionally drop the first write
        loc.fill("")
        loc.type(value, delay=15)


_DISMISS_JS = """() => {
    const ok = el => el && !el.disabled && el.getAttribute('aria-disabled') !== 'true' && el.getClientRects().length;
    for (const sel of ['#onetrust-accept-btn-handler', '#onetrust-reject-all-handler',
                       '[data-test="notification-drawer-no-thanks-button"]', '[data-test="plus-no-thanks"]']) {
        const el = document.querySelector(sel);
        if (ok(el)) { el.click(); return sel; }
    }
    const texts = new Set(['accept all', 'no thanks', 'maybe later', 'not now', "i'll do it later"]);
    for (const b of document.querySelectorAll('button')) {
        if (!ok(b) || b.closest('#cerolingoModMenu, [data-test^="challenge"]')) continue;
        if (texts.has((b.innerText || '').trim().toLowerCase())) { b.click(); return b.innerText; }
    }
    return null;
}"""


def dismiss_overlays(page):
    try:
        return page.evaluate(_DISMISS_JS)
    except Exception:
        return None


def goto(page, url, retries=3):
    leaf = url.rstrip("/").split("/")[-1]
    for attempt in range(retries):
        if page.is_closed():
            raise WindowClosed()
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=25000)
            return
        except Exception as exc:
            if page.is_closed():
                raise WindowClosed() from exc
            if attempt == retries - 1:
                raise
            if ("interrupted" in str(exc).lower() or "aborted" in str(exc).lower()) and leaf in page.url:
                return  # the SPA redirected mid-navigation, we are where we wanted to be
            time.sleep(0.5)


def debug_dump(page, name):
    try:
        os.makedirs(os.path.dirname(env.ACCOUNTS_PATH), exist_ok=True)
        shot = os.path.join(os.path.dirname(env.ACCOUNTS_PATH), f"{name}.png")
        page.screenshot(path=shot)
        ui.dim(f"url: {page.url}")
        ui.dim(f"screenshot: {shot}")
    except Exception:
        pass


# -- lessons ------------------------------------------------------------------------------
_LESSON_INSPECT_JS = """() => {
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
}"""

_LESSON_READY_JS = """() => Boolean(document.querySelector(
    '[data-test^="challenge"], [data-test="player-next"], [data-test="story-start"], [data-test="stories-element"]'))"""


CHEST = 'button[aria-label="Open chest"]'


def ensure_on_lesson(page):
    """Get to a running lesson. /lesson bounces back to /learn while an unopened treasure chest
    is the next item on the path, so chests are opened first."""
    if "/lesson" not in page.url:
        goto(page, LESSON_URL)
    tried_learn = False
    end = time.time() + 20
    while time.time() < end:
        if page.is_closed():
            raise WindowClosed()
        dismiss_overlays(page)
        if "/lesson" in page.url:
            try:
                if page.evaluate(_LESSON_READY_JS):
                    return True
            except Exception:
                pass
        elif "/learn" in page.url and wait_until(lambda: is_visible(page, CHEST), 1.5, 0.15):
            click(first(page, CHEST))
            ui.dim("Opened a treasure chest")
            time.sleep(1.5)
            goto(page, LESSON_URL)
        elif "/learn" in page.url and not tried_learn:
            tried_learn = True
            link = first(page, 'a[href*="/lesson"]:not([href*="test"])')
            node = first(page, '[data-test*="skill-path-level"]')
            if link.is_visible() and click(link):
                continue
            if node.is_visible() and click(node):
                time.sleep(0.25)
                if link.is_visible() and click(link):
                    continue
            goto(page, LESSON_URL)
        time.sleep(0.15)
    return "/lesson" in page.url


def wait_lesson_complete(page, init_script, timeout=90):
    end = time.time() + timeout
    saw_ui = False
    last_sig, last_change = "", time.time()
    last = {"skip": 0.0, "next": 0.0, "fallback": 0.0}

    def press(selector, key, now):
        if click(first(page, selector)):
            last[key] = now
            time.sleep(0.2)
            return True
        return False

    while time.time() < end:
        if page.is_closed():
            raise WindowClosed()
        try:
            st = page.evaluate(_LESSON_INSPECT_JS)
        except Exception:
            time.sleep(0.2)
            continue
        if not st["solverReady"] and init_script:
            try:
                page.evaluate(init_script)
            except Exception:
                pass
        saw_ui = saw_ui or st["hasLessonUi"]
        if saw_ui and not st["inLesson"]:
            return True

        now = time.time()
        if st["sig"] != last_sig:
            last_sig, last_change = st["sig"], now
        stalled = now - last_change

        # Speaking exercises are not solvable: skip straight away.
        if (
            st["hasSpeak"]
            and st["hasSkip"]
            and now - last["skip"] > 0.8
            and press('[data-test="player-skip"]', "skip", now)
        ):
            continue
        if (
            st["hasPlayerNext"]
            and st["nextEnabled"]
            and stalled >= 1.0
            and now - last["next"] > 0.8
            and press('[data-test="player-next"]', "next", now)
        ):
            continue
        if (
            stalled >= 2.5
            and st["hasSkip"]
            and now - last["skip"] > 1.5
            and press('[data-test="player-skip"]', "skip", now)
        ):
            continue
        if stalled >= 6.0 and now - last["fallback"] > 2.0:
            for sel in (
                '[data-test="challenge-choice"]',
                '[role="radio"]',
                'button[data-test*="-challenge-tap-token"][aria-disabled="false"]',
            ):
                if is_visible(page, sel):
                    click(first(page, sel))
                    last["fallback"] = now
                    break
        time.sleep(0.2)
    return False


def run_lessons(page, init_script, total, label="Lessons"):
    """Complete `total` lessons on /lesson (0 = keep going until the window is closed).
    Returns the number of finished lessons; raises WindowClosed / StepError."""
    done = 0
    shown = total or "∞"
    try:
        while not total or done < total:
            for attempt in range(2):
                if ensure_on_lesson(page) and wait_lesson_complete(page, init_script):
                    done += 1
                    if total:
                        ui.progress(label, done, total)
                    else:
                        ui.progress(label, done, done, "(close the window to stop)")
                    break
                ui.progress_done()
                ui.warn(f"Lesson {done + 1}/{shown} timed out (attempt {attempt + 1}/2), retrying")
                debug_dump(page, "lesson_stuck")
                goto(page, LESSON_URL)
            else:
                error = StepError(f"Timed out completing lesson {done + 1}/{shown}")
                error.done = done
                raise error
    finally:
        ui.progress_done()
    return done
