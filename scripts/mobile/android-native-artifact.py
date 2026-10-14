#!/usr/bin/env python3
"""Transfer a channel's native libraries and generated Android inputs between CI jobs."""
import argparse
import json
import os
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
PROJECT = ROOT / 'crates/gproxy-host-tauri/gen/android'
ABIS = {'aarch64': 'arm64-v8a', 'x86_64': 'x86_64'}
GENERATED = ['tauri.settings.gradle', 'app/tauri.build.gradle.kts',
             'app/tauri.properties', 'app/proguard-tauri.pro',
             'app/src/main/assets/tauri.conf.json']


def identity(distribution):
    config = json.loads((ROOT / 'crates/gproxy-host-tauri/tauri.conf.json').read_text())
    return {'distribution': distribution, 'commit': os.environ['GPROXY_BUILD_HASH'],
            'version': os.environ['GPROXY_BUILD_VERSION'], 'identifier': config['identifier']}


def export(distribution, arch, directory):
    destination = directory / distribution / arch
    destination.mkdir(parents=True)
    meta = identity(distribution)
    abi = ABIS[arch]
    native = PROJECT / 'app/src/main/jniLibs' / abi
    if not (native / 'libgproxy_host_tauri.so').is_file():
        raise ValueError(f'Missing {abi} application library')
    shutil.copytree(native, destination / 'jniLibs' / abi, symlinks=False)
    generated = 'app/src/main/java/' + meta['identifier'].replace('.', '/') + '/generated'
    for relative in [*GENERATED, generated]:
        source = PROJECT / relative
        if not source.exists():
            if relative in GENERATED[:3] or relative == generated:
                raise ValueError(f'Missing generated Android input: {relative}')
            continue
        target = destination / 'project' / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(source, target)
        else:
            shutil.copy2(source, target)
    apk = ROOT / f'dist/mobile/{distribution}/{arch}/gproxy-{distribution}-{arch}.apk'
    shutil.copy2(apk, destination / 'unsigned.apk')
    (destination / 'build.json').write_text(json.dumps({**meta, 'arch': arch}) + '\n')


def import_native(distribution, directory):
    expected = identity(distribution)
    builds = []
    for arch in ABIS:
        source = directory / distribution / arch
        actual = json.loads((source / 'build.json').read_text())
        if actual != {**expected, 'arch': arch}:
            raise ValueError(f'Wrong Android build input: {source}')
        if not (source / 'jniLibs' / ABIS[arch] / 'libgproxy_host_tauri.so').is_file():
            raise ValueError(f'Missing native ABI: {arch}')
        builds.append(source)
    first, second = builds
    common = lambda folder: {p.relative_to(folder): p.read_bytes() for p in folder.rglob('*') if p.is_file()}
    if common(first / 'project') != common(second / 'project'):
        raise ValueError('The two ABIs have different generated Android project inputs')
    native = PROJECT / 'app/src/main/jniLibs'
    shutil.rmtree(native, ignore_errors=True)
    native.mkdir(parents=True)
    for arch, source in zip(ABIS, builds):
        shutil.copytree(source / 'jniLibs' / ABIS[arch], native / ABIS[arch])
    shutil.copytree(first / 'project', PROJECT, dirs_exist_ok=True)
    print(f'Imported {distribution}: {", ".join(ABIS.values())}; Rust compilation is not needed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('export', 'import'))
    parser.add_argument('distribution', choices=('direct', 'fdroid', 'google-play'))
    parser.add_argument('directory', type=Path)
    parser.add_argument('--arch', choices=ABIS)
    args = parser.parse_args()
    if args.operation == 'export':
        if not args.arch:
            parser.error('export requires --arch')
        export(args.distribution, args.arch, args.directory)
    else:
        import_native(args.distribution, args.directory)
