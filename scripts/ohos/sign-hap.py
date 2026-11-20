#!/usr/bin/env python3
"""Sign and verify a HarmonyOS HAP or AppGallery APP with the SDK signer."""
import argparse
import base64
import json
import os
import re
import time
import uuid
import shutil
from pathlib import Path
import subprocess
import tempfile
import zipfile

import json5


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="HAP or APP to sign")
    parser.add_argument("output", type=Path, help="destination signed package")
    signing = parser.add_mutually_exclusive_group(required=True)
    signing.add_argument("--config", type=Path, help="one DevEco/Hvigor signingConfigs entry")
    signing.add_argument("--release-env", action="store_true",
                         help="read OHOS_SIGNING_MATERIAL and OHOS_SIGNING_PASSWORD")
    parser.add_argument("--tool", required=True, type=Path, help="SDK hap-sign-tool.jar")
    parser.add_argument("--background", action="store_true",
                        help="declare taskKeeping and mobile background ACL before signing")
    parser.add_argument("--debug-template", type=Path,
                        help="local SDK unsigned debug profile template")
    parser.add_argument("--profile-config", type=Path,
                        help="local profile signer configuration (same material fields)")
    parser.add_argument("--udid", action="append", help="debug device UDID; repeat for multiple devices")
    args = parser.parse_args()
    if args.debug_template and (not args.profile_config or not args.udid):
        parser.error("--debug-template requires --profile-config and at least one --udid")
    if not args.debug_template and (args.profile_config or args.udid):
        parser.error("--profile-config and --udid require --debug-template")
    source, output = args.input.resolve(), args.output.resolve()
    if source == output:
        parser.error("input and output must be different files")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="gproxy-hap-sign-") as temporary:
        work = Path(temporary)
        if args.release_env:
            if args.background or args.debug_template:
                parser.error("release signing uses the HAP and the issued profile as supplied")
            bundle = json.loads(os.environ.pop("OHOS_SIGNING_MATERIAL"))
            password = os.environ.pop("OHOS_SIGNING_PASSWORD")
            material = {"keyAlias": bundle["keyAlias"]}
            for field, key, filename in [("certpath", "certificate", "release.cer"),
                                         ("profile", "profile", "release.p7b"),
                                         ("storeFile", "keystore", "release.p12")]:
                path = work / filename
                path.write_bytes(base64.b64decode(bundle[key], validate=True))
                material[field] = str(path)
        else:
            material = json5.loads(args.config.read_text())["material"]

        def material_path(key, fields=material, config_path=args.config):
            path = Path(fields[key])
            if not path.is_absolute():
                path = config_path.resolve().parent / path
            return str(path.resolve(strict=True))

        if args.background:
            # Repack only to update declarations. Signing below recreates signature
            # and code-sign blocks; native library bytes are copied unchanged.
            patched = work / "background.hap"
            with zipfile.ZipFile(source) as original, zipfile.ZipFile(patched, "w") as target:
                manifest = json.loads(original.read("module.json"))
                module = manifest["module"]
                permissions = module.setdefault("requestPermissions", [])
                for name in ["ohos.permission.KEEP_BACKGROUND_RUNNING",
                             "ohos.permission.KEEP_BACKGROUND_RUNNING_SYSTEM"]:
                    if not any(item["name"] == name for item in permissions):
                        permissions.append({"name": name})
                ability = next(item for item in module["abilities"]
                               if item["name"] == "EntryAbility")
                modes = ability.setdefault("backgroundModes", [])
                if "taskKeeping" not in modes:
                    modes.append("taskKeeping")
                for entry in original.infolist():
                    data = (json.dumps(manifest).encode() if entry.filename == "module.json"
                            else original.read(entry))
                    target.writestr(entry, data)
            source = patched
        tool = ["java", "-jar", str(args.tool.resolve(strict=True))]
        profile_path = None
        if args.debug_template:
            profile = json5.loads(args.debug_template.read_text())
            if profile["type"] != "debug":
                parser.error("--debug-template must contain a debug profile")
            with zipfile.ZipFile(source) as archive:
                bundle = json.loads(archive.read("module.json"))["app"]["bundleName"]
            certificate_path = material_path("certpath")
            certificate = Path(certificate_path).read_bytes()
            leaf = re.search(rb"-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----", certificate, re.S)
            certificate = (leaf.group().decode() + "\n" if leaf else subprocess.check_output([
                "openssl", "x509", "-inform", "DER", "-in", certificate_path, "-outform", "PEM",
            ], text=True))
            profile["uuid"] = str(uuid.uuid4())
            now = int(time.time())
            profile["validity"] = {"not-before": now - 60, "not-after": now + 30 * 86400}
            profile["bundle-info"]["bundle-name"] = bundle
            profile["bundle-info"]["development-certificate"] = certificate
            profile["debug-info"] = {"device-id-type": "udid", "device-ids": args.udid}
            if args.background:
                acls = profile.setdefault("acls", {}).setdefault("allowed-acls", [])
                if "ohos.permission.KEEP_BACKGROUND_RUNNING_SYSTEM" not in acls:
                    acls.append("ohos.permission.KEEP_BACKGROUND_RUNNING_SYSTEM")
            unsigned_profile = work / "debug-profile.json"
            unsigned_profile.write_text(json.dumps(profile))
            profile_path = work / "debug-profile.p7b"
            profile_material = json5.loads(args.profile_config.read_text())["material"]
            subprocess.run(tool + [
                "sign-profile", "-mode", "localSign", "-keyAlias", profile_material["keyAlias"],
                "-signAlg", profile_material.get("signAlg", "SHA256withECDSA"),
                "-profileCertFile", material_path("certpath", profile_material, args.profile_config),
                "-keystoreFile", material_path("storeFile", profile_material, args.profile_config),
                "-pwdInputMode", "1", "-inFile", str(unsigned_profile), "-outFile", str(profile_path),
            ], check=True)
        signed = work / ("signed" + source.suffix)
        options = [
            "sign-app", "-mode", "localSign", "-keyAlias", material["keyAlias"],
            "-signAlg", material.get("signAlg", "SHA256withECDSA"),
            "-appCertFile", material_path("certpath"),
            "-profileFile", str(profile_path) if profile_path else material_path("profile"),
            "-keystoreFile", material_path("storeFile"),
            "-inFile", str(source), "-outFile", str(signed),
        ]
        if args.release_env:
            # Java expands the private argument file without exposing passwords in argv.
            argfile = work / "sign.args"
            arguments = tool[1:] + options + ["-keystorePwd", password, "-keyPwd", password]
            argfile.write_text("\n".join(json.dumps(arg, ensure_ascii=False) for arg in arguments))
            subprocess.run(["java", "@" + str(argfile)], check=True)
        else:
            subprocess.run(tool + options + ["-pwdInputMode", "1"], check=True)
        subprocess.run(tool + [
            "verify-app", "-inFile", str(signed),
            "-outCertChain", str(work / "certificate.cer"),
            "-outProfile", str(work / "profile.p7b"),
        ], check=True)
        if args.release_env and (work / "profile.p7b").read_bytes() != Path(material["profile"]).read_bytes():
            raise ValueError("Verified application profile differs from the release profile")
        shutil.copyfile(signed, output)
    print(f"Signature verified: {output}")
    if args.background:
        print("Background permissions declared; device acceptance and runtime grant are checked by the OS.")


if __name__ == "__main__":
    main()
