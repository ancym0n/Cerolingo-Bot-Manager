const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("api", {
  send: (channel, data) => {
    const validChannels = ["python"];
    if (validChannels.includes(channel)) {
      ipcRenderer.send(channel, data);
    }
  },
  receive: (channel, func) => {
    const validChannels = ["python"];
    if (validChannels.includes(channel)) {
      ipcRenderer.on(channel, (event, ...args) => func(...args));
    }
  },
  onSetVersion: (callback) =>
    ipcRenderer.on("set-version", (event, version) => callback(version)),
  openExternal: (url) => ipcRenderer.send("open-external", url),
  onUpdateAvailable: (cb) =>
    ipcRenderer.on("update-available", (_e, info) => cb(info)),
  onUpdateProgress: (cb) =>
    ipcRenderer.on("update-progress", (_e, pct) => cb(pct)),
  onUpdateDone: (cb) =>
    ipcRenderer.on("update-done", (_e, result) => cb(result)),
  installUpdate: () => ipcRenderer.send("update-install"),
});
