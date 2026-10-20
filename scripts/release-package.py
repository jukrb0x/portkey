#!/usr/bin/env python3
"""Validate release versions and package already-built native applications."""
import argparse
import hashlib
import os
import pathlib
import plistlib
import re
import subprocess
import sys
import tempfile
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent


def version():
    cargo = (ROOT / 'Cargo.toml').read_text()
    value = re.search(r'^version = "([^"]+)"', cargo, re.M).group(1)
    if value != '1.0.3':
        raise ValueError('This release preparation supports only 1.0.3')
    info = plistlib.loads((ROOT / 'Resources/Rust-Info.plist').read_bytes())
    if (info['CFBundleShortVersionString'], info['CFBundleVersion'], info['PortKeyReleaseVersion']) != ('1.0.3', '5', value):
        raise ValueError('macOS version metadata does not match Cargo')
    lock = (ROOT / 'Cargo.lock').read_text()
    if f'name = "portkey"\nversion = "{value}"' not in lock:
        raise ValueError('Cargo.lock version does not match Cargo')
    rc = (ROOT / 'native/windows/PortKey.rc').read_text()
    if f'VALUE "ProductVersion", "{value}\\0"' not in rc:
        raise ValueError('Windows version metadata does not match Cargo')
    return value


def validate_windows(source, release):
    if sys.platform != 'win32':
        raise ValueError('Windows package validation requires PowerShell on Windows')
    environment = dict(os.environ, PORTKEY_PACKAGE=str(source.resolve()))
    actual = subprocess.run(
        ['pwsh', '-NoProfile', '-NonInteractive', '-Command',
         "(Get-Item -LiteralPath (Join-Path $env:PORTKEY_PACKAGE 'PortKey.exe') -ErrorAction Stop).VersionInfo.ProductVersion"],
        env=environment, check=True, capture_output=True, text=True,
    ).stdout.strip()
    if actual != release:
        raise ValueError(f'Windows executable ProductVersion {actual!r} does not match {release!r}')
    subprocess.run(['pwsh', '-NoProfile', '-NonInteractive', '-File',
                    str(ROOT / 'scripts/test-windows-package.ps1'),
                    '-BinaryDirectory', str(source.resolve())], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag')
    parser.add_argument('--platform', choices=['macos-arm64', 'windows-x64'])
    parser.add_argument('--source', type=pathlib.Path, help='Optional staged .app or Windows package directory')
    args = parser.parse_args()
    if args.source and not args.platform:
        parser.error('--source requires --platform')
    release = version()
    if args.tag is not None and args.tag != f'v{release}':
        parser.error(f'tag must exactly equal v{release}')
    if not args.platform:
        print(release)
        return
    destination = ROOT / 'dist/release'
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / f'PortKey-{release}-{args.platform}.zip'
    if args.platform == 'macos-arm64':
        if sys.platform != 'darwin':
            parser.error('macOS packaging requires ditto on macOS')
        source = args.source or ROOT / 'dist/PortKey.app'
        bundle = plistlib.loads((source / 'Contents/Info.plist').read_bytes())
        if bundle.get('PortKeyReleaseVersion') != release:
            parser.error('Rebuild the macOS bundle with matching release metadata first')
        subprocess.run(['lipo', str(source / 'Contents/MacOS/PortKey'), '-verify_arch', 'arm64'], check=True)
        with tempfile.TemporaryDirectory(prefix='portkey-release-') as temporary:
            staged = pathlib.Path(temporary) / 'PortKey.app'
            subprocess.run(['ditto', str(source), str(staged)], check=True)
            subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(staged), str(archive)], check=True)
    else:
        source = args.source or ROOT / 'dist/PortKey-Windows'
        if not (source / 'PortKey.exe').is_file():
            parser.error('Build the native Windows package first')
        validate_windows(source, release)
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as output:
            for item in sorted(source.rglob('*')):
                if item.is_file():
                    output.write(item, pathlib.Path('PortKey-Windows') / item.relative_to(source))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix('.zip.sha256').write_text(f'{digest}  {archive.name}\n')
    print(archive)


if __name__ == '__main__':
    main()
