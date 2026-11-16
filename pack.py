"""Build Linnea-Setup.exe: release binaries -> dist/payload.zip -> the installer that carries it.

    python pack.py                 (set CARGO_TARGET_DIR to a short path if this folder is deep)

The interface must be built first (cd ui && pnpm install && pnpm build).
"""
import os
import shutil
import subprocess
import sys
import zipfile

ROOT = os.path.dirname(os.path.abspath(__file__))
TARGET = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
DIST = os.path.join(ROOT, "dist")


def cargo(*args):
    subprocess.run(["cargo", *args], cwd=ROOT, check=True)


def main():
    if not os.path.isfile(os.path.join(ROOT, "ui", "dist", "index.html")):
        sys.exit("build the interface first: cd ui && pnpm install && pnpm build")
    cargo("build", "--release", "-p", "linnea-cli", "-p", "linnea-app")
    os.makedirs(DIST, exist_ok=True)
    payload = os.path.join(DIST, "payload.zip")
    skip_dirs = {"__pycache__", "node_modules", ".git"}
    with zipfile.ZipFile(payload, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for exe in ("linnea.exe", "linnea-app.exe", "DirectML.dll"):
            built = os.path.join(TARGET, "release", exe)
            # the onnxruntime build step leaves DirectML.dll as a link that can point nowhere: then the copy
            # kept in bin/ goes in, and without either the installed app would not start, so stop
            src = built if os.path.isfile(built) else os.path.join(ROOT, "bin", exe)
            if not os.path.isfile(src):
                sys.exit(f"{exe} is missing from {built} and from bin/")
            z.write(src, f"bin/{exe}")
        for folder in ("library", "ui/dist", "skill"):
            base = os.path.join(ROOT, folder)
            for d, dirs, files in os.walk(base):
                dirs[:] = [x for x in dirs if x not in skip_dirs]
                for f in files:
                    full = os.path.join(d, f)
                    z.write(full, os.path.relpath(full, ROOT).replace("\\", "/"))
        for f in ("README.md",):
            z.write(os.path.join(ROOT, f), f)
        z.write(os.path.join(ROOT, "assets", "linnea.ico"), "linnea.ico")
    print(f"payload {os.path.getsize(payload) / 1e6:.1f} MB")
    # The installer embeds the payload; make sure it rebuilds when the payload changes.
    os.utime(os.path.join(ROOT, "crates", "setup", "src", "main.rs"))
    cargo("build", "--release", "-p", "linnea-setup")
    out = os.path.join(DIST, "Linnea-Setup.exe")
    shutil.copy2(os.path.join(TARGET, "release", "Linnea-Setup.exe"), out)
    for exe in ("linnea.exe", "linnea-app.exe", "DirectML.dll"):
        if not os.path.isfile(os.path.join(TARGET, "release", exe)):
            continue
        try:
            shutil.copy2(os.path.join(TARGET, "release", exe), os.path.join(ROOT, "bin", exe))
        except OSError as e:
            print(f"bin/{exe} not updated (in use?): {e}")
    print(f"installer {out} ({os.path.getsize(out) / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()
