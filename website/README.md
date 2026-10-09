# LessDB Website

The official-style landing page for [LessDB](../) — **one database for agents
and humans** — a static site with no build step and no dependencies.

## Files

| File | Purpose |
|---|---|
| `index.html` | Single-page site: hero, curl install, live demo, why, features, **control & governance**, benchmarks, **full quickstart guide** (all 11 steps), integrations, footer |
| `content/docs/agent-blueprints.md` | Public deployment profiles from a single assistant to software factories, communities and swarms |
| `content/docs/agent-scenarios.md` | Public SC-01–SC-06 conformance scenarios and LessVille adoption mapping |
| `styles.css` | Bioluminescent terminal theme (warm black + firefly amber glow) |
| `app.js` | Pixel-art mascot renderer (**LUMO** the firefly), word swap, terminal loop, stat counters, install tabs, copy button, scroll reveals |
| `_mascot_preview.py` | Dev helper — prints an ASCII preview of LUMO's pixel map |

## Run it locally

```bash
cd website
python3 -m http.server 8080
# open http://localhost:8080
```

Any static file server works (`npx serve`, `caddy file-server`, `miniserve`, …).
Deploy by dropping the folder on any static host (Cloudflare Pages, GitHub
Pages, Netlify, S3, …).

## The mascot: LUMO

LUMO is a pixel-art firefly: the smallest creature in the database swamp, but
the one that glows. Fireflies convert energy to light at ~100% efficiency —
which is exactly the LessDB pitch: *extremely resource-efficient, visible in
the dark, and impossible to lose in a cluster*. Agents and humans both
navigate by its light: SQL, vectors, and shared data work, one glow at a time.

## Positioning notes

- The site implements the positioning from [docs/POSITIONING.md](../docs/POSITIONING.md):
  *"the analytical database your agents and your analysts share"* — one
  governed system of record, two front doors, one audit trail.
- Pitch copy avoids engine-internal and borrowed terminology — no Firefly,
  no "AI-native"/"memory layer" language, and no other databases are
  mentioned anywhere in the marketing prose.
- The literal `ENGINE = Firefly` syntax appears **only inside quickstart
  code blocks** (steps 3 and 10) because it is the real CREATE TABLE syntax
  the CLI accepts — those commands must be copy-pasteable and correct.
- The governance section describes the product's control plane (roles on
  every door, per-agent identity/tenancy, one audit trail, approval gates).
  Shipped-vs-roadmap status for each item is tracked truthfully in
  [docs/AGENT-GOVERNANCE.md](../docs/AGENT-GOVERNANCE.md) §1.

- Glowing amber abdomen (pulses like real bioluminescence)
- Translucent teal wings (flap animation)
- Warm blinking eyes + antenna tips that pulse
- A red scarf, because every mascot needs character

The pixel map lives in `app.js` (`PIXEL_MAP`, 21×24) and is rendered as SVG
rects at any size: hero (10× scale), pill/face, header, footer, final CTA.
Edit the map, then preview with `python3 _mascot_preview.py`.

## Distribution — Cloudflare packages

The site presents the curl-first distribution model: every release (CLI
tarballs, Python wheels, Node packages) is published to a package host on
Cloudflare and installed from there:

```bash
curl -fsSL https://packages.lessdb.dev/install.sh | sh
```

The package host is `https://lessdb.dev/npm/`. It appears in the generated
installer and package documentation; keep it aligned with the Pages Function
when changing distribution:

1. `index.html` → the curl tab + quickstart steps 1 and 10
2. `app.js` → the `INSTALL_COMMANDS` map

The installer contract the site describes: fetch the release tarball for the
detected OS/arch, unpack into `~/.lessdb`, and add one PATH line to the shell
profile. Keep that behavior when you write `install.sh` so the docs stay true.

## Editing

- Copy/content: `index.html`
- Theme tokens (colors, fonts, spacing): `styles.css` → `:root`
- Animations & interactions: `app.js`
