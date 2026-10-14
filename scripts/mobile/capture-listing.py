#!/usr/bin/env python3
"""Capture the current Console with isolated demonstration API data, not a device build.

Start Vite first. Requires Playwright, its FFmpeg helper and imageio-ffmpeg.
No real account, credential, model request or billing record is used.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
import time
import tomllib
from urllib.parse import parse_qs, urlparse

import imageio_ffmpeg
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
NOW = 1790928000000
CAPABILITIES = dict(refresh=False, quotaQuery=False, quotaReset=False, services=False, websocket=False)
LABELS = {
    "en": ("Demo user", "Primary gateway", "Backup gateway", "Local models", "Demo credential"),
    "zh-CN": ("演示用户", "研发网关", "备用线路", "本地模型", "演示凭据"),
    "zh-TW": ("示範使用者", "研發閘道", "備用線路", "本機模型", "示範憑證"),
}


def summary(requests):
    return dict(requests=requests, inputTokens=requests * 1800, outputTokens=requests * 240,
                cachedInputTokens=requests * 720, cacheCreationTokens=requests * 120,
                cacheCreation5mTokens=requests * 120, cacheCreation30mTokens=0,
                cacheCreation1hTokens=0, reasoningTokens=requests * 40, quantities={},
                cost=f"{requests * .003:.3f}", currency="USD", truncated=False, scanned=requests)


class Demonstration:
    def __init__(self, language, origin, revision):
        self.language, self.origin, self.revision = language, origin, revision
        self.unknown = []
        labels = LABELS[language]
        self.user = dict(id="demo-user", name=labels[0], role="admin", hasPassword=True)
        self.scope = dict(kind="instance", id=None, name=None, organizationId=None, selector="instance", current=True)
        self.providers = [dict(id=f"demo-provider-{i}", name=name, displayName=labels[i + 1],
                               channel="openai", baseUrl=base, connectionProfileId=None, proxy=None,
                               config={}, enabled=True, createdAtMs=NOW - 86400000)
                          for i, (name, base) in enumerate([
                              ("primary", "https://primary.example.invalid/v1"),
                              ("backup", "https://backup.example.invalid/v1"),
                              ("local", "http://127.0.0.1:11434/v1")])]
        self.credentials = [dict(id=f"demo-credential-{i}", providerId=p["id"], organizationId=None,
                                 teamId=None, userId=None, label=f"{labels[4]} {i + 1}", authKind="api_key",
                                 hasSecret=True, version=1, connectionProfileId=None, proxy=None, metadata={},
                                 expiresAtMs=None, status="active", statusReason=None, enabled=True)
                            for i, p in enumerate(self.providers)]
        self.routes = [dict(id=f"demo-route-{i}", name=name, strategy=strategy,
                            sessionAffinity=True, maxAttempts=3, enabled=True)
                       for i, (name, strategy) in enumerate([
                           ("assistant-default", "weighted"), ("coding-balanced", "round_robin"),
                           ("local-chat", "failover")])]
        self.members = [dict(id=f"demo-member-{i}", routeId="demo-route-0", providerId=p["id"],
                             upstreamModel="chat-model", tier=0 if i < 2 else 1,
                             weight=[70, 30, 100][i], enabled=True)
                        for i, p in enumerate(self.providers)]
        self.models = [dict(id=f"demo-model-{i}", providerId="demo-provider-0", upstreamName=name,
                            modelId=None, metadata={}, enabled=True, hasPrice=False)
                       for i, name in enumerate(["chat-model", "reasoning-model", "vision-model"])]

    def page(self, rows, query):
        for key in ("providerId", "routeId"):
            if key in query:
                rows = [r for r in rows if r.get(key) == query[key][0]]
        if "search" in query:
            needle = query["search"][0].lower()
            rows = [r for r in rows if needle in json.dumps(r, ensure_ascii=False).lower()]
        limit = int(query.get("pageSize", ["20"])[0])
        offset = (int(query.get("page", ["1"])[0]) - 1) * limit
        return dict(items=rows[offset:offset + limit], total=len(rows), offset=offset, limit=limit)

    def answer(self, request):
        url = urlparse(request.url)
        path, query = url.path, parse_qs(url.query)
        if path == "/info":
            return dict(instanceName="GPROXY", version=VERSION, hash=self.revision)
        if path == "/portal/api/context":
            return dict(user=self.user, organizations=[], teams=[], features=dict(
                canCreateKeys=True, canChangePassword=True, canSeeLogs=True, canSeeConsole=True))
        if path == "/admin/api/context":
            sections = ["providers", "credentials", "models", "provider-models", "routes", "route-members",
                        "connection-profiles", "rule-sets", "rules", "settings", "usage", "logs"]
            return dict(user=self.user, callerKind="session", scopeHeader="x-gproxy-admin-scope",
                        scope=self.scope, scopes=[self.scope], sections=[dict(
                            id=s, path=f"/admin/api/{s}", capabilities=["read", "write"]) for s in sections])
        if path in ("/portal/api/usage", "/admin/api/usage"):
            counts = [round(18 + i * .5 + 12 * (1 + math.sin(i * .8))) for i in range(28)]
            start = NOW - 604800000
            return dict(fromMs=start, toMs=NOW, summary=summary(sum(counts)), groups=[], trend=[dict(
                startMs=start + i * 21600000, endMs=start + (i + 1) * 21600000,
                summary=summary(count)) for i, count in enumerate(counts)])
        if path == "/portal/api/quota":
            return [dict(ownerKind="user", ownerId="demo-user", windowKey="daily", period="1d",
                         modelPattern=None, unit="USD", used="2.40", limit="10", usedPercent="24",
                         startsAtMs=NOW - 28800000, resetsAtMs=NOW + 57600000)]
        if path == "/admin/api/channels":
            return [dict(id="openai", displayName="OpenAI-compatible", loginModes=["api_key"],
                         capabilities=CAPABILITIES, configKeys=[])]
        if path == "/admin/api/credentials/providers":
            return [dict(id=p["id"], name=p["name"], displayName=p["displayName"], channel=p["channel"],
                         enabled=p["enabled"], loginModes=["api_key"], capabilities=CAPABILITIES)
                    for p in self.providers]
        families = {"providers": self.providers, "credentials": self.credentials, "routes": self.routes,
                    "route-members": self.members, "provider-models": self.models,
                    "connection-profiles": [], "price-rules": [], "rule-sets": [], "rules": []}
        for family, rows in families.items():
            prefix = f"/admin/api/{family}"
            if path == prefix:
                return self.page(rows, query)
            if path.startswith(prefix + "/"):
                row = next((r for r in rows if r["id"] == path[len(prefix) + 1:]), None)
                if row is not None:
                    if request.method == "PATCH":
                        row.update(request.post_data_json)
                    return row
        raise ValueError(f"Unimplemented demonstration API: {request.method} {path}")

    def intercept(self, route):
        url = urlparse(route.request.url)
        if url.netloc != self.origin:
            route.abort()
        elif url.path == "/info" or url.path.startswith(("/admin/api/", "/portal/api/")):
            try:
                route.fulfill(json=self.answer(route.request))
            except ValueError as error:
                self.unknown.append(str(error))
                route.fulfill(status=501, json={"error": {"code": "demo_missing", "message": str(error)}})
        else:
            route.continue_()


def settled(page):
    page.locator("main").wait_for()
    page.evaluate("document.fonts.ready")
    page.wait_for_timeout(900)


def label(language, key):
    translations = {}
    for domain in ("common", "navigation", "pages"):
        translations.update(json.loads((ROOT / f"console/src/locales/{language}/{domain}.json").read_text()))
    value = translations
    for part in key.split("."):
        value = value[part]
    return value


def timestamp(seconds):
    milliseconds = round(seconds * 1000)
    hours, milliseconds = divmod(milliseconds, 3600000)
    minutes, milliseconds = divmod(milliseconds, 60000)
    seconds, milliseconds = divmod(milliseconds, 1000)
    return f"{hours:02}:{minutes:02}:{seconds:02},{milliseconds:03}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:5186")
    parser.add_argument("--output", type=Path, default=ROOT / "dist/mobile/listing-materials")
    parser.add_argument("--languages", nargs="+", default=list(LABELS))
    parser.add_argument("--video", action="store_true")
    args = parser.parse_args()
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    args.output.mkdir(parents=True, exist_ok=True)
    records = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path="/usr/bin/google-chrome", headless=True,
                                             args=["--no-sandbox"])
        for language in args.languages:
            for layout, viewport, scale in [("phone-layout", dict(width=432, height=768), 2.5),
                                             ("desktop", dict(width=1920, height=1080), 1)]:
                folder = args.output / language / layout
                folder.mkdir(parents=True, exist_ok=True)
                video = args.video and layout == "desktop" and language in ("en", "zh-CN")
                options = dict(viewport=viewport, device_scale_factor=scale, locale=language,
                               timezone_id="Asia/Shanghai", color_scheme="light", reduced_motion="reduce")
                if video:
                    options.update(record_video_dir=str(folder), record_video_size=viewport)
                context = browser.new_context(**options)
                context.add_init_script("localStorage.setItem('gproxy-console-lang', " + json.dumps(language) +
                                        "); localStorage.setItem('gproxy-console-theme', 'light');")
                demo = Demonstration(language, urlparse(args.url).netloc, revision)
                context.route("**/*", demo.intercept)
                page = context.new_page()
                started = time.monotonic()
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
                scenes = [("01-overview", "/"), ("02-providers", "/providers/demo-provider-0/credentials"),
                          ("03-model-routing", "/model-routes"), ("04-models", "/providers/demo-provider-0/models")]
                for name, path in scenes:
                    page.goto(args.url + "/console" + path, wait_until="networkidle")
                    settled(page)
                    if errors or demo.unknown:
                        raise RuntimeError(json.dumps(dict(errors=errors, api=demo.unknown)))
                    page.screenshot(path=str(folder / (name + ".png")), animations="disabled")
                if video:
                    walkthrough_offset = time.monotonic() - started
                    walkthrough_start = time.monotonic()
                    captions = []
                    def mark(english, chinese):
                        captions.append((time.monotonic() - walkthrough_start,
                                         chinese if language == "zh-CN" else english))
                    mark("GPROXY Console walkthrough - demonstration data, not a device recording",
                         "GPROXY 控制台功能演示 · 使用演示数据 · 非设备录像")
                    page.goto(args.url + "/console/", wait_until="networkidle")
                    settled(page)
                    page.wait_for_timeout(4000)
                    mark("Manage providers and credentials you configure",
                         "统一管理自行配置的供应商和凭据")
                    page.get_by_role("link", name=label(language, "nav.providers"), exact=True).click()
                    settled(page)
                    page.get_by_role("link", name=LABELS[language][1], exact=False).first.click()
                    settled(page)
                    page.wait_for_timeout(5000)
                    mark("Manage the upstream models offered by each provider",
                         "管理各供应商提供的上游模型")
                    page.get_by_role("tab", name=label(language, "nav.provider-models"), exact=True).click()
                    settled(page)
                    page.wait_for_timeout(5000)
                    mark("Configure routing targets, relative weights and fallback tiers",
                         "配置模型路由目标、相对权重和故障切换层级")
                    page.goto(args.url + "/console/model-routes", wait_until="networkidle")
                    settled(page)
                    page.get_by_role("button", name=label(language, "management.manage"), exact=True).first.click()
                    page.get_by_role("dialog").wait_for()
                    page.wait_for_timeout(6000)
                    page.keyboard.press("Escape")
                    mark("Open-source gateway; provider accounts and subscriptions are separate",
                         "开源网关 · 供应商账号、模型订阅和费用由用户自行配置")
                    page.goto(args.url + "/console/about", wait_until="networkidle")
                    settled(page)
                    page.wait_for_timeout(5000)
                if errors or demo.unknown:
                    raise RuntimeError(json.dumps(dict(errors=errors, api=demo.unknown)))
                video_handle = page.video
                records.append(dict(language=language, layout=layout, viewport=viewport,
                                    screenshots=[f"{name}.png" for name, _ in scenes], errors=errors))
                context.close()
                if video:
                    raw = Path(video_handle.path())
                    final = folder / "console-walkthrough.mp4"
                    end = time.monotonic() - walkthrough_start
                    subtitles = folder / "console-walkthrough.srt"
                    blocks = []
                    for index, (start, caption) in enumerate(captions):
                        stop = captions[index + 1][0] if index + 1 < len(captions) else end
                        blocks.append(f"{index + 1}\n{timestamp(start)} --> {timestamp(stop)}\n{caption}\n")
                    subtitles.write_text("\n".join(blocks), encoding="utf-8")
                    subtitle_filter = (f"subtitles=filename='{subtitles.resolve()}':"
                                       "force_style='FontName=Noto Sans CJK SC,FontSize=18,Outline=1,Shadow=0,MarginV=12'")
                    subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(), "-y", "-ss", str(walkthrough_offset),
                                    "-i", str(raw), "-an", "-vf", subtitle_filter,
                                    "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", "30",
                                    "-movflags", "+faststart", str(final)], check=True,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    raw.unlink()
                print(language, layout, "captured", flush=True)
        browser.close()
    (args.output / "capture-manifest.json").write_text(json.dumps(dict(
        sourceCommit=revision, capturedAt=int(time.time()), renderer="Chrome / current Console",
        data="isolated demonstration API fixtures; no real accounts or usage",
        deviceCapture=False, records=records), ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
