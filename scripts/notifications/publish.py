#!/usr/bin/env python3
"""Build the signed announcement feed from GitHub Discussions and publish it.

Every discussion in the repository's Announcements category that a maintainer
opened and that is not closed becomes one notice. The body carries the notice's
metadata and its translations in HTML comments, which GitHub does not render:

    <!-- gproxy-notification
    severity: warning            info | warning | critical (required)
    id: example-2026-11          optional; default discussion-<number>
    published_at: 2026-11-04T00:00:00Z   optional; default the creation time
    expires_at: 2027-01-01T00:00:00Z     optional
    affects: >=4.1.0, <4.1.9             optional semver range
    channels: release, beta              optional; default every channel
    -->

    <!-- en -->
    # Title
    Body, in the Markdown subset the console renders.

    <!-- zh-CN -->
    # 标题
    正文

`en` is required; `zh-CN`, `zh-TW` and `ru` are optional. The whole feed is
rebuilt on every run, so editing a discussion edits its notice and closing or
deleting it withdraws the notice.

The feed is signed with the update signing key and uploaded to a `notifications`
release on GitHub, CNB and GitLab, which is where clients read it. A copy is
written to `--docs` (a gproxy-docs checkout) for clients that predate this and
still read it from the documentation site.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "gitlab"))
import publish  # noqa: E402  (three-platform release hosts)

TAG = "notifications"
FILES = ("notifications.json", "notifications.json.sig")
LOCALES = ("en", "zh-CN", "zh-TW", "ru")
MAINTAINERS = {"OWNER", "MEMBER", "COLLABORATOR"}
META = re.compile(r"<!--\s*gproxy-notification\s*\n(.*?)-->", re.S)
SECTION = re.compile(r"<!--\s*(en|zh-CN|zh-TW|ru)\s*-->")
RFC3339 = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$")
COMPARATOR = re.compile(r"^(?:=|>=|>|<=|<|\^|~)?(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$")
CHANNELS = {"dev", "beta", "release"}
QUERY = """
query($owner: String!, $name: String!, $after: String) {
  repository(owner: $owner, name: $name) {
    discussions(first: 50, after: $after, orderBy: {field: CREATED_AT, direction: ASC}) {
      pageInfo { hasNextPage endCursor }
      nodes { number url closed createdAt authorAssociation body category { slug } }
    }
  }
}
"""


def discussions(repo):
    owner, name = repo.split("/")
    after = None
    while True:
        body = json.dumps({"query": QUERY, "variables": {"owner": owner, "name": name, "after": after}})
        request = urllib.request.Request("https://api.github.com/graphql", data=body.encode(), headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}", "Content-Type": "application/json"})
        with urllib.request.urlopen(request, timeout=60) as response:
            data = json.load(response)
        if data.get("errors"):
            raise RuntimeError(f"GraphQL: {data['errors']}")
        page = data["data"]["repository"]["discussions"]
        yield from page["nodes"]
        if not page["pageInfo"]["hasNextPage"]:
            return
        after = page["pageInfo"]["endCursor"]


def notice(discussion):
    """One discussion as a feed entry, or a list of reasons it is not one."""
    where = discussion["url"]
    body = discussion["body"].replace("\r\n", "\n")
    meta = META.search(body)
    if not meta:
        return None, [f"{where}: no <!-- gproxy-notification --> block"]
    fields = {}
    for line in meta.group(1).splitlines():
        if line.strip():
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    errors = [f"{where}: unknown field {key}" for key in fields
              if key not in {"severity", "id", "published_at", "expires_at", "affects", "channels"}]
    entry = {
        "id": fields.get("id") or f"discussion-{discussion['number']}",
        "severity": fields.get("severity", ""),
        "published_at": fields.get("published_at") or discussion["createdAt"],
    }
    if entry["severity"] not in {"info", "warning", "critical"}:
        errors.append(f"{where}: severity must be info, warning or critical")
    for key in ("published_at", "expires_at"):
        value = fields.get(key) if key == "expires_at" else entry[key]
        if value is not None and not RFC3339.match(value):
            errors.append(f"{where}: {key} must be RFC 3339")
    if fields.get("expires_at"):
        entry["expires_at"] = fields["expires_at"]
    if fields.get("affects"):
        if not all(COMPARATOR.match(part.strip()) for part in fields["affects"].split(",")):
            errors.append(f"{where}: affects must be a comma-separated semver range")
        entry["affects"] = fields["affects"]
    if fields.get("channels"):
        channels = [value.strip() for value in fields["channels"].split(",") if value.strip()]
        if not set(channels) <= CHANNELS:
            errors.append(f"{where}: channels must be among dev, beta, release")
        entry["channels"] = channels
    content = {}
    parts = SECTION.split(body[meta.end():])
    for locale, text in zip(parts[1::2], parts[2::2]):
        lines = text.strip().splitlines()
        if not lines or not lines[0].startswith("# "):
            errors.append(f"{where}: the {locale} section must start with a '# ' title")
            continue
        content[locale] = {"title": lines[0][2:].strip(), "body": "\n".join(lines[1:]).strip()}
        if not content[locale]["title"] or not content[locale]["body"]:
            errors.append(f"{where}: the {locale} section needs a title and a body")
    if "en" not in content:
        errors.append(f"{where}: an <!-- en --> section is required")
    entry["content"] = {locale: content[locale] for locale in LOCALES if locale in content}
    return entry, errors


def build(repo):
    entries, errors = [], []
    for discussion in discussions(repo):
        if discussion["category"]["slug"] != "announcements" or discussion["closed"]:
            continue
        if discussion["authorAssociation"] not in MAINTAINERS:
            print(f"skipping {discussion['url']}: not opened by a maintainer")
            continue
        entry, problems = notice(discussion)
        errors += problems
        if entry and not problems:
            entries.append(entry)
    ids = [entry["id"] for entry in entries]
    errors += [f"duplicate id {value}" for value in sorted({i for i in ids if ids.count(i) > 1})]
    if errors:
        raise SystemExit("Not publishing; fix these discussions first:\n  " + "\n  ".join(errors))
    entries.sort(key=lambda entry: entry["published_at"], reverse=True)
    return json.dumps({"version": 1, "notifications": entries}, ensure_ascii=False, indent=2) + "\n"


def sign(feed, directory):
    """Write the feed and its detached base64 Ed25519 signature; verify both."""
    (directory / FILES[0]).write_text(feed)
    with tempfile.TemporaryDirectory() as work:
        key = Path(work) / "key.pem"
        key.write_bytes(base64.b64decode(os.environ["UPDATE_SIGNING_PRIVATE_KEY_B64"]))
        key.chmod(0o600)
        public = subprocess.check_output(["openssl", "pkey", "-in", key, "-pubout", "-outform", "DER"])
        if base64.b64encode(public[-32:]).decode() != os.environ["UPDATE_SIGNING_PUBLIC_KEY_B64"]:
            raise SystemExit("UPDATE_SIGNING_PRIVATE_KEY_B64 does not match UPDATE_SIGNING_PUBLIC_KEY_B64")
        signature = Path(work) / "sig.bin"
        subprocess.run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", key,
                        "-in", directory / FILES[0], "-out", signature], check=True)
        pem = Path(work) / "pub.pem"
        subprocess.run(["openssl", "pkey", "-in", key, "-pubout", "-out", pem], check=True)
        subprocess.run(["openssl", "pkeyutl", "-verify", "-rawin", "-pubin", "-inkey", pem,
                        "-sigfile", signature, "-in", directory / FILES[0]],
                       check=True, stdout=subprocess.DEVNULL)
        (directory / FILES[1]).write_text(base64.b64encode(signature.read_bytes()).decode())


def head(url):
    """The host's current main commit, which its `notifications` tag is created at."""
    output = subprocess.check_output(["git", "ls-remote", url, "refs/heads/main"], text=True)
    return output.split()[0]


