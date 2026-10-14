#!/usr/bin/env python3
"""Package the matching Ability HAR instead of the CLI template's beta.0 HAR."""
import json
import gzip
import tarfile
import os
from pathlib import Path
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

# Require identical Rust crate trees so the HAR cannot drift away from the
# image's compiled Rust interface, including optional WebView settings.
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
# ohpm incorporates the HAR digest into paths embedded in Ark bytecode. Keep
# the dependency archive stable before installation, rather than patching ABC.
epoch = int(subprocess.check_output(
    ["git", "-C", str(source), "show", "-s", "--format=%ct", pin["revision"]], text=True))
with tarfile.open(archives[0], "r:*") as original, (vendor / "ability.har").open("wb") as output:
    with gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=epoch, compresslevel=9) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as normalized:
            for member in sorted(original.getmembers(), key=lambda entry: entry.name):
                member.uid = member.gid = 0
                member.uname = member.gname = ""
                member.mtime = epoch
                member.pax_headers = {key: value for key, value in member.pax_headers.items()
                                      if key not in {"mtime", "atime", "ctime", "uid", "gid", "uname", "gname"}}
                contents = original.extractfile(member) if member.isfile() else None
                try:
                    normalized.addfile(member, contents)
                finally:
                    if contents is not None:
                        contents.close()
package = project / "entry/oh-package.json5"
config = json5.loads(package.read_text())
config["dependencies"]["@ohos-rs/ability"] = "file:../vendor/ability.har"
package.write_text(json.dumps(config, indent=2) + "\n")
print(f'Using Ability HAR {pin["revision"]}, matching Rust {pin["rust_revision"]}')
