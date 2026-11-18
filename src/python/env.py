"""Shared runtime helpers: data paths, Playwright bootstrap and the app-style browser window."""

import glob
import os
import re
import shutil
import subprocess
import sys
import tempfile

import ui

SCRIPT_DIR = os.path.dirname(os.path.realpath(__file__))
DATA_DIR = os.environ.get("CBM_DATA_DIR", SCRIPT_DIR)
ACCOUNTS_PATH = os.path.join(DATA_DIR, "logs", "accounts.txt")
BASE_URL = os.environ.get("CBM_BASE_URL", "https://www.duolingo.com").rstrip("/")


def arg(index, default, kind=str):
    """Read positional CLI arg `index` (1-based) converted to `kind` (bool/int/str)."""
    if len(sys.argv) <= index:
        return default
    raw = sys.argv[index]
    if kind is bool:
        return raw.strip().lower() == "true"
    if kind is int:
        return int(raw) if raw.strip().lstrip("-").isdigit() else default
    return raw


def pip_install(*packages):
    base = [sys.executable, "-m", "pip", "install", "--disable-pip-version-check", "-q"]
    if subprocess.call(base + list(packages)) != 0:
        subprocess.call(base + ["--user", "--break-system-packages", *packages])


def ensure_requests():
    try:
        import requests
    except ImportError:
        ui.warn("Installing missing dependency: requests")
        pip_install("requests")
        import requests
    return requests


def ensure_playwright():
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        ui.warn("Installing missing dependency: playwright (one-time)")
        pip_install("playwright")
        subprocess.call([sys.executable, "-m", "playwright", "install", "chromium"])
        from playwright.sync_api import sync_playwright
    return sync_playwright


def find_chromium_executable():
    candidates = []
    home = os.path.expanduser("~")
    pw_path = os.environ.get("PLAYWRIGHT_BROWSERS_PATH", "")
    if sys.platform == "win32":
        if pw_path:
            candidates += sorted(
                glob.glob(os.path.join(pw_path, "chromium-*", "chrome-win*", "chrome.exe")),
                reverse=True,
            )
        local = os.environ.get("LOCALAPPDATA", "")
        if local:
            candidates += sorted(
                glob.glob(os.path.join(local, "ms-playwright", "chromium-*", "chrome-win*", "chrome.exe")),
                reverse=True,
            )
        for key in ("PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"):
            base = os.environ.get(key, "")
            if base:
                candidates.append(os.path.join(base, "Google", "Chrome", "Application", "chrome.exe"))
                candidates.append(os.path.join(base, "Microsoft", "Edge", "Application", "msedge.exe"))
    elif sys.platform == "darwin":
        if pw_path:
            candidates += sorted(
                glob.glob(os.path.join(pw_path, "chromium-*/chrome-mac*/Chromium.app/Contents/MacOS/Chromium")),
                reverse=True,
            )
        candidates += sorted(
            glob.glob(os.path.join(home, "Library/Caches/ms-playwright/chromium-*/chrome-mac*/Chromium.app/Contents/MacOS/Chromium")),
            reverse=True,
        )
        candidates.append("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
    else:
        if pw_path:
            candidates += sorted(
                glob.glob(os.path.join(pw_path, "chromium-*/chrome-linux*/chrome")),
                reverse=True,
            )
        candidates += sorted(
            glob.glob(os.path.join(home, ".cache/ms-playwright/chromium-*/chrome-linux*/chrome")),
            reverse=True,
        )
        for name in ("chromium", "chromium-browser", "google-chrome-stable", "google-chrome", "brave-browser"):
            found = shutil.which(name)
            if found:
                candidates.append(found)
    return next((c for c in candidates if c and os.path.isfile(c)), None)


class AppWindow:
    """A throw-away Chromium profile shown as a clean app window (no tabs / address bar)."""

    def __init__(self, playwright, *, size=(1180, 780), mute=False, headless=False, init_script=None):
        headless = bool(headless or os.environ.get("CBM_HEADLESS"))
        self.profile = tempfile.mkdtemp(prefix="cbm-profile-")
        args = [
            "--app=about:blank",
            f"--window-size={size[0]},{size[1]}",
            "--disable-blink-features=AutomationControlled",
            "--force-dark-mode",
            "--no-first-run",
            "--no-default-browser-check",
            "--password-store=basic",
            "--disable-features=Translate,PasswordLeakDetection",
        ]
        if mute:
            args.append("--mute-audio")
        opts = dict(
            user_data_dir=self.profile,
            headless=headless,
            args=args,
            no_viewport=True,
            locale="en-US",
            color_scheme="dark",
            ignore_default_args=["--enable-automation"],
        )
        try:
            try:
                self.context = playwright.chromium.launch_persistent_context(**opts)
            except Exception:
                exe = find_chromium_executable()
                if not exe:
                    raise
                self.context = playwright.chromium.launch_persistent_context(executable_path=exe, **opts)
        except BaseException:
            shutil.rmtree(self.profile, ignore_errors=True)
            raise
        self.context.set_default_timeout(15000)
        if init_script:
            self.context.add_init_script(init_script)
        self.page = self.context.pages[0] if self.context.pages else self.context.new_page()

    def wait_closed(self):
        try:
            while self.context.pages and not self.page.is_closed():
                self.page.wait_for_timeout(1000)
        except Exception:
            pass

    def close(self):
        try:
            self.context.close()
        except Exception:
            pass
        shutil.rmtree(self.profile, ignore_errors=True)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()


def load_accounts():
    """Parse accounts.txt (`email:password:cookie` per line) into dicts that have a jwt_token."""
    accounts = []
    if not os.path.exists(ACCOUNTS_PATH):
        return accounts
    with open(ACCOUNTS_PATH, "r", encoding="utf-8") as f:
        for line in f:
            match = re.match(r"([^:]+):([^:]+):(.*)", line.strip())
            token = re.search(r"jwt_token=([^;\s]+)", match.group(3)) if match else None
            if match and token:
                accounts.append(
                    {
                        "email": match.group(1),
                        "password": match.group(2),
                        "jwt_token": token.group(1),
                        "cookies": match.group(3),
                    }
                )
    return accounts


def account_cookies(account):
    """All `name=value` pairs saved for an account, as Playwright cookies on .duolingo.com."""
    cookies = {}
    for part in account["cookies"].split(";"):
        name, sep, value = part.strip().partition("=")
        if sep and name and value:
            cookies[name] = value
    cookies["jwt_token"] = account["jwt_token"]
    return [
        {"name": k, "value": v, "domain": ".duolingo.com", "path": "/", "secure": True, "sameSite": "Lax"}
        for k, v in cookies.items()
    ]


def user_id(token):
    """Duolingo user id stored in the JWT `sub` claim (None if the token is malformed)."""
    import base64
    import json

    try:
        payload = token.split(".")[1]
        return json.loads(base64.urlsafe_b64decode(payload + "=" * (-len(payload) % 4)))["sub"]
    except Exception:
        return None


def logged_in(window):
    """True once the window holds a jwt_token cookie and Duolingo shows the home page."""
    try:
        has_jwt = any(c.get("name") == "jwt_token" for c in window.context.cookies())
        return has_jwt and "/learn" in window.page.url
    except Exception:
        return False
