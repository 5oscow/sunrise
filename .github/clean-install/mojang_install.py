"""Lays out Minecraft 26.3 in the official launcher's folder layout from Mojang's own servers, as a player's
launcher does when 26.3 is played once: versions/26.3/{26.3.json,26.3.jar}, libraries/, assets/{indexes,objects}/.
Usage: mojang_install.py <.minecraft folder>"""
import concurrent.futures as cf
import hashlib
import json
import os
import platform
import sys
import urllib.request
from pathlib import Path

MANIFEST = "https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json"
root = Path(sys.argv[1])
osname = {"Linux": "linux", "Windows": "windows", "Darwin": "osx"}[platform.system()]


def fetch(url, path, sha1=None):
    path = Path(path)
    if path.is_file() and (sha1 is None or hashlib.sha1(path.read_bytes()).hexdigest() == sha1):
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    for attempt in range(4):
        try:
            data = urllib.request.urlopen(url, timeout=60).read()
            break
        except OSError:
            if attempt == 3:
                raise
    if sha1 and hashlib.sha1(data).hexdigest() != sha1:
        raise SystemExit(f"{url}: sha1 differs")
    path.write_bytes(data)


def allowed(rules):
    ok = not rules
    for r in rules or []:
        if "os" in r and r["os"].get("name") not in (None, osname):
            continue
        ok = r["action"] == "allow"
    return ok


version = root / "versions/26.3"
fetch(MANIFEST, version / "26.3.json")
meta = json.loads((version / "26.3.json").read_text())
jobs = [(meta["downloads"]["client"]["url"], version / "26.3.jar", meta["downloads"]["client"]["sha1"])]
for lib in meta["libraries"]:
    art = lib.get("downloads", {}).get("artifact")
    if art and allowed(lib.get("rules")):
        jobs.append((art["url"], root / "libraries" / art["path"], art["sha1"]))
index = meta["assetIndex"]
fetch(index["url"], root / f"assets/indexes/{index['id']}.json", index["sha1"])
objects = json.loads((root / f"assets/indexes/{index['id']}.json").read_text())["objects"]
for o in objects.values():
    h = o["hash"]
    jobs.append((f"https://resources.download.minecraft.net/{h[:2]}/{h}", root / f"assets/objects/{h[:2]}/{h}", h))
with cf.ThreadPoolExecutor(32) as pool:
    list(pool.map(lambda j: fetch(*j), jobs))
print(f"{len(jobs)} files under {root}")
