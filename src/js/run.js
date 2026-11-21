const setupOverlay = document.getElementById("setup-overlay");
const setupText = document.getElementById("setup-text");
const setupFill = document.getElementById("setup-fill");
const setupClose = document.getElementById("setup-close");

function showSetup(text, fraction, error) {
  if (!setupOverlay) return;
  setupOverlay.hidden = false;
  setupOverlay.classList.toggle("error", Boolean(error));
  setupText.textContent = text;
  setupFill.style.width = `${Math.round(Math.min(Math.max(fraction, 0), 1) * 100)}%`;
  setupClose.hidden = !error;
}

function hideSetup() {
  if (setupOverlay) setupOverlay.hidden = true;
}

if (setupClose) setupClose.addEventListener("click", hideSetup);

if (window.api && typeof window.api.onSetupProgress === "function") {
  window.api.onSetupProgress((p) => {
    if (p.error) showSetup(p.error, 0, true);
    else if (p.done) hideSetup();
    else showSetup(p.text, p.fraction, false);
  });
}

function runScript(id) {
  if (!id) return;
  const loadedConfig =
    typeof getSavedConfig === "function"
      ? getSavedConfig(id)
      : JSON.parse(localStorage.getItem(`${id}-config`));
  if (loadedConfig === null && id !== "manage-accounts") return;

  if (typeof cerolingoLog === "function") {
    cerolingoLog("run.js", `Starting ${id}`, loadedConfig);
  }
  if (!window.api || typeof window.api.runBot !== "function") return;
  window.api.runBot(id, loadedConfig || {}).catch((err) => {
    showSetup(String(err), 0, true);
  });
}
