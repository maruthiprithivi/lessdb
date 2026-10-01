#!/usr/bin/env node
// Downloads the matching prebuilt `lessdb` binary from lessdb.dev (or
// LESSDB_DOWNLOAD_BASE) and verifies its SHA-256. Runs on `npm install`.
"use strict";
const fs = require("fs");
const path = require("path");
const os = require("os");
const crypto = require("crypto");
const { execFileSync } = require("child_process");
const https = require("https");

const BASE = process.env.LESSDB_DOWNLOAD_BASE || "https://lessdb.dev/dl";
// Track the npm package version (or pin via LESSDB_VERSION), so the
// wrapper and the binary release stay in lockstep without hardcoding.
const VERSION = process.env.LESSDB_VERSION || `v${require("./package.json").version}`;
const PLATFORMS = require("./platforms.json");

const map = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
};
const key = `${process.platform}-${process.arch}`;
const triple = map[key];
if (!triple) {
  console.error(`lessdb: no prebuilt binary for ${key} — install via brew or cargo instead`);
  process.exit(0);
}
const file = `lessdb-${VERSION}-${triple}.tar.gz`;
const want = PLATFORMS[triple];
if (!want) {
  console.error(`lessdb: missing checksum for ${triple}`);
  process.exit(1);
}

const dir = path.join(__dirname, "bin");
const url = `${BASE}/${file}?v=${want.slice(0, 12)}`;
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "lessdb-"));

function get(url) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { "user-agent": "lessdb-npm-installer" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        return get(res.headers.location).then(resolve, reject);
      }
      if (res.statusCode !== 200) return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve(Buffer.concat(chunks)));
    }).on("error", reject);
  });
}

(async () => {
  console.log(`lessdb: downloading ${url}`);
  const buf = await get(url);
  const got = crypto.createHash("sha256").update(buf).digest("hex");
  if (got !== want) {
    console.error(`lessdb: sha256 mismatch\n  got  ${got}\n  want ${want}`);
    process.exit(1);
  }
  console.log(`lessdb: sha256 ok (${got.slice(0, 16)}…)`);
  const tgz = path.join(tmp, file);
  fs.writeFileSync(tgz, buf);
  execFileSync("tar", ["-xzf", tgz, "-C", tmp, "--strip-components=1"]);
  const src = path.join(tmp, "lessdb");
  fs.mkdirSync(dir, { recursive: true });
  fs.copyFileSync(src, path.join(dir, "lessdb"));
  fs.chmodSync(path.join(dir, "lessdb"), 0o755);
  fs.rmSync(tmp, { recursive: true, force: true });
  console.log("lessdb: installed — try `npx lessdb sql \"SELECT 1\"`");
})().catch((e) => { console.error("lessdb install failed:", e.message); process.exit(1); });
