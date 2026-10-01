#!/usr/bin/env python3
"""Consolidate public release metadata without rebuilding or changing packages."""
import argparse
import os
from pathlib import Path
import tempfile

from publish import Github, Gitlab, Cnb


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
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
