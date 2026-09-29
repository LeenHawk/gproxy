#!/usr/bin/env python3
"""Package Linux-cross-built macOS executables with Apple's disk-image tools."""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import zipfile


root = Path.cwd()
output = root / "dist/dmg"
output.mkdir(parents=True, exist_ok=True)
config = json.loads((root / "crates/gproxy-host-tauri/tauri.conf.json").read_text())
rows = json.loads((root / "scripts/release-targets.json").read_text())["include"]
subprocess.run(["sudo", "softwareupdate", "--install-rosetta", "--agree-to-license"], check=True)
for row in rows:
    if row["os"] != "macos":
        continue
    target, artifact = row["target"], row["application_artifact"]
    metadata = json.loads((root / f"dist/release/{artifact}.provenance.json").read_text())
    if metadata["commit"] != os.environ["CI_COMMIT_SHA"]:
        raise ValueError(f"Wrong source commit for {target}")
    version = metadata["version"].split("-")[0]
    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        cli = work / "gproxy"
        with zipfile.ZipFile(root / f"dist/release/{row['artifact']}.zip") as archive:
            cli.write_bytes(archive.read("gproxy"))
        cli.chmod(0o755)
        subprocess.run(["codesign", "--verify", "--strict", str(cli)], check=True)
        arch = "arm64" if target.startswith("aarch64") else "x86_64"
        subprocess.run(["arch", f"-{arch}", str(cli), "--version"], check=True)
        subprocess.run(["arch", f"-{arch}", str(cli), "--help"], stdout=subprocess.DEVNULL, check=True)

        subprocess.run([sys.executable, "scripts/package-macos-cli.py", "--binary", str(cli),
                        "--artifact", row["artifact"], "--output-dir", str(output)], check=True)

        image_root = work / "image"
        app = image_root / "GPROXY.app"
        contents = app / "Contents"
        (contents / "MacOS").mkdir(parents=True)
        (contents / "Resources").mkdir()
        binary = contents / "MacOS/gproxy-desktop"
        shutil.copy2(root / f"dist/macos/{target}/gproxy-desktop", binary)
        binary.chmod(0o755)
        shutil.copy2(root / "crates/gproxy-host-tauri/icons/icon.icns", contents / "Resources/icon.icns")
        with (contents / "Info.plist").open("wb") as stream:
            plistlib.dump({
                "CFBundleDevelopmentRegion": "en", "CFBundleExecutable": "gproxy-desktop",
                "CFBundleIdentifier": config["identifier"], "CFBundleInfoDictionaryVersion": "6.0",
                "CFBundleName": config["productName"], "CFBundleDisplayName": config["productName"],
                "CFBundlePackageType": "APPL", "CFBundleShortVersionString": version,
                "CFBundleVersion": version, "CFBundleIconFile": "icon.icns",
                "NSHighResolutionCapable": True,
                "LSMinimumSystemVersion": "11.0" if arch == "arm64" else "10.13",
                "NSAppTransportSecurity": {"NSAllowsLocalNetworking": True},
            }, stream)
        subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
        archive_path = output / f"{artifact}.zip"
        subprocess.run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(archive_path)], check=True)
        with archive_path.open("rb") as stream:
            zip_digest = hashlib.file_digest(stream, "sha256").hexdigest()
        Path(str(archive_path) + ".sha256").write_text(f"{zip_digest}  {archive_path.name}\n")
        (image_root / "Applications").symlink_to("/Applications")
        package = output / f"{artifact}.dmg"
        subprocess.run(["hdiutil", "create", "-ov", "-format", "UDZO", "-fs", "HFS+",
                        "-volname", config["productName"], "-srcfolder", str(image_root), str(package)], check=True)
        subprocess.run(["hdiutil", "verify", str(package)], check=True)
        digest = hashlib.sha256()
        with package.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        package.with_suffix(".dmg.sha256").write_text(f"{digest.hexdigest()}  {package.name}\n")
