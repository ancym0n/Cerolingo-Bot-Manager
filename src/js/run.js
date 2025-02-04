function runScript(id) {
  let fileName;
  let options = [];

  let localStorageId = `${id}-config`;
  let loadedConfig = JSON.parse(localStorage.getItem(localStorageId));
  if (id === "") return;
  if (loadedConfig === null && id != "manage-accounts") return;

  if (id === "manage-accounts") {
    fileName = "manageAccounts.py";
  } else if (id === "create-account") {
    fileName = `createAccount.py`;
    options = [
      loadedConfig.usernamePrefix,
      loadedConfig.emailPrefix,
      loadedConfig.emailDomain,
      loadedConfig.qualify,
      loadedConfig.loop,
      loadedConfig.console,
    ];
  } else if (id === "farm") {
    fileName = `farmXp.py`;
    options = [
      loadedConfig.lessonsPerAccount,
      loadedConfig.legit,
      loadedConfig.loop,
      loadedConfig.console,
    ];
  }
  if (fileName) window.api.send("python", [fileName, options]);
}
