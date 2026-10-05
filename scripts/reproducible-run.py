#!/usr/bin/env python3
"""Run a build with stable source identity and remapped compiler paths."""
import importlib.util
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("reproducible_env", ROOT / "scripts/reproducible-env.py")
config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config)


def build_environment(target=None):
    env = os.environ.copy()
    env.update(config.environment())
    mappings = [(ROOT, "/gproxy"),
                (Path(env.get("CARGO_HOME", Path.home() / ".cargo")).resolve(), "/cargo")]
    if env.get("CARGO_TARGET_DIR"):
        mappings.append((Path(env["CARGO_TARGET_DIR"]).resolve(), "/gproxy/target"))
    encoded = env.get("CARGO_ENCODED_RUSTFLAGS")
    flags = encoded.split("\x1f") if encoded else shlex.split(env.get("RUSTFLAGS", ""))
    for path, replacement in mappings:
        flags.append(f"--remap-path-prefix={path}={replacement}")
    target = target or env.get("TARGET_TRIPLE", "")
    if target.endswith("-windows-msvc"):
        flags.extend(["-C", "link-arg=/Brepro"])
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    if os.name != "nt":
        for name in ("CFLAGS", "CXXFLAGS", "CGO_CFLAGS", "CGO_CXXFLAGS"):
            arguments = shlex.split(env.get(name, ""))
            arguments.extend(f"-ffile-prefix-map={path}={replacement}" for path, replacement in mappings)
            env[name] = shlex.join(arguments)
    goflags = shlex.split(env.get("GOFLAGS", ""))
    goflags.extend(["-trimpath", "-buildvcs=false"])
    env["GOFLAGS"] = shlex.join(goflags)
    return env


if __name__ == "__main__":
    command = sys.argv[1:]
    if command[:1] == ["--"]:
        command.pop(0)
    if not command:
        sys.exit("usage: reproducible-run.py COMMAND [ARG ...]")
    command[0] = shutil.which(command[0]) or command[0]
    target = None
    if "--target" in command:
        target = command[command.index("--target") + 1]
    sys.exit(subprocess.call(command, env=build_environment(target)))
