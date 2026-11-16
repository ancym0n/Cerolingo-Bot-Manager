function cerolingoLog(tag, message, ...extra) {
  console.log(
    `%c CerolingoBM · ${tag} %c ${message} `,
    "background: #6877fe; color: #e7ecff; padding: 3px 6px; border-radius: 4px 0 0 4px; font-weight: bold; font-family: sans-serif;",
    "background: #a1b2ff; color: #454faa; padding: 3px 6px; border-radius: 0 4px 4px 0; font-weight: bold; font-family: sans-serif;",
    ...extra,
  );
}

cerolingoLog("main.js", "loaded");

function applyVersion(rawVersion) {
  if (!rawVersion) return;
  const clean = String(rawVersion).replace(/^v/i, "");
  document.title = `CerolingoBM v${clean}`;
  const versionEl = document.getElementById("version");
  if (versionEl) {
    versionEl.textContent = `v${clean}`;
  }
}

if (window.api && typeof window.api.onSetVersion === "function") {
  window.api.onSetVersion((version) => {
    applyVersion(version);
  });
} else {
  applyVersion("");
}

const allButtons = document.querySelectorAll(".button");
allButtons.forEach((buttonEl) => {
  const id = buttonEl.id;
  if (!id) return;

  const buttonText = buttonEl.querySelector(".button-text");
  const buttonSettings = buttonEl.querySelector(".button-settings");

  if (buttonText) {
    buttonText.setAttribute("onClick", `runScript('${id}')`);
  }
  if (buttonSettings) {
    buttonSettings.setAttribute("onClick", `settings('${id}')`);
  }
});

document.addEventListener("click", (event) => {
  const target = event.target;

  const link = target.closest?.("a");
  if (link && link.href && link.href.startsWith("http")) {
    if (window.api && typeof window.api.openExternal === "function") {
      event.preventDefault();
      window.api.openExternal(link.href);
    }
    return;
  }

  if (target.id === "settings-backdrop") {
    if (typeof closeSettings === "function") {
      closeSettings();
    }
    return;
  }

  const btnTarget = target.closest?.(".button-text, .button-settings");
  if (btnTarget && !btnTarget.closest(".is-soon")) {
    btnTarget.classList.add("transform-scale");
    setTimeout(() => {
      btnTarget.classList.remove("transform-scale");
    }, 220);
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && typeof closeSettings === "function") {
    const settingsEl = document.getElementById("settings");
    if (settingsEl && settingsEl.classList.contains("show-settings")) {
      closeSettings();
    }
  }
});

// Optional update prompt: never forced, "Later" hides it for that version.
(() => {
  if (!window.api || !window.api.onUpdateAvailable) return;
  const toast = document.getElementById("update-toast");
  const text = document.getElementById("update-text");
  const install = document.getElementById("update-install");
  const later = document.getElementById("update-later");
  const skipKey = "skipped-update";
  let version = "";

  window.api.onUpdateAvailable((info) => {
    if (localStorage.getItem(skipKey) === info.version) return;
    version = info.version;
    text.textContent = `Version ${version} is available`;
    install.textContent = info.direct ? "Update" : "Open release";
    toast.hidden = false;
  });
  later.addEventListener("click", () => {
    localStorage.setItem(skipKey, version);
    toast.hidden = true;
  });
  install.addEventListener("click", () => {
    install.disabled = later.disabled = true;
    text.textContent = "Downloading update...";
    window.api.installUpdate();
  });
  window.api.onUpdateProgress((pct) => {
    text.textContent = `Downloading update... ${Math.round(pct * 100)}%`;
  });
  window.api.onUpdateDone((result) => {
    install.disabled = later.disabled = false;
    if (result === "opened") toast.hidden = true;
    else text.textContent = "Installing update...";
  });
})();
