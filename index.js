const { app, BrowserWindow, ipcMain, shell } = require("electron");
const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");
const { ensureRuntime } = require("./src/js/runtime");
const updater = require("./src/js/updater");

if (process.platform === "linux" && process.env.APPDIR) {
  const schemaDir = `${process.env.APPDIR}/usr/share/glib-2.0/schemas`;
  if (
    process.env.GSETTINGS_SCHEMA_DIR &&
    !fs.existsSync(path.join(schemaDir, "gschemas.compiled"))
  ) {
    const cleaned = process.env.GSETTINGS_SCHEMA_DIR.split(path.delimiter)
      .filter((p) => p && p !== schemaDir)
      .join(path.delimiter);
    if (cleaned) process.env.GSETTINGS_SCHEMA_DIR = cleaned;
    else delete process.env.GSETTINGS_SCHEMA_DIR;
  }
}

const ICON_PNG = path.join(__dirname, "src", "img", "logo.png");
const ICON_ICO = path.join(__dirname, "build", "logo.ico");

function getCleanChildEnv() {
  const cleanEnv = { ...process.env };
  if (process.platform === "linux" && process.env.APPDIR) {
    delete cleanEnv.LD_LIBRARY_PATH;
    delete cleanEnv.GSETTINGS_SCHEMA_DIR;
  }
  return cleanEnv;
}

function openWithCleanEnv(target) {
  if (process.platform === "linux" && process.env.APPDIR) {
    const cleanEnv = getCleanChildEnv();
    try {
      const child = spawn("xdg-open", [target], {
        detached: true,
        stdio: "ignore",
        env: cleanEnv,
      });
      child.on("error", () => {
        if (/^https?:\/\//.test(target)) shell.openExternal(target);
        else shell.openPath(target);
      });
      child.unref();
      return;
    } catch (_) {}
  }
  if (/^https?:\/\//.test(target)) {
    shell.openExternal(target);
  } else {
    shell.openPath(target);
  }
}

function findExecutable(binaryName) {
  const pathEnv = process.env.PATH || "";
  const pathExt =
    process.platform === "win32"
      ? (process.env.PATHEXT || ".EXE;.CMD;.BAT").split(";")
      : [""];
  const extraDirs =
    process.platform === "win32"
      ? []
      : [
          path.join(os.homedir(), ".local", "bin"),
          "/usr/local/bin",
          "/usr/bin",
          "/bin",
        ];
  const dirs = [...pathEnv.split(path.delimiter), ...extraDirs].filter(Boolean);

  for (const dir of dirs) {
    for (const ext of pathExt) {
      const fullPath = path.join(dir, binaryName + ext);
      try {
        if (fs.existsSync(fullPath) && fs.statSync(fullPath).isFile()) {
          return fullPath;
        }
      } catch (_) {}
    }
  }
  return null;
}

function shQuote(v) {
  return `'${String(v).replace(/'/g, `'\\''`)}'`;
}

function launchWindowsTerminal(pythonCmd, scriptPath, strArgs) {
  const batArg = (a) =>
    `"${String(a).replace(/%/g, "%%").replace(/"/g, '\\"')}"`;
  const lines = [
    "@echo off",
    "chcp 65001 >nul",
    "title Cerolingo Bot Manager",
    `"${pythonCmd}" "${scriptPath}" ${strArgs.map(batArg).join(" ")}`.trim(),
    "echo.",
    "echo [Process finished with exit code %errorlevel%]",
    "pause",
  ];
  const batPath = path.join(
    os.tmpdir(),
    `cbm-run-${Date.now()}-${Math.floor(Math.random() * 1e6)}.bat`,
  );
  fs.writeFileSync(batPath, lines.join("\r\n") + "\r\n");
  const child = spawn(
    process.env.ComSpec || "cmd.exe",
    ["/d", "/s", "/c", `start "Cerolingo Bot Manager" "${batPath}"`],
    {
      detached: true,
      stdio: "ignore",
      windowsHide: true,
      windowsVerbatimArguments: true,
    },
  );
  child.on("error", () => {});
  child.unref();
}

function launchLinuxTerminal(command) {
  const holdCommand = `printf '\\033]0;Cerolingo Bot Manager\\007'; ${command}; printf '\\nPress Enter to close...'; read _`;
  const candidates = [
    ["x-terminal-emulator", ["-e", "sh", "-c", holdCommand]],
    ["ghostty", ["-e", "sh", "-c", holdCommand]],
    ["alacritty", ["-e", "sh", "-c", holdCommand]],
    ["kitty", ["sh", "-c", holdCommand]],
    ["foot", ["sh", "-c", holdCommand]],
    ["wezterm", ["start", "--", "sh", "-c", holdCommand]],
    ["gnome-terminal", ["--", "sh", "-c", holdCommand]],
    ["konsole", ["-e", "sh", "-c", holdCommand]],
    ["xfce4-terminal", ["-x", "sh", "-c", holdCommand]],
    ["xterm", ["-e", "sh", "-c", holdCommand]],
  ];

  const cleanEnv = getCleanChildEnv();
  for (const [bin, binArgs] of candidates) {
    const resolved = findExecutable(bin);
    if (resolved) {
      const child = spawn(resolved, binArgs, {
        detached: true,
        stdio: "ignore",
        env: cleanEnv,
      });
      child.on("error", () => {});
      child.unref();
      return;
    }
  }

  const fallback = spawn("sh", ["-c", command], {
    detached: true,
    stdio: "ignore",
    env: cleanEnv,
  });
  fallback.on("error", () => {});
  fallback.unref();
}

