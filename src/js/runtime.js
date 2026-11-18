// First-run bootstrap: downloads a private Python, installs the Python
// packages and the Playwright Chromium browser into the user's data folder,
// so the app works on machines without Node.js / Python / Playwright.
const { app, BrowserWindow } = require("electron");
const { spawn } = require("child_process");
const crypto = require("crypto");
const fs = require("fs");
const https = require("https");
const path = require("path");

const PBS_RELEASE = "20261110";
const PBS_PYTHON = "3.12.16";
const PBS_ASSETS = {
  win32: {
    file: `cpython-${PBS_PYTHON}+${PBS_RELEASE}-x86_64-pc-windows-msvc-install_only.tar.gz`,
    sha256: "e23fe5b16926de1d81661b1a97352e89fb167549c55a6825796ccd0f32ec7f1b",
  },
  linux: {
    file: `cpython-${PBS_PYTHON}+${PBS_RELEASE}-x86_64-unknown-linux-gnu-install_only.tar.gz`,
    sha256: "c2f87d4b4e8db3abddb0f116c6ccfc503d0cbdbb9ee8faacddad1b679ec12e99",
  },
};
const PY_PACKAGES = ["playwright", "requests", "certifi"];
const MARKER_VERSION = 1;

const runtimeDir = () => path.join(app.getPath("userData"), "runtime");
const browsersDir = () => path.join(runtimeDir(), "browsers");
const markerPath = () => path.join(runtimeDir(), "ready.json");
const pythonPath = () =>
  process.platform === "win32"
    ? path.join(runtimeDir(), "python", "python.exe")
    : path.join(runtimeDir(), "python", "bin", "python3");

function applyEnv() {
  process.env.PLAYWRIGHT_BROWSERS_PATH = browsersDir();
  if (app.isPackaged) process.env.CBM_DATA_DIR = app.getPath("userData");
  process.env.PYTHONUTF8 = "1";
  process.env.PYTHONIOENCODING = "utf-8";
}

function isReady() {
  try {
    const m = JSON.parse(fs.readFileSync(markerPath(), "utf8"));
    return m.version === MARKER_VERSION && fs.existsSync(pythonPath());
  } catch (_) {
    return false;
  }
}

function download(url, dest, onProgress, redirects = 0) {
  return new Promise((resolve, reject) => {
    https
      .get(
        url,
        { headers: { "User-Agent": "cerolingo-bot-manager" } },
        (res) => {
          if (
            [301, 302, 303, 307, 308].includes(res.statusCode) &&
            res.headers.location
          ) {
            res.resume();
            if (redirects > 8) return reject(new Error("Too many redirects"));
            return resolve(
              download(res.headers.location, dest, onProgress, redirects + 1),
            );
          }
          if (res.statusCode !== 200) {
            res.resume();
            return reject(
              new Error(`Download failed (HTTP ${res.statusCode})`),
            );
          }
          const total = Number(res.headers["content-length"] || 0);
          let received = 0;
          const out = fs.createWriteStream(dest);
          res.on("data", (chunk) => {
            received += chunk.length;
            if (total) onProgress(received / total);
          });
          res.pipe(out);
          out.on("finish", () => out.close(resolve));
          out.on("error", reject);
          res.on("error", reject);
        },
      )
      .on("error", reject);
  });
}

function sha256File(file) {
  return new Promise((resolve, reject) => {
    const h = crypto.createHash("sha256");
    fs.createReadStream(file)
      .on("data", (d) => h.update(d))
      .on("end", () => resolve(h.digest("hex")))
      .on("error", reject);
  });
}

function run(cmd, args, opts = {}) {
  return new Promise((resolve, reject) => {
    const childEnv = { ...process.env };
    if (process.platform === "linux" && process.env.APPDIR) {
      delete childEnv.LD_LIBRARY_PATH;
      delete childEnv.GSETTINGS_SCHEMA_DIR;
    }
    const child = spawn(cmd, args, {
      windowsHide: true,
      env: childEnv,
      ...opts,
    });
    let tail = "";
    const collect = (d) => {
      tail = (tail + d.toString()).slice(-1500);
    };
    child.stdout.on("data", collect);
    child.stderr.on("data", collect);
    child.on("error", reject);
    child.on("close", (code) =>
      code === 0
        ? resolve()
        : reject(
            new Error(`${path.basename(cmd)} exited with ${code}\n${tail}`),
          ),
    );
  });
}

