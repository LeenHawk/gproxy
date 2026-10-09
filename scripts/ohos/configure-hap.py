#!/usr/bin/env python3
"""Configure the generated Tauri HAP project with optional external signing credentials."""
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
import json5

project = Path("crates/gproxy-host-tauri/gen/ohos")
version = os.environ.get("GPROXY_BUILD_VERSION") or subprocess.check_output(
    ["bash", "scripts/release-metadata.sh", "version"], text=True
).strip()
app_scope = project / "AppScope/app.json5"
app_config = json5.loads(app_scope.read_text())
app_config["app"]["bundleName"] = "com.leenhawk.gproxy.desktop"
app_config["app"]["versionName"] = version
major, minor, patch = map(int, version.split("-")[0].split("."))
app_config["app"]["versionCode"] = major * 1_000_000 + minor * 1_000 + patch
app_scope.write_text(json.dumps(app_config, indent=2) + "\n")
icon = Path("crates/gproxy-host-tauri/icons/icon.png")
for resource in ("AppScope/resources/base/media/foreground.png",
                 "entry/src/main/resources/base/media/foreground.png",
                 "entry/src/main/resources/base/media/startIcon.png"):
    shutil.copyfile(icon, project / resource)
# CLI owns native compilation. This hook only strips the staged library before
# signing, including when the toolchain template changes its build callback.
hvigor = project / "entry/hvigorfile.ts"
packer_args = json.dumps([
    str(Path("scripts/pack-mobile-native.py").resolve()),
    str((project / "entry/libs" / os.environ["OHOS_ARCH"]).resolve()),
    str(Path(os.environ["OHOS_NATIVE_HOME"]) / "llvm/bin/llvm-strip"),
    "--strip-only",
])
checkout = json.dumps(str(Path.cwd()))
hvigor.write_text("""import { hapTasks } from '@ohos/hvigor-ohos-plugin';
import { HvigorNode, HvigorPlugin } from '@ohos/hvigor';
import { execFileSync } from 'child_process';

const packNative: HvigorPlugin = {
  pluginId: 'gproxy-pack-native',
  apply(node: HvigorNode) {
    node.getTaskByName('default@ConfigureCmake')!.afterRun(() => {
      if (process.env.GPROXY_OHOS_REUSE_NATIVE !== "1") {
""" + f'        execFileSync("python3", {packer_args}, {{ cwd: {checkout}, stdio: "inherit" }});\n' + """      }
    });
  }
};
export default { system: hapTasks, plugins: [packNative] };
""")
profile = project / "build-profile.json5"
config = json5.loads(profile.read_text())
config["app"]["signingConfigs"] = []
signing_path = os.environ.get("GPROXY_OHOS_SIGNING_CONFIG")
signing = json5.loads(Path(signing_path).read_text()) if signing_path else None
if signing:
    for field in ("certpath", "profile", "storeFile"):
        if not Path(signing["material"][field]).is_file():
            raise ValueError(f"Missing HarmonyOS signing material: {field}")
    config["app"]["signingConfigs"] = [signing]
for product in config["app"]["products"]:
    product.pop("signingConfig", None)
    if signing:
        product["signingConfig"] = signing["name"]
    product["compatibleSdkVersion"] = "6.0.0(20)"
    product["compileSdkVersion"] = "6.0.0(20)"
    product["targetSdkVersion"] = "6.0.0(20)"
profile.write_text(json.dumps(config, indent=2) + "\n")
module = project / "entry/build-profile.json5"
config = json5.loads(module.read_text())
config.setdefault("buildOption", {}).setdefault("externalNativeOptions", {})["abiFilters"] = [os.environ["OHOS_ARCH"]]
module.write_text(json.dumps(config, indent=2) + "\n")
# Server credentials and databases must not enter cloud backups.
module = project / "entry/src/main/module.json5"
config = json5.loads(module.read_text())
# Compress the ZIP entries, not the ELF payload: installation extracts intact
# libraries, preserving the section table required by OHOS's loader.
config["module"]["compressNativeLibs"] = True
config["module"].pop("extensionAbilities", None)
config["module"]["deviceTypes"] = ["phone", "tablet", "2in1"]
permissions = config["module"].setdefault("requestPermissions", [])
if not any(item["name"] == "ohos.permission.KEEP_BACKGROUND_RUNNING" for item in permissions):
    permissions.append({"name": "ohos.permission.KEEP_BACKGROUND_RUNNING"})
if os.environ.get("GPROXY_OHOS_BACKGROUND_ACL") == "1":
    permissions.append({"name": "ohos.permission.KEEP_BACKGROUND_RUNNING_SYSTEM"})
for ability in config["module"]["abilities"]:
    if ability["name"] == "EntryAbility":
        ability["launchType"] = "singleton"
        ability["backgroundModes"] = ["taskKeeping"]
        # Avoid the system splash before an automatic hidden/minimized launch.
        ability["startWindow"] = "$profile:gproxy_start_window"
module.write_text(json.dumps(config, indent=2) + "\n")
# Keep the application bridge in source control, rather than editing generated
# or shared SDK files. The matching NativeAbility supplies Rust's init context.
source = Path("scripts/ohos/application")
entry = project / "entry/src/main"
for name in ("EntryAbility.ets", "GproxyBackground.ets", "GproxyTray.ets"):
    shutil.copyfile(source / name, entry / "ets/entryability" / name)
profile = entry / "resources/base/profile/gproxy_start_window.json"
profile.write_text(json.dumps({
    "startWindowType": "REQUIRED_HIDE", "startWindowAppIcon": "$media:startIcon",
    "startWindowBackgroundColor": "$color:start_window_background"
}, indent=2) + "\n")
raw = entry / "resources/rawfile"
raw.mkdir(parents=True, exist_ok=True)
shutil.copyfile(icon, raw / "gproxy-tray.png")
types = entry / "cpp/types/libgproxy_host_tauri"
types.mkdir(parents=True, exist_ok=True)
shutil.copyfile(source / "Index.d.ts", types / "Index.d.ts")
(types / "oh-package.json5").write_text(json.dumps({
    "name": "libgproxy_host_tauri.so", "version": "1.0.0", "types": "./Index.d.ts"
}, indent=2) + "\n")
package = project / "entry/oh-package.json5"
config = json5.loads(package.read_text())
config.setdefault("dependencies", {})["libgproxy_host_tauri.so"] = "file:./src/main/cpp/types/libgproxy_host_tauri"
package.write_text(json.dumps(config, indent=2) + "\n")
subprocess.run([sys.executable, "scripts/ohos/prepare-har.py"], check=True)
print(project)
