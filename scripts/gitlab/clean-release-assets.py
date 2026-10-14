#!/usr/bin/env python3
"""Consolidate public release metadata without rebuilding or changing packages."""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request

from publish import Github, Gitlab, Cnb


def repair_gitlab_manifests(github_release, gitlab, directory):
    asset = next(asset for asset in github_release["assets"] if asset["name"] == "manifest.json")
    with urllib.request.urlopen(asset["browser_download_url"], timeout=60) as response:
        source = json.load(response)
    private_key = directory / "private.pem"
    private_key.write_bytes(base64.b64decode(os.environ["UPDATE_SIGNING_PRIVATE_KEY_B64"]))
    private_key.chmod(0o600)
    public_der = subprocess.check_output([
        "openssl", "pkey", "-in", str(private_key), "-pubout", "-outform", "DER"])
    if public_der[-32:] != base64.b64decode(os.environ["UPDATE_SIGNING_PUBLIC_KEY_B64"]):
        raise ValueError("Update signing key pair does not match")
    payload_file, signature_file = directory / "payload", directory / "signature"
    manifest_file = directory / "manifest.json"
    tag = github_release["tag_name"]
    root = os.environ["CI_PROJECT_URL"]
    for release_tag, channel in ((tag, "release"), ("release", "release"), ("staging", "beta")):
        release = gitlab.api("GET", f"/releases/{release_tag}")
        link = next(link for link in release["assets"]["links"] if link["name"] == "manifest.json")
        with urllib.request.urlopen(link["url"], timeout=60) as response:
            current = json.load(response)
        if current["version"] != source["version"]:
            print(f"gitlab: skip {release_tag}; it points to {current['version']}", flush=True)
            continue
        manifest = dict(source, channel=channel, notes_url=f"{root}/-/releases/{tag}")
        manifest["artifacts"] = [dict(artifact, url=f"{root}/-/releases/{tag}/downloads/{artifact['url'].rsplit('/', 1)[1]}")
                                 for artifact in source["artifacts"]]
        payload = f"{channel}\n{manifest['version']}\n{manifest['notes_url']}\n{manifest['min_compatible_data_version']}\n"
        payload += "".join(f"{a['target_triple']}|{a['url']}|{a['sha256']}|{a['size']}\n" for a in manifest["artifacts"])
        payload_file.write_text(payload)
        subprocess.run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(private_key),
                        "-in", str(payload_file), "-out", str(signature_file)], check=True)
        manifest["signature"] = base64.b64encode(signature_file.read_bytes()).decode()
        manifest_file.write_text(json.dumps(manifest) + "\n")
        gitlab.upload(release, manifest_file)
        print(f"gitlab: repaired {release_tag} manifest as {channel} {manifest['version']}", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--repair-gitlab-manifests", action="store_true")
    args = parser.parse_args()
    tag = "v" + args.version.removeprefix("v")
    github = Github()
    release = github.api("GET", f"/releases/tags/{tag}")
    if release["draft"]:
        raise ValueError("Only an already published release can be tidied")
    assets = []
    page = 1
    while True:
        batch = github.api("GET", f"/releases/{release['id']}/assets?per_page=100&page={page}")
        assets.extend(batch)
        if len(batch) < 100:
            break
        page += 1
    metadata = lambda name: name.endswith((".sha256", ".provenance.json"))
    packages = sorted((asset for asset in assets if not metadata(asset["name"])
                       and asset["name"] not in ("manifest.json", "SHA256SUMS")),
                      key=lambda asset: asset["name"])
    if not packages or any(not asset.get("digest", "").startswith("sha256:") for asset in packages):
        raise ValueError("GitHub must provide a SHA256 digest for every package")
    with tempfile.TemporaryDirectory(prefix="release-metadata-", dir=os.environ.get("RUNNER_TEMP")) as directory:
        summary = Path(directory) / "SHA256SUMS"
        summary.write_text("".join(f"{asset['digest'][7:]}  {asset['name']}\n" for asset in packages))
        hosts = [github, Gitlab(), Cnb()]
        if args.repair_gitlab_manifests:
            if release["prerelease"]:
                raise ValueError("GitLab channel repair requires a stable release")
            repair_gitlab_manifests(release, hosts[1], Path(directory))
        for host in hosts:
            if host.name == "github":
                current, names = release, {asset["name"] for asset in assets}
            elif host.name == "gitlab":
                current = host.api("GET", f"/releases/{tag}")
                names = {asset["name"] for asset in current["assets"]["links"]}
            else:
                current = host.client.request("GET", f"/releases/tags/{tag}")
                names = {asset["name"] for asset in current["assets"]}
            host.upload(current, summary)
            host.prune(current, {name for name in names if not metadata(name)} | {"SHA256SUMS"})
            print(f"{host.name}: consolidated {len(packages)} checksums; packages and manifest preserved", flush=True)


if __name__ == "__main__":
    main()
