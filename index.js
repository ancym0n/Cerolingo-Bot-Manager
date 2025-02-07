const { app, BrowserWindow, ipcMain, shell } = require("electron");
const { exec, spawn } = require("child_process");
const path = require("path");
if (require("electron-squirrel-startup")) return app.quit();

function runPythonScript(scriptName, args = []) {
  return new Promise((resolve, reject) => {
    const isDev =
      process.defaultApp || /[\\/]electron[\\/]/.test(process.execPath);
    const scriptPath = isDev
      ? path.join(app.getAppPath(), "src", "python", scriptName)
      : path.join(process.resourcesPath, "python", scriptName);
    const pythonCmd = process.platform === "win32" ? "py" : "python3";
    const openInNewTerminal = args[args.length - 1] === true;
    if (openInNewTerminal) {
      args.pop();
    }

    let pythonScriptPath = isDev
      ? "src/python/" + scriptName
      : "resources/python/" + scriptName;
    const command = `${pythonCmd} ${pythonScriptPath} ${args.join(" ")}`;
    console.log(command);

    if (openInNewTerminal) {
      if (process.platform === "win32") {
        spawn("cmd", ["/c", "start", "cmd", "/c", String(command)], {
          detached: true,
        });
      } else if (process.platform === "darwin") {
        spawn("osascript", [
          "-e",
          `tell application "Terminal" to do script "${command}"`,
        ]);
      } else {
        spawn("x-terminal-emulator", ["-e", command], { detached: true });
      }
      resolve("Script started in new terminal.");
    } else {
      exec(command, (error, stdout, stderr) => {
        if (error) {
          return reject(new Error(`Error: ${error.message}`));
        }
        if (stderr) {
          return reject(new Error(`Stderr: ${stderr}`));
        }
        return resolve(stdout);
      });
    }
  });
}

function createWindow() {
  const mainWindow = new BrowserWindow({
    width: 800,
    height: 600,
    resizable: false,
    fullscreenable: false,
    autoHideMenuBar: true,
    icon: path.join(__dirname, "scr/img/logo.ico"),
    webPreferences: {
      preload: path.join(__dirname, "src/js/preload.js"),
      contextIsolation: true,
      enableRemoteModule: false,
      nodeIntegration: false,
      devTools: false, // CHANGE TO FALSE
    },
  });

  mainWindow.loadFile(path.join(__dirname, "src", "views", "index.html"));
  mainWindow.once("ready-to-show", () => {
    mainWindow.show();
    mainWindow.webContents.send("set-version", app.getVersion());
  });
}

app.whenReady().then(() => {
  createWindow();
  ipcMain.on("python", (event, data) => {
    runPythonScript(data[0], data[1]).then((output) => console.log(output));
  });

  app.on("activate", () => {
    if (process.platform === "darwin") {
      app.dock.setIcon(path.join(__dirname, "scr/img/logo.png"));
    }
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

ipcMain.on("open-external", (event, url) => {
  if (!/^https?:\/\//.test(url)) {
    console.error("URL must include http:// or https://");
    return;
  }
  shell.openExternal(url);
});
