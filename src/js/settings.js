document.addEventListener("click", (event) => {
  const target = event.target;
  if (target.classList.value.includes("settings-done")) scanInputs();
});

function settings(id) {
  console.log(`Settings for ${id}`);
  if (document.querySelector("#settings").innerHTML != "") return;
  let settingsElement = document.querySelector("#settings");
  let timeout = 0;
  if (settingsElement.classList.value.includes("show-settings")) {
    closeSettings();
    timeout = 600;
  }
  let htmlCode;
  if (id === "farm") {
    htmlCode = `
    <h1>Xp Farm Settings</h1>
    <section id="settings-buttons">
      <label for="lessons-per-account">Lessons per account</label>
      <input
        id="lessons-per-account"
        type="number"
        spellcheck="false"
        min="1"
        max="1000000000"
        step="1"
        oninput="this.value=parseInt(this.value)"
      />
      <section class="settings-checkboxes">
        <div class="settings-checkbox" id="xpMode" type="on">
          <p>Legit</p>
        </div>
        <div class="settings-checkbox" id="xpLoop" type="off">
          <p>Loop</p>
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
      <label for="username">Username prefix</label>
      <input
        id="username"
        type="text"
        spellcheck="false"
        minlength="1"
        maxlength="20"
      />
      <section class="email-section">
        <div class="email-labels">
          <label for="prefix">Prefix</label>
          <label for="domain">Domain</label>
        </div>
        <div>
          <input id="prefix" type="text" spellcheck="false" />
          <p>@</p>
          <input id="domain" type="text" spellcheck="false" />
        </div>
        <p class="example">
          prefix: Andrew32+ | domain: gmail.com
          <br />
          => Andrew32+8934593@gmail.com
        </p>
      </section>
      <section class="settings-checkboxes">
        <div class="settings-checkbox" id="acQualify" type="on">
          <p>Qualify</p>
        </div>
        <div class="settings-checkbox" id="acLoop" type="off">
          <p>Loop</p>
        </div>
        <div class="settings-checkbox" id="acConsole" type="off">
          <p>Console</p>
        </div>
      </section>
    </section>
    <div class="settings-done" onclick="closeSettings()">Done</div>
    `;
  }

  setTimeout(() => {
    settingsElement.innerHTML = htmlCode;
    loadConfig(id);

    settingsElement.classList.add("show-settings");
    settingsElement.classList.add(id);
  }, timeout);
}

function closeSettings() {
  let settingsElement = document.querySelector("#settings");
  settingsElement.setAttribute("class", "");
  setTimeout(() => {
    settingsElement.innerHTML = "";
  }, 600);
}

function loadConfig(id) {
  let localStorageId = `${id}-config`;
  let checkboxes = [];
  let inputs = [];
  if (id === "farm") {
    console.log(localStorage.getItem(localStorageId));
    if (!localStorage.getItem(localStorageId)) {
      let lessonsPerAccount = 9;
      let legit = false;
      let loop = true;
      let console = true;

      let config = {
        lessonsPerAccount: lessonsPerAccount,
        legit: legit,
        loop: loop,
        console: console,
      };
      localStorage.setItem(localStorageId, JSON.stringify(config));
    } else {
      let loadedConfig = JSON.parse(localStorage.getItem(localStorageId));
      console.log(`Loaded config for '${id}'`, loadedConfig);

      let settingsElement = document.querySelector("#settings");
      let lessonsPerAccountInput = settingsElement.querySelector(
        "#lessons-per-account"
      );
      let legitCheckbox = settingsElement.querySelector("#xpMode");
      let loopCheckbox = settingsElement.querySelector("#xpLoop");
      let consoleCheckbox = settingsElement.querySelector("#xpConsole");

      lessonsPerAccountInput.value = loadedConfig.lessonsPerAccount;
      legitCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.legit));
      loopCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.loop));
      consoleCheckbox.setAttribute(
        "type",
        booleanToSwitch(loadedConfig.console)
      );

      checkboxes = [loopCheckbox, legitCheckbox, consoleCheckbox];
      inputs = [lessonsPerAccountInput];
    }
  } else if (id === "create-account") {
    if (!localStorage.getItem(localStorageId)) {
      let usernamePrefix = "Cerolingo";
      let emailPrefix = "cero";
      let emailDomain = "lingo.com";
      let qualify = true;
      let loop = false;
      let console = false;

      let config = {
        usernamePrefix: usernamePrefix,
        emailPrefix: emailPrefix,
        emailDomain: emailDomain,
        qualify: qualify,
        loop: loop,
        console: console,
      };
      localStorage.setItem(localStorageId, JSON.stringify(config));
    } else {
      let loadedConfig = JSON.parse(localStorage.getItem(localStorageId));
      console.log(`Loaded config for '${id}'`, loadedConfig);

      let settingsElement = document.querySelector("#settings");
      let usernameInput = settingsElement.querySelector("#username");
      let emailPrefixInput = settingsElement.querySelector("#prefix");
      let emailDomainInput = settingsElement.querySelector("#domain");
      let qualifyCheckbox = settingsElement.querySelector("#acQualify");
      let loopCheckbox = settingsElement.querySelector("#acLoop");
      let consoleCheckbox = settingsElement.querySelector("#acConsole");

      usernameInput.value = loadedConfig.usernamePrefix;
      emailPrefixInput.value = loadedConfig.emailPrefix;
      emailDomainInput.value = loadedConfig.emailDomain;
      qualifyCheckbox.setAttribute(
        "type",
        booleanToSwitch(loadedConfig.qualify)
      );
      loopCheckbox.setAttribute("type", booleanToSwitch(loadedConfig.loop));
      consoleCheckbox.setAttribute(
        "type",
        booleanToSwitch(loadedConfig.console)
      );

      checkboxes = [qualifyCheckbox, loopCheckbox, consoleCheckbox];
      inputs = [usernameInput, emailPrefixInput, emailDomainInput];
    }
  }

  inputs.forEach((i) => {
    i.setAttribute("configId", id);
  });
  checkboxes.forEach((c) => {
    c.setAttribute("onClick", `saveCheckbox('${id}','${c.id}')`);
  });
}

