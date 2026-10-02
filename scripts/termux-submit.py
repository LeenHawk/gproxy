#!/usr/bin/env python3
"""Submit a published stable release to termux-packages."""
import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = "termux/termux-packages"
FORK = "LeenHawk/termux-packages"
BRANCH = "gproxy"
OUTPUT = ROOT / "dist/termux"


def run(*args, cwd=ROOT, **kwargs):
    return subprocess.run(args, cwd=cwd, check=True, **kwargs)


def read(*args, cwd=ROOT):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def api(path, data=None, method=None):
    command = ["gh", "api", path]
    if method:
        command += ["--method", method]
    if data is not None:
        command += ["--input", "-"]
    result = subprocess.run(command, input=json.dumps(data) if data is not None else None,
                            text=True, capture_output=True, check=True)
    return json.loads(result.stdout)


def version_tuple(version):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError(f"Expected a stable version, got {version!r}")
    return tuple(map(int, version.split(".")))


def recipe_version(text):
    match = re.search(r'^TERMUX_PKG_VERSION=["\']?([0-9.]+)["\']?$', text, re.M)
    if not match:
        raise ValueError("Cannot determine Termux recipe version")
    return match[1]


def update_recipe(text, version, checksum):
    # Preserve the maintainer's build commands, dependencies and patches.
    expected = 'TERMUX_PKG_SRCURL="https://codeload.github.com/LeenHawk/gproxy/tar.gz/refs/tags/v${TERMUX_PKG_VERSION}"'
    if expected not in text:
        raise ValueError("Source URL changed; review the submission script before updating")
    old = recipe_version(text)
    if version_tuple(old) > version_tuple(version):
        raise ValueError("Refusing to downgrade a newer recipe")
    old_checksum = re.search(r'^TERMUX_PKG_SHA256=["\']?([a-f0-9]{64})["\']?$', text, re.M)
    if not old_checksum:
        raise ValueError("Cannot determine source checksum")
    if old == version and old_checksum[1] != checksum:
        raise ValueError("Source checksum changed for an existing version")
    text = re.sub(r'^TERMUX_PKG_VERSION=.*$', f'TERMUX_PKG_VERSION="{version}"', text, flags=re.M)
    text = re.sub(r'^TERMUX_PKG_SHA256=.*$', f'TERMUX_PKG_SHA256={checksum}', text, flags=re.M)
    if old != version:
        text = re.sub(r'^TERMUX_PKG_REVISION=.*\n', '', text, flags=re.M)
    return text


def submission_text(version, accepted):
    release = f"https://github.com/LeenHawk/gproxy/releases/tag/v{version}"
    if accepted:
        title = f"bump(main/gproxy): {version}"
        body = f"Update GPROXY to [{version}]({release}).\n\n"
    else:
        title = "addpkg(main/gproxy): AI API gateway with a web console"
        body = (
            f"Add [GPROXY {version}]({release}), an AGPL-3.0-or-later gateway "
            "for multiple AI providers with a web configuration interface. "
            "It can run locally in Termux without root.\n\n"
            "- Builds the CLI and embedded web assets from source for aarch64 and x86_64.\n"
            "- Uses Termux OpenSSL and libc++; disables executable self-update.\n"
            "- Replaces the upstream `gproxy-cli` package, which owns the same command.\n\n"
        )
    body += "Checks: source version/checksum, patch application and package lint.\n"
    return title, body


