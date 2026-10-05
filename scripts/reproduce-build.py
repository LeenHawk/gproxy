#!/usr/bin/env python3
"""Build twice from clean checkouts at fixed paths, as recommended by F-Droid."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def remove_tree(path):
    # Go module downloads use read-only directories even for their owner.
    # This tree was created by this invocation; never follow its symlinks.
    for directory, _, _ in os.walk(path, followlinks=False):
        mode = os.stat(directory, follow_symlinks=False).st_mode
        os.chmod(directory, stat.S_IMODE(mode) | stat.S_IRWXU)
    def readonly(function, name, error):
        if os.name != "nt" or not isinstance(error, PermissionError):
            raise error
        os.chmod(name, stat.S_IREAD | stat.S_IWRITE)
        function(name)
    shutil.rmtree(path, onexc=readonly)


def hashes(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            with path.open("rb") as stream:
                result[path.relative_to(directory).as_posix()] = hashlib.file_digest(stream, "sha256").hexdigest()
    return result


def reproduce(command, output, result, canonical):
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    epoch = subprocess.check_output(["git", "show", "-s", "--format=%ct", "HEAD"], cwd=ROOT, text=True).strip()
    result.mkdir(parents=True, exist_ok=True)
    # Do not reuse or remove a pre-existing directory in shared /tmp.
    canonical.mkdir(mode=0o700)
    work = canonical / "source"
    env = os.environ.copy()
    env.update(REPRO_SOURCE_ROOT=str(ROOT), SOURCE_DATE_EPOCH=epoch, GPROXY_BUILD_HASH=commit,
               CARGO_HOME=str(canonical / "cargo"), CARGO_TARGET_DIR=str(work / "target"),
               GRADLE_USER_HOME=str(work / ".gradle-home"), GOCACHE=str(work / ".go-cache"),
               CARGO_INCREMENTAL="0", SCCACHE_DISABLE="1", GPROXY_UNSIGNED_BUILD="1")
    env.pop("RUSTC_WRAPPER", None)
    for name, folder in (("ANDROID_HOME", "android-sdk"), ("ANDROID_NDK_HOME", "android-ndk")):
        if env.get(name):
            link = canonical / folder
            link.symlink_to(Path(env[name]).resolve(), target_is_directory=True)
            env[name] = str(link)
    if env.get("ANDROID_HOME"):
        env["ANDROID_SDK_ROOT"] = env["ANDROID_HOME"]
    if env.get("ANDROID_NDK_HOME"):
        env["ANDROID_NDK_ROOT"] = env["NDK_HOME"] = env["ANDROID_NDK_HOME"]
    report = {"commit": commit, "source_date_epoch": int(epoch), "working_directory": str(work),
              "cargo_home": env["CARGO_HOME"], "cargo_target_dir": env["CARGO_TARGET_DIR"],
              "command": command, "passed": False}
    try:
        attempts = []
        for attempt in ("first", "second"):
            subprocess.run(["git", "clone", "--quiet", "--no-hardlinks", "--shared", "--no-checkout", str(ROOT), str(work)], check=True)
            subprocess.run(["git", "checkout", "--quiet", "--detach", commit], cwd=work, check=True)
            print(f"{attempt}: {commit} at {work}", flush=True)
            subprocess.run(command, cwd=work, env=env, check=True)
            source = work / output
            if not source.is_dir():
                raise ValueError(f"Build did not produce {output}")
            destination = result / attempt
            shutil.copytree(source, destination)
            manifest = hashes(destination)
            if not manifest:
                raise ValueError("Cannot verify an empty output directory")
            attempts.append(manifest)
            remove_tree(work)
        first, second = attempts
        report["files"] = [{"name": name, "first": first.get(name), "second": second.get(name),
                            "equal": first.get(name) == second.get(name)}
                           for name in sorted(first.keys() | second.keys())]
        report["passed"] = first == second
        for entry in report["files"]:
            print(f"{'equal' if entry['equal'] else 'DIFFERENT'}: {entry['name']}")
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (result / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        remove_tree(canonical)
    return report["passed"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", default="dist/release")
    parser.add_argument("--result", type=Path, required=True)
    parser.add_argument("--canonical", type=Path, default=Path("C:/gproxy-reproduce" if os.name == "nt" else "/tmp/gproxy-reproduce"))
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a build command is required after --")
    sys.exit(0 if reproduce(command, args.output, args.result.resolve(), args.canonical) else 1)
