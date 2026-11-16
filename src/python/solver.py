"""Mod menu / auto solver bundle, built from the Cerolingo extension repository.

The extension sources are downloaded from GitHub on every run (a few dozen KB),
glued onto a tiny `chrome.*` bridge (js/bridge.js) and cached on disk, so the bot
always runs the newest mod menu and still works offline from the last good copy.

CLI:  python solver.py --update   # refresh the cache and print the result
"""

import base64
import json
import os
import re
import ssl
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

import env
import ui

REPO = os.environ.get("CBM_SOLVER_REPO", "ancym0n/Cerolingo")
REF = os.environ.get("CBM_SOLVER_REF", "HEAD")
RAW = f"https://raw.githubusercontent.com/{REPO}/{REF}/"

# Order matters: audio hooks first, menu last (matches the extension's runtime behaviour).
MODULES = [
    "audio", "main", "solver", "challenges", "bypass", "highlight", "xp", "menu",
]
CSS_PATH = "src/styles/content.css"
ASSET_PATHS = ["assets/title.svg"]
MANIFEST_PATH = "manifest.json"

BRIDGE_PATH = os.path.join(env.SCRIPT_DIR, "js", "bridge.js")
CACHE_DIR = os.path.join(env.DATA_DIR, "cache")
CACHE_FILE = os.path.join(CACHE_DIR, "solver-bundle.js")
META_FILE = os.path.join(CACHE_DIR, "solver-meta.json")


# Bot Manager tweaks layered on top of the upstream modules. Each one is optional:
# if upstream changes the matched text the patch is skipped and the stock code runs.
_FAST_HELPERS = (
    "const sleep = (ms) => new Promise((r) => setTimeout(r, ms));\n"
    "const isFastMode = () => Boolean(INITIAL_CONFIG.fastMode);\n"
    "const stepSleep = (ms) => sleep(isFastMode() ? Math.min(ms, 15) : ms);\n"
)
# Match-challenge fix (multi-word tokens use data-test="house-challenge-tap-token my").
# Identical to the fix in the extension repo; once upstream ships it these become no-ops.
_TOKEN_SEL_OLD = 'data-test$="-challenge-tap-token"'
_TOKEN_SEL_NEW = 'data-test*="-challenge-tap-token"'
_TOKEN_TEXT_END = '  return btn.innerText.replace(/[\\d\\n]/g, "").trim();\n}\n\n'
_NORM_TOKEN = (
    '// Tokens are case/whitespace-normalised so "My house " still matches "my house".\n'
    "function normToken(text) {\n"
    '  return String(text ?? "")\n'
    '    .normalize("NFC")\n'
    '    .replace(/\\s+/g, " ")\n'
    "    .trim()\n"
    "    .toLocaleLowerCase();\n"
    "}\n\n"
)
_MATCH_OLD = """    const findBtn = (text) =>
      available.find(
        (b) =>
          getTokenText(b) === text ||
          b.getAttribute("data-test") === `${text}-challenge-tap-token`,
      );

    const btn1 = findBtn(words[0]);
    const btn2 = findBtn(words[1]);

    if (btn1 && btn2 && btn1 !== btn2) {"""
_MATCH_NEW = """    const findBtn = (text, exclude) => {
      const want = normToken(text);
      return available.find(
        (b) =>
          b !== exclude &&
          (normToken(getTokenText(b)) === want ||
            normToken(b.getAttribute("data-test")) ===
              `${want}-challenge-tap-token`),
      );
    };

    // Pick the first two words that resolve to two different buttons
    // (learningToken and learningWord can be the same text).
    let btn1 = null;
    let btn2 = null;
    for (const word of words) {
      if (!btn1) btn1 = findBtn(word);
      else if (!btn2) btn2 = findBtn(word, btn1);
    }

    if (btn1 && btn2) {"""
