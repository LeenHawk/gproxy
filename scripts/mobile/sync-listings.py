#!/usr/bin/env python3
"""Generate shared Android listing text and website privacy pages from reviewed sources."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]
listing = json.loads((root / "distribution/mobile/listings.json").read_text())
for locale, text in listing["locales"].items():
    if len(text["shortDescription"]) >= 80 or text["shortDescription"].endswith("."):
        raise ValueError(f"Invalid F-Droid short description: {locale}")
    directory = root / "fastlane/metadata/android" / locale
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "title.txt").write_text(listing["name"] + "\n")
    (directory / "short_description.txt").write_text(text["shortDescription"] + "\n")
    description = text["description"] + "\n\n" + text["androidNote"] + "\n"
    if len(description) > 4000:
        raise ValueError(f"Android description exceeds 4000 characters: {locale}")
    (directory / "full_description.txt").write_text(description)
    (directory / "privacy_url.txt").write_text(text["privacyUrl"] + "\n")
    policy = (root / "distribution/mobile/privacy" / f"privacy-{locale}.txt").read_text()
    title, body = policy.split("\n", 1)
    page = root / "docs/src/content/docs" / ("zh-cn/legal/privacy.md" if locale == "zh-CN" else "legal/privacy.md")
    page.parent.mkdir(parents=True, exist_ok=True)
    page.write_text(f"---\ntitle: {title}\ndescription: GPROXY mobile application privacy information\n---\n\n{body.lstrip()}")
print("Generated Android listing text and English/Chinese privacy pages")
