"""Package verified native standalone and VST3 builds for producers."""
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import zipfile

source, output, platform = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
output.mkdir(parents=True, exist_ok=True)
windows = platform == 'Windows-x64'
exe = 'SlopStretch.exe' if windows else 'SlopStretch'
original = source / 'target/release' / ('plugin-standalone.exe' if windows else 'plugin-standalone')
bundle = source / 'target/bundled/SlopStretch.vst3'
assert original.is_file() and bundle.is_dir()
commit = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
install = ('Run SlopStretch.exe. Copy SlopStretch.vst3 to C:\\Program Files\\Common Files\\VST3 and rescan your DAW. Windows x64. Unsigned binaries; Windows may show a reputation warning.' if windows else 'Run ./SlopStretch (chmod +x SlopStretch if your extractor drops permissions). Copy SlopStretch.vst3 to ~/.vst3/ and rescan your DAW. Built on Ubuntu 24.04 x64; requires glibc 2.39 or newer, ALSA, X11/XWayland and OpenGL. File dialogs use the desktop portal.')
checksums = []
with tempfile.TemporaryDirectory() as temp:
    for kind in ['Desktop', 'Standalone', 'VST3']:
        name = f'SlopStretch-0.1.0-{platform}-{kind}'
        root = pathlib.Path(temp) / name
        root.mkdir()
        if kind != 'VST3':
            shutil.copy2(original, root / exe)
        if kind != 'Standalone':
            shutil.copytree(bundle, root / bundle.name)
        (root / 'START-HERE.txt').write_text('SLOPSTRETCH — NOW YOUR OPERATING SYSTEM HAS FEELINGS\n\n' + install + '\n\nLoad a WAV, turn up SLOP, press Release the Slop. Bake Ambient Lore exports a WAV. Start with your speakers low.\n\nNative builds, tests, offline audio rendering and VST3 ABI smoke checks passed. No Windows/Linux DAW or interactive audio-device qualification yet.\n\nhttps://github.com/S33G/SlopStretch\nMade with <3 with AI\n', encoding='utf-8')
        licenses = root / 'Licenses'
        licenses.mkdir()
        for path in [source / 'LICENSE-MIT', source / 'LICENSE-APACHE', source / 'vendor/rvst/THIRD_PARTY_LICENSES.txt', source / 'vendor/rvst/THIRD_PARTY_NOTICES.md', *sorted((source / 'crates/plugin-ui/assets/fonts').glob('*OFL.txt'))]:
            shutil.copy2(path, licenses / path.name)
        hashes = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file()}
        (root / 'BUILD-INFO.json').write_text(json.dumps({'source_commit': commit, 'source_tag': 'v0.1.0', 'platform': platform, 'rust': subprocess.check_output(['rustc', '--version'], text=True).strip(), 'sha256': hashes}, indent=2))
        archive = output / (name + '.zip')
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as z:
            for p in sorted(root.rglob('*')):
                z.write(p, p.relative_to(root.parent))
        with zipfile.ZipFile(archive) as z:
            assert z.testzip() is None
            for relative, digest in hashes.items():
                assert hashlib.sha256(z.read(name + '/' + relative)).hexdigest() == digest
        checksums.append(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name)
(output / f'SHA256SUMS-{platform}.txt').write_text('\n'.join(checksums) + '\n')
print('\n'.join(checksums))
