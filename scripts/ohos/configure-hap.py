#!/usr/bin/env python3
"""Configure the generated Tauri HAP project without supplying signing credentials."""
import json
import os
from pathlib import Path
import json5

project = Path("crates/gproxy-host-tauri/gen/ohos")
if not project.is_dir():
    project = Path("crates/gproxy-host-tauri/gen/open_harmony")
if not project.is_dir():
    candidates = list(Path("crates/gproxy-host-tauri/gen").glob("*/AppScope/app.json5"))
    if len(candidates) != 1:
        raise ValueError("Could not locate the generated OpenHarmony project")
    project = candidates[0].parent.parent
profile = project / "build-profile.json5"
config = json5.loads(profile.read_text())
config["app"]["signingConfigs"] = []
for product in config["app"]["products"]:
    product.pop("signingConfig", None)
    product["compatibleSdkVersion"] = "6.0.0(20)"
    product["compileSdkVersion"] = "6.0.0(20)"
profile.write_text(json.dumps(config, indent=2) + "\n")
module = project / "entry/build-profile.json5"
config = json5.loads(module.read_text())
config.setdefault("buildOption", {}).setdefault("externalNativeOptions", {})["abiFilters"] = [os.environ["OHOS_ARCH"]]
module.write_text(json.dumps(config, indent=2) + "\n")
# Server credentials and databases must not enter cloud backups.
module = project / "entry/src/main/module.json5"
config = json5.loads(module.read_text())
config["module"].pop("extensionAbilities", None)
module.write_text(json.dumps(config, indent=2) + "\n")
print(project)
