import os, subprocess
script_dir = os.path.dirname(os.path.realpath(__file__))
accounts_path = os.path.join(script_dir, "logs", "accounts.txt")
subprocess.run(["notepad.exe", accounts_path])
print("Opened accounts.txt\n > Path: "+accounts_path)