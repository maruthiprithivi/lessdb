# lessdb — npm distribution of the LessDB CLI

Installs the prebuilt `lessdb` binary for your platform (downloaded from
lessdb.dev, SHA-256 verified) and exposes it as `lessdb` / `npx lessdb`.

This package is **hosted on Cloudflare, not registry.npmjs.org**:

```sh
npm install -g lessdb --registry https://lessdb.dev/npm/
lessdb sql "SELECT 1"
# or install the tarball directly:
npm install -g https://lessdb.dev/npm/lessdb/-/lessdb-0.2.0.tgz
# or without installing:
npx --registry https://lessdb.dev/npm/ lessdb --version
```

Set `LESSDB_DOWNLOAD_BASE` to mirror the binary artifacts (e.g. your own
CDN) and `LESSDB_VERSION` to pin a release instead of the package's own.
