function runScript(id) {
  if (!id) return;

  let fileName;
  let options = [];

  const localStorageId = `${id}-config`;
  const loadedConfig =
    typeof getSavedConfig === "function"
      ? getSavedConfig(id)
      : JSON.parse(localStorage.getItem(localStorageId));

  if (loadedConfig === null && id !== "manage-accounts") return;

  if (id === "manage-accounts") {
    fileName = "manageAccounts.py";
  } else if (id === "create-account") {
    fileName = "createAccount.py";
    options = [
      loadedConfig.usernamePrefix,
      loadedConfig.emailPrefix,
      loadedConfig.emailDomain,
      loadedConfig.qualify,
      loadedConfig.loop,
      loadedConfig.hidden,
      loadedConfig.console,
    ];
  } else if (id === "farm") {
    fileName = "farmXp.py";
    options = [
      loadedConfig.lessonsPerAccount,
      loadedConfig.legit,
      loadedConfig.loop,
      loadedConfig.hidden,
      loadedConfig.console,
    ];
  } else if (id === "solver") {
    fileName = "autoSolver.py";
    options = [
      loadedConfig.accountIndex,
      loadedConfig.autoSolver,
      loadedConfig.botHighlight,
      loadedConfig.audioBlock,
      loadedConfig.storyHighlight,
      loadedConfig.legendaryBypass,
      loadedConfig.lessons,
      loadedConfig.hidden,
      loadedConfig.console,
    ];
  }

  if (fileName) {
    if (typeof cerolingoLog === "function") {
      cerolingoLog("run.js", `Starting ${fileName}`, options);
    }
    if (window.api && typeof window.api.send === "function") {
      window.api.send("python", [fileName, options]);
    }
  }
}
