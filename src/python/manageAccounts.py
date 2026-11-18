"""Open accounts.txt in the system's default editor."""

import os
import subprocess
import sys

import env
import ui


def main():
    path = env.ACCOUNTS_PATH
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if not os.path.exists(path):
        open(path, "w", encoding="utf-8").close()

    if sys.platform == "win32":
        os.startfile(path)  # noqa: S606 - opens with the registered editor
    else:
        clean_env = os.environ.copy()
        if sys.platform.startswith("linux") and clean_env.get("APPDIR"):
            clean_env.pop("LD_LIBRARY_PATH", None)
            clean_env.pop("GSETTINGS_SCHEMA_DIR", None)
        subprocess.Popen(
            ["open" if sys.platform == "darwin" else "xdg-open", path],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=clean_env,
        )
    ui.banner("Account Manager")
    ui.ok("Opened accounts.txt")
    ui.dim(path)
    ui.dim("Format: email:password:cookie  (one account per line)")


if __name__ == "__main__":
    main()
