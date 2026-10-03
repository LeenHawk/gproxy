#!/usr/bin/env python3
"""Submit a published stable release to the existing fdroiddata fork."""
import argparse
import base64
from copy import deepcopy
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.parse
import urllib.request

from fdroidserver import metadata
import yaml

ROOT = Path(__file__).resolve().parents[2]
APP = "com.leenhawk.gproxy.app"
FILE = f"metadata/{APP}.yml"
OUTPUT = ROOT / "dist/mobile/fdroid" / FILE
UPSTREAM = "fdroid/fdroiddata"
FORK = "LeenHawk/fdroiddata"


def encode(value):
    return urllib.parse.quote(str(value), safe="")


def api(path, method="GET", data=None, optional=False):
    headers = {"Content-Type": "application/json"}
    if token := os.environ.get("FDROID_GITLAB_TOKEN"):
        headers["PRIVATE-TOKEN"] = token
    request = urllib.request.Request(
        f"https://gitlab.com/api/v4/{path}",
        data=json.dumps(data).encode() if data is not None else None,
        headers=headers, method=method,
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if optional and error.code == 404:
            return None
        raise RuntimeError(f"GitLab {method} {path}: HTTP {error.code}") from None


def recipe(project, ref):
    result = api(
        f"projects/{project}/repository/files/{encode(FILE)}?ref={encode(ref)}",
        optional=True,
    )
    if result is None:
        return None, None
    return yaml.safe_load(base64.b64decode(result["content"])), result["last_commit_id"]


def latest_code(document):
    if not document:
        return 0
    return max([document.get("CurrentVersionCode", 0)] + [
        build["versionCode"] for build in document.get("Builds", [])
    ])


def release_recipe(version):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Expected a stable version such as 4.0.3")
    tag = f"v{version}"
    release = json.loads(subprocess.check_output([
        "gh", "release", "view", tag, "--repo", "LeenHawk/gproxy",
        "--json", "isDraft,isPrerelease,publishedAt",
    ], text=True))
    if release["isDraft"] or release["isPrerelease"] or not release["publishedAt"]:
        raise ValueError("F-Droid submission requires a published stable release")
    subprocess.run([
        "git", "merge-base", "--is-ancestor", f"refs/tags/{tag}", "origin/main",
    ], cwd=ROOT, check=True)
    subprocess.run([
        "python3", str(ROOT / "scripts/mobile/fdroid-metadata.py"),
        f"refs/tags/{tag}", "--output", str(OUTPUT),
    ], cwd=ROOT, check=True)
    result = yaml.safe_load(OUTPUT.read_text())
    if result["CurrentVersion"] != version:
        raise ValueError("Release tag and source version do not match")
    return result


def open_submission(upstream, fork):
    page = 1
    while True:
        rows = api(f"projects/{upstream}/merge_requests?state=opened&"
                   f"author_username={encode(FORK.split('/')[0])}&per_page=100&page={page}")
        matches = [row for row in rows if row["source_project_id"] == fork and (
            row["source_branch"] == "new-app-gproxy"
            or row["source_branch"].startswith("gproxy-v")
        )]
        if matches:
            if len(matches) != 1:
                raise ValueError("Multiple open GPROXY submissions; resolve them first")
            return matches[0]
        if len(rows) < 100:
            return None
        page += 1


def submit(generated, dry_run=False):
    upstream = api(f"projects/{encode(UPSTREAM)}")
    fork = api(f"projects/{encode(FORK)}")
    if fork["visibility"] != "public":
        raise ValueError("The fdroiddata fork must be public")
    target = upstream["default_branch"]
    accepted, _ = recipe(upstream["id"], target)
    code, version = generated["CurrentVersionCode"], generated["CurrentVersion"]
    if latest_code(accepted) >= code:
        print(f"F-Droid already contains versionCode {code} or newer; skipping.")
        return

    mr = open_submission(upstream["id"], fork["id"])
    branch = mr["source_branch"] if mr else f"gproxy-v{version}"
    current, last_commit = recipe(fork["id"], branch)
    if latest_code(current) > code:
        print(f"The pending submission is newer than {version}; skipping.")
        return
    same_version = latest_code(current) == code
    if same_version:
        builds = [b for b in current["Builds"] if b["versionCode"] == code]
        if len(builds) != 1 or builds[0]["commit"] != generated["Builds"][0]["commit"]:
            raise ValueError("Existing versionCode points to a different source commit")
        if mr:
            print(f"Already submitted: {mr['web_url']}")
            return

    document = deepcopy(current or accepted or generated)
    if not same_version:
        # Retain reviewer changes to the build recipe and existing released builds.
        build = deepcopy(document["Builds"][-1])
        build.update({key: generated["Builds"][0][key]
                      for key in ("versionName", "versionCode", "commit")})
        document["Builds"] = document["Builds"] + [build] if accepted else [build]
        document["CurrentVersion"] = version
        document["CurrentVersionCode"] = code
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(yaml.safe_dump(document, sort_keys=False))
    metadata.write_metadata(OUTPUT, metadata.parse_metadata(OUTPUT))
    if dry_run:
        print(f"Dry run: would submit {version} on {FORK}:{branch}; recipe: {OUTPUT}")
        return
    if not os.environ.get("FDROID_GITLAB_TOKEN"):
        raise ValueError("FDROID_GITLAB_TOKEN is required; CI supplies it from GITLAB_RELEASE_TOKEN")
    if not same_version:
        action = {"action": "update" if current else "create", "file_path": FILE,
                  "content": OUTPUT.read_text()}
        if last_commit:
            action["last_commit_id"] = last_commit
        commit = {"branch": branch, "commit_message": f"Update GPROXY to {version}",
                  "actions": [action]}
        if not api(f"projects/{fork['id']}/repository/branches/{encode(branch)}", optional=True):
            commit.update(start_project=upstream["id"], start_branch=target)
            if accepted:
                action["action"] = "update"
        api(f"projects/{fork['id']}/repository/commits", "POST", commit)

    description = (
        f"Submit GPROXY {version} (versionCode {code}), AGPL-3.0-or-later.\n\n"
        f"Source: https://github.com/LeenHawk/gproxy/releases/tag/v{version}\n\n"
        f"Pinned commit: `{generated['Builds'][0]['commit']}`. "
        "English/Chinese Fastlane metadata lives in the upstream source repository. "
        "The upstream maintainer authorizes this submission.\n\n"
        "The recipe builds the ARM64 Android APK from source and declares NonFreeNet "
        "for proprietary AI integrations. It uses F-Droid signing; upstream-signature "
        "reproducible builds have not been established. Device/runtime testing is "
        "not established by this submission.\n\n"
        "This release was submitted automatically. F-Droid CI and maintainer review "
        "are pending for this revision. Existing reviewer recipe changes are retained."
    )
    details = {"title": f"Update GPROXY to {version}" if accepted else "New app: GPROXY",
               "description": description}
    if mr:
        result = api(f"projects/{upstream['id']}/merge_requests/{mr['iid']}", "PUT", details)
    else:
        result = api(f"projects/{fork['id']}/merge_requests", "POST", {
            **details, "source_branch": branch, "target_branch": target,
            "target_project_id": upstream["id"], "remove_source_branch": True,
            "squash": True, "allow_collaboration": True,
        })
    print(f"Submitted: {result['web_url']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    submit(release_recipe(args.version), args.dry_run)


if __name__ == "__main__":
    main()
