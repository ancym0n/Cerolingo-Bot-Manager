"""Farm XP on every saved account.

Legit mode: opens duolingo.com/lesson in a browser logged in as each account, lets the
Cerolingo mod menu solve real lessons and tracks lessons + XP (read before/after from Duolingo).
Fast mode (Legit off): completes practice sessions directly through the API.

Args: lessons per account, legit, loop, hidden
"""

import json
import sys
import time

import env
import solver
import ui
from lessons import StepError, WindowClosed, goto, run_lessons, wait_until

lessons_per_account = max(env.arg(1, 9, int), 1)
legit = env.arg(2, False, bool)
loop = env.arg(3, False, bool)
hidden = env.arg(4, False, bool)

API = "https://www.duolingo.com/2017-06-30"
XP_PER_LESSON = 13
CHALLENGE_TYPES = [
    "assist",
    "characterIntro",
    "characterMatch",
    "characterPuzzle",
    "characterSelect",
    "characterTrace",
    "characterWrite",
    "completeReverseTranslation",
    "definition",
    "dialogue",
    "extendedMatch",
    "extendedListenMatch",
    "form",
    "freeResponse",
    "gapFill",
    "judge",
    "listen",
    "listenComplete",
    "listenMatch",
    "match",
    "name",
    "listenComprehension",
    "listenIsolation",
    "listenSpeak",
    "listenTap",
    "orderTapComplete",
    "partialListen",
    "partialReverseTranslate",
    "patternTapComplete",
    "radioBinary",
    "radioImageSelect",
    "radioListenMatch",
    "radioListenRecognize",
    "radioSelect",
    "readComprehension",
    "reverseAssist",
    "sameDifferent",
    "select",
    "selectPronunciation",
    "selectTranscription",
    "svgPuzzle",
    "syllableTap",
    "syllableListenTap",
    "speak",
    "tapCloze",
    "tapClozeTable",
    "tapComplete",
    "tapCompleteTable",
    "tapDescribe",
    "translate",
    "transliterate",
    "transliterationAssist",
    "typeCloze",
    "typeClozeTable",
    "typeComplete",
    "typeCompleteTable",
    "writeComprehension",
]



def lessons_word(n):
    return f"{n} lesson{'' if n == 1 else 's'}"


# -- fast mode (API) ----------------------------------------------------------------------
class Blocked(Exception):
    """Duolingo refused the API request (HTTP 401/403)."""


def user_info(session, token):
    """Return (fromLanguage, learningLanguage) for the account behind `token`."""
    uid = env.user_id(token)
    res = session.get(f"{API}/users/{uid}", params={"fields": "fromLanguage,learningLanguage"}, timeout=20)
    if res.status_code in (401, 403):
        raise Blocked(res.status_code)
    data = res.json() if res.ok else {}
    return data.get("fromLanguage") or "en", data.get("learningLanguage") or "es"


def run_api_lesson(session, from_lang, learn_lang):
    body = {
        "challengeTypes": CHALLENGE_TYPES,
        "fromLanguage": from_lang,
        "isFinalLevel": False,
        "isV2": True,
        "juicy": True,
        "learningLanguage": learn_lang,
        "smartTipsVersion": 2,
        "type": "GLOBAL_PRACTICE",
    }
    res = session.post(f"{API}/sessions", data=json.dumps(body), timeout=30)
    if res.status_code in (401, 403):
        raise Blocked(res.status_code)
    if res.status_code != 200:
        return False
    sess = res.json()
    if not sess.get("id"):
        return False
    now = int(time.time())
    done = {
        **sess,
        "heartsLeft": 0,
        "startTime": now - 60,
        "enableBonusPoints": True,
        "endTime": now,
        "failed": False,
        "maxInLessonStreak": 9,
        "shouldLearnThings": True,
    }
    return session.put(f"{API}/sessions/{sess['id']}", data=json.dumps(done), timeout=30).ok