function booleanToSwitch(bool) {
  return bool ? "on" : "off";
}

function saveCheckbox(id, elementId) {
  let element = document.getElementById(elementId);
  let localStorageId = `${id}-config`;
  let newValue = element.getAttribute("type") === "on" ? false : true;
  let loadedConfig = JSON.parse(localStorage.getItem(localStorageId));
  let config;
  if (id === "farm") {
    if (elementId === "xpMode") {
      config = {
        lessonsPerAccount: loadedConfig.lessonsPerAccount,
        legit: newValue,
        loop: loadedConfig.loop,
        console: loadedConfig.console,
      };
    } else if (elementId === "xpLoop") {
      config = {
        lessonsPerAccount: loadedConfig.lessonsPerAccount,
        legit: loadedConfig.legit,
        loop: newValue,
        console: loadedConfig.console,
      };
    } else if (elementId === "xpConsole") {
      console.log("xpConsole", newValue);
      config = {
        lessonsPerAccount: loadedConfig.lessonsPerAccount,
        legit: loadedConfig.legit,
        loop: loadedConfig.loop,
        console: newValue,
      };
    }
  } else if (id === "create-account") {
    if (elementId === "acQualify") {
      config = {
        usernamePrefix: loadedConfig.usernamePrefix,
        emailPrefix: loadedConfig.emailPrefix,
        emailDomain: loadedConfig.emailDomain,
        qualify: newValue,
        loop: loadedConfig.loop,
        console: loadedConfig.console,
      };
    } else if (elementId === "acLoop") {
      config = {
        usernamePrefix: loadedConfig.usernamePrefix,
        emailPrefix: loadedConfig.emailPrefix,
        emailDomain: loadedConfig.emailDomain,
        qualify: loadedConfig.qualify,
        loop: newValue,
        console: loadedConfig.console,
      };
    } else if (elementId === "acConsole") {
      config = {
        usernamePrefix: loadedConfig.usernamePrefix,
        emailPrefix: loadedConfig.emailPrefix,
        emailDomain: loadedConfig.emailDomain,
        qualify: loadedConfig.qualify,
        loop: loadedConfig.loop,
        console: newValue,
      };
    }
  }
  console.log(config);
  console.log(JSON.stringify(config));
  localStorage.setItem(localStorageId, JSON.stringify(config));
}

function saveInput(id, elementId, text) {
  let localStorageId = `${id}-config`;
  let element = document.getElementById(elementId);
  text = element.getAttribute("type") === "number" ? parseInt(text) : text;
  let loadedConfig = JSON.parse(localStorage.getItem(localStorageId));
  let config;
  if (id === "farm") {
    if (elementId === "lessons-per-account" && text > 0) {
      config = {
        lessonsPerAccount: text,
        legit: loadedConfig.legit,
        loop: loadedConfig.loop,
        console: loadedConfig.console,
      };
    }
  } else if (
    id === "create-account" &&
    text.length > 0 &&
    text.replace(/[^a-z0-9.]/gi, "").length > 0
  ) {
    if (elementId === "username") {
      config = {
        usernamePrefix: text.replace(/[^a-z0-9.]/gi, ""),
        emailPrefix: loadedConfig.emailPrefix,
        emailDomain: loadedConfig.emailDomain,
        qualify: loadedConfig.qualify,
        loop: loadedConfig.loop,
        console: loadedConfig.console,
      };
    } else if (elementId === "prefix") {
      config = {
        usernamePrefix: loadedConfig.usernamePrefix,
        emailPrefix: text.replace(/[^a-z0-9.+]/gi, ""),
        emailDomain: loadedConfig.emailDomain,
        qualify: loadedConfig.qualify,
        loop: loadedConfig.loop,
        console: loadedConfig.console,
      };
    } else if (elementId === "domain") {
      config = {
        usernamePrefix: loadedConfig.usernamePrefix,
        emailPrefix: loadedConfig.emailPrefix,
        emailDomain: text.replace(/[^a-z0-9.]/gi, ""),
        qualify: loadedConfig.qualify,
        loop: loadedConfig.loop,
        console: loadedConfig.console,
      };
    }
  }

  config = config != undefined ? config : loadedConfig;
  localStorage.setItem(localStorageId, JSON.stringify(config));
}

function scanInputs() {
  document.querySelectorAll("input").forEach((i, index) => {
    setTimeout(() => {
      saveInput(i.getAttribute("configId"), i.id, i.value);
    }, index * 10);
  });
}
