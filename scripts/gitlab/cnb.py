#!/usr/bin/env python3
"""CNB release API adapter; credentials remain in CNB_TOKEN."""
import json
import os
from pathlib import Path
import urllib.error
import urllib.parse
import urllib.request


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

    def release(self, tag, body, prerelease=True):
        release = self.request("GET", f"/releases/tags/{urllib.parse.quote(tag)}", missing=True)
        if release is None:
            release = self.request("POST", "/releases", {
                "tag_name": tag, "target_commitish": self.commit,
                "name": f"gproxy {tag}", "body": body, "draft": True,
                "prerelease": prerelease, "make_latest": "false",
            })
        return release

