const DEFAULT_CONFIGS = {
  farm: {
    lessonsPerAccount: 9,
    legit: true,
    loop: true,
    hidden: false,
    console: true,
  },
  "create-account": {
    usernamePrefix: "Cerolingo",
    emailPrefix: "cero",
    emailDomain: "lingo.com",
    qualify: true,
    loop: false,
    hidden: false,
    console: true,
  },
  solver: {
    accountIndex: 1,
    autoSolver: true,
    botHighlight: true,
    audioBlock: true,
    storyHighlight: true,
    legendaryBypass: true,
    lessons: 5,
    hidden: false,
    console: true,
  },
};

function ensureDefaultConfigs() {
  Object.entries(DEFAULT_CONFIGS).forEach(([id, defaults]) => {
    const key = `${id}-config`;
    if (!localStorage.getItem(key)) {
      localStorage.setItem(key, JSON.stringify(defaults));
    }
  });
}

ensureDefaultConfigs();

function getSavedConfig(id) {
  ensureDefaultConfigs();
  const key = `${id}-config`;
  try {
    const parsed = JSON.parse(localStorage.getItem(key));
    if (parsed && typeof parsed === "object") {
      return { ...DEFAULT_CONFIGS[id], ...parsed };
    }
  } catch (_) {}
  return { ...DEFAULT_CONFIGS[id] };
}

function settings(id) {
  const settingsElement = document.querySelector("#settings");
  if (!settingsElement) return;

  let timeout = 0;
  if (settingsElement.classList.contains("show-settings")) {
    closeSettings();
    timeout = 320;
  }

  let htmlCode = "";
  if (id === "farm") {
    htmlCode = `
      <h1>XP Farm Settings</h1>
      <section id="settings-buttons">
        <div>
          <label for="lessons-per-account">Lessons per account</label>
          <input
            id="lessons-per-account"
            type="number"
            spellcheck="false"
            min="1"
            max="1000000000"
            step="1"
            oninput="this.value=this.value.replace(/[^0-9]/g, '')"
          />
        </div>
        <section class="settings-checkboxes">
          <div class="settings-checkbox" id="xpMode" type="on">
            <p>Legit</p>
          </div>
          <div class="settings-checkbox" id="xpLoop" type="off">
            <p>Loop</p>
          </div>
          <div class="settings-checkbox" id="xpHidden" type="off">
            <p>Hidden</p>
          </div>
          <div class="settings-checkbox" id="xpConsole" type="off">
            <p>Console</p>
          </div>
        </section>
      </section>
      <div class="settings-done" onclick="closeSettings()">Done</div>
    `;
  } else if (id === "create-account") {
    htmlCode = `
      <h1>Account Botting Settings</h1>
      <section id="settings-buttons">
        <div>
          <label for="username">Username prefix</label>
          <input
            id="username"
            type="text"
            spellcheck="false"
            minlength="1"
            maxlength="20"
          />
        </div>
        <section class="email-section">
          <div class="email-labels">
            <label for="prefix">Email Prefix</label>
            <label for="domain">Domain</label>
          </div>
          <div class="email-inputs">
            <input id="prefix" type="text" spellcheck="false" />
            <p>@</p>
            <input id="domain" type="text" spellcheck="false" />
          </div>
          <p class="example" id="email-example">
            prefix: cero | domain: lingo.com
            <br />
            => cero8934593@lingo.com
          </p>
        </section>
        <section class="settings-checkboxes">
          <div class="settings-checkbox" id="acQualify" type="on">
            <p>Qualify</p>
          </div>
          <div class="settings-checkbox" id="acLoop" type="off">
            <p>Loop</p>
          </div>
          <div class="settings-checkbox" id="acHidden" type="off">
            <p>Hidden</p>
          </div>
          <div class="settings-checkbox" id="acConsole" type="off">
            <p>Console</p>
          </div>
        </section>
      </section>
      <div class="settings-done" onclick="closeSettings()">Done</div>
    `;
  } else if (id === "solver") {
    htmlCode = `
      <h1>Auto Solver Settings</h1>
      <section id="settings-buttons">
        <div>
          <label for="solver-account">Saved account # (0 = manual login)</label>
          <input
            id="solver-account"
            type="number"
            spellcheck="false"
            min="0"
            max="100000"
            step="1"
            oninput="this.value=this.value.replace(/[^0-9]/g, '')"
          />
        </div>
        <div>
          <label for="solver-lessons">Lessons to solve (0 = until closed)</label>
          <input
            id="solver-lessons"
            type="number"
            spellcheck="false"
            min="0"
            max="100000"
            step="1"
            oninput="this.value=this.value.replace(/[^0-9]/g, '')"
          />
        </div>
        <section class="settings-checkboxes">
          <div class="settings-checkbox" id="solverAuto" type="on">
            <p>Auto Solver</p>
          </div>
          <div class="settings-checkbox" id="solverHighlight" type="on">
            <p>Blue FX</p>
          </div>
          <div class="settings-checkbox" id="solverAudio" type="on">
            <p>Audio Block</p>
          </div>
          <div class="settings-checkbox" id="solverPath" type="on">
            <p>Highlighter</p>
          </div>
          <div class="settings-checkbox" id="solverBypass" type="on">
            <p>Bypass</p>
          </div>
          <div class="settings-checkbox" id="solverHidden" type="off">
            <p>Hidden</p>
          </div>
          <div class="settings-checkbox" id="solverConsole" type="on">
            <p>Console</p>
          </div>
        </section>
      </section>
      <div class="settings-done" onclick="closeSettings()">Done</div>
    `;
  } else {
    return;
  }

  setTimeout(() => {
    settingsElement.innerHTML = htmlCode;
    settingsElement.setAttribute("aria-hidden", "false");
    loadConfig(id);

    document.body.classList.add("settings-open");
    settingsElement.className = `show-settings ${id}`;
  }, timeout);
}