_STORY_DT_OLD = """  if (dt.endsWith("-challenge-tap-token")) {
    add(dt.slice(0, -"-challenge-tap-token".length));
  }"""
_STORY_DT_NEW = """  // "my house" can be encoded as "house-challenge-tap-token my".
  const dtMatch = dt.match(/^(.*?)-challenge-tap-token(?:\\s+(.*))?$/);
  if (dtMatch) {
    add(dtMatch[1]);
    if (dtMatch[2]) add(`${dtMatch[2]} ${dtMatch[1]}`);
  }"""

PATCHES = {
    "solver": [(_STORY_DT_OLD, _STORY_DT_NEW)],
    "challenges": [
        (_TOKEN_SEL_OLD, _TOKEN_SEL_NEW),
        (_TOKEN_TEXT_END + "function setReactInputValue(", _TOKEN_TEXT_END + _NORM_TOKEN + "function setReactInputValue("),
        (_MATCH_OLD, _MATCH_NEW),
        ("const sleep = (ms) => new Promise((r) => setTimeout(r, ms));\n", _FAST_HELPERS),
        ("await sleep(45);", "if (!isFastMode()) await sleep(45);"),
        (
            "if (!isFxEnabled()) {\n    setReactInputValue(el, value);",
            "if (!isFxEnabled() || isFastMode()) {\n"
            "    if (isFxEnabled()) {\n"
            '      const wrap = el.closest("label") || el;\n'
            '      el.classList.add("cerolingo-bot-typing");\n'
            '      if (wrap !== el) wrap.classList.add("cerolingo-bot-typing");\n'
            '      spawnBotFxOverlay(wrap, "typing");\n'
            "    }\n"
            "    setReactInputValue(el, value);",
        ),
        (re.compile(r"await sleep\((55|65|80|100)\);"), r"await stepSleep(\1);"),
        ("await sleep(160);", "await sleep(isFastMode() ? 45 : 160);"),
        ("setInterval(handleChallengeStep, 200);", "setInterval(handleChallengeStep, isFastMode() ? 80 : 200);"),
    ],
    "highlight": [
        (
            "if (!existing) document.head.appendChild(storyHighlightStyle);",
            "if (!existing) (document.head || document.documentElement)?.appendChild(storyHighlightStyle);",
        ),
    ],
}


def apply_patches(name, text):
    skipped = 0
    for old, new in PATCHES.get(name, []):
        if isinstance(old, str):
            if old not in text:
                skipped += new not in text  # already applied upstream -> silent no-op
                continue
            text = text.replace(old, new)
        else:
            text, count = old.subn(new, text)
            skipped += 0 if count else 1
    return text, skipped


def _ssl_contexts():
    """System trust store first, then certifi's bundle. The standalone Python the app ships
    on Windows often can't build a CA chain from the OS store (CERTIFICATE_VERIFY_FAILED)."""
    yield ssl.create_default_context()
    try:
        import certifi  # installed with `requests`

        yield ssl.create_default_context(cafile=certifi.where())
    except ImportError:
        pass


_SSL = []  # the context that worked, reused by every download thread


def _fetch(path, timeout=15):
    req = urllib.request.Request(RAW + path, headers={"User-Agent": "cerolingo-bot-manager"})
    error = None
    for ctx in _SSL or _ssl_contexts():
        try:
            with urllib.request.urlopen(req, timeout=timeout, context=ctx) as res:
                data = res.read()
            if not _SSL:
                _SSL.append(ctx)
            return data
        except urllib.error.URLError as exc:
            if not isinstance(getattr(exc, "reason", None), ssl.SSLCertVerificationError):
                raise
            error = exc
    raise error


def _download():
    paths = [f"src/content/{m}.js" for m in MODULES] + [CSS_PATH, MANIFEST_PATH, *ASSET_PATHS]
    with ThreadPoolExecutor(max_workers=8) as pool:
        blobs = list(pool.map(_fetch, paths))
    return dict(zip(paths, blobs))


