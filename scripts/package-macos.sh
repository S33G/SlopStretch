#!/bin/sh
# Build local artifacts. Does not install plugins into host scan folders.
set -eu
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != Darwin ]; then
  echo "This script packages a macOS app. Use cargo xtask bundle on other systems." >&2
  exit 1
fi

cargo build --release --locked -p plugin-standalone --no-default-features
cargo xtask bundle --release

python3 - <<'PY'
import json
import pathlib
import plistlib
import shutil
import subprocess

metadata = json.loads(subprocess.check_output(
    ['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1']))
target = pathlib.Path(metadata['target_directory'])
dist = pathlib.Path('dist')
dist.mkdir(exist_ok=True)
app = dist / 'SlopStretch.app'
contents = app / 'Contents'
(contents / 'MacOS').mkdir(parents=True, exist_ok=True)
(contents / 'Resources').mkdir(exist_ok=True)
shutil.copy2(target / 'release' / 'plugin-standalone', contents / 'MacOS' / 'SlopStretch')
with (contents / 'Info.plist').open('wb') as f:
    plistlib.dump({
        'CFBundleName': 'SlopStretch',
        'CFBundleDisplayName': 'SlopStretch',
        'CFBundleExecutable': 'SlopStretch',
        'CFBundleIdentifier': 'audio.slop.slopstretch.standalone',
        'CFBundleVersion': '0.1.0',
        'CFBundleShortVersionString': '0.1.0',
        'CFBundlePackageType': 'APPL',
        'NSHighResolutionCapable': True,
        'LSMinimumSystemVersion': '11.0',
    }, f)
for suffix in ('vst3', 'clap'):
    source = target / 'bundled' / f'SlopStretch.{suffix}'
    destination = dist / source.name
    if destination.exists():
        shutil.rmtree(destination) if destination.is_dir() else destination.unlink()
    shutil.copytree(source, destination) if source.is_dir() else shutil.copy2(source, destination)
print(app.resolve())
PY

# Local ad-hoc signing; no Developer ID signing or notarization is implied.
codesign --force --deep --sign - dist/SlopStretch.app
codesign --force --deep --sign - dist/SlopStretch.vst3
codesign --force --deep --sign - dist/SlopStretch.clap
echo "Local app and plugin bundles are in dist/."
