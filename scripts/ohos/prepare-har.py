#!/usr/bin/env python3
"""Package the matching Ability HAR instead of the CLI template's beta.0 HAR."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import json5

root = Path(__file__).resolve().parents[2]
pin = json.loads((root / "scripts/ohos/ability-har.json").read_text())
toolchain_pins = json.loads((root / "scripts/ohos/tauri-pins.json").read_text())
if pin["rust_revision"] != toolchain_pins["ability"]["revision"]:
    raise ValueError("Ability HAR compatibility pin must match the Rust runtime")
source = Path(os.environ.get("RUNNER_TEMP", root / "target")) / "gproxy-ability-har"
source.mkdir(parents=True, exist_ok=True)
subprocess.run(["git", "init", str(source)], check=True)
subprocess.run(["git", "-C", str(source), "fetch", "--depth", "1",
                f'https://github.com/{pin["repository"]}.git', pin["revision"]], check=True)
subprocess.run(["git", "-C", str(source), "checkout", "--detach", "FETCH_HEAD"], check=True)

# The fork only changes ArkTS browser storage. Require identical Rust crate trees
# so the HAR cannot silently drift away from the image's compiled Rust interface.
runtime = Path(os.environ["OHOS_TAURI_SOURCES"]) / "ability"
def crate_tree(path):
    return subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD:crates"], text=True).strip()
if crate_tree(source) != crate_tree(runtime):
    raise ValueError("Ability HAR and Rust runtime have different crate interfaces")
subprocess.run(["bash", "scripts/pack.sh"], cwd=source, check=True)
archives = list(source.glob("*.har"))
if len(archives) != 1:
    raise ValueError(f"Expected one Ability HAR, found {archives}")
project = root / "crates/gproxy-host-tauri/gen/ohos"
vendor = project / "vendor"
vendor.mkdir(exist_ok=True)
shutil.copyfile(archives[0], vendor / "ability.har")
package = project / "entry/oh-package.json5"
config = json5.loads(package.read_text())
config["dependencies"]["@ohos-rs/ability"] = "file:../vendor/ability.har"
package.write_text(json.dumps(config, indent=2) + "\n")
print(f'Using Ability HAR {pin["revision"]}, matching Rust {pin["rust_revision"]}')
