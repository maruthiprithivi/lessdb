import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";
import ts from "typescript";

// Execute the actual Worker source with only the Cloudflare platform boundary
// replaced. No network, live Durable Object, credentials, or duplicate API logic.
const source = readFileSync(new URL("../src/index.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;

function loadWorker() {
  let now = 100_000;
  class Clock extends Date {
    static now() { return now; }
  }
  class DurableObject {
    constructor(ctx, env) { this.ctx = ctx; this.env = env; }
  }
  const scope = vm.createContext({
    exports: {}, Date: Clock, Request, Response, Headers, structuredClone,
    setTimeout, clearTimeout, console, TextEncoder, URL,
    require(name) {
      if (name === "cloudflare:workers") return { DurableObject };
      if (name === "@cloudflare/containers") return {
        Container: DurableObject,
        getContainer: (binding, name) => binding.get(binding.idFromName(name)),
      };
      throw new Error(`Unexpected import: ${name}`);
    },
  });
  vm.runInContext(compiled + `
    globalThis.api = { createTownApi, patchStateBody, realismPatchEnabled,
      Budget, budgetStateStore, handleApi, TOWN, CKPT_PREFIX };`, scope);
  return { ...scope.api, worker: scope.exports.default, advance: (ms) => { now += ms; } };
}

const clean = () => new Request("http://container/api/state");
const snapshot = (minute = 600) => JSON.stringify({ clock: { minute_of_day: minute }, agents: [], background: [] });
const plain = (value) => JSON.parse(JSON.stringify(value));
function store(initial = null) {
  let value = initial;
  return {
    writes: [],
    async get() { return value; },
    async put(next) { value = next; this.writes.push(next); },
  };
}

test("cold state is fetched once, persisted raw, and returned with security headers", async () => {
  const { createTownApi } = loadWorker();
  const api = createTownApi();
  const shared = store();
  const body = snapshot();
  let calls = 0;
  const town = { async fetch(request) { calls++; assert.equal(request.url, clean().url); return new Response(body); } };
  const first = await api.handleState(town, clean(), { "x-frame-options": "DENY" }, { store: shared });
  assert.equal(first.body, body);
  assert.equal(first.headers["x-lessville-state"], "live");
  assert.equal(first.headers["cache-control"], "no-store");
  assert.equal(first.headers["x-frame-options"], "DENY");
  const second = await api.handleState(town, clean(), {}, { store: shared });
  assert.equal(second.headers["x-lessville-state"], "fresh");
  assert.equal(calls, 1);
  assert.equal(shared.writes.length, 1);
  assert.equal(shared.writes[0].body, body);
});

test("a new isolate serves shared stale state immediately and coalesces background refresh", async () => {
  const { createTownApi, advance } = loadWorker();
  const api = createTownApi();
  const before = snapshot(600), after = snapshot(605);
  const shared = store({ at: 90_000, body: before, status: 200 });
  const pending = [];
  let resolve, calls = 0;
  const town = { fetch() { calls++; return new Promise((r) => { resolve = r; }); } };
  const opts = { store: shared, waitUntil: (task) => pending.push(task) };
  const first = await api.handleState(town, clean(), {}, opts);
  assert.equal(first.body, before);
  assert.equal(first.headers["x-lessville-state"], "stale");
  await api.handleState(town, clean(), {}, opts);
  assert.equal(calls, 1);
  assert.equal(pending.length, 1);
  resolve(new Response(after));
  await pending[0];
  assert.equal(shared.writes[0].body, after);
  advance(350);
  const stale = await api.handleState(town, clean(), {}, opts);
  assert.equal(stale.body, after);
  assert.equal(stale.headers["x-lessville-state"], "stale");
  assert.equal(calls, 1, "refresh is throttled even after the 350ms fresh interval");
  advance(4_650);
  await api.handleState(town, clean(), {}, opts);
  assert.equal(calls, 2, "background refresh becomes due after five seconds");
  resolve(new Response(snapshot(610)));
  await pending[1];
});

test("timeout without a saved state returns retryable 503; non-state requests also time out", async () => {
  const api = loadWorker().createTownApi({ timeoutMs: 2 });
  const town = { fetch: () => new Promise(() => {}) };
  const state = await api.handleState(town, clean());
  assert.equal(state.status, 503);
  assert.equal(state.headers["x-lessville-state"], "unavailable");
  assert.equal(state.headers["retry-after"], "5");
  const other = await api.handleProxied(town, new Request("http://container/api/health"));
  assert.equal(other.timedOut, true);
  assert.equal(other.status, 503);
});

test("failed refresh preserves stale state and cannot poison durable last-good state", async () => {
  const { createTownApi } = loadWorker();
  for (const reply of [() => new Response("upstream failed", { status: 502 }), () => new Response("not JSON")]) {
    const shared = store({ at: 80_000, body: snapshot(), status: 200 });
    const api = createTownApi();
    const pending = [];
    const res = await api.handleState({ fetch: async () => reply() }, clean(), {}, {
      store: shared, waitUntil: (task) => pending.push(task),
    });
    await Promise.all(pending);
    assert.equal(res.status, 200);
    assert.equal(res.body, snapshot());
    assert.equal(shared.writes.length, 0);
    assert.equal(api.getStateCache().body, snapshot());
  }
});

test("invalid shared/cold upstream responses are never cached as town state", async () => {
  const api = loadWorker().createTownApi();
  const shared = store({ at: 99_999, body: '{"error":"failure"}', status: 200 });
  const res = await api.handleState({ fetch: async () => new Response("busy", { status: 503 }) }, clean(), {}, { store: shared });
  assert.equal(res.status, 503);
  assert.equal(res.headers["x-lessville-state"], "upstream");
  assert.equal(api.getStateCache(), null);
  assert.equal(shared.writes.length, 0);
});

test("shared storage failure is tolerated while a live snapshot remains usable", async () => {
  const api = loadWorker().createTownApi();
  const shared = { get: async () => { throw new Error("offline"); }, put: async () => { throw new Error("offline"); } };
  const result = await api.handleState({ fetch: async () => new Response(snapshot()) }, clean(), {}, { store: shared });
  assert.equal(result.status, 200);
  assert.equal(api.getStateCache().body, snapshot());
});

test("healthz preserves production availability semantics and reports snapshot age", async () => {
  const { createTownApi } = loadWorker();
  const empty = await createTownApi().healthz(null);
  assert.equal(empty.ok, false);
  assert.equal(empty.town_fetch_timeout_ms, 8_000);
  const health = await createTownApi().healthz({ cap: 9500 }, store({ at: 1, body: snapshot(), status: 200 }));
  assert.equal(health.ok, true, "availability includes stale cache, not proof the live simulation advances");
  assert.equal(health.state.age_ms, 99_999);
  assert.equal(health.keepalive.ok, false);
  assert.equal(health.budget.cap, 9500);
});

test("keepalive records health failures but still refreshes the raw shared state", async () => {
  const api = loadWorker().createTownApi();
  const shared = store(), paths = [], errors = [];
  const town = { async fetch(req) {
    paths.push(new URL(req.url).pathname);
    return paths.length === 1 ? new Response("failed", { status: 502 }) : new Response(snapshot(700));
  } };
  const status = await api.runKeepalive(town, { log() {}, error(...args) { errors.push(args); } }, shared);
  assert.equal(status.ok, false);
  assert.equal(status.status, 502);
  assert.deepEqual(paths, ["/api/health", "/api/state"]);
  assert.equal(errors[0][0], "HEALTH_FAIL");
  assert.equal(shared.writes[0].body, snapshot(700));
});

test("Budget keeps the existing singleton snapshot key separate from neuron accounting", async () => {
  const { Budget, budgetStateStore } = loadWorker();
  const data = new Map([["2026-11-17", 42]]);
  const budget = new Budget({ storage: {
    get: async (key) => data.get(key), put: async (key, value) => data.set(key, value),
  } }, {});
  const shared = budgetStateStore(budget);
  assert.equal(await shared.get(), null);
  await shared.put({ at: 0, body: snapshot(), status: 0 });
  assert.deepEqual(plain(data.get("state_snapshot")), { at: 100_000, body: snapshot(), status: 200 });
  assert.equal(data.get("2026-11-17"), 42);
  await budget.putStateSnapshot({ body: 12 });
  assert.equal((await shared.get()).body, snapshot());
});

test("disabled legacy filter preserves authoritative shelter wait and upstairs sleep byte-for-byte", async () => {
  const { createTownApi, patchStateBody, realismPatchEnabled } = loadWorker();
  // These tuples are emitted by backend sim.rs activity_fields, including Maya's
  // upstairs home at the cafe. The old adapter rewrites both into other actions.
  const body = JSON.stringify({ clock: { minute_of_day: 23 * 60 }, agents: [
    { id: "lin", location: "bus_stop", activity: "Waiting at the bus stop", activity_kind: "commuting", station: "shelter", pose: "idle", travel: null },
    { id: "maya", location: "cafe", activity: "Asleep", activity_kind: "sleeping", station: "upstairs", pose: "sleep", life: { energy: 50 } },
  ] }, null, 2);
  const changed = JSON.parse(patchStateBody(body));
  assert.equal(changed.agents[0].station, "street");
  assert.equal(changed.agents[0].pose, "walk_side");
  assert.equal(changed.agents[1].activity_kind, "idle");
  const shared = store();
  const api = createTownApi();
  const town = { fetch: async () => new Response(body) };
  const legacy = await api.handleState(town, clean(), {}, { store: shared, realismPatch: true });
  assert.notEqual(legacy.body, body);
  assert.equal(shared.writes[0].body, body, "the legacy transform must never be persisted");
  const authoritative = await api.handleState(town, clean(), {}, { store: shared, realismPatch: realismPatchEnabled("0") });
  assert.equal(authoritative.body, body);
  assert.equal(patchStateBody("malformed"), "malformed");
});

test("legacy opt-in parsing retains deployed behavior", () => {
  const { realismPatchEnabled } = loadWorker();
  for (const value of [true, 1, "1", " true ", "TRUE"]) assert.equal(realismPatchEnabled(value), true);
  for (const value of [false, 0, "0", "false", "yes", undefined, null]) assert.equal(realismPatchEnabled(value), false);
});

test("public API remains observer-only and forwards clean requests to the existing singleton", async () => {
  const { handleApi, TOWN, CKPT_PREFIX } = loadWorker();
  assert.equal(TOWN, "town-v3-retention-2026-11-14");
  assert.equal(CKPT_PREFIX, "lessville/v3/");
  let allowed = true, calls = 0;
  const town = { async fetch(request) {
    calls++;
    assert.equal(request.headers.get("authorization"), null);
    assert.equal(request.url, "http://container/api/health?detail=1");
    return new Response("ok", { headers: { "access-control-allow-origin": "*" } });
  } };
  const env = {
    RL: { limit: async () => ({ success: allowed }) },
    AGENT: { idFromName: (name) => { assert.equal(name, TOWN); return name; }, get: () => town },
  };
  const url = new URL("https://ville.lessdb.dev/api/health?detail=1");
  assert.equal((await handleApi(new Request(url, { method: "POST" }), env, url, {})).status, 403);
  allowed = false;
  assert.equal((await handleApi(new Request(url), env, url, {})).status, 429);
  allowed = true;
  const response = await handleApi(new Request(url, { headers: { authorization: "private" } }), env, url, {});
  assert.equal(calls, 1);
  assert.equal(response.headers.get("access-control-allow-origin"), null);
  assert.equal(response.headers.get("x-frame-options"), "DENY");
});

test("deployment keeps production bindings and disables the incompatible post-filter explicitly", () => {
  const config = readFileSync(new URL("../wrangler.jsonc", import.meta.url), "utf8");
  const parsed = ts.parseConfigFileTextToJson("wrangler.jsonc", config).config;
  assert.equal(parsed.name, "lessville");
  assert.equal(parsed.vars.LESSVILLE_REALISM_PATCH, "0");
  assert.deepEqual(parsed.durable_objects.bindings, [
    { name: "AGENT", class_name: "LessvilleAgent" }, { name: "BUDGET", class_name: "Budget" },
  ]);
  assert.deepEqual(parsed.migrations, [{ tag: "v1", new_sqlite_classes: ["LessvilleAgent", "Budget"] }]);
  assert.equal(parsed.r2_buckets[0].bucket_name, "lessville-data");
  assert.equal(parsed.ratelimits[0].namespace_id, "4207");
  assert.equal(parsed.containers[0].class_name, "LessvilleAgent");
  assert.equal(parsed.vars.INTERNAL_KEY, undefined);
});

test("Worker routes healthz and scheduled refresh through the existing shared Budget object", async () => {
  const { worker, TOWN } = loadWorker();
  const paths = [], pending = [], shared = store({ at: 99_000, body: snapshot(), status: 200 });
  const stub = {
    status: async (cap) => ({ used: 1, cap }),
    getStateSnapshot: () => shared.get(),
    putStateSnapshot: (next) => shared.put(next),
  };
  const env = {
    DAILY_NEURON_CAP: "9500",
    BUDGET: { idFromName: (name) => { assert.equal(name, "global"); return name; }, get: () => stub },
    AGENT: { idFromName: (name) => { assert.equal(name, TOWN); return name; }, get: () => ({
      async fetch(request) {
        const path = new URL(request.url).pathname;
        paths.push(path);
        return new Response(path === "/api/state" ? snapshot(750) : "healthy");
      },
    }) },
  };
  const ctx = { waitUntil: (task) => pending.push(task) };
  const response = await worker.fetch(new Request("https://ville.lessdb.dev/api/healthz", {
    headers: { "user-agent": "Mozilla/5.0" },
  }), env, ctx);
  const health = await response.json();
  assert.equal(response.status, 200);
  assert.equal(health.service, "lessville-worker");
  assert.equal(health.budget.used, 1);
  assert.equal(paths.length, 0, "healthz must not wake the container");
  await worker.scheduled({}, env, ctx);
  await Promise.all(pending);
  assert.deepEqual(paths, ["/api/health", "/api/state"]);
  assert.equal(shared.writes[0].body, snapshot(750));
});
