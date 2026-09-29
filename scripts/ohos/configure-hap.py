#!/usr/bin/env python3
"""Configure the generated Tauri HAP project without supplying signing credentials."""
import json
import os
import shutil
import subprocess
from pathlib import Path
import json5

project = Path("crates/gproxy-host-tauri/gen/ohos")
version = os.environ.get("GPROXY_BUILD_VERSION") or subprocess.check_output(
    ["bash", "scripts/release-metadata.sh", "version"], text=True
).strip()
app_scope = project / "AppScope/app.json5"
app_config = json5.loads(app_scope.read_text())
app_config["app"]["bundleName"] = "dev.gproxy.desktop"
app_config["app"]["versionName"] = version
major, minor, patch = map(int, version.split("-")[0].split("."))
app_config["app"]["versionCode"] = major * 1_000_000 + minor * 1_000 + patch
app_scope.write_text(json.dumps(app_config, indent=2) + "\n")
icon = Path("crates/gproxy-host-tauri/icons/icon.png")
for resource in ("AppScope/resources/base/media/foreground.png",
                 "entry/src/main/resources/base/media/foreground.png",
                 "entry/src/main/resources/base/media/startIcon.png"):
    shutil.copyfile(icon, project / resource)
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
