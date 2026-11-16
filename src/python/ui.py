"""Console output helpers: one consistent, readable look for every bot script."""

import os
import sys
import time

APP_NAME = "Cerolingo Bot Manager"
APP_VERSION = os.environ.get("CBM_VERSION", "")


def _enable_ansi():
    if hasattr(sys.stdout, "reconfigure"):
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace", line_buffering=True)
        except Exception:
            pass
    if os.name == "nt":
        try:
            import ctypes

            kernel = ctypes.windll.kernel32
            handle = kernel.GetStdHandle(-11)
            mode = ctypes.c_uint32()
            if kernel.GetConsoleMode(handle, ctypes.byref(mode)):
                kernel.SetConsoleMode(handle, mode.value | 0x0004)
        except Exception:
            pass


_enable_ansi()

_COLOR = (sys.stdout.isatty() or bool(os.environ.get("FORCE_COLOR"))) and not os.environ.get("NO_COLOR")


def _rgb(r, g, b):
    return f"\x1b[38;2;{r};{g};{b}m" if _COLOR else ""


RESET = "\x1b[0m" if _COLOR else ""
BOLD = "\x1b[1m" if _COLOR else ""
DIM = "\x1b[2m" if _COLOR else ""

PRIMARY = _rgb(104, 119, 254)
LIGHT = _rgb(179, 208, 255)
GREEN = _rgb(123, 255, 208)
RED = _rgb(255, 133, 133)
YELLOW = _rgb(255, 214, 120)
GRAY = _rgb(130, 142, 160)

_WIDTH = 58
_started = time.time()
_inline = False


def _break():
    """Finish a pending in-place progress line before printing anything else."""
    global _inline
    if _inline:
        sys.stdout.write("\n")
        _inline = False


def _stamp():
    return f"{GRAY}{time.strftime('%H:%M:%S')}{RESET}"


def _line(tag, color, text):
    _break()
    print(f"{_stamp()} {color}{BOLD}{tag:<5}{RESET} {text}")


def set_title(text):
    if _COLOR:
        sys.stdout.write(f"\x1b]0;{text}\x07")
        sys.stdout.flush()


def banner(title, subtitle=""):
    version = f"  v{APP_VERSION}" if APP_VERSION else ""
    set_title(f"{APP_NAME} - {title}")
    bar = "─" * _WIDTH
    print()
    print(f"{PRIMARY}╭{bar}╮{RESET}")
    head = f"{APP_NAME}{version}"
    print(f"{PRIMARY}│{RESET} {BOLD}{LIGHT}{head:<{_WIDTH - 1}}{RESET}{PRIMARY}│{RESET}")
    print(f"{PRIMARY}│{RESET} {title:<{_WIDTH - 1}}{PRIMARY}│{RESET}")
    if subtitle:
        print(f"{PRIMARY}│{RESET} {GRAY}{subtitle:<{_WIDTH - 1}}{RESET}{PRIMARY}│{RESET}")
    print(f"{PRIMARY}╰{bar}╯{RESET}")
    print()


def section(text):
    _break()
    print(f"\n{PRIMARY}{BOLD}▌ {text}{RESET}")


def step(index, total, text):
    _break()
    print(f"{_stamp()} {PRIMARY}{BOLD}[{index}/{total}]{RESET} {BOLD}{text}{RESET}")


def info(text):
    _line("INFO", LIGHT, text)


def ok(text):
    _line("OK", GREEN, text)


def warn(text):
    _line("WARN", YELLOW, text)


def error(text):
    _line("ERROR", RED, text)


def dim(text):
    _break()
    print(f"{_stamp()}       {GRAY}{text}{RESET}")


def config(rows):
    """Print a small key/value table of the loaded settings."""
    section("Configuration")
    width = max(len(k) for k, _ in rows)
    for key, value in rows:
        if isinstance(value, bool):
            value = f"{GREEN}on{RESET}" if value else f"{GRAY}off{RESET}"
        print(f"  {GRAY}{key:<{width}}{RESET}  {value}")
    print()


def progress(label, done, total, suffix=""):
    """Draw / update a single-line progress bar (call done() when finished)."""
    global _inline
    _inline = True
    total = max(total, 1)
    cells = 24
    filled = int(cells * done / total)
    bar = f"{PRIMARY}{'█' * filled}{GRAY}{'░' * (cells - filled)}{RESET}"
    sys.stdout.write(f"\r{_stamp()} {label} {bar} {done}/{total} {suffix}\x1b[K" if _COLOR else f"\r{label} {done}/{total} {suffix}")
    sys.stdout.flush()


def progress_done():
    _break()
    sys.stdout.flush()


def summary(title, rows):
    _break()
    section(title)
    width = max(len(k) for k, _ in rows)
    for key, value in rows:
        print(f"  {GRAY}{key:<{width}}{RESET}  {value}")
    print()


def elapsed():
    seconds = int(time.time() - _started)
    return f"{seconds // 60}m {seconds % 60:02d}s" if seconds >= 60 else f"{seconds}s"


def farewell(text="Done"):
    _break()
    print(f"\n{GREEN}{BOLD}✔ {text}{RESET} {GRAY}({elapsed()}){RESET}")
