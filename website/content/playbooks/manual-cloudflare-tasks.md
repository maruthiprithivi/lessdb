# Manual Cloudflare tasks (one-time setup checklist)

Everything the site and the release pipeline need from Cloudflare, with
exactly which steps are done and which need a human. Work top to bottom.

## ✅ Already done (2026-08-28)

- [x] `wrangler login` — authenticated as `team@lessdb.dev`
      (account `154cd854…`). Re-auth with `npx wrangler login` when the
      token expires.
- [x] Zone **lessdb.dev** exists in that account (DNS is managed by
      Cloudflare).
- [x] DNS: `lessdb.dev` → CNAME `lessdb.pages.dev` (proxied). Verified
      live: `https://lessdb.dev/` serves the site.
- [x] Pages project **lessdb** created; the site deploys with
      `wrangler pages deploy website --project-name lessdb --branch main`.
- [x] R2 bucket **lessdb-downloads** created; release tarballs uploaded
      (`wrangler r2 object put … --file … --remote`).
- [x] Pages Function `/dl/[[path]]` (R2 binding from `website/wrangler.toml`)
      serves the artifacts — no public bucket access needed.
- [x] Custom domain **lessdb.dev** attached to the Pages project.

## ☐ To do (human steps, in order)

### 1. ~~Publish the npm package~~ → Hosted on Cloudflare (done)

The npm wrapper is **not** published to registry.npmjs.org. It is packed
and served from this site: `scripts/package-npm.sh` writes
`dist/lessdb-<ver>.tgz` + the packument, they are uploaded to R2
(`npm/…` keys), and the Pages Function at `/npm/*` serves them. Users
install with:

```sh
npm install -g lessdb --registry https://lessdb.dev/npm/
# or: npm install -g https://lessdb.dev/npm/lessdb/-/lessdb-<ver>.tgz
```

No `npm login`, no `npm publish` — ever.

### 2. Create a Cloudflare API token for CI deploys

Dashboard → My Profile → API Tokens → Create Token → "Edit Cloudflare
Workers" template, scope it to the lessdb.dev zone + your account, then:

```sh
# locally, so the values live only in the repo:
# GitHub repo → Settings → Secrets and variables → Actions → New secret
CLOUDFLARE_API_TOKEN   = <the token>
CLOUDFLARE_ACCOUNT_ID  = <your-cloudflare-account-id>
```

CI deploys the site on every push that touches `website/` (workflow
`.github/workflows/deploy-website.yml`).

### 3. Verify the GitHub Release mirror

The `v0.1.0` release has the four tarballs attached
(`gh release view v0.1.0`). If the public download URLs 404 (the asset
CDN can lag a few minutes after creation), re-check in a while or
re-upload with `gh release upload v0.1.0 dist/*.tar.gz --clobber`.

### 4. (Optional) Second wrangler auth for other machines

```sh
npx wrangler login                 # browser OAuth, same account
```

### 5. (Optional) R2 API token for direct bucket scripting

Only needed for `wrangler r2` scripting outside the Pages Function.
Dashboard → R2 → Manage R2 API Tokens → Create (Object Read & Write on
`lessdb-downloads`). Store it with SOPS
(`sops secrets/production.enc.yaml`, see the
[secrets playbook](/playbooks/secrets-sops)).

## Deploy commands cheat-sheet

```sh
python3 scripts/build-site.py                              # regenerate site from content
cd website && wrangler pages deploy . --project-name lessdb --branch main
wrangler r2 object put lessdb-downloads/<f> --file dist/<f> --remote
```
