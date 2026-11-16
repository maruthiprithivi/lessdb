# Lessville Cloudflare runtime

`ville.lessdb.dev` serves the Godot web client through this Worker and runs the
authoritative Rust simulation in one `LessvilleAgent` container. R2 checkpoints
use the existing `lessville/v3/` prefix. The parent repository's Cloudflare Pages
website is a separate deployment.

The current configuration runs one `basic` instance (1/4 vCPU, 1 GiB memory,
4 GB disk). The previous `lite` deployment repeatedly timed out at eight seconds
while using 80–89% of its 1/16-vCPU allowance. Capacity is a hypothesis to verify
with live health and checkpoint checks, not an established root cause. This keeps
the single writer, five-minute checkpoints and seven-day history. At continuous
30-day runtime, the extra provisioned memory/disk cost about $5.22 before shared
allowances; CPU is usage-billed. See [current pricing](https://developers.cloudflare.com/containers/platform/pricing/).

The 2026-11-17 rollout passed live health, resident-continuity and fresh-checkpoint
checks. Three health responses took 0.308–0.416 seconds; two complete CPU buckets
averaged 21.76% of the new allocation. This supports retaining `basic`, without
establishing the earlier timeout's cause or long-term endurance. See the
[release qualification](../../docs/RELEASE-QUALIFICATION.md) for evidence and limits.

## Build the Linux agent

From the Lessville project root, with Python 3 and Docker running:

```sh
python3 tools/build_cloudflare_linux.py --image-tag lessville-agent:local
```

The helper freezes the backend and vendored engine into a temporary source tree,
checks the engine files against their provenance hashes,
builds with the locked dependencies using Rust 1.96.1 in the pinned official
Debian 12 image, and targets Linux/amd64 even on Apple Silicon. One compiler job
is the default for Docker VMs with 2 GB RAM; `--jobs N` changes that limit.
The first build downloads the locked crates and can take substantially longer
under CPU emulation. No credentials or world directories are mounted.

The parent repository also provides the **Lessville Linux release** GitHub Actions
workflow for native x86 builds. It runs the same helper with four jobs and publishes
only binary chunks, the release marker and source manifest as a seven-day artifact.
It has read-only repository access and does not deploy or receive Cloudflare credentials.
Validate the downloaded artifact and image before deployment, just as for a local build.

The build uses Docker volumes `lessville-cargo-registry` and
`lessville-linux-amd64-target` as caches. It preserves `Cargo.lock`. It validates
that the executable is an x86-64 ELF, writes the 24 existing image-layer chunks
under `container/bin-parts/`, and updates the tracked `container/release-marker.txt`
with source and executable hashes. Startup recombines and verifies those chunks.
The optional image tag builds the pinned Ubuntu 24.04 runtime image locally.

The complete source-input hash manifest is written to
`.local/cloudflare-build/build-manifest.json`. Chunks, caches and that manifest
are local artifacts; the helper, image recipe and release marker are source.
The vendored engine has its own source provenance in
`vendor/lessdb/PROVENANCE.json`. This is a reproducible preparation procedure;
bit-for-bit reproducibility across independent toolchains is not assumed.

The helper refuses to proceed when `container/data/` contains world files. Keep
only the empty `.keep` there. The image runs as UID/GID 10001 and restores its
world from R2 at runtime; never bake a local recording world into the image.

## Build the Web client

Install Godot 4.7.2 and its matching Web export templates, then run from the
Lessville root:

```sh
./tools/export_web.sh
python3 tools/prepare_cloudflare_web.py
```

The second command stages static files in `deploy/cloudflare/public/` and writes
`.local/cloudflare-web.json` with the two binary hashes, sizes and R2 keys. Upload
each binary to `lessville-data/<key>` using project-local `wrangler r2 object put
--remote --file <file>` before deploying. Verify the published bytes against the
manifest; already-hosted matching hashes can be reused. The immutable paths
keep old and new client releases independent. Generated exports stay out of Git.

## Validate the local image

Use a disposable local container with cloud integrations disabled:

```sh
docker run --detach --name lessville-local-check --platform linux/amd64 \
  --network none --memory 256m --memory-swap 256m \
  --env LESSVILLE_PUBLIC=0 --env LESSVILLE_LLM_URL= \
  --env LESSVILLE_CKPT_URL= --env LESSVILLE_INTERNAL_KEY= \
  --env LESSVILLE_LIVE_WEATHER=0 lessville-agent:local
docker exec lessville-local-check curl --fail http://127.0.0.1:8790/api/health
docker exec lessville-local-check curl --fail http://127.0.0.1:8790/api/state
docker stop --time 30 lessville-local-check
docker rm lessville-local-check
```

The empty local world is disposable and is not the live R2 checkpoint. Validate
the health and state responses, logs, persistence on a local container restart,
and memory use before a public rollout. A copied production checkpoint restore
requires a separate read-only snapshot and isolated local data directory.

## Release boundary

This helper does not export the Godot client, publish R2 assets, set secrets or
deploy the Worker. Prepare the web assets in `public/`, run the Worker checks,
and validate the existing checkpoint restore before deployment. Preserve the
current singleton name, `lessville/v3/` checkpoint prefix and internal secret;
do not run two writers against the same checkpoint key.

From this directory, `npm ci` and `npm run check` validate the Worker and
`npm run deploy` performs the authorized Cloudflare deployment. Wrangler can
[activate the Worker before its container rollout finishes](https://developers.cloudflare.com/containers/guides/deploy/), so a successful
Worker upload is not sufficient evidence that the replacement simulation is
healthy. Confirm restored clock/population and subsequent checkpoint success
after the container rollout.
