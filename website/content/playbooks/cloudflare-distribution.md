# Binary distribution on Cloudflare (R2 + Pages)

This is how the public release artifacts are hosted and served at
`https://lessdb.dev/dl/<file>` — Cloudflare is used for **distribution
only**; the database itself runs wherever you run it.

## Layout

| piece | what |
|---|---|
| `lessdb-downloads` (R2 bucket) | the immutable release tarballs + SHA-256 files |
| Pages project `lessdb` | the website (lessdb.dev) |
| Pages Function `/dl/[[path]]` | serves bucket objects at `/dl/<file>` via an R2 binding — no public bucket access needed |
| `packages/npm` | npm wrapper whose postinstall fetches + verifies from `/dl` |
| `homebrew-lessdb` tap | formula fetching from `/dl` |

## Releasing a version

```sh
# 1. build on each platform (macOS arm64 and Linux x64 today)
bash scripts/package-release.sh 0.1.0          # -> dist/*.tar.gz + .sha256
# on the Linux box: same script, same version

# 2. upload (note: --remote, the default is a local simulation)
wrangler r2 object put lessdb-downloads/<file> --file dist/<file> --remote

# 3. update packages/npm/platforms.json checksums, bump package.json
cd packages/npm && npm publish

# 4. bump the Homebrew formula (url + sha256 + version), push the tap
# 5. mirror on GitHub Releases:
gh release create v0.1.0 dist/*.tar.gz dist/*.tar.gz.sha256

# 6. regenerate + deploy the site (downloads manifest.json reads dist/)
python3 scripts/build-site.py
wrangler pages deploy website --project-name lessdb --branch main
```

`website/downloads/manifest.json` is generated from `dist/*.sha256`, so
the downloads page always lists what's actually in the bucket.

## Deploying the site

```sh
cd website
wrangler pages deploy . --project-name lessdb --branch main
```

See also the [manual Cloudflare tasks playbook](/playbooks/manual-cloudflare-tasks)
for the one-time account setup steps.
