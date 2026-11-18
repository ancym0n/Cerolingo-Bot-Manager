const { contextBridge, ipcRenderer } = require("electron");

function onChannel(channel, handler) {
  ipcRenderer.removeAllListeners(channel);
  ipcRenderer.on(channel, handler);
}

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
      onChannel(channel, (_event, ...args) => func(...args));
    }
  },
  onSetVersion: (callback) =>
    onChannel("set-version", (_event, version) => callback(version)),
  openExternal: (url) => ipcRenderer.send("open-external", url),
  onUpdateAvailable: (cb) =>
    onChannel("update-available", (_e, info) => cb(info)),
  onUpdateProgress: (cb) => onChannel("update-progress", (_e, pct) => cb(pct)),
  onUpdateDone: (cb) => onChannel("update-done", (_e, result) => cb(result)),
  installUpdate: () => ipcRenderer.send("update-install"),
});