def farm_api(account, requests):
    session = requests.Session()
    session.headers.update(
        {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {account['jwt_token']}",
            "User-Agent": "Mozilla/5.0",
        }
    )
    try:
        from_lang, learn_lang = user_info(session, account["jwt_token"])
        ui.dim(f"course: {learn_lang} (from {from_lang})")
        ok = 0
        for n in range(1, lessons_per_account + 1):
            try:
                ok += bool(run_api_lesson(session, from_lang, learn_lang))
            except requests.RequestException as exc:
                ui.warn(f"network error: {exc.__class__.__name__}")
            ui.progress("XP", n, lessons_per_account, f"+{ok * XP_PER_LESSON} xp")
    except Blocked as exc:
        ui.progress_done()
        ui.error(f"Duolingo blocked the API request (HTTP {exc}) - turn on Legit mode instead")
        return 0, 0
    ui.progress_done()
    if ok:
        ui.ok(f"{ok}/{lessons_word(lessons_per_account)} completed (~{ok * XP_PER_LESSON} xp)")
    else:
        ui.error("No lesson was accepted - is the token expired?")
    return ok, ok * XP_PER_LESSON


# -- legit mode (real lessons in the browser) ---------------------------------------------
_TOTAL_XP_JS = """async ([id, jwt]) => {
    try {
        const r = await fetch(`/2017-06-30/users/${id}?fields=totalXp`, {
            headers: { Authorization: `Bearer ${jwt}` }, credentials: 'include' });
        return r.ok ? ((await r.json()).totalXp ?? null) : null;
    } catch (_) { return null; }
}"""


def total_xp(page, account):
    uid = env.user_id(account["jwt_token"])
    try:
        return page.evaluate(_TOTAL_XP_JS, [uid, account["jwt_token"]]) if uid else None
    except Exception:
        return None


def farm_legit(account, playwright, init_script):
    with env.AppWindow(playwright, size=(1280, 820), mute=True, headless=hidden, init_script=init_script) as window:
        page = window.page
        window.context.add_cookies(env.account_cookies(account))
        goto(page, f"{env.BASE_URL}/learn")
        if not wait_until(lambda: env.logged_in(window), 10, 0.25):
            if page.is_closed():
                raise WindowClosed()
            ui.error("Saved token no longer works - skipping this account")
            return 0, 0
        before = total_xp(page, account)
        ui.dim(f"total xp: {before if before is not None else '?'}")
        try:
            done = run_lessons(page, init_script, lessons_per_account)
        except StepError as exc:
            done = getattr(exc, "done", 0)
            ui.error(str(exc))
        goto(page, f"{env.BASE_URL}/learn")
        after = total_xp(page, account)
        gained = after - before if before is not None and after is not None else None
        xp_text = f"+{gained} xp, total {after}" if gained is not None else "xp unknown"
        (ui.ok if done else ui.error)(f"{done}/{lessons_word(lessons_per_account)} on /lesson ({xp_text})")
        return done, gained or 0


# -- main ---------------------------------------------------------------------------------
def main():
    mode = "Legit: real lessons on duolingo.com/lesson" if legit else "Fast: practice sessions via the API"
    ui.banner("XP Farm", mode)
    rows = [("Lessons per account", str(lessons_per_account)), ("Legit mode", legit), ("Loop", loop)]
    if legit:
        rows.append(("Hidden browser", hidden))
    ui.config(rows)

    init_script = playwright_cm = playwright = requests = None
    if legit:
        ui.section("Mod menu")
        try:
            init_script = solver.build_init_script(
                {"autoSolver": True, "botHighlight": True, "audioBlock": True, "storyHighlight": True,
                 "legendaryBypass": True, "fastMode": True}
            )
        except RuntimeError as exc:
            ui.error(str(exc))
            return 1
        playwright_cm = env.ensure_playwright()()
        playwright = playwright_cm.__enter__()
    else:
        requests = env.ensure_requests()

    cycles = lessons = xp = 0
    try:
        while True:
            accounts = env.load_accounts()
            if not accounts:
                ui.warn("No valid accounts found in accounts.txt (create some first)")
                break
            ui.section(f"Cycle {cycles + 1}: {len(accounts)} account(s)")
            for i, account in enumerate(accounts, 1):
                ui.step(i, len(accounts), account["email"])
                done, gained = farm_legit(account, playwright, init_script) if legit else farm_api(account, requests)
                lessons += done
                xp += gained
            cycles += 1
            if not loop:
                break
    except WindowClosed:
        ui.progress_done()
        ui.warn("Browser window closed - stopping")
    except KeyboardInterrupt:
        ui.progress_done()
        ui.warn("Interrupted")
    finally:
        if playwright_cm:
            playwright_cm.__exit__(None, None, None)

    ui.summary(
        "Summary",
        [("Cycles", str(cycles)), ("Lessons", str(lessons)), ("XP gained", str(xp) if legit else f"~{xp}")],
    )
    ui.farewell("XP farm finished")
    return 0


if __name__ == "__main__":
    sys.exit(main())
