// Addon-owned launcher: follow the runtime selected for this worktree at run start.
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { pathToFileURL } = require('node:url');
const root = fs.realpathSync(path.resolve(__dirname, '../..'));
const key = 'manusGameRuntime.' + createHash('sha256').update(root).digest('hex') + '.path';
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^GIT_/i.test(key)));
env.GIT_CONFIG_NOSYSTEM = '1';
env.GIT_CONFIG_GLOBAL = process.platform === 'win32' ? 'NUL' : '/dev/null';
const selected = spawnSync('git', ['config', '--local', '--no-includes', '--get', key], { cwd: root, env, encoding: 'utf8', timeout: 1000, maxBuffer: 16384, windowsHide: true });
const runtime = selected.status === 0 ? selected.stdout.trim() : process.env.GAME_RUNTIME;
if (!runtime || !path.isAbsolute(runtime)) { console.error('Set GAME_RUNTIME to the current Game runtime directory from init/attach'); process.exit(1); }
const [entry, ...args] = process.argv.slice(2);
if (!/^\.\/[a-z-]+\.mjs$/.test(entry || '')) { console.error('Invalid Game runtime command'); process.exit(1); }
const target = path.resolve(runtime, entry);
process.env.GAME_RUNTIME = runtime;
process.argv = [process.execPath, target, ...args];
import(pathToFileURL(target).href).catch(error => { console.error(error); process.exitCode = 1; });
