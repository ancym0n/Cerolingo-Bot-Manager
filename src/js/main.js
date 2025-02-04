window.api.onSetVersion((version) => {
  document.title = "CerolingoBM v" + version;
});

let allButtons = document.querySelectorAll(".button");
allButtons.forEach((e) => {
  let id = e.id;
  if (id != "") {
    console.log(id);
    let buttonText = document.querySelector(`#${id} .button-text`);
    let buttonSettings = document.querySelector(`#${id} .button-settings`);
    buttonText.setAttribute("onClick", `runScript('${id}')`);
    if (buttonSettings)
      buttonSettings.setAttribute("onClick", `settings('${id}')`);
  }
});

document.addEventListener("click", (event) => {
  const target = event.target;
  if (target.tagName === "A" && target.href.startsWith("http")) {
    event.preventDefault();
    window.api.openExternal(target.href);
  }
  if (
    target.classList.value.includes("settings-checkbox") &&
    !target.classList.value.includes("settings-checkboxes")
  ) {
    let type = target.getAttribute("type");
    if (type === "on") target.setAttribute("type", "off");
    else if (type === "off") target.setAttribute("type", "on");
    target.style.transform = "scale(0.9)";
    setTimeout(() => {
      target.style.transform = "scale(1)";
    }, 300);
  } else if (
    target.classList.value.includes("button-text") ||
    target.classList.value.includes("button-settings")
  ) {
    let className = "transform-scale";
    target.classList.add(className);
    setTimeout(() => {
      target.classList.remove(className);
    }, 300);
  } else if (target.getAttribute("src") === "../img/settings.svg") {
    let parent = target.parentElement;
    let className = "transform-scale";
    parent.classList.add(className);
    setTimeout(() => {
      parent.classList.remove(className);
    }, 300);
  }
});
