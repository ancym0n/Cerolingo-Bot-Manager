; Custom NSIS texts for the Cerolingo Bot Manager installer
!macro customHeader
  BrandingText "Cerolingo Bot Manager  |  by ancym0n"

  !define MUI_WELCOMEPAGE_TITLE "Welcome to Cerolingo Bot Manager"
  !define MUI_WELCOMEPAGE_TEXT "This wizard will install Cerolingo Bot Manager on your computer.$\r$\n$\r$\nPython, Playwright and Chromium are downloaded automatically on first launch, so an internet connection is needed then.$\r$\n$\r$\nClick Next to continue."

  !define MUI_FINISHPAGE_TITLE "Cerolingo Bot Manager is ready"
  !define MUI_FINISHPAGE_TEXT "Installation finished successfully.$\r$\n$\r$\nOn first launch the app will set up Python, Playwright and Chromium for you."
  !define MUI_FINISHPAGE_LINK "Source code on GitHub"
  !define MUI_FINISHPAGE_LINK_LOCATION "https://github.com/ancym0n/Cerolingo-Bot-Manager"
  !define MUI_FINISHPAGE_LINK_COLOR "7b93ff"

  !define MUI_UNCONFIRMPAGE_TEXT_TOP "Cerolingo Bot Manager will be removed from your computer."
!macroend
