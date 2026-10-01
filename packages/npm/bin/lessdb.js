#!/usr/bin/env node
// Shim: executes the downloaded `lessdb` binary.
"use strict";
const path = require("path");
const { spawnSync } = require("child_process");
const bin = path.join(__dirname, "lessdb");
const r = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (r.error) { console.error(`lessdb: binary missing (${r.error.message}) — run \`npm rebuild lessdb\``); process.exit(1); }
process.exit(r.status ?? 1);
