#!/usr/bin/env python3
"""Resolve branch publication to the updater's dev, beta and release channels."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tomllib


def context():
    source_version = tomllib.loads(Path("Cargo.toml").read_text())["workspace"]["package"]["version"]
    tag = os.environ.get("CI_COMMIT_TAG", "")
    ref_type = os.environ.get("GITHUB_REF_TYPE", "tag" if tag else "branch")
    ref = os.environ.get("GITHUB_REF_NAME") or tag or os.environ.get("CI_COMMIT_REF_NAME", "dev")
    sha = os.environ.get("GITHUB_SHA") or os.environ["CI_COMMIT_SHA"]
    version = source_version
    branch = ""
    if ref_type == "tag":
        if ref != f"v{source_version}":
            raise ValueError("Release tag must match the workspace version")
        channel = "beta" if "-" in source_version else "release"
        release_tag = ref
        manifest_version = version
    elif ref == "dev":
        branch, channel, release_tag, manifest_version = "dev", "dev", "nightly", sha
    elif ref == "main":
        branch, channel, release_tag = "main", "beta", "staging"
        base = source_version.split("-", 1)[0].split("+", 1)[0]
        major, minor, patch = map(int, base.split("."))
        if "-" not in source_version:
            patch += 1
        sequence = os.environ.get("GITHUB_RUN_ID") or os.environ.get("CI_PIPELINE_ID")
        if not sequence or not sequence.isdecimal():
            raise ValueError("A numeric pipeline ID is required for rolling beta versions")
        version = f"{major}.{minor}.{patch}-beta.{sequence}"
        manifest_version = version
    else:
        raise ValueError("Only main, dev and version tags may publish")
    if "--validate" in sys.argv:
        if branch == "dev" or ref_type == "tag":
            main = "refs/remotes/origin/main"
            ancestor, descendant = (main, sha) if branch == "dev" else (sha, main)
            result = subprocess.run(["git", "merge-base", "--is-ancestor", ancestor, descendant])
            if result.returncode:
                raise ValueError("Rebase dev on origin/main before pushing; stable releases must come from main")
    return dict(version=version, source_version=source_version, channel=channel,
                release_tag=release_tag, manifest_version=manifest_version,
                branch=branch)


if __name__ == "__main__":
    value = context()
    if "--shell" in sys.argv:
        names = dict(version="GPROXY_BUILD_VERSION", source_version="GPROXY_SOURCE_VERSION",
                     channel="GPROXY_BUILD_CHANNEL", release_tag="RELEASE_TAG",
                     manifest_version="GPROXY_MANIFEST_VERSION", branch="GPROXY_PUBLISH_BRANCH")
        for key, name in names.items():
            print(f"export {name}={shlex.quote(value[key])}")
    else:
        print(json.dumps(value))
