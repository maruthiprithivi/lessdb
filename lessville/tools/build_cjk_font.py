#!/usr/bin/env python3
"""Reproduce the approved Noto Sans SC runtime WOFF2 from its pinned upstream font."""
import argparse
import hashlib
import io
import json
from pathlib import Path


def subset_codepoints(metadata, repertoire):
    wanted = set(repertoire)
    for first, last in metadata['supplementalRanges']:
        wanted.update(range(first, last + 1))
    return wanted | set(metadata['supplementalCodepoints'])


def build(source, manifest_path):
    import brotli
    import fontTools
    from fontTools import subset
    from fontTools.ttLib import TTFont

    metadata = json.loads(manifest_path.read_text())
    if fontTools.__version__ != metadata['fontTools'] or brotli.__version__ != metadata['brotli']:
        raise ValueError('Use the pinned game-runtime-font.requirements.txt versions for reproducible bytes')
    if hashlib.sha256(source.read_bytes()).hexdigest() != metadata['sourceSha256']:
        raise ValueError('The upstream font does not match its approved pin')
    corpus = manifest_path.with_name('game-runtime-font-codepoints.json')
    if hashlib.sha256(corpus.read_bytes()).hexdigest() != metadata['codepointManifestSha256']:
        raise ValueError('The fixed common-character repertoire changed; do not shrink it to UI text')
    repertoire = set(json.loads(corpus.read_text()))
    if len(repertoire) != metadata['unicodeCodepoints']:
        raise ValueError('Unexpected common-character repertoire size')
    required = repertoire - set(metadata['repertoireExceptions'])
    with TTFont(source, recalcTimestamp=False) as font:
        if required - set(font.getBestCmap()):
            raise ValueError('The pinned font must cover the fixed repertoire except its recorded exceptions')
        if font['name'].getDebugName(16) != metadata['derivedFamily'] or 'Open Font License' not in (font['name'].getDebugName(13) or ''):
            raise ValueError('Unexpected runtime font identity or license')
        if font['OS/2'].fsType != 0:
            raise ValueError('The runtime font must have unrestricted embedding')
        options = subset.Options()
        options.flavor = 'woff2'
        options.layout_features = ['*']
        options.name_IDs = ['*']
        options.notdef_outline = True
        options.glyph_names = False
        worker = subset.Subsetter(options)
        worker.populate(unicodes=subset_codepoints(metadata, repertoire))
        worker.subset(font)
        font['head'].modified = metadata['headModified']
        font.flavor = 'woff2'
        output = io.BytesIO()
        font.save(output)
    data = output.getvalue()
    with TTFont(io.BytesIO(data)) as actual:
        if required - set(actual.getBestCmap()):
            raise ValueError('Generated font lost required common characters')
    if len(data) != metadata['size'] or hashlib.sha256(data).hexdigest() != metadata['sha256']:
        raise ValueError('Generated font differs from approved deterministic bytes')
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True, help='Pinned upstream NotoSansSC[wght].ttf')
    parser.add_argument('--output', type=Path, required=True, help='Approved complete common-character WOFF2 output')
    parser.add_argument('--manifest', type=Path, default=Path(__file__).with_name('game-runtime-font.json'))
    parser.add_argument('--check', action='store_true', help='Rebuild in memory and compare without writing')
    args = parser.parse_args()
    data = build(args.source, args.manifest)
    if args.check:
        if args.output.read_bytes() != data:
            raise ValueError('Runtime font output is stale')
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(data)
    print('Verified Noto Sans SC common Simplified Chinese font:', len(data), 'bytes')


if __name__ == '__main__':
    main()
