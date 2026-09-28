#!/usr/bin/env python3
"""Publish one verified artifact set to GitHub, GitLab and CNB."""
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.error
import urllib.parse
import urllib.request
import zipfile

from cnb import Cnb as CnbApi


def request(method, url, token, body=None, header="Authorization", missing=False):
    headers = {header: f"Bearer {token}" if header == "Authorization" else token,
               "Accept": "application/json", "Content-Type": "application/json"}
    req = urllib.request.Request(url, method=method, headers=headers,
                                 data=None if body is None else json.dumps(body).encode())
    try:
        with urllib.request.urlopen(req, timeout=120) as response:
            data = response.read()
            return json.loads(data) if data else None
    except urllib.error.HTTPError as error:
        if missing and error.code == 404:
            return None
        raise RuntimeError(f"{method} {urllib.parse.urlsplit(url).path}: HTTP {error.code}") from None


def git_push(url, ref, token, username, force=False):
    host = urllib.parse.urlsplit(url).netloc
    env = os.environ | {
        "GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_COUNT": "2",
        "GIT_CONFIG_KEY_0": f"http.https://{host}/.extraheader",
        "GIT_CONFIG_VALUE_0": "Authorization: Basic " + base64.b64encode(f"{username}:{token}".encode()).decode(),
        "GIT_CONFIG_KEY_1": "credential.helper", "GIT_CONFIG_VALUE_1": "",
    }
    subprocess.run(["git", "push", *( ["--force"] if force else []), url,
                    f"{os.environ['CI_COMMIT_SHA']}:{ref}"], env=env, check=True)


class Github:
    name = "github"
    def __init__(self):
        self.repo = os.environ["GITHUB_REPOSITORY"]
        self.token = os.environ["GH_TOKEN"]
        self.root = f"https://api.github.com/repos/{self.repo}"

    def api(self, method, path, body=None, missing=False):
        return request(method, self.root + path, self.token, body, missing=missing)

    def release(self, tag, notes, prerelease):
        release = self.api("GET", f"/releases/tags/{tag}", missing=True)
        return release or self.api("POST", "/releases", {
            "tag_name": tag, "target_commitish": os.environ["CI_COMMIT_SHA"],
            "name": f"gproxy {tag}", "body": notes, "draft": True,
            "prerelease": prerelease, "make_latest": "false",
        })

    def upload(self, release, path):
        for asset in release.get("assets", []):
            if asset["name"] == path.name:
                self.api("DELETE", f"/releases/assets/{asset['id']}")
        url = release["upload_url"].split("{")[0] + "?name=" + urllib.parse.quote(path.name)
        with path.open("rb") as stream:
            req = urllib.request.Request(url, method="POST", data=stream, headers={
                "Authorization": f"Bearer {self.token}", "Content-Type": "application/octet-stream",
                "Content-Length": str(path.stat().st_size),
            })
            with urllib.request.urlopen(req, timeout=600) as response:
                response.read()

    def finish(self, release, notes, prerelease, latest=False):
        self.api("PATCH", f"/releases/{release['id']}", {
            "draft": False, "body": notes, "prerelease": prerelease,
            "make_latest": "true" if latest else "false",
        })

    def move_tag(self, tag):
        ref = self.api("GET", f"/git/ref/tags/{tag}", missing=True)
        if ref:
            self.api("PATCH", f"/git/refs/tags/{tag}", {"sha": os.environ["CI_COMMIT_SHA"], "force": True})
        else:
            self.api("POST", "/git/refs", {"ref": f"refs/tags/{tag}", "sha": os.environ["CI_COMMIT_SHA"]})


class Gitlab:
    name = "gitlab"
    def __init__(self):
        self.root = f"{os.environ['CI_API_V4_URL']}/projects/{os.environ['CI_PROJECT_ID']}"
        self.token = os.environ["GITLAB_RELEASE_TOKEN"]
        self.url = os.environ["CI_PROJECT_URL"] + ".git"

    def api(self, method, path, body=None, missing=False):
        return request(method, self.root + path, self.token, body, "PRIVATE-TOKEN", missing)

    def release(self, tag, notes, prerelease):
        release = self.api("GET", f"/releases/{tag}", missing=True)
        # GitLab has no draft releases. Upload all package files before making
        # a new release visible, then create it with all links in one request.
        return release or {"tag_name": tag, "pending": True, "assets": {"links": []}}

    def upload(self, release, path):
        # The pipeline id makes manifests immutable in the package registry,
        # including a rerun of the same source commit.
        version = os.environ["CI_PIPELINE_ID"]
        url = f"{self.root}/packages/generic/gproxy/{version}/{urllib.parse.quote(path.name)}"
        with path.open("rb") as stream:
            req = urllib.request.Request(url, method="PUT", data=stream, headers={
                "PRIVATE-TOKEN": self.token, "Content-Type": "application/octet-stream",
                "Content-Length": str(path.stat().st_size),
            })
            with urllib.request.urlopen(req, timeout=600) as response:
                response.read()
        tag = urllib.parse.quote(release["tag_name"], safe="")
        link = next((a for a in release.get("assets", {}).get("links", []) if a["name"] == path.name), None)
        body = {"name": path.name, "url": url, "direct_asset_path": "/" + path.name, "link_type": "package"}
        if release.get("pending"):
            release["assets"]["links"].append(body)
        else:
            self.api("PUT" if link else "POST", f"/releases/{tag}/assets/links" + (f"/{link['id']}" if link else ""), body)

    def finish(self, release, notes, prerelease, latest=False):
        if release.get("pending"):
            self.api("POST", "/releases", {
                "tag_name": release["tag_name"], "ref": os.environ["CI_COMMIT_SHA"],
                "name": f"gproxy {release['tag_name']}", "description": notes,
                "assets": release["assets"],
            })
        else:
            self.api("PUT", f"/releases/{release['tag_name']}", {"description": notes})

    def move_tag(self, tag):
        git_push(self.url, f"refs/tags/{tag}", self.token, "oauth2", force=True)