def submit(version, dry_run, notes):
    requested = version_tuple(version)
    release = json.loads(read("gh", "release", "view", f"v{version}", "--repo", "LeenHawk/gproxy",
                              "--json", "isDraft,isPrerelease,publishedAt"))
    if release["isDraft"] or release["isPrerelease"] or not release["publishedAt"]:
        raise ValueError("Only published stable releases can be submitted")
    run("git", "fetch", "origin", "main", f"refs/tags/v{version}:refs/tags/v{version}")
    run("git", "merge-base", "--is-ancestor", f"v{version}", "origin/main")
    prs = api(f"repos/{UPSTREAM}/pulls?state=all&head=LeenHawk:{BRANCH}&per_page=100")
    pending = next((p for p in prs if p["state"] == "open"), None)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="submission-", dir=OUTPUT) as directory:
        work = Path(directory)
        checkout = work / "packages"
        run("git", "clone", "--depth", "1", f"https://github.com/{UPSTREAM}.git", str(checkout))
        base = read("git", "branch", "--show-current", cwd=checkout)
        package = checkout / "packages/gproxy"
        recipe = package / "build.sh"
        accepted = recipe.exists()
        if accepted and version_tuple(recipe_version(recipe.read_text())) >= requested:
            print("Termux already contains this version or a newer one; skipping.")
            return
        if not pending and prs and not prs[0]["merged_at"]:
            raise ValueError("The previous PR was closed without merging; maintainer review is required")
        run("git", "remote", "add", "submission", f"https://github.com/{FORK}.git", cwd=checkout)
        if pending:
            run("git", "fetch", "--depth", "1", "submission", BRANCH, cwd=checkout)
            run("git", "checkout", "-B", BRANCH, "FETCH_HEAD", cwd=checkout)
            lease = read("git", "rev-parse", "HEAD", cwd=checkout)
            if version_tuple(recipe_version(recipe.read_text())) > requested:
                print("The pending PR contains a newer version; skipping.")
                return
        else:
            run("git", "checkout", "-b", BRANCH, cwd=checkout)
            remote = read("git", "ls-remote", "submission", f"refs/heads/{BRANCH}", cwd=checkout)
            lease = remote.split()[0] if remote else ""
        if not recipe.exists():
            shutil.copytree(ROOT / "distribution/termux/gproxy", package)
        archive = work / "source.tar.gz"
        url = f"https://codeload.github.com/LeenHawk/gproxy/tar.gz/refs/tags/v{version}"
        with urllib.request.urlopen(url, timeout=60) as response, archive.open("wb") as output:
            shutil.copyfileobj(response, output)
        with archive.open("rb") as source_file:
            checksum = hashlib.file_digest(source_file, "sha256").hexdigest()
        recipe.write_text(update_recipe(recipe.read_text(), version, checksum))
        source = work / "source"
        with tarfile.open(archive) as bundle:
            bundle.extractall(source, filter="data")
        source = source / f"gproxy-{version}"
        import tomllib
        if tomllib.loads((source / "Cargo.toml").read_text())["workspace"]["package"]["version"] != version:
            raise ValueError("Release version does not match the source archive")
        for patch in sorted(package.glob("*.patch")):
            run("patch", "--batch", "--forward", "-p1", "-i", str(patch), cwd=source)
        run("bash", "-n", str(recipe))
        run("bash", "scripts/lint-packages.sh", "packages/gproxy/build.sh", cwd=checkout)
        preview = OUTPUT / "gproxy"
        if preview.exists():
            shutil.rmtree(preview)
        shutil.copytree(package, preview)
        title, body = submission_text(version, accepted)
        if notes:
            body += "\n" + notes.read_text()
        (OUTPUT / "PR.md").write_text(body)
        if dry_run:
            print(f"Dry run: {title}; review {OUTPUT}")
            return
        run("git", "add", "--", "packages/gproxy", cwd=checkout)
        changed = subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=checkout).returncode
        if changed:
            run("git", "-c", "user.name=GPROXY release automation", "-c",
                "user.email=LeenHawk@users.noreply.github.com", "commit", "-m", title, cwd=checkout)
        elif pending:
            print(f"Already submitted: {pending['html_url']}")
            return
        run("git", "-c", "credential.helper=", "-c", "credential.helper=!gh auth git-credential",
            "push", f"--force-with-lease=refs/heads/{BRANCH}:{lease}", "submission",
            f"HEAD:refs/heads/{BRANCH}", cwd=checkout)
        if pending:
            result = api(f"repos/{UPSTREAM}/pulls/{pending['number']}",
                         {"title": title, "body": body}, "PATCH")
        else:
            result = api(f"repos/{UPSTREAM}/pulls", {"title": title, "body": body,
                         "head": f"LeenHawk:{BRANCH}", "base": base, "maintainer_can_modify": True}, "POST")
        print(f"Submitted: {result['html_url']}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--validation-notes", type=Path)
    args = parser.parse_args()
    submit(args.version, args.dry_run, args.validation_notes)