PATCH_WARNINGS = []


def build_bundle(files):
    with open(BRIDGE_PATH, "r", encoding="utf-8") as f:
        bridge = f.read()

    version = json.loads(files[MANIFEST_PATH].decode("utf-8")).get("version", "0")
    css = files[CSS_PATH].decode("utf-8")
    assets = {}
    for path in ASSET_PATHS:
        mime = "image/svg+xml" if path.endswith(".svg") else "image/png"
        assets[path] = f"data:{mime};base64,{base64.b64encode(files[path]).decode('ascii')}"

    parts = []
    for name in MODULES:
        text = files[f"src/content/{name}.js"].decode("utf-8").replace("\r\n", "\n").rstrip()
        text, skipped = apply_patches(name, text)
        if skipped:
            PATCH_WARNINGS.append(f"{name}.js: {skipped} tweak(s) no longer match upstream and were skipped")
        parts.append(f"// --- src/content/{name}.js ---\n{text}\n")
    modules = "\n".join(parts)

    for marker in ("/*__ASSETS__*/", "/*__CSS__*/", "/*__VERSION__*/", "/*__MODULES__*/"):
        if marker not in bridge:
            raise RuntimeError(f"bridge.js is missing placeholder {marker}")
    bundle = (
        bridge.replace("/*__ASSETS__*/ {}", json.dumps(assets))
        .replace('/*__CSS__*/ ""', json.dumps(css))
        .replace('/*__VERSION__*/ "0"', json.dumps(version))
        .replace("/*__MODULES__*/", modules)
    )
    return bundle, version


def _read_cache():
    try:
        with open(CACHE_FILE, "r", encoding="utf-8") as f:
            bundle = f.read()
        with open(META_FILE, "r", encoding="utf-8") as f:
            meta = json.load(f)
        return bundle, meta
    except Exception:
        return None, {}


def _write_cache(bundle, meta):
    os.makedirs(CACHE_DIR, exist_ok=True)
    for path, content in ((CACHE_FILE, bundle), (META_FILE, json.dumps(meta))):
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            f.write(content)
        os.replace(tmp, path)


def load_bundle(quiet=False):
    """Return the solver bundle source, downloading the newest extension code when online."""
    cached, meta = _read_cache()
    try:
        started = time.time()
        bundle, version = build_bundle(_download())
        changed = bundle != cached
        _write_cache(bundle, {"version": version, "repo": REPO, "fetched": int(time.time())})
        if not quiet:
            for warning in PATCH_WARNINGS:
                ui.warn(warning)
            old = meta.get("version")
            if cached is None:
                ui.ok(f"Mod menu v{version} downloaded from {REPO} ({time.time() - started:.1f}s)")
            elif changed:
                ui.ok(f"Mod menu updated: v{old} -> v{version}")
            else:
                ui.ok(f"Mod menu v{version} is up to date")
        return bundle
    except Exception as exc:
        if cached:
            if not quiet:
                ui.warn(f"Could not reach {REPO} ({exc.__class__.__name__}); using cached mod menu v{meta.get('version', '?')}")
            return cached
        raise RuntimeError(
            f"Could not download the mod menu from github.com/{REPO} and no cached copy exists: {exc}"
        ) from exc


def build_init_script(settings, quiet=False):
    """Bundle prefixed with the settings the Bot Manager wants the mod menu to start with."""
    return f"window.__CEROLINGO_CONFIG__ = {json.dumps(settings)};\n" + load_bundle(quiet)


if __name__ == "__main__":
    if "--update" in sys.argv or len(sys.argv) == 1:
        ui.banner("Mod menu update")
        try:
            text = load_bundle()
            ui.info(f"Bundle size: {len(text) / 1024:.1f} KB  ({CACHE_FILE})")
        except Exception as exc:
            ui.error(str(exc))
            sys.exit(1)
