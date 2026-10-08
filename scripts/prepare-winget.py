#!/usr/bin/env python3
"""Generate versioned WinGet manifests from verified signed release MSIX packages."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request
import zipfile
import xml.etree.ElementTree as ET

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
        ['gh', 'api', f'repos/{UPSTREAM}/contents/{path}/{version}/LeenHawk.GPROXY.{edition}.installer.yaml'],
        capture_output=True, text=True)
    if result.returncode == 0:
        doc = yaml.safe_load(base64.b64decode(json.loads(result.stdout)['content']))
        if all(entry.get('InstallerType', doc.get('InstallerType')) == 'msix'
               for entry in doc['Installers']):
            return True
    elif 'HTTP 404' not in result.stderr:
        raise RuntimeError(result.stderr)
    return has_open_pr(edition, version)


def has_open_pr(edition, version):
    path = f'manifests/l/LeenHawk/GPROXY/{edition}'
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


def package_family(identity):
    digest = hashlib.sha256(identity['Publisher'].encode('utf-16le')).digest()[:8]
    alphabet = str.maketrans('ABCDEFGHIJKLMNOPQRSTUVWXYZ234567', '0123456789abcdefghjkmnpqrstvwxyz')
    publisher = base64.b32encode(digest).decode().rstrip('=').translate(alphabet)
    return identity['Name'] + '_' + publisher


def msix_metadata(path):
    with zipfile.ZipFile(path) as archive:
        if 'AppxSignature.p7x' not in archive.namelist():
            return None
        manifest = ET.fromstring(archive.read('AppxManifest.xml'))
    # Require a trusted timestamped package signature, whether the release
    # was signed by SignPath or Microsoft Store.
    subprocess.run(['pwsh', '-NoProfile', '-Command',
        "$s = Get-AuthenticodeSignature -LiteralPath $env:WINGET_MSIX_PATH; "
        "if ($s.Status -ne 'Valid' -or -not $s.TimeStamperCertificate) "
        "{ throw 'MSIX requires a trusted timestamped signature' }"],
        env={**os.environ, 'WINGET_MSIX_PATH': str(path.resolve())}, check=True)
    ns = {'m': 'http://schemas.microsoft.com/appx/manifest/foundation/windows10'}
    identity = manifest.find('m:Identity', ns).attrib
    minimum = manifest.find('m:Dependencies/m:TargetDeviceFamily', ns).attrib['MinVersion']
    return identity, minimum


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
                 '--json', 'isDraft,isPrerelease,publishedAt')
    if release['isDraft'] or release['isPrerelease']:
        raise ValueError('WinGet requires a published stable release')
    downloads = args.output / 'downloads' / edition
    downloads.mkdir(parents=True, exist_ok=True)
    prefix = 'gproxy-tauri' if edition == 'Desktop' else 'gproxy'
    names = [f'{prefix}-windows-{arch}.msix' for arch in ('x86_64', 'aarch64')]
    command = ['gh', 'release', 'download', f'v{version}', '--repo', REPO,
               '--dir', str(downloads), '--clobber']
    for name in ['SHA256SUMS', *names]:
        command.extend(['--pattern', name])
    subprocess.run(command, check=True)
    checksums = dict((name.lstrip('*'), digest) for digest, name in
                     (line.split(maxsplit=1) for line in
                      (downloads / 'SHA256SUMS').read_text().splitlines() if line.strip()))
    packages = {}
    for name in names:
        digest = verify_zip(downloads / name, checksums[name], 'AppxManifest.xml')
        metadata = msix_metadata(downloads / name)
        if metadata is None:
            print(f'{edition} {version}: unsigned MSIX; skipping WinGet submission.')
            return
        identity, minimum = metadata
        arch = 'x64' if name.endswith('-x86_64.msix') else 'arm64'
        expected_name = 'LeenHawk.GPROXYGateway' if edition == 'Desktop' else 'LeenHawk.GPROXYCLI'
        if (identity['Name'] != expected_name or identity['Version'] != version + '.0'
                or identity['ProcessorArchitecture'] != arch):
            raise ValueError(f'MSIX identity/version/architecture mismatch: {name}')
        packages[arch] = dict(Architecture=arch, InstallerUrl=f'https://github.com/{REPO}/releases/download/v{version}/{name}',
                              InstallerSha256=digest, PackageFamilyName=package_family(identity), MinimumOSVersion=minimum)
    destination = args.output / 'manifests' / edition
    destination.mkdir(parents=True, exist_ok=True)
    for template in sorted((TEMPLATES / edition / '4.0.0').glob('*.yaml')):
        doc = yaml.safe_load(template.read_text(encoding='utf-8'))
        doc['PackageVersion'] = version
        if 'ReleaseNotesUrl' in doc:
            doc['ReleaseNotesUrl'] = f'https://github.com/{REPO}/releases/tag/v{version}'
        if doc['ManifestType'] == 'installer':
            doc['ReleaseDate'] = release['publishedAt'][:10]
            doc['InstallerType'] = 'msix'
            doc['UpgradeBehavior'] = 'install'
            doc.pop('NestedInstallerType', None)
            doc.pop('NestedInstallerFiles', None)
            doc['Installers'] = list(packages.values())
        schema_url = f'https://raw.githubusercontent.com/microsoft/winget-cli/master/schemas/JSON/manifests/v{doc["ManifestVersion"]}/manifest.{doc["ManifestType"]}.{doc["ManifestVersion"]}.json'
        with urllib.request.urlopen(schema_url, timeout=60) as response:
            schema = json.load(response)
        jsonschema.validate(doc, schema)
        (destination / template.name).write_text(yaml.safe_dump(doc, allow_unicode=True, sort_keys=False), encoding='utf-8')
    print(f'Validated {edition} {version}: {destination}')


if __name__ == '__main__':
    main()