async function runPythonScript(scriptName, rawArgs = []) {
  const pythonCmd = await ensureRuntime(ICON_PNG);
  return new Promise((resolve, reject) => {
    const isDev = !app.isPackaged;
    const scriptPath = isDev
      ? path.join(app.getAppPath(), "src", "python", scriptName)
      : path.join(process.resourcesPath, "python", scriptName);
    const args = Array.isArray(rawArgs) ? [...rawArgs] : [];
    const openInNewTerminal =
      args.length > 0 && typeof args[args.length - 1] === "boolean"
        ? args.pop()
        : false;
    const strArgs = args.map((arg) => String(arg));
    const quotedArgs = strArgs
      .map((a) => (/[\s"]/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a))
      .join(" ");
    const command =
      process.platform === "linux"
        ? [pythonCmd, scriptPath, ...strArgs].map(shQuote).join(" ")
        : `"${pythonCmd}" "${scriptPath}" ${quotedArgs}`.trim();

    if (openInNewTerminal) {
      if (process.platform === "win32") {
        launchWindowsTerminal(pythonCmd, scriptPath, strArgs);
      } else if (process.platform === "darwin") {
        const escaped = command.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
        spawn("osascript", [
          "-e",
          `tell application "Terminal" to do script "${escaped}"`,
        ]);
      } else {
        launchLinuxTerminal(command);
      }
      resolve("Script started in new terminal.");
    } else {
      const child = spawn(pythonCmd, [scriptPath, ...strArgs], {
        stdio: "ignore",
        windowsHide: true,
        env: getCleanChildEnv(),
      });
      child.on("error", (err) => reject(new Error(`Error: ${err.message}`)));
      child.on("close", (code) =>
        resolve(`${scriptName} exited with code ${code}`),
      );
    }
  });
}

function createWindow() {
  const iconPath =
    process.platform === "win32" && fs.existsSync(ICON_ICO)
      ? ICON_ICO
      : ICON_PNG;

  const mainWindow = new BrowserWindow({
    width: 820,
    height: 620,
    minWidth: 360,
    minHeight: 480,
    resizable: true,
    fullscreenable: true,
    autoHideMenuBar: true,
    backgroundColor: "#131f24",
    icon: iconPath,
    webPreferences: {
      preload: path.join(__dirname, "src", "js", "preload.js"),
      contextIsolation: true,
      sandbox: false,
      nodeIntegration: false,
      devTools: false,
    },
  });

  mainWindow
    .loadFile(path.join(__dirname, "src", "views", "index.html"))
    .then(() => {
      if (!mainWindow.isDestroyed()) {
        mainWindow.webContents.send("set-version", app.getVersion());
      }
    })
    .catch(() => {});
  mainWindow.once("ready-to-show", () => {
    if (!mainWindow.isDestroyed()) {
      mainWindow.show();
      mainWindow.webContents.send("set-version", app.getVersion());
    }
  });
  return mainWindow;
}

const gotLock = app.requestSingleInstanceLock();
if (!gotLock) app.quit();

app.on("second-instance", () => {
  const [win] = BrowserWindow.getAllWindows();
  if (win) {
    if (win.isMinimized()) win.restore();
    win.focus();
  }
});

if (gotLock)
  app.whenReady().then(() => {
    process.env.CBM_VERSION = app.getVersion();
    const mainWindow = createWindow();
    updater.start((info) => {
      if (!mainWindow.isDestroyed())
        mainWindow.webContents.send("update-available", info);
    });
    ipcMain.on("update-install", async () => {
      const send = (pct) => {
        if (!mainWindow.isDestroyed())
          mainWindow.webContents.send("update-progress", pct);
      };
      const result = await updater.installUpdate(send);
      if (!mainWindow.isDestroyed())
        mainWindow.webContents.send("update-done", result);
    });
    ipcMain.on("python", (event, data) => {
      runPythonScript(data[0], data[1])
        .then((output) => {
          if (output) console.log(output);
        })
        .catch((err) => console.error(err.message));
    });

    app.on("activate", () => {
      if (process.platform === "darwin") {
        app.dock.setIcon(ICON_PNG);
      }
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  });

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

ipcMain.on("open-external", (_event, url) => {
  if (!/^https?:\/\//.test(String(url || ""))) {
    console.error("URL must include http:// or https://");
    return;
  }
  openWithCleanEnv(String(url));
});
