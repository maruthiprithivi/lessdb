#!/usr/bin/env python3
"""Install the pinned bundled Noto Sans SC common Simplified Chinese face and its OFL notice."""
import argparse
import hashlib
import os
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FONT_PATH = Path('assets/template/fonts/NotoSansSC-VF.subset.woff2')
FONT_SHA256 = '38f4c5f927f746b3dfa4c3a5d7edf0b7a3e828e27438418fa54f9e1565289412'
FONT_SIZE = 1869368
LICENSE_NAME = 'NotoSansSC-OFL.txt'


def verify_font(path):
    data = Path(path).read_bytes()
    if len(data) != FONT_SIZE or hashlib.sha256(data).hexdigest() != FONT_SHA256:
        raise ValueError('The bundled runtime CJK requires the pinned common Noto Sans SC font; UI-only subsets and alternate bytes are rejected: ' + str(path))
    return data


def verify_license(path):
    # SIL OFL 1.1 requires the license to accompany redistributed font bytes.
    if not Path(path).is_file() or 'SIL Open Font License' not in Path(path).read_text(encoding='utf-8', errors='replace'):
        raise ValueError('Keep the SIL Open Font License beside the bundled runtime font: ' + str(path))


def atomic_write(target, data):
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(data)
        temporary.chmod(0o644)
        os.replace(temporary, target)
    finally:
        if temporary is not None and temporary.exists():
            temporary.unlink()


def install_font(root=ROOT, source=None, check=False):
    root = Path(root).resolve()
    target = root / FONT_PATH
    license_target = target.with_name(LICENSE_NAME)
    if check or source is None:
        verify_font(target)
    else:
        data = verify_font(source)  # Validate before replacing the working font.
        source_license = Path(source).with_name(LICENSE_NAME)
        if not license_target.exists() and source_license.is_file():
            verify_license(source_license)
            atomic_write(license_target, source_license.read_bytes())
        atomic_write(target, data)
        verify_font(target)
    verify_license(license_target)
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--project-root', type=Path, default=ROOT)
    parser.add_argument('--source', type=Path, help='Existing verified NotoSansSC-VF.subset.woff2 font (its NotoSansSC-OFL.txt sibling is copied when missing)')
    parser.add_argument('--check', action='store_true', help='Verify the hydrated runtime font and license without writing')
    args = parser.parse_args()
    target = install_font(args.project_root, args.source, args.check)
    print('Verified common Simplified Chinese font:', target)


if __name__ == '__main__':
    main()