function updateEmailExample() {
  const exampleEl = document.getElementById("email-example");
  if (!exampleEl) return;
  const prefixInput = document.getElementById("prefix");
  const domainInput = document.getElementById("domain");
  const prefix =
    (prefixInput?.value || "").replace(/[^a-z0-9.+]/gi, "") || "cero";
  const domain =
    (domainInput?.value || "").replace(/[^a-z0-9.]/gi, "") || "lingo.com";
  exampleEl.innerHTML = `prefix: ${prefix} | domain: ${domain}<br />=> ${prefix}8934593@${domain}`;
}

function closeSettings() {
  const settingsElement = document.querySelector("#settings");
  if (!settingsElement) return;

  scanInputs();
  document.body.classList.remove("settings-open");
  settingsElement.setAttribute("class", "");
  settingsElement.setAttribute("aria-hidden", "true");

  setTimeout(() => {
    if (!settingsElement.classList.contains("show-settings")) {
      settingsElement.innerHTML = "";
    }
  }, 320);
}

function loadConfig(id) {
  const loadedConfig = getSavedConfig(id);
  const settingsElement = document.querySelector("#settings");
  if (!settingsElement) return;

  let checkboxes = [];
  let inputs = [];

  if (id === "farm") {
    const lessonsPerAccountInput = settingsElement.querySelector(
      "#lessons-per-account",
    );
    const legitCheckbox = settingsElement.querySelector("#xpMode");
    const loopCheckbox = settingsElement.querySelector("#xpLoop");
    const hiddenCheckbox = settingsElement.querySelector("#xpHidden");
    const consoleCheckbox = settingsElement.querySelector("#xpConsole");

    lessonsPerAccountInput.value = loadedConfig.lessonsPerAccount;
    legitCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.legit));
    loopCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.loop));
    consoleCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.console));

    hiddenCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.hidden));

    checkboxes = [legitCheckbox, loopCheckbox, hiddenCheckbox, consoleCheckbox];
    inputs = [lessonsPerAccountInput];
  } else if (id === "create-account") {
    const usernameInput = settingsElement.querySelector("#username");
    const emailPrefixInput = settingsElement.querySelector("#prefix");
    const emailDomainInput = settingsElement.querySelector("#domain");
    const qualifyCheckbox = settingsElement.querySelector("#acQualify");
    const loopCheckbox = settingsElement.querySelector("#acLoop");
    const hiddenCheckbox = settingsElement.querySelector("#acHidden");
    const consoleCheckbox = settingsElement.querySelector("#acConsole");

    usernameInput.value = loadedConfig.usernamePrefix;
    emailPrefixInput.value = loadedConfig.emailPrefix;
    emailDomainInput.value = loadedConfig.emailDomain;
    qualifyCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.qualify));
    loopCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.loop));
    hiddenCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.hidden));
    consoleCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.console));

    updateEmailExample();

    checkboxes = [
      qualifyCheckbox,
      loopCheckbox,
      hiddenCheckbox,
      consoleCheckbox,
    ];
    inputs = [usernameInput, emailPrefixInput, emailDomainInput];
  } else if (id === "solver") {
    const accountInput = settingsElement.querySelector("#solver-account");
    const lessonsInput = settingsElement.querySelector("#solver-lessons");
    const hiddenCheckbox = settingsElement.querySelector("#solverHidden");
    const autoCheckbox = settingsElement.querySelector("#solverAuto");
    const highlightCheckbox = settingsElement.querySelector("#solverHighlight");
    const audioCheckbox = settingsElement.querySelector("#solverAudio");
    const pathCheckbox = settingsElement.querySelector("#solverPath");
    const bypassCheckbox = settingsElement.querySelector("#solverBypass");
    const consoleCheckbox = settingsElement.querySelector("#solverConsole");

    accountInput.value = loadedConfig.accountIndex;
    lessonsInput.value = loadedConfig.lessons;
    hiddenCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.hidden));
    autoCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.autoSolver));
    highlightCheckbox.setAttribute(
      "type",
      booleanToSwitch(loadedConfig.botHighlight),
    );
    audioCheckbox.setAttribute(
      "type",
      booleanToSwitch(loadedConfig.audioBlock),
    );
    pathCheckbox.setAttribute(
      "type",
      booleanToSwitch(loadedConfig.storyHighlight),
    );
    bypassCheckbox.setAttribute(
      "type",
      booleanToSwitch(loadedConfig.legendaryBypass),
    );
    consoleCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.console));

    checkboxes = [
      autoCheckbox,
      highlightCheckbox,
      audioCheckbox,
      pathCheckbox,
      bypassCheckbox,
      hiddenCheckbox,
      consoleCheckbox,
    ];
    inputs = [accountInput, lessonsInput];
  }

  inputs.forEach((inputEl) => {
    inputEl.setAttribute("configId", id);
    inputEl.addEventListener("input", () => {
      saveInput(id, inputEl.id, inputEl.value);
      if (id === "create-account") {
        updateEmailExample();
      }
    });
  });

  checkboxes.forEach((checkboxEl) => {
    checkboxEl.setAttribute(
      "onClick",
      `saveCheckbox('${id}','${checkboxEl.id}')`,
    );
  });
}

