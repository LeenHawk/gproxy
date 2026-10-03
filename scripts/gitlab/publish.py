#!/usr/bin/env python3
"""Publish one verified artifact set to GitHub, GitLab and CNB."""
import base64
from concurrent.futures import ThreadPoolExecutor
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


def git_push(url, ref, token, username, force=False, mirror=False):
    host = urllib.parse.urlsplit(url).netloc
    env = os.environ | {
        "GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_COUNT": "2",
        "GIT_CONFIG_KEY_0": f"http.https://{host}/.extraheader",
        "GIT_CONFIG_VALUE_0": "Authorization: Basic " + base64.b64encode(f"{username}:{token}".encode()).decode(),
        "GIT_CONFIG_KEY_1": "credential.helper", "GIT_CONFIG_VALUE_1": "",
    }
    options = ["--force"] if force else []
    if mirror:
        remote = subprocess.check_output(["git", "ls-remote", url, ref], env=env, text=True).split()
        options = [f"--force-with-lease={ref}:{remote[0] if remote else ''}"]
    subprocess.run(["git", "push", *options, url,
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

    def prune(self, release, keep):
        page = 1
        obsolete = []
        while True:
            assets = self.api("GET", f"/releases/{release['id']}/assets?per_page=100&page={page}")
            obsolete.extend(asset for asset in assets if asset["name"] not in keep)
            if len(assets) < 100:
                break
            page += 1
        for asset in obsolete:
            self.api("DELETE", f"/releases/assets/{asset['id']}")

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
        # Release and beta manifests differ even within the same pipeline.
        version = f"{os.environ['CI_PIPELINE_ID']}-{release['tag_name']}"
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

    def prune(self, release, keep):
        tag = urllib.parse.quote(release["tag_name"], safe="")
        current = self.api("GET", f"/releases/{tag}")
        for link in current["assets"]["links"]:
            if link["name"] not in keep:
                self.api("DELETE", f"/releases/{tag}/assets/links/{link['id']}")

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
               else f"refs/heads/{os.environ.get('GPROXY_PUBLISH_BRANCH') or 'dev'}")
        git_push("https://cnb.cool/LeenHawk/gproxy.git", ref, os.environ["CNB_TOKEN"], "cnb",
                 mirror=ref.startswith("refs/heads/"))

    def release(self, tag, notes, prerelease):
        return self.client.release(tag, notes, prerelease)

    def upload(self, release, path):
        self.client.upload(path, release["id"])

    def finish(self, release, notes, prerelease, latest=False):
        self.client.request("PATCH", f"/releases/{release['id']}", {
            "draft": False, "body": notes, "prerelease": prerelease,
            "make_latest": "true" if latest else "false",
        })

    def prune(self, release, keep):
        self.client.prune(release, keep)

    def move_tag(self, tag):
        git_push("https://cnb.cool/LeenHawk/gproxy.git", f"refs/tags/{tag}", os.environ["CNB_TOKEN"], "cnb", force=True)


def verify_packages():
    directory = Path("dist/release")
    matrix = json.loads(Path("scripts/release-targets.json").read_text())["include"]
    expected = ["gproxy-edge.wasm", "gproxy-edge-cloudflare.zip", "gproxy-edge.provenance.json"]
    if os.environ.get("GITHUB_ACTIONS") == "true":
        for arch in ("x86_64", "aarch64"):
            name = f"gproxy-serverless-linux-{arch}-musl"
            expected += [name + ".zip", name + ".provenance.json"]
            with zipfile.ZipFile(directory / (name + ".zip")) as archive:
                if "gproxy-serverless" not in archive.namelist():
                    raise ValueError(f"Missing serverless executable: {name}")
        for row in matrix:
            if not (name := row.get("headless_artifact")):
                continue
            expected += [name + ".zip", name + ".provenance.json"]
            with zipfile.ZipFile(directory / (name + ".zip")) as archive:
                executable = "gproxy.exe" if row["os"] == "windows" else "gproxy"
                if executable not in archive.namelist():
                    raise ValueError(f"Missing headless executable: {name}")
                if row["os"] == "android" and "gproxy.bin" not in archive.namelist():
                    raise ValueError(f"Missing headless Android binary: {name}")
    extensions = {"linux": ".deb", "macos": ".dmg", "windows": ".msix", "android": ".apk", "ohos": ".hap"}
    for row in matrix:
        cli = row["artifact"]
        expected += [cli + ".zip", cli + ".provenance.json"]
        with zipfile.ZipFile(directory / (cli + ".zip")) as archive:
            executable = "gproxy.exe" if row["os"] == "windows" else "gproxy"
            if executable not in archive.namelist() or any("gproxy-desktop" in n for n in archive.namelist()):
                raise ValueError(f"Wrong executable in CLI archive: {cli}")
        if row["os"] != "ohos":
            cli_extension = ".deb" if row["os"] == "android" else extensions[row["os"]]
            expected.append(cli + cli_extension)
        if app := row.get("application_artifact"):
            expected += [app + extensions[row["os"]], app + ".provenance.json"]
            if row["os"] not in ("android", "ohos"):
                expected.append(app + ".zip")
                with zipfile.ZipFile(directory / (app + ".zip")) as archive:
                    executable = "gproxy-desktop.exe" if row["os"] == "windows" else "gproxy-desktop"
                    if not any(Path(name).name == executable for name in archive.namelist()):
                        raise ValueError(f"Missing Application executable: {app}")
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


def upload_assets(host, release, assets):
    def upload(path):
        host.upload(release, path)
        return path

    # Wait for every upload before publishing the signed manifest or moving
    # channel pointers. A failed upload still aborts publication.
    with ThreadPoolExecutor(max_workers=4) as pool:
        for path in pool.map(upload, assets):
            print(f"{host.name}: uploaded {path.name}", flush=True)


def release_assets(directory):
    extensions = {".zip", ".deb", ".dmg", ".msix", ".apk", ".hap", ".wasm"}
    assets = sorted(path for path in directory.iterdir()
                    if path.is_file() and path.suffix in extensions)
    checksums = directory / "SHA256SUMS"
    checksums.write_text("".join(
        f"{Path(str(path) + '.sha256').read_text().split()[0]}  {path.name}\n"
        for path in assets
    ))
    return [*assets, checksums]


def main():
    if os.environ.get("VERIFY_ONLY") == "true":
        verify_packages()
        return
    hosts = [Github(), Gitlab(), Cnb()]
    channel = os.environ["GPROXY_BUILD_CHANNEL"]
    tag = os.environ["RELEASE_TAG"]
    branch = os.environ.get("GPROXY_PUBLISH_BRANCH", "")
    def current():
        return not branch or hosts[0].api("GET", f"/git/ref/heads/{branch}")["object"]["sha"] == os.environ["CI_COMMIT_SHA"]
    if not current():
        print(f"Skipping stale publication: {branch} has advanced")
        return
    hosts[2].sync()
    source_version = os.environ.get("GPROXY_SOURCE_VERSION", os.environ["GPROXY_BUILD_VERSION"])
    notes = Path(f"docs/release-notes/v{source_version}.md").read_text()
    builder = ("GitHub Actions" if os.environ.get("GITHUB_ACTIONS") == "true"
               else "CNB CI" if os.environ.get("CNB_BUILD_ID") else "GitLab CI")
    notes += f"\n\nBuilt once by [{builder}]({os.environ['CI_PIPELINE_URL']}). CLI (`gproxy-*`) and Application (`gproxy-tauri-*`) packages are separate.\n"
    assets = release_assets(Path("dist/release"))
    # Each host gets the same package bytes and a manifest signed for its URLs.
    for host in hosts:
        release = host.release(tag, notes, channel != "release")
        upload_assets(host, release, assets)
        if branch:
            if not current():
                print(f"Skipping stale manifest: {branch} advanced while uploading")
                return
            host.move_tag(tag)
        manifest = Path("dist/manifests") / host.name / "manifest.json"
        host.upload(release, manifest)
        host.finish(release, notes, channel != "release", channel == "release")
        if branch:
            host.prune(release, {path.name for path in assets} | {"manifest.json"})
        if channel != "dev" and not branch:
            versions = [r["tag_name"][1:] for r in hosts[0].api("GET", "/releases?per_page=100")
                        if r["tag_name"].startswith("v4.") and not r["draft"]
                        and r["prerelease"] == (channel == "beta")]
            newest = subprocess.check_output(["sort", "-V"], input="\n".join(versions), text=True).splitlines()
            if newest and newest[-1] != os.environ["GPROXY_BUILD_VERSION"]:
                continue
            if channel == "release" and host.name != "github":
                pointer = host.release("release", f"Signed release channel manifest for {tag}.", True)
                host.move_tag("release")
                host.upload(pointer, manifest)
                host.finish(pointer, f"Signed release channel manifest for {tag}.", True)
            beta_manifest = manifest if channel == "beta" else manifest.parent / "beta" / "manifest.json"
            staging = host.release("staging", notes, True)
            upload_assets(host, staging, assets)
            host.move_tag("staging")
            host.upload(staging, beta_manifest)
            host.finish(staging, notes, True)
            host.prune(staging, {path.name for path in assets} | {"manifest.json"})
        # v4.0.0 clients used the old beta URL; keep it as a manifest-only alias.
        if channel in ("beta", "release"):
            beta_manifest = manifest if channel == "beta" else manifest.parent / "beta" / "manifest.json"
            pointer = host.release("beta", "Compatibility alias for the staging update channel.", True)
            host.move_tag("beta")
            host.upload(pointer, beta_manifest)
            host.finish(pointer, "Compatibility alias for the staging update channel.", True)
        print(f"{host.name}: published {tag}", flush=True)


if __name__ == "__main__":
    main()
