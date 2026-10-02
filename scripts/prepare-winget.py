#!/usr/bin/env python3
"""Generate versioned WinGet manifests from verified stable release ZIPs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import urllib.request
import zipfile

import jsonschema
import yaml

REPO = 'LeenHawk/gproxy'
UPSTREAM = 'microsoft/winget-pkgs'
TEMPLATES = Path('.github/winget/manifests/l/LeenHawk/GPROXY')


def gh(*args):
    return json.loads(subprocess.check_output(['gh', *args], text=True))


def already_submitted(edition, version):
    path = f'manifests/l/LeenHawk/GPROXY/{edition}'
    result = subprocess.run(
        ['gh', 'api', f'repos/{UPSTREAM}/contents/{path}/{version}'],
        capture_output=True, text=True)
    if result.returncode == 0:
        return True
    if 'HTTP 404' not in result.stderr:
        raise RuntimeError(result.stderr)
    prs = gh('pr', 'list', '--repo', UPSTREAM, '--state', 'open', '--search',
             f'"LeenHawk.GPROXY.{edition}" "{version}" in:title',
             '--json', 'number')
    for pr in prs:
        files = gh('api', '--paginate', '--slurp',
                   f'repos/{UPSTREAM}/pulls/{pr["number"]}/files')
        if any(file['filename'].startswith(f'{path}/{version}/')
               for page in files for file in page):
            return True
    return False


def verify_zip(path, expected, executable):
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest().upper()
    if digest != expected.upper():
        raise ValueError(f'Checksum mismatch: {path.name}')
    with zipfile.ZipFile(path) as archive:
        if executable not in archive.namelist():
            raise ValueError(f'{path.name} does not contain {executable}')
    return digest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', required=True)
    parser.add_argument('--edition', choices=['CLI', 'Desktop'], required=True)
    parser.add_argument('--output', type=Path, default=Path('dist/winget'))
    parser.add_argument('--skip-existing', action='store_true')
    args = parser.parse_args()
    version, edition = args.version, args.edition
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        parser.error('Expected a stable version such as 4.0.0')
    if args.skip_existing and already_submitted(edition, version):
        print(f'{edition} {version} is already submitted; skipping.')
        return
    release = gh('release', 'view', f'v{version}', '--repo', REPO,
                 '--json', 'isDraft,isPrerelease,publishedAt,assets')
    if release['isDraft'] or release['isPrerelease']:
        raise ValueError('WinGet requires a published stable release')
    downloads = args.output / 'downloads' / edition
    downloads.mkdir(parents=True, exist_ok=True)
    prefix = 'gproxy-tauri' if edition == 'Desktop' else 'gproxy'
    names = [f'{prefix}-windows-{arch}.zip' for arch in ('x86_64', 'aarch64')]
    command = ['gh', 'release', 'download', f'v{version}', '--repo', REPO,
               '--dir', str(downloads), '--clobber']
    for name in ['SHA256SUMS', *names]:
        command.extend(['--pattern', name])
    subprocess.run(command, check=True)
    checksums = dict((name.lstrip('*'), digest) for digest, name in
                     (line.split(maxsplit=1) for line in
                      (downloads / 'SHA256SUMS').read_text().splitlines() if line.strip()))
    destination = args.output / 'manifests' / edition
    destination.mkdir(parents=True, exist_ok=True)
    for template in sorted((TEMPLATES / edition / '4.0.0').glob('*.yaml')):
        doc = yaml.safe_load(template.read_text())
        doc['PackageVersion'] = version
        if 'ReleaseNotesUrl' in doc:
            doc['ReleaseNotesUrl'] = f'https://github.com/{REPO}/releases/tag/v{version}'
        if doc['ManifestType'] == 'installer':
            doc['ReleaseDate'] = release['publishedAt'][:10]
            for installer in doc['Installers']:
                arch = {'x64': 'x86_64', 'arm64': 'aarch64'}[installer['Architecture']]
                name = f'{prefix}-windows-{arch}.zip'
                installer['InstallerUrl'] = f'https://github.com/{REPO}/releases/download/v{version}/{name}'
                installer['InstallerSha256'] = verify_zip(
                    downloads / name, checksums[name], doc['NestedInstallerFiles'][0]['RelativeFilePath'])
        schema_url = f'https://raw.githubusercontent.com/microsoft/winget-cli/master/schemas/JSON/manifests/v{doc["ManifestVersion"]}/manifest.{doc["ManifestType"]}.{doc["ManifestVersion"]}.json'
        with urllib.request.urlopen(schema_url, timeout=60) as response:
            schema = json.load(response)
        jsonschema.validate(doc, schema)
        (destination / template.name).write_text(yaml.safe_dump(doc, allow_unicode=True, sort_keys=False), encoding='utf-8')
    print(f'Validated {edition} {version}: {destination}')


if __name__ == '__main__':
    main()