function booleanToSwitch(bool) {
  return bool ? "on" : "off";
}

function saveCheckbox(id, elementId) {
  const element = document.getElementById(elementId);
  if (!element) return;

  const localStorageId = `${id}-config`;
  const currentOn = element.getAttribute("type") === "on";
  const newValue = !currentOn;

  element.setAttribute("type", booleanToSwitch(newValue));

  const loadedConfig = getSavedConfig(id);
  const config = { ...loadedConfig };

  if (id === "farm") {
    if (elementId === "xpMode") config.legit = newValue;
    else if (elementId === "xpLoop") config.loop = newValue;
    else if (elementId === "xpHidden") config.hidden = newValue;
    else if (elementId === "xpConsole") config.console = newValue;
  } else if (id === "create-account") {
    if (elementId === "acQualify") config.qualify = newValue;
    else if (elementId === "acLoop") config.loop = newValue;
    else if (elementId === "acHidden") config.hidden = newValue;
    else if (elementId === "acConsole") config.console = newValue;
  } else if (id === "solver") {
    if (elementId === "solverAuto") config.autoSolver = newValue;
    else if (elementId === "solverHighlight") config.botHighlight = newValue;
    else if (elementId === "solverAudio") config.audioBlock = newValue;
    else if (elementId === "solverPath") config.storyHighlight = newValue;
    else if (elementId === "solverBypass") config.legendaryBypass = newValue;
    else if (elementId === "solverHidden") config.hidden = newValue;
    else if (elementId === "solverConsole") config.console = newValue;
  }

  localStorage.setItem(localStorageId, JSON.stringify(config));
}

function saveInput(id, elementId, text) {
  if (!id) return;
  const localStorageId = `${id}-config`;
  const element = document.getElementById(elementId);
  if (!element) return;

  const loadedConfig = getSavedConfig(id);
  const config = { ...loadedConfig };

  if (id === "farm") {
    const num = parseInt(text, 10);
    if (
      elementId === "lessons-per-account" &&
      Number.isFinite(num) &&
      num > 0
    ) {
      config.lessonsPerAccount = num;
    }
  } else if (id === "create-account") {
    const raw = String(text || "");
    if (elementId === "username") {
      const cleaned = raw.replace(/[^a-z0-9.]/gi, "");
      if (cleaned.length > 0) config.usernamePrefix = cleaned;
    } else if (elementId === "prefix") {
      const cleaned = raw.replace(/[^a-z0-9.+]/gi, "");
      if (cleaned.length > 0) config.emailPrefix = cleaned;
    } else if (elementId === "domain") {
      const cleaned = raw.replace(/[^a-z0-9.]/gi, "");
      if (cleaned.length > 0) config.emailDomain = cleaned;
    }
  } else if (id === "solver") {
    const num = parseInt(text, 10);
    if (Number.isFinite(num) && num >= 0) {
      if (elementId === "solver-account") config.accountIndex = num;
      else if (elementId === "solver-lessons") config.lessons = num;
    }
  }

  localStorage.setItem(localStorageId, JSON.stringify(config));
}

function scanInputs() {
  document.querySelectorAll("#settings input").forEach((inputEl) => {
    saveInput(inputEl.getAttribute("configId"), inputEl.id, inputEl.value);
  });
}
