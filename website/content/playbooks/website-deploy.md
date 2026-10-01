# Deploying this website (lessdb.dev)

The site itself is fully static — docs, playbooks, use cases, downloads
catalog, and the interactive architecture diagram. No build step, no
runtime: Cloudflare Pages serves the files straight from the repo's
`website/` directory.

## Build the content

Docs pages are generated from the repo's markdown so the site never
drifts from the source of truth:

```sh
python3 scripts/build-site.py     # docs/*.md + website/content/** -> website/
```

## Deploy with wrangler

```sh
cd website
wrangler pages deploy . --project-name lessdb --branch main
```

Custom domain (one-time, or via the dashboard → Pages → lessdb →
Custom domains):

```
POST /accounts/<ACCOUNT_ID>/pages/projects/lessdb/domains
  { "name": "lessdb.dev" }
```

Cloudflare provisions the DNS record automatically (the zone must be in
the same account). `www.lessdb.dev` is added the same way.

## Release binaries (downloads)

`scripts/package-release.sh` builds the release binaries (macOS arm64 +
Linux x86_64, base and `cloud` variants), tars them with checksums, and
uploads them to the R2 bucket `lessdb-downloads` (note: `wrangler r2
object put` needs `--remote` — the default is a local simulation):

```sh
wrangler r2 object put lessdb-downloads/<file> --file dist/<file> --remote
```

Artifacts are served at `https://lessdb.dev/dl/<file>` by the Pages
Function in `website/functions/dl/[[path]].ts` (an R2 binding declared in
`website/wrangler.toml`) — no public r2.dev access needed. The install
script on the site (`/install.sh`) picks the right archive for the
visitor's OS/arch and verifies the SHA-256; the downloads page renders
`website/downloads/manifest.json`.

## Distribution channels

The same artifacts feed every installer:

* **curl** — `/install.sh` (site) downloads from `/dl/<file>`, verifies SHA-256.
  The default version comes from `/downloads/latest.json` (never hardcoded);
  `LESSDB_VERSION=v0.2.0` pins one.
* **npm** — `packages/npm` is a wrapper package whose postinstall downloads
  the right binary from `/dl` and verifies it; `bin/lessdb.js` spawns it.
  The package is **hosted on Cloudflare, not registry.npmjs.org**:
  `scripts/package-npm.sh` packs the tarball + packument into `dist/`, they
  are uploaded to R2 keys `npm/<pkg>-<ver>.tgz` and `npm/lessdb`, and the
  Pages Function `website/functions/npm/[[path]].ts` serves them at
  `/npm/`. Users install with
  `npm install -g lessdb --registry https://lessdb.dev/npm/` (or the
  tarball URL directly). Never `npm publish`.
* **Homebrew** — tap `lessdb/homebrew-lessdb` (repo
  `lessdb.dev/downloads/`); the formula fetches from
  `/dl`. Bump the formula (url + sha256 + version) on each release.
* **GitHub Releases** — `gh release create v<ver> dist/*.tar.gz
  dist/*.tar.gz.sha256` attaches the same artifacts as a mirror.

## Versioning

The workspace `Cargo.toml` is the single source of truth (crates inherit
`version.workspace = true`; `lessdb --version` reads it via
`CARGO_PKG_VERSION`). Bump every release with
`scripts/bump-version.sh <x.y.z>` — it updates Cargo.toml + the npm
wrapper's package.json in lockstep. Tarball names, `manifest.json`,
`latest.json`, `platforms.json` and the packument are all generated from
that one number (`scripts/package-release.sh`, `scripts/gen-manifests.sh`,
`scripts/package-npm.sh`), so no 0.1.0 should ever be hardcoded again.

Release order: `bump-version.sh` → build on each platform
(`package-release.sh`) → `package-npm.sh` once → upload dist/ to R2 →
`gen-manifests.sh` → brew formula bump → GitHub release → deploy site.

## Iterating

* `wrangler pages dev .` — local preview with live reload.
* Commit docs changes → `scripts/build-site.py` → `wrangler pages deploy`
  → live in seconds. There is no queue, no CDN cache to purge (Pages
  serves fresh assets immediately).
