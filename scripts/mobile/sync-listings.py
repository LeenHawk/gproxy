#!/usr/bin/env python3
"""Generate shared Android listing text and website privacy pages from reviewed sources."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]


def markdown(body):
    """Plain-text policy to Markdown: after the opening date and publisher
    block, a block whose first line is a short title
    becomes a heading, and every other line its own paragraph, because Markdown
    joins lines that are separated by a single newline."""
    out = []
    for index, block in enumerate(body.strip().split("\n\n")):
        lines = [line.strip() for line in block.splitlines() if line.strip()]
        if index and len(lines) > 1 and len(lines[0]) < 60 and lines[0][-1] not in ".。:：":
            out.append("## " + lines.pop(0))
        out.extend(lines)
    return "\n\n".join(out) + "\n"

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
    policy = (root / "distribution/mobile/privacy" / f"privacy-{'ru' if locale == 'ru-RU' else locale}.txt").read_text()
    title, body = policy.split("\n", 1)
    page = root / "docs" / {"en-US": "legal/privacy.mdx", "zh-CN": "zh-cn/legal/privacy.mdx", "ru-RU": "ru/legal/privacy.mdx"}[locale]
    page.parent.mkdir(parents=True, exist_ok=True)
    description = "Конфиденциальность в мобильном приложении GPROXY" if locale == "ru-RU" else "GPROXY mobile application privacy information"
    page.write_text(f"---\ntitle: {json.dumps(title, ensure_ascii=False)}\ndescription: {json.dumps(description, ensure_ascii=False)}\n---\n\n{markdown(body)}")
print("Generated Android listing text and localized privacy pages")
