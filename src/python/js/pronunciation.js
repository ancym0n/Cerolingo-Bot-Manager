(() => {
  let interval = setInterval(() => {
    if (!window.location.href.includes("alphabets/en/pronunciation")) {
      clearInterval(interval);
      clearInterval(continueButton);
    }
    const randomChoice = () => (Math.random() < 0.5 ? 1 : 2);
    let buttons = document.querySelectorAll("[role=radio]");
    if (buttons.length === 2) {
      buttons[randomChoice() - 1].click();
    } else if (buttons.length === 0 || buttons.length > 2) {
      let skip = document.querySelector("[data-test=player-skip]");
      skip.click();
    }
  }, 2);
  let continueButton = setInterval(() => {
    let next = document.querySelector("[data-test=player-next]");
    next.click();
    setTimeout(() => {
      next.click();
    }, 1);
  }, 2);
})();