const PAGE = `<!doctype html><meta charset="utf-8"><style>
*{box-sizing:border-box}body{margin:0;height:100vh;display:flex;flex-direction:column;
align-items:center;justify-content:center;background:#131f24;color:#fff;
font-family:Nunito,'Segoe UI',Arial,sans-serif;padding:24px;text-align:center;user-select:none}
h1{font-size:20px;margin:0 0 6px}p{margin:0 0 18px;color:#8fa4ff;font-size:13px}
.bar{width:100%;height:12px;background:#18272e;border-radius:8px;overflow:hidden}
#fill{height:100%;width:0;background:linear-gradient(90deg,#6877fe,#a1b2ff);
border-radius:8px;transition:width .25s}#step{margin-top:14px;font-size:13px;color:#b3d0ff;min-height:34px}
#err{color:#ff8585;font-size:12px;white-space:pre-wrap;max-height:90px;overflow:auto}
</style><h1>Setting up Cerolingo Bot Manager</h1>
<p>First launch only - downloading Python, Playwright and Chromium.<br>This can take a few minutes.</p>
<div class="bar"><div id="fill"></div></div><div id="step">Starting...</div><div id="err"></div>
<script>function set(p,t){document.getElementById('fill').style.width=(p*100)+'%';
document.getElementById('step').textContent=t}
function fail(m){document.getElementById('step').textContent='Setup failed';
document.getElementById('err').textContent=m}</script>`;

function openProgressWindow(iconPath) {
  const win = new BrowserWindow({
    width: 480,
    height: 280,
    resizable: false,
    minimizable: false,
    maximizable: false,
    autoHideMenuBar: true,
    backgroundColor: "#131f24",
    icon: iconPath,
    title: "Cerolingo Bot Manager - Setup",
    webPreferences: {
      contextIsolation: true,
      sandbox: false,
      nodeIntegration: false,
    },
  });
  win.setMenuBarVisibility(false);
  win
    .loadURL("data:text/html;charset=utf-8," + encodeURIComponent(PAGE))
    .catch(() => {});
  const ready = new Promise((r) => win.webContents.once("did-finish-load", r));
  const call = async (fn, ...args) => {
    await ready;
    if (!win.isDestroyed())
      win.webContents
        .executeJavaScript(
          `${fn}(${args.map((a) => JSON.stringify(a)).join(",")})`,
        )
        .catch(() => {});
  };
  return {
    win,
    set: (p, t) => call("set", p, t),
    fail: (m) => call("fail", m),
  };
}

async function install(ui) {
  const asset = PBS_ASSETS[process.platform];
  if (!asset) throw new Error(`Unsupported platform: ${process.platform}`);
  const root = runtimeDir();
  fs.rmSync(root, { recursive: true, force: true });
  fs.mkdirSync(root, { recursive: true });

  // 1. Python (0 - 35%)
  const archive = path.join(root, "python.tar.gz");
  const url = `https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_RELEASE}/${encodeURIComponent(asset.file)}`;
  ui.set(0.01, `Downloading Python ${PBS_PYTHON}...`);
  let lastPct = -1;
  await download(url, archive, (f) => {
    const pct = Math.round(f * 100);
    if (pct === lastPct) return;
    lastPct = pct;
    ui.set(0.01 + f * 0.3, `Downloading Python ${PBS_PYTHON}... ${pct}%`);
  });
  ui.set(0.31, "Verifying download...");
  if ((await sha256File(archive)) !== asset.sha256)
    throw new Error("Python archive checksum mismatch");
  ui.set(0.33, "Extracting Python...");
  const sysTar =
    process.platform === "win32"
      ? path.join(
          process.env.SystemRoot || "C:\\Windows",
          "System32",
          "tar.exe",
        )
      : null;
  const tarCmd = sysTar && fs.existsSync(sysTar) ? sysTar : "tar";
  await run(tarCmd, ["-xzf", archive, "-C", root]);
  fs.rmSync(archive, { force: true });
  if (!fs.existsSync(pythonPath()))
    throw new Error("Python executable not found after extraction");

  // 2. Python packages (35 - 60%)
  ui.set(0.4, `Installing ${PY_PACKAGES.join(", ")}...`);
  await run(pythonPath(), [
    "-m",
    "pip",
    "install",
    "--disable-pip-version-check",
    "--no-warn-script-location",
    ...PY_PACKAGES,
  ]);

  // 3. Chromium (60 - 100%)
  ui.set(0.65, "Downloading Chromium browser (this is the big one)...");
  await run(pythonPath(), ["-m", "playwright", "install", "chromium"]);

  fs.writeFileSync(
    markerPath(),
    JSON.stringify({ version: MARKER_VERSION, python: PBS_PYTHON }),
  );
  ui.set(1, "All done!");
}

let pending = null;

// Resolves to the path of the Python interpreter to use for the bot scripts.
function ensureRuntime(iconPath) {
  applyEnv();
  if (isReady()) return Promise.resolve(pythonPath());
  if (pending) return pending;
  pending = (async () => {
    const ui = openProgressWindow(iconPath);
    try {
      await install(ui);
      await new Promise((r) => setTimeout(r, 600));
      return pythonPath();
    } catch (err) {
      fs.rmSync(runtimeDir(), { recursive: true, force: true });
      await ui.fail(String(err.message || err));
      await new Promise((r) => setTimeout(r, 6000));
      throw err;
    } finally {
      pending = null;
      if (!ui.win.isDestroyed()) ui.win.close();
    }
  })();
  return pending;
}

module.exports = { ensureRuntime, pythonPath };
