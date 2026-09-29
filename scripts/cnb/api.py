#!/usr/bin/env python3
"""CNB commit artifacts and releases. Credentials stay in CNB_TOKEN."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile


class Cnb:
    def __init__(self):
        self.endpoint = os.environ.get("CNB_API_ENDPOINT", "https://api.cnb.cool").rstrip("/")
        self.repo = os.environ["CNB_REPO_SLUG"]
        self.commit = os.environ["CNB_COMMIT"]
        self.token = os.environ["CNB_TOKEN"]
        self.root = f"{self.endpoint}/{self.repo}/-"

    def request(self, method, path, data=None, missing=False):
        url = path if path.startswith("https://") else self.root + path
        if urllib.parse.urlsplit(url).netloc != urllib.parse.urlsplit(self.endpoint).netloc:
            raise ValueError("CNB API request must stay on the API host")
        request = urllib.request.Request(
            url, method=method,
            data=None if data is None else json.dumps(data).encode(),
            headers={"Authorization": f"Bearer {self.token}",
                     "Accept": "application/vnd.cnb.api+json",
                     "Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                body = response.read()
                return json.loads(body) if body else None
        except urllib.error.HTTPError as error:
            if missing and error.code == 404:
                return None
            # Do not print request URLs: confirmation URLs contain upload tokens.
            raise RuntimeError(f"CNB API {method} failed with HTTP {error.code}") from None

    def upload(self, path, release_id=None):
        path = Path(path)
        base = (f"/releases/{release_id}" if release_id else
                f"/git/commit-assets/{self.commit}")
        body = {"asset_name": path.name, "size": path.stat().st_size,
                "ttl": 0 if release_id else 3}
        if release_id:
            body["overwrite"] = True
        ticket = self.request("POST", base + "/asset-upload-url", body)
        # Upload directly to the pre-signed storage URL, without the CNB token.
        with path.open("rb") as stream:
            request = urllib.request.Request(ticket["upload_url"], data=stream, method="PUT",
                                             headers={"Content-Length": str(path.stat().st_size)})
            with urllib.request.urlopen(request, timeout=600) as response:
                response.read()
        verify = ticket["verify_url"]
        if not verify.startswith("https://"):
            verify = self.endpoint + verify
        self.request("POST", verify)
        print(f"Uploaded {path.name} ({path.stat().st_size} bytes)", flush=True)

    def download(self, name, destination):
        url = (f"https://cnb.cool/{self.repo}/-/commit-assets/download/"
               f"{self.commit}/{urllib.parse.quote(name)}")
        destination = Path(destination)
        destination.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(url, timeout=600) as response, destination.open("wb") as stream:
            shutil.copyfileobj(response, stream)

    def collect(self):
        matrix = json.loads(Path("scripts/release-targets.json").read_text())["include"]
        for target in [row["target"] for row in matrix] + ["edge"]:
            bundle = Path("dist/downloads") / f"{os.environ['RELEASE_RUN_ID']}-{target}.tar.gz"
            self.download(bundle.name, bundle)
            with tarfile.open(bundle) as archive:
                archive.extractall("dist", filter="data")
        # CLI and Application have separate names. Both sets must be complete.
        for row in matrix:
            names = [row["artifact"] + ".zip"]
            with zipfile.ZipFile(Path("dist/release") / names[0]) as archive:
                executable = "gproxy.exe" if row["os"] == "windows" else "gproxy"
                if executable not in archive.namelist():
                    raise ValueError(f"CLI archive is missing {executable}: {names[0]}")
                if any("gproxy-desktop" in name for name in archive.namelist()):
                    raise ValueError(f"Application executable found in CLI archive: {names[0]}")
            if row["os"] == "android":
                names.append(row["artifact"] + ".apk")
            if app := row.get("application_artifact"):
                extension = {"linux": ".deb", "windows": ".exe", "macos": ".app.zip", "android": ".apk"}[row["os"]]
                names.append(app + extension)
            for name in names:
                package = Path("dist/release") / name
                expected = Path(str(package) + ".sha256").read_text().split()[0]
                with package.open("rb") as stream:
                    actual = hashlib.file_digest(stream, "sha256").hexdigest()
                if expected != actual:
                    raise ValueError(f"Checksum mismatch: {name}")
        print("All CLI and Application packages collected and checksums verified")

    def wait_ci(self):
        deadline = time.monotonic() + 7200
        while time.monotonic() < deadline:
            status = self.request("GET", f"/build/status/{os.environ['CNB_BUILD_ID']}")
            checks = [p for p in status["pipelinesStatus"].values() if p["name"] == "CI"]
            if not checks:
                raise RuntimeError("Release requires a CI pipeline in the same build")
            if all(p["status"] == "success" for p in checks):
                print("CI passed for this release commit")
                return
            if any(p["status"] in ("error", "cancel", "skipped") for p in checks):
                raise RuntimeError("CI did not pass; refusing publication")
            time.sleep(15)
        raise RuntimeError("Timed out waiting for CI")

    def move_tag(self, tag):
        subprocess.run(["git", "config", "credential.https://cnb.cool.helper", "!cnb git-credential"], check=True)
        subprocess.run(["git", "tag", "-f", tag, self.commit], check=True)
        subprocess.run(["git", "push", "--force", "origin", f"refs/tags/{tag}"], check=True)

    def prune(self, release, keep):
        current = self.request("GET", f"/releases/{release['id']}")
        for asset in current.get("assets", []):
            if asset["name"] not in keep:
                self.request("DELETE", f"/releases/{release['id']}/assets/{asset['id']}")

    def release(self, tag, body, prerelease=True):
        release = self.request("GET", f"/releases/tags/{urllib.parse.quote(tag)}", missing=True)
        if release is None:
            release = self.request("POST", "/releases", {
                "tag_name": tag, "target_commitish": self.commit,
                "name": f"gproxy {tag}", "body": body, "draft": True,
                "prerelease": prerelease, "make_latest": "false",
            })
        return release

    def publish(self):
        directory = Path("dist/release")
        manifest = json.loads((directory / "manifest.json").read_text())
        channel = manifest["channel"]
        tag = os.environ["RELEASE_TAG"]
        notes = Path(f"docs/release-notes/v{os.environ['GPROXY_BUILD_VERSION']}.md").read_text()
        notes += ("\n\nCNB packages: `gproxy-*` are CLI/server builds; `gproxy-tauri-*` are "
                  "Application builds. Windows Application uses an NSIS installer; macOS "
                  "Application uses an ad-hoc signed `.app.zip`. Store submissions and "
                  "GitHub attestations are not produced by CNB.\n")
        if channel == "dev":
            latest = subprocess.check_output(["git", "ls-remote", "origin", "refs/heads/dev"], text=True).split()[0]
            if latest != self.commit:
                print("Skipping stale nightly: dev has advanced")
                return
            if manifest["version"] != self.commit:
                raise ValueError("Nightly manifest must identify this commit")
        release = self.release(tag, notes, prerelease=channel != "release")
        assets = [file for file in sorted(directory.iterdir())
                  if file.is_file() and file.name != "manifest.json"
                  and (channel != "dev" or not file.name.endswith((".sha256", ".provenance.json")))]
        for file in assets:
            self.upload(file, release["id"])
        if channel == "dev":
            latest = subprocess.check_output(["git", "ls-remote", "origin", "refs/heads/dev"], text=True).split()[0]
            if latest != self.commit:
                print("Skipping stale manifest: dev advanced while uploading")
                return
            self.move_tag(tag)
        # Publish the manifest last, once every signed URL resolves.
        self.upload(directory / "manifest.json", release["id"])
        self.request("PATCH", f"/releases/{release['id']}", {
            "body": notes, "draft": False, "prerelease": channel != "release",
            "make_latest": "true" if channel == "release" else "false",
        })
        if channel == "dev":
            self.prune(release, {p.name for p in assets} | {"manifest.json"})
        if channel != "dev":
            releases = self.request("GET", "/releases?page_size=100")
            versions = [r["tag_name"][1:] for r in releases
                        if r["tag_name"].startswith("v4.") and not r["draft"]
                        and r["prerelease"] == (channel == "beta")]
            ordered = subprocess.check_output(["sort", "-V"], input="\n".join(versions), text=True).splitlines()
            if ordered and ordered[-1] != os.environ["GPROXY_BUILD_VERSION"]:
                print("Skipping stale channel pointer")
                return
            pointer = self.release(channel, f"Signed manifest for the {channel} channel ({tag}).")
            self.move_tag(channel)
            self.upload(directory / "manifest.json", pointer["id"])
            self.request("PATCH", f"/releases/{pointer['id']}", {"draft": False, "prerelease": True, "make_latest": "false"})
        print(f"Published https://cnb.cool/{self.repo}/-/releases/tag/{tag}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["upload", "download", "collect", "wait-ci", "publish"])
    parser.add_argument("paths", nargs="*")
    args = parser.parse_args()
    cnb = Cnb()
    if args.command == "upload":
        for path in args.paths:
            cnb.upload(path)
    elif args.command == "download":
        cnb.download(*args.paths)
    elif args.command == "collect":
        cnb.collect()
    elif args.command == "wait-ci":
        cnb.wait_ci()
    else:
        cnb.publish()


if __name__ == "__main__":
    main()