def upload(directory, repo):
    notes = ("The signed announcement feed GPROXY reads. It is rebuilt from the "
             f"Announcements discussions of https://github.com/{repo}; these files "
             "are not a release.")
    hosts = [
        (publish.Github, f"https://github.com/{repo}.git"),
        (publish.Cnb, "https://cnb.cool/LeenHawk/gproxy.git"),
        (publish.Gitlab, "https://gitlab.com/leenhawk1/gproxy.git"),
    ]
    for kind, url in hosts:
        # A new tag can only point at a commit the host already has, and the
        # mirrors trail GitHub; the commit is irrelevant to the files.
        os.environ["CI_COMMIT_SHA"] = head(url)
        host = kind()
        release = host.release(TAG, notes, True)
        for name in FILES:
            host.upload(release, directory / name)
        host.finish(release, notes, True)
        host.prune(release, set(FILES))
        print(f"{host.name}: published {TAG}", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True, help="directory for the signed feed")
    parser.add_argument("--docs", type=Path, help="gproxy-docs checkout to copy the feed into")
    parser.add_argument("--dry-run", action="store_true", help="build and sign only")
    args = parser.parse_args()
    repo = os.environ["GITHUB_REPOSITORY"]
    feed = build(repo)
    args.out.mkdir(parents=True, exist_ok=True)
    sign(feed, args.out)
    print(feed)
    if args.docs:
        (args.docs / "public" / FILES[0]).write_text(feed)
    if not args.dry_run:
        upload(args.out, repo)


if __name__ == "__main__":
    main()
