#!/usr/bin/env python3
"""Fetch and pin the experimental OHOS tools when building the toolchain image."""
import subprocess
import json
import os
from pathlib import Path
import re

root = Path.cwd()
source = Path(os.environ["OHOS_TAURI_SOURCES"]).resolve()
pins = json.loads((root / "scripts/ohos/tauri-pins.json").read_text())
marker = source / "gproxy-pins.json"

for name, pin in pins.items():
    checkout = source / name
    subprocess.run(["git", "init", str(checkout)], check=True)
    subprocess.run(["git", "-C", str(checkout), "remote", "add", "origin",
                    f"https://github.com/{pin['repository']}.git"], check=True)
    subprocess.run(["git", "-C", str(checkout), "fetch", "--depth", "1", "origin", pin["revision"]], check=True)
    subprocess.run(["git", "-C", str(checkout), "checkout", "--detach", "FETCH_HEAD"], check=True)
# Match the Ability revision in Tauri's own upstream lockfile.
for checkout in (source / "tauri", source / "wry", source / "tao"):
    for manifest in checkout.rglob("Cargo.toml"):
        text = manifest.read_text().replace(
            'git = "https://github.com/harmony-contrib/openharmony-ability.git"',
            'git = "https://github.com/harmony-contrib/openharmony-ability.git", rev = "' + pins["ability"]["revision"] + '"')
        manifest.write_text(text)
upstream = source / "tauri/Cargo.toml"
text = upstream.read_text()
for name in ("wry", "tao", "cargo-mobile2"):
    text = re.sub(rf'^{name} = .*$', f'{name} = {{ path = "{source / name}" }}', text, flags=re.MULTILINE)
upstream.write_text(text)
# Huawei's SDK has native directly below openharmony, without an API subdir.
env_file = source / "cargo-mobile2/src/open_harmony/env.rs"
text = env_file.read_text()
old = '''self.ohos_home
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .as_os_str()
            .to_os_string()'''
if old not in text:
    raise ValueError("Upstream OpenHarmony SDK layout helper changed")
env_file.write_text(text.replace(old, 'std::env::var_os("DEVECO_SDK_HOME").expect("DEVECO_SDK_HOME is required")'))
marker.write_text(json.dumps(pins, indent=2) + "\n")

