// Bridge to the Rust (Tauri) backend; keeps the same window.api shape the UI used before.
(() => {
  const tauri = window.__TAURI__;
  if (!tauri) return;
  const { invoke } = tauri.core;
  const { listen } = tauri.event;
  const handlers = {};
  const on = (event) => (cb) => {
    if (handlers[event]) handlers[event]();
    handlers[event] = null;
    listen(event, (e) => cb(e.payload)).then((unlisten) => {
      handlers[event] = unlisten;
    });
  };
  let updateDone = () => {};

  window.api = {
    runBot: (id, config) => invoke("run_bot", { id, config: config || {} }),
    openExternal: (url) => invoke("open_external", { url }).catch(console.error),
    onSetVersion: (cb) => invoke("app_version").then(cb, () => cb("")),
    onUpdateAvailable: on("update-available"),
    onUpdateProgress: on("update-progress"),
    onSetupProgress: on("setup-progress"),
    onUpdateDone: (cb) => {
      updateDone = cb;
    },
    installUpdate: () =>
      invoke("install_update").then(
        (result) => updateDone(result),
        () => updateDone("opened"),
      ),
  };
})();