class Cnb:
    name = "cnb"
    def __init__(self):
        os.environ["CNB_COMMIT"] = os.environ["CI_COMMIT_SHA"]
        os.environ["CNB_REPO_SLUG"] = "LeenHawk/gproxy"
        self.client = CnbApi()

    def sync(self):
        ref = (f"refs/tags/{os.environ['CI_COMMIT_TAG']}" if os.environ.get("CI_COMMIT_TAG")
               else "refs/heads/dev")
        git_push("https://cnb.cool/LeenHawk/gproxy.git", ref, os.environ["CNB_TOKEN"], "cnb")

    def release(self, tag, notes, prerelease):
        return self.client.release(tag, notes, prerelease)

    def upload(self, release, path):
        self.client.upload(path, release["id"])

    def finish(self, release, notes, prerelease, latest=False):
        self.client.request("PATCH", f"/releases/{release['id']}", {
            "draft": False, "body": notes, "prerelease": prerelease,
            "make_latest": "true" if latest else "false",
        })

    def move_tag(self, tag):
        git_push("https://cnb.cool/LeenHawk/gproxy.git", f"refs/tags/{tag}", os.environ["CNB_TOKEN"], "cnb", force=True)


def verify_packages():
    directory = Path("dist/release")
    matrix = json.loads(Path("scripts/release-targets.json").read_text())["include"]
    expected = ["gproxy-edge.wasm", "gproxy-edge-cloudflare.zip", "gproxy-edge.provenance.json"]
    extensions = {"linux": ".deb", "macos": ".dmg", "windows": ".msix", "android": ".apk"}
    if os.environ.get("CNB_BUILD_ID"):
        extensions.update(macos=".app.zip", windows=".exe")
    for row in matrix:
        cli = row["artifact"]
        expected += [cli + ".zip", cli + ".provenance.json"]
        with zipfile.ZipFile(directory / (cli + ".zip")) as archive:
            executable = "gproxy.exe" if row["os"] == "windows" else "gproxy"
            if executable not in archive.namelist() or any("gproxy-desktop" in n for n in archive.namelist()):
                raise ValueError(f"Wrong executable in CLI archive: {cli}")
        if row["os"] == "android":
            expected.append(cli + ".apk")
        if app := row.get("application_artifact"):
            expected += [app + extensions[row["os"]],
                         app + ".provenance.json"]
    for name in expected:
        path = directory / name
        if not path.is_file():
            raise ValueError(f"Missing release artifact: {name}")
        if not name.endswith(".provenance.json"):
            expected_hash = Path(str(path) + ".sha256").read_text().split()[0]
            with path.open("rb") as stream:
                actual = hashlib.file_digest(stream, "sha256").hexdigest()
            if actual != expected_hash:
                raise ValueError(f"Release checksum mismatch: {name}")
    print("Complete CLI, Application and Edge artifact sets verified")


def main():
    if os.environ.get("VERIFY_ONLY") == "true":
        verify_packages()
        return
    hosts = [Github(), Gitlab(), Cnb()]
    channel = os.environ["GPROXY_BUILD_CHANNEL"]
    tag = os.environ["RELEASE_TAG"]
    if channel == "dev" and hosts[0].api("GET", "/git/ref/heads/dev")["object"]["sha"] != os.environ["CI_COMMIT_SHA"]:
        print("Skipping stale nightly publication: GitHub dev has advanced")
        return
    hosts[2].sync()
    notes = Path(f"docs/release-notes/v{os.environ['GPROXY_BUILD_VERSION']}.md").read_text()
    builder = "CNB CI" if os.environ.get("CNB_BUILD_ID") else "GitLab CI"
    notes += f"\n\nBuilt once by [{builder}]({os.environ['CI_PIPELINE_URL']}). CLI (`gproxy-*`) and Application (`gproxy-tauri-*`) packages are separate.\n"
    assets = sorted(path for path in Path("dist/release").iterdir() if path.is_file())
    # Each host gets the same package bytes and a manifest signed for its URLs.
    for host in hosts:
        release = host.release(tag, notes, channel != "release")
        for path in assets:
            host.upload(release, path)
            print(f"{host.name}: uploaded {path.name}", flush=True)
        if channel == "dev":
            if hosts[0].api("GET", "/git/ref/heads/dev")["object"]["sha"] != os.environ["CI_COMMIT_SHA"]:
                print("Skipping stale manifest: GitHub dev advanced while uploading")
                return
            host.move_tag(tag)
        manifest = Path("dist/manifests") / host.name / "manifest.json"
        host.upload(release, manifest)
        host.finish(release, notes, channel != "release", channel == "release")
        if channel != "dev" and (channel == "beta" or host.name != "github"):
            versions = [r["tag_name"][1:] for r in hosts[0].api("GET", "/releases?per_page=100")
                        if r["tag_name"].startswith("v4.") and not r["draft"]
                        and r["prerelease"] == (channel == "beta")]
            newest = subprocess.check_output(["sort", "-V"], input="\n".join(versions), text=True).splitlines()
            if newest and newest[-1] != os.environ["GPROXY_BUILD_VERSION"]:
                continue
            pointer = host.release(channel, f"Signed {channel} channel manifest for {tag}.", True)
            host.move_tag(channel)
            host.upload(pointer, manifest)
            host.finish(pointer, f"Signed {channel} channel manifest for {tag}.", True)
        print(f"{host.name}: published {tag}", flush=True)


if __name__ == "__main__":
    main()
