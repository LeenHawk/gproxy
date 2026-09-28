#!/usr/bin/env python3
"""Generate CNB jobs from the same target matrix used by GitHub releases."""
import json
from pathlib import Path

pipelines = []
secrets = ["https://cnb.cool/LeenHawk/gproxy-secrets/-/blob/main/release.json"]
for row in json.loads(Path("scripts/release-targets.json").read_text())["include"]:
    target, os_name = row["target"], row["os"]
    image = {"windows": ".cnb/Dockerfile.cross", "macos": ".cnb/Dockerfile.cross",
             "android": ".cnb/Dockerfile.android"}.get(os_name, ".cnb/Dockerfile")
    stages = [
        {"name": "Download Console bundle", "script": "bash scripts/cnb/package.sh prepare"},
        {"name": "Build and package CLI / server", "timeout": "90m", "script": "bash scripts/cnb/package.sh cli"},
    ]
    if os_name == "android":
        stages[-1]["imports"] = secrets
    if row.get("application_artifact"):
        stages.append({"name": "Build and package Application", "timeout": "90m", "script": "bash scripts/cnb/package.sh application"})
        if os_name == "android":
            stages[-1]["imports"] = secrets
    stages.append({"name": "Upload verified packages", "script": "bash scripts/cnb/package.sh upload"})
    pipeline = {
        "name": f"Packages ({target})",
        "runner": {"tags": "cnb:arch:arm64:v8" if os_name == "linux" and target.startswith("aarch64") else "cnb:arch:amd64", "cpus": 16},
        "docker": {"build": image, "volumes": ["/usr/local/cargo/registry:copy-on-write", "/usr/local/cargo/git:copy-on-write", "/root/.cache:copy-on-write", "/root/.gradle:copy-on-write", "target:copy-on-write"]},
        "env": {"TARGET_TRIPLE": target, "TARGET_OS": os_name, "ARTIFACT_NAME": row["artifact"],
                "APPLICATION_ARTIFACT": row.get("application_artifact", ""), "BUILDER": row["builder"],
                "UPX_ENABLED": str(row["upx"]).lower(), "UPX_VERSION": row.get("upx_version", "5.2.1"),
                "NDK_TARGET": row.get("ndk_target", "")},
        "stages": stages,
    }
    if row["builder"] == "cargo-alpine":
        pipeline["services"] = ["docker"]
    pipelines.append(pipeline)
pipelines.append({
    "name": "Edge / Cloudflare", "runner": {"tags": "cnb:arch:amd64", "cpus": 16},
    "docker": {"build": ".cnb/Dockerfile", "volumes": ["/usr/local/cargo/registry:copy-on-write", "target:copy-on-write"]},
    "env": {"TARGET_TRIPLE": "wasm32-unknown-unknown", "ARTIFACT_NAME": "gproxy-edge", "BUILDER": "worker-build"},
    "stages": [{"name": "Build and verify Edge bundle", "timeout": "90m", "script": "bash scripts/cnb/package.sh edge"}],
})
print(json.dumps({"**": {"api_trigger_release": pipelines}}, indent=2))
