# Runtime font sources

Every font file in this directory tree is an SIL Open Font License 1.1 font,
subset to WOFF2. Its license is the `<Family>-OFL.txt` file beside it; keep the
two together when copying, renaming or redistributing a font. Web exports list
these files on their Open Source Licenses page.

Noto Sans SC (`NotoSansSC-VF.subset.woff2`, variable weight) supplies the
common Simplified Chinese glyph fallback after the template's body font. Its
hash and size are in `assets.lock.json` and `runtime-cjk.json`. It covers the
shared common repertoire, including player input, rather than only current UI
copy. Rare characters outside that repertoire remain unsupported.

Use `python3 tools/install_cjk_font.py --check` to verify the pinned runtime
font and its license. Loader fonts are separate lightweight subsets of the same
approved family. Runtime imports disable system fallback; web exports bundle the font.
