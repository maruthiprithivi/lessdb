// Lessville on Cloudflare — ville.lessdb.dev
//
//  /                 static Godot web build (Workers static assets)
//  /api/*            read-only proxy to the single LessDB agent container (observer mode)
//  /internal/llm     container -> Workers AI, one short task per call, hard daily neuron budget
//  /internal/ckpt/*  container -> R2 (lessville-data) checkpoint archives, via native binding
//  cron */5          keeps the one shared town running
//
// The container never holds provider or R2 credentials; it authenticates to the two
// internal routes with INTERNAL_KEY (a Worker secret passed in as an env var).

import { Container, getContainer } from "@cloudflare/containers";
import { DurableObject } from "cloudflare:workers";

export interface Env {
	AGENT: DurableObjectNamespace<LessvilleAgent>;
	BUDGET: DurableObjectNamespace<Budget>;
	AI: Ai;
	BUCKET: R2Bucket;
	ASSETS: Fetcher;
	RL: RateLimit;
	INTERNAL_KEY: string;
	DAILY_NEURON_CAP: string;
	PUBLIC_ORIGIN: string;
	CKPT_HOURLY_KEEP: string;
	LESSVILLE_REALISM_PATCH: string;
}

// Legacy observer-only compatibility filter from production version
// 366d146e-3a77-437b-b4e4-b978f7113ddb. Disabled for the current backend:
// it rewrites authoritative shelter waits and upstairs sleep into different actions.
// Keep the historical transform available for an explicit legacy-backend rollback.
interface LegacyClock { minute_of_day?: number | string }
interface LegacyAgent {
  location?: string; station?: string; pose?: string; activity_kind?: string;
  activity?: string; role?: string; occupation?: string; place?: string;
}
interface LegacyBackground { location?: string; activity?: string; target?: string }
interface LegacyState { clock?: LegacyClock; agents?: LegacyAgent[]; background?: LegacyBackground[] }

// src/realism/catalog.js
const SEAT_STATIONS: Record<string, Set<string>> = {
  cafe: new Set(["table1", "table2", "table3"]),
  office: new Set(["desk"]),
  home: new Set(["dining", "living"]),
  school: new Set(["lecture"]),
  gym: new Set(["bench"])
};
const SIT_POSES = new Set(["seat", "sit", "seated", "sit_down"]);
const SLEEP_KINDS = new Set(["sleeping", "sleep", "asleep"]);
const WALK_POSES = new Set(["walk", "walk_side", "walk_front", "walk_back", "run"]);
const HOURS: Record<string, [number, number]> = {
  cafe: [7 * 60, 21 * 60],
  office: [9 * 60, 18 * 60],
  school: [8 * 60, 16 * 60],
  grocery: [8 * 60, 21 * 60],
  gym: [6 * 60, 22 * 60]
};
const SLEEP_WINDOW = [22 * 60, 6 * 60 + 30];
const CAFE_STAFF_ROLES = new Set(["Owner", "Barista"]);
const ACTIVITY_KIND_HINTS: [string[], string][] = [
  [["getting ready", "tidying", "exploring"], "idle"],
  [["heading", "walking to", "on the way", "commuting"], "commuting"],
  [["running the counter", "pulling espresso", "serving"], "serving"],
  [["quarterly report", "focus work", "typing"], "typing"],
  [["lecture", "studying", "exam"], "studying"],
  [["workout", "gym"], "workout"],
  [["cooking", "dinner with", "breakfast"], "cooking"],
  [["asleep", "sleeping", "in bed"], "sleeping"]
];
function inSleepWindow(minuteOfDay: number) {
  const [start, end] = SLEEP_WINDOW;
  return start <= minuteOfDay || minuteOfDay < end;
}
function locationOpen(location: string, minuteOfDay: number) {
  const hours = HOURS[location];
  if (!hours) return true;
  const [start, end] = hours;
  return start <= minuteOfDay && minuteOfDay < end;
}
function isSeat(location: string, station: string) {
  const seats = SEAT_STATIONS[location];
  return Boolean(seats && seats.has(station));
}

// src/realism/check.js
function modMinute(minuteOfDay: unknown) {
  if (minuteOfDay == null) return null;
  const n = Number(minuteOfDay);
  if (!Number.isFinite(n)) return null;
  return (n % (24 * 60) + 24 * 60) % (24 * 60);
}
function hintKind(activity: string) {
  const text = (activity || "").toLowerCase();
  for (const [phrases, kind] of ACTIVITY_KIND_HINTS) {
    if (phrases.some((p) => text.includes(p))) return kind;
  }
  return null;
}

// src/realism/patch.js
function fixAgent(agent: LegacyAgent, clock: LegacyClock) {
  const a = { ...agent };
  let loc = a.location || "";
  let station = a.station || "";
  let pose = a.pose || "";
  let kind = a.activity_kind || "";
  const activity = a.activity || "";
  const role = a.role || a.occupation || "";
  const minute = modMinute((clock || {}).minute_of_day);
  const hinted = hintKind(activity);
  const walking = /walk|heading|bus stop|on the way|commuting/i.test(activity);
  if ((loc === "away" || station === "offmap") && (walking || WALK_POSES.has(pose) || kind === "commuting" || hinted === "commuting")) {
    a.location = "transit";
    a.place = "On the way";
    a.station = "street";
    a.pose = WALK_POSES.has(pose) ? pose : "walk_side";
    a.activity_kind = "commuting";
    loc = a.location;
    station = a.station;
    pose = a.pose;
    kind = a.activity_kind;
  }
  if (hinted && hinted !== kind) {
    if (SLEEP_KINDS.has(kind) && hinted !== "sleeping") {
      a.activity_kind = hinted;
      kind = hinted;
    } else if (["commuting", "serving", "typing", "studying", "workout", "cooking"].includes(hinted)) {
      a.activity_kind = hinted;
      kind = hinted;
    }
  }
  if (loc === "cafe" && kind === "serving" && !CAFE_STAFF_ROLES.has(role)) {
    a.activity_kind = activity.toLowerCase().includes("caf\xE9") || activity.toLowerCase().includes("cafe") || station === "queue" ? "waiting" : "idle";
    kind = a.activity_kind;
    if (!a.activity || String(a.activity).startsWith("Exploring")) {
      a.activity = station === "queue" ? "Waiting at Less Caf\xE9" : a.activity;
    }
  }
  if (loc === "cafe" && kind === "serving" && CAFE_STAFF_ROLES.has(role) && station === "queue") {
    if (/espresso/i.test(activity) || role === "Barista") {
      a.station = "espresso";
      a.pose = "cook";
    } else {
      a.station = "till";
      a.pose = "talk";
    }
    station = a.station;
    pose = a.pose;
  }
  if (loc === "cafe" && kind === "serving" && minute != null && !locationOpen("cafe", minute)) {
    a.activity_kind = "idle";
    a.activity = /clos/i.test(activity) ? activity : "Closing up";
    a.pose = "idle";
    kind = a.activity_kind;
    pose = a.pose;
  }
  if (SLEEP_KINDS.has(kind)) {
    const reallySleep = /asleep|sleeping|in bed/i.test(activity);
    const night = minute == null || inSleepWindow(minute);
    if (!reallySleep) {
      a.activity_kind = hinted && hinted !== "sleeping" ? hinted : "idle";
      kind = a.activity_kind;
    } else if (!night || !["home", "away"].includes(loc)) {
      a.activity_kind = "idle";
      kind = "idle";
    }
  }
  if (kind === "commuting") {
    if (!["transit", "bus", "bus_stop", "town"].includes(loc)) {
      a.location = "transit";
      a.place = "On the way";
      loc = a.location;
    }
    if (!["street", "cabin"].includes(station)) {
      a.station = "street";
      station = "street";
    }
    if (!WALK_POSES.has(pose)) {
      a.pose = "walk_side";
      pose = a.pose;
    }
  }
  if (SIT_POSES.has(pose) && !isSeat(loc, station)) {
    a.pose = "idle";
    pose = "idle";
  }
  if (kind === "typing") {
    a.location = "office";
    a.station = "desk";
    a.pose = "type";
    a.place = a.place || "Office";
  }
  return a;
}
function fixBackground(citizens: LegacyBackground[] | undefined) {
  if (!Array.isArray(citizens)) return citizens;
  const out = [];
  for (const c of citizens) {
    const b = { ...c };
    const loc = b.location || "";
    const activity = (b.activity || "").toLowerCase();
    const target = b.target || "";
    if (activity.includes("walking") && loc === target) {
      if (loc === "cafe") b.activity = "At Less Caf\xE9";
      else if (loc === "home") b.activity = "At home";
      else if (loc === "office") b.activity = "At the office";
      else if (loc === "park") b.activity = "At Fountain Park";
      else b.activity = `At ${loc}`;
    } else if (activity.includes("walking") && loc === "home" && target && target !== "home") {
      b.location = "transit";
    }
    out.push(b);
  }
  return out;
}
function patchStateInner(state: LegacyState) {
  const out = structuredClone(state);
  const clock = out.clock || {};
  if (Array.isArray(out.agents)) {
    out.agents = out.agents.map((a) => fixAgent(a, clock));
  }
  if (Object.prototype.hasOwnProperty.call(out, "background")) {
    out.background = fixBackground(out.background);
  }
  return out;
}
function patchStateBody(body: string) {
  try {
    if (typeof body !== "string" || !body) return body;
    const parsed = JSON.parse(body);
    const patched = patchStateInner(parsed);
    return JSON.stringify(patched);
  } catch {
    return body;
  }
}
function realismPatchEnabled(value: unknown) {
  if (value === true || value === 1) return true;
  if (typeof value === "string") return value.trim() === "1" || value.trim().toLowerCase() === "true";
  return false;
}

// Reliability behavior ported from the deployed Worker without changing its
// cache key, timing, stale fallback, or singleton identities. Cached bodies stay raw.
interface StateSnapshot { at: number; body: string; status: number }
interface StateStore {
  get(): Promise<StateSnapshot | null>;
  put(snapshot: StateSnapshot): Promise<void>;
}
interface StateOptions {
  store?: StateStore;
  waitUntil?: (task: Promise<void>) => void;
  realismPatch?: boolean;
}
interface TownFetcher { fetch(request: Request): Promise<Response> }
interface Keepalive { ok: boolean; at: number; ms: number; error: string | null; status: number }

// src/town-api.js
const TOWN_FETCH_TIMEOUT_MS = 8e3;
const STATE_TTL_MS = 350;
const REFRESH_INTERVAL_MS = 5e3;
const KEEPALIVE_FRESH_MS = 15 * 60 * 1e3;
const STATE_SNAPSHOT_KEY = "state_snapshot";
function budgetStateStore(budgetStub: DurableObjectStub<Budget>): StateStore {
  return {
    async get() {
      return budgetStub.getStateSnapshot();
    },
    async put(snap: StateSnapshot) {
      return budgetStub.putStateSnapshot(snap);
    }
  };
}
function isStateShape(body: unknown) {
  if (typeof body !== "string" || !body) return false;
  try {
    const parsed = JSON.parse(body);
    return Boolean(parsed && typeof parsed === "object" && "clock" in parsed && "agents" in parsed);
  } catch {
    return false;
  }
}
function applyRealismPatch(body: string, enabled: boolean) {
  if (!enabled) return body;
  return patchStateBody(body);
}
function wantsRealismPatch(opts: StateOptions, defaultEnabled: boolean) {
  if (opts && Object.prototype.hasOwnProperty.call(opts, "realismPatch")) {
    return Boolean(opts.realismPatch);
  }
  return Boolean(defaultEnabled);
}
function createTownApi({ timeoutMs = TOWN_FETCH_TIMEOUT_MS, realismPatch = false } = {}) {
  let stateCache: StateSnapshot | null = null;
  let lastKeepalive: Keepalive = { ok: false, at: 0, ms: 0, error: null, status: 0 };
  let refreshing: Promise<void> | null = null;
  let lastRefreshAt = 0;
  function withTimeout<T>(promise: Promise<T>, ms: number, label = "town fetch timeout") {
    let id: ReturnType<typeof setTimeout>;
    const timeout = new Promise<never>((_, reject) => {
      id = setTimeout(() => reject(new Error(label)), ms);
    });
    return Promise.race([promise, timeout]).finally(() => clearTimeout(id));
  }
  async function fetchTown(town: TownFetcher, request: Request, ms = timeoutMs) {
    return withTimeout(town.fetch(request), ms);
  }
  function stateHeaders(base: Record<string, string>, extra: Record<string, string> = {}) {
    return { "content-type": "application/json", "cache-control": "no-store", ...base, ...extra };
  }
  async function loadShared(store?: StateStore | null) {
    if (stateCache || !store) return stateCache;
    try {
      const shared = await store.get();
      if (shared && isStateShape(shared.body)) stateCache = shared;
    } catch {
    }
    return stateCache;
  }
  async function persist(snap: StateSnapshot, store?: StateStore | null) {
    stateCache = snap;
    lastRefreshAt = snap.at;
    if (store) {
      try {
        await store.put(snap);
      } catch {
      }
    }
  }
  async function refreshLive(town: TownFetcher, clean: Request, store?: StateStore | null) {
    if (refreshing) return refreshing;
    lastRefreshAt = Date.now();
    refreshing = (async () => {
      try {
        const res = await fetchTown(town, clean, timeoutMs);
        const body = await res.text();
        if (res.ok && isStateShape(body)) {
          await persist({ at: Date.now(), body, status: res.status }, store);
        }
      } catch {
      } finally {
        refreshing = null;
      }
    })();
    return refreshing;
  }
  function scheduleRefresh(town: TownFetcher, clean: Request, store: StateStore | undefined, waitUntil?: (task: Promise<void>) => void) {
    const due = !lastRefreshAt || Date.now() - lastRefreshAt >= REFRESH_INTERVAL_MS;
    if (!due && refreshing) return;
    if (!due && lastRefreshAt) return;
    const task = refreshLive(town, clean, store);
    if (typeof waitUntil === "function") waitUntil(task);
  }
  async function handleState(town: TownFetcher, clean: Request, securityHeaders: Record<string, string> = {}, opts: StateOptions = {}) {
    const store = opts.store;
    const waitUntil = opts.waitUntil;
    const patchOn = wantsRealismPatch(opts, realismPatch);
    await loadShared(store);
    const now = Date.now();
    if (stateCache && isStateShape(stateCache.body)) {
      const fresh = now - stateCache.at < STATE_TTL_MS;
      if (!fresh) scheduleRefresh(town, clean, store, waitUntil);
      return {
        body: applyRealismPatch(stateCache.body, patchOn),
        status: 200,
        headers: stateHeaders(securityHeaders, {
          "x-lessville-state": fresh ? "fresh" : "stale"
        })
      };
    }
    try {
      const res = await fetchTown(town, clean, timeoutMs);
      const body = await res.text();
      if (res.ok && isStateShape(body)) {
        await persist({ at: Date.now(), body, status: res.status }, store);
        return {
          body: applyRealismPatch(body, patchOn),
          status: 200,
          headers: stateHeaders(securityHeaders, { "x-lessville-state": "live" })
        };
      }
      return {
        body,
        status: res.status,
        headers: stateHeaders(securityHeaders, { "x-lessville-state": "upstream" })
      };
    } catch (error) {
      return {
        body: JSON.stringify({ error: "The town is waking up \u2014 try again in a few seconds." }),
        status: 503,
        headers: stateHeaders(securityHeaders, { "retry-after": "5", "x-lessville-state": "unavailable" }),
        error
      };
    }
  }
  async function handleProxied(town: TownFetcher, clean: Request, securityHeaders: Record<string, string> = {}) {
    try {
      const res = await fetchTown(town, clean);
      return { response: res, timedOut: false as const };
    } catch (error) {
      return {
        timedOut: true as const,
        error,
        body: JSON.stringify({ error: "The town is waking up \u2014 try again in a few seconds." }),
        status: 503,
        headers: stateHeaders(securityHeaders, { "retry-after": "5" })
      };
    }
  }
  async function healthz(budget: unknown, store?: StateStore) {
    const started = Date.now();
    await loadShared(store);
    const state = stateCache ? { available: true, age_ms: Date.now() - stateCache.at, status: stateCache.status } : { available: false, age_ms: null, status: null };
    const keepalive = lastKeepalive.at ? { ...lastKeepalive, age_ms: Date.now() - lastKeepalive.at } : { ok: false, at: 0, ms: 0, error: "no keepalive yet", status: 0, age_ms: null };
    const keepaliveFresh = Boolean(keepalive.ok && keepalive.age_ms != null && keepalive.age_ms < KEEPALIVE_FRESH_MS);
    const ok = keepaliveFresh || state.available;
    return {
      ok,
      service: "lessville-worker",
      town_fetch_timeout_ms: timeoutMs,
      budget: budget ?? null,
      state,
      keepalive,
      elapsed_ms: Date.now() - started
    };
  }
  async function runKeepalive(town: TownFetcher, log: Pick<Console, "log" | "error"> = console, store: StateStore | null = null) {
    const t0 = Date.now();
    try {
      const res = await fetchTown(town, new Request("http://container/api/health"));
      const ms = Date.now() - t0;
      if (!res.ok) {
        lastKeepalive = { ok: false, at: Date.now(), ms, error: `status ${res.status}`, status: res.status };
        log.error("HEALTH_FAIL", lastKeepalive);
      } else {
        lastKeepalive = { ok: true, at: Date.now(), ms, error: null, status: res.status };
        log.log("keepalive", res.status, ms);
      }
    } catch (error) {
      lastKeepalive = { ok: false, at: Date.now(), ms: Date.now() - t0, error: String(error), status: 0 };
      log.error("HEALTH_FAIL", lastKeepalive);
    }
    try {
      const res = await fetchTown(town, new Request("http://container/api/state"));
      const body = await res.text();
      if (res.ok && isStateShape(body)) {
        await persist({ at: Date.now(), body, status: res.status }, store);
      }
    } catch {
    }
    return lastKeepalive;
  }
  function seedState(body: string, status = 200, at = Date.now()) {
    stateCache = { at, body, status };
  }
  function getStateCache() {
    return stateCache;
  }
  function getKeepalive() {
    return lastKeepalive;
  }
  return { fetchTown, handleState, handleProxied, healthz, runKeepalive, seedState, getStateCache, getKeepalive, withTimeout };
}


const townApi = createTownApi({ timeoutMs: TOWN_FETCH_TIMEOUT_MS });

const MODEL = "@cf/meta/llama-3.1-8b-instruct-fp8-fast";
// Workers AI pricing for this model (neurons per token).
const NEURONS_IN = 4119 / 1_000_000;
const NEURONS_OUT = 34868 / 1_000_000;
const MAX_TOKENS = 60;
// v1 (2026-10-08..2026-11-14) is preserved read-only in R2 as the archived first town;
// v2 started fresh after the unbounded hot-table growth outage of 2026-11-14.
const CKPT_PREFIX = "lessville/v3/";
// Version the singleton instance when a durable old container can outlive an image
// rollout. The new instance restores the shared world from the existing R2 checkpoint;
// this avoids deleting the application or exposing a public restart control.
const TOWN = "town-v3-retention-2026-11-14";
const SIMULATION_STAGING = [
	{
		name: "Security Lab",
		href: "https://8810-ia3doky0b60f5sph9qj4b-9f69283c.sg2.manus.computer/",
		label: "Six personas, trust scores, guardrail evidence and LessDB provenance.",
	},
	{
		name: "Unified Embodied Lab",
		href: "https://8805-ia3doky0b60f5sph9qj4b-9f69283c.sg2.manus.computer/",
		label: "Reachy Mini and MicroDuck sources in one shared-coordinate observer view.",
	},
	{
		name: "MicroDuck robotd view",
		href: "https://8798-ia3doky0b60f5sph9qj4b-9f69283c.sg2.manus.computer/",
		label: "Official MicroDuck model with robotd simulator body readback.",
	},
	{
		name: "MicroDuck swarm",
		href: "https://8803-ia3doky0b60f5sph9qj4b-9f69283c.sg2.manus.computer/",
		label: "Four official-model MicroDucks with bounded LessDB context telemetry.",
	},
] as const;

export class LessvilleAgent extends Container<Env> {
	defaultPort = 8790;
	sleepAfter = "10m";
	pingEndpoint = "localhost/api/health";

	constructor(ctx: DurableObjectState<{}>, env: Env) {
		super(ctx, env);
		this.envVars = {
			LESSVILLE_PUBLIC: "1",
			LESSVILLE_DATA: "/data",
			LESSVILLE_PORT: "8790",
			LESSVILLE_CHECKPOINT_SECS: "300",
				LESSVILLE_LLM_URL: `${env.PUBLIC_ORIGIN}/internal/llm`,
				LESSVILLE_CKPT_URL: `${env.PUBLIC_ORIGIN}/internal/ckpt`,
				LESSVILLE_CKPT_LABEL: "r2://lessville-data/lessville/v3",
				LESSVILLE_LIVE_WEATHER: "1",
				LESSVILLE_WEATHER_LAT: "1.3521",
				LESSVILLE_WEATHER_LON: "103.8198",
				LESSVILLE_WEATHER_REFRESH_SECS: "3600",
				LESSVILLE_INTERNAL_KEY: env.INTERNAL_KEY,
		};
	}

	override onStart() {
		console.log("lessville container started");
	}

	override onStop(p: { exitCode?: number; reason?: string }) {
		console.log("lessville container stopped", JSON.stringify(p));
	}

	override onError(error: unknown) {
		console.error("lessville container error", error);
		throw error;
	}
}

/** One instance ("global"): neuron accounting and the shared last-good observer snapshot. */
export class Budget extends DurableObject<Env> {
	private day() {
		return new Date().toISOString().slice(0, 10);
	}

	async reserve(estimate: number, cap: number) {
		const day = this.day();
		const used = (await this.ctx.storage.get<number>(day)) ?? 0;
		if (used + estimate > cap) return { ok: false, used, cap, day };
		await this.ctx.storage.put(day, used + estimate);
		return { ok: true, used: used + estimate, cap, day };
	}

	async settle(day: string, estimate: number, actual: number) {
		const used = (await this.ctx.storage.get<number>(day)) ?? 0;
		const next = Math.max(0, used - estimate + actual);
		await this.ctx.storage.put(day, next);
		return next;
	}

	async status(cap: number) {
		const day = this.day();
		return { day, used: (await this.ctx.storage.get<number>(day)) ?? 0, cap };
	}

	async getStateSnapshot() {
		return (await this.ctx.storage.get<StateSnapshot>(STATE_SNAPSHOT_KEY)) ?? null;
	}

	async putStateSnapshot(snapshot: StateSnapshot) {
		if (!snapshot || typeof snapshot.body !== "string") return;
		await this.ctx.storage.put(STATE_SNAPSHOT_KEY, {
			at: Number(snapshot.at) || Date.now(),
			body: snapshot.body,
			status: Number(snapshot.status) || 200,
		});
	}

}

const SECURITY_HEADERS: Record<string, string> = {
	"x-content-type-options": "nosniff",
	"referrer-policy": "strict-origin-when-cross-origin",
	"permissions-policy": "camera=(), microphone=(), geolocation=(), payment=()",
	"x-frame-options": "DENY",
	"strict-transport-security": "max-age=31536000",
	// Same-origin isolation: every game resource is same-origin, so this is harmless and
	// lets the Godot runtime use SharedArrayBuffer if the export needs it.
	"cross-origin-opener-policy": "same-origin",
	"cross-origin-embedder-policy": "require-corp",
	"cross-origin-resource-policy": "same-origin",
};

function json(body: unknown, status = 200, extra: Record<string, string> = {}) {
	return new Response(JSON.stringify(body), {
		status,
		headers: { "content-type": "application/json", "cache-control": "no-store", ...SECURITY_HEADERS, ...extra },
	});
}

// ------------------------------------------------------------------ crawler policy
// robots.txt asks; BLOCKED_UA enforces it for bots that identify themselves.
// Internal container routes are authenticated separately and are never UA-filtered.
const ROBOTS_TXT = `# ville.lessdb.dev - fictional research simulation
# The live API and game binaries are not for crawling. AI training and bulk
# scrapers are refused, and self-identified ones are blocked at the edge.

User-agent: GPTBot
User-agent: ChatGPT-User
User-agent: OAI-SearchBot
User-agent: ClaudeBot
User-agent: Claude-Web
User-agent: anthropic-ai
User-agent: CCBot
User-agent: Google-Extended
User-agent: Applebot-Extended
User-agent: PerplexityBot
User-agent: Perplexity-User
User-agent: Bytespider
User-agent: Amazonbot
User-agent: meta-externalagent
User-agent: FacebookBot
User-agent: Diffbot
User-agent: cohere-ai
User-agent: AI2Bot
User-agent: Omgilibot
User-agent: ImagesiftBot
User-agent: Timpibot
User-agent: PetalBot
User-agent: SemrushBot
User-agent: AhrefsBot
User-agent: MJ12bot
User-agent: DotBot
User-agent: DataForSeoBot
User-agent: img2dataset
Disallow: /

User-agent: *
Disallow: /api/
Disallow: /internal/
Disallow: /manus-storage/
Crawl-delay: 10
Allow: /
`;

const BLOCKED_UA = new RegExp(
	[
		"GPTBot", "ChatGPT-User", "OAI-SearchBot", "ClaudeBot", "Claude-Web", "anthropic-ai", "CCBot",
		"Google-Extended", "Applebot-Extended", "PerplexityBot", "Perplexity-User", "Bytespider", "Amazonbot",
		"meta-externalagent", "meta-externalfetcher", "FacebookBot", "Diffbot", "cohere-ai", "AI2Bot", "Omgili",
		"ImagesiftBot", "Timpibot", "YouBot", "PetalBot", "SemrushBot", "AhrefsBot", "MJ12bot", "DotBot",
		"DataForSeoBot", "BLEXBot", "magpie-crawler", "img2dataset", "Scrapy", "python-requests", "aiohttp",
		"Go-http-client", "HeadlessChrome", "PhantomJS", "zgrab", "masscan", "nikto", "sqlmap", "Nuclei",
		"curl/", "Wget",
	].join("|"),
	"i",
);

function timingSafeEqual(a: string, b: string) {
	const enc = new TextEncoder();
	const x = enc.encode(a);
	const y = enc.encode(b);
	if (x.length !== y.length) return false;
	return (crypto.subtle as unknown as { timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean }).timingSafeEqual(x, y);
}

function authorized(req: Request, env: Env) {
	const k = req.headers.get("x-lessville-key") ?? "";
	return env.INTERNAL_KEY && env.INTERNAL_KEY.length >= 32 && timingSafeEqual(k, env.INTERNAL_KEY);
}

// ------------------------------------------------------------------ model gateway

interface LlmRequest {
	task: string;
	agent: string;
	max_tokens: number;
	messages: { role: string; content: string }[];
}

async function handleLlm(req: Request, env: Env) {
	if (req.method !== "POST") return json({ error: "POST only" }, 405);
	const raw = await req.text();
	if (raw.length > 6000) return json({ error: "request too large" }, 413);
	let body: LlmRequest;
	try {
		body = JSON.parse(raw);
	} catch {
		return json({ error: "invalid JSON" }, 400);
	}
	if (!["phrase", "reflect"].includes(body.task)) return json({ error: "unknown task" }, 400);
	if (!Array.isArray(body.messages) || body.messages.length !== 2) return json({ error: "exactly one system and one user message" }, 400);
	if (body.messages[0].role !== "system" || body.messages[1].role !== "user") return json({ error: "bad roles" }, 400);
	const maxTokens = Math.min(MAX_TOKENS, Math.max(8, Number(body.max_tokens) || 40));
	const cap = Number(env.DAILY_NEURON_CAP) || 9500;
	const chars = body.messages.reduce((n, m) => n + String(m.content).length, 0);
	const estimate = (chars / 3) * NEURONS_IN + maxTokens * NEURONS_OUT;
	const budget = env.BUDGET.get(env.BUDGET.idFromName("global"));
	const r = await budget.reserve(estimate, cap);
	if (!r.ok) return json({ error: "daily neuron budget reached", budget: { used: r.used, cap } }, 429);
	try {
		const out = (await env.AI.run(MODEL as keyof AiModels, {
			messages: body.messages,
			max_tokens: maxTokens,
			temperature: body.task === "phrase" ? 0.3 : 0.5,
		} as never)) as { response?: string; usage?: { prompt_tokens?: number; completion_tokens?: number } };
		const pt = out.usage?.prompt_tokens ?? Math.ceil(chars / 3);
		const ct = out.usage?.completion_tokens ?? maxTokens;
		const neurons = pt * NEURONS_IN + ct * NEURONS_OUT;
		const used = await budget.settle(r.day, estimate, neurons);
		return json({ text: out.response ?? "", usage: { prompt_tokens: pt, completion_tokens: ct }, neurons, model: MODEL, budget: { used, cap } });
	} catch (e) {
		// Unknown consumption on failure: keep the reservation (conservative).
		return json({ error: `model call failed: ${String(e).slice(0, 200)}`, budget: { used: r.used, cap } }, 502);
	}
}

// ------------------------------------------------------------------ checkpoints

async function pruneHourly(env: Env) {
	const keep = Math.max(24, Math.min(168, Number(env.CKPT_HOURLY_KEEP) || 168));
	const listed = await env.BUCKET.list({ prefix: `${CKPT_PREFIX}hourly/`, limit: 1000 });
	const hours = listed.objects
		.map((o) => ({ key: o.key, hour: Number(/\/hourly\/(\d+)\.tar\.gz$/.exec(o.key)?.[1] ?? NaN) }))
		.filter((o) => Number.isSafeInteger(o.hour))
		.sort((a, b) => b.hour - a.hour);
	const stale = hours.slice(keep);
	await Promise.all(stale.map((o) => env.BUCKET.delete(o.key)));
	return stale.length;
}

async function handleCkpt(req: Request, env: Env, object: string) {
	if (!/^(latest|hourly\/\d{1,10})\.tar\.gz$/.test(object)) return json({ error: "bad object name" }, 400);
	const key = CKPT_PREFIX + object;
	if (req.method === "GET") {
		const o = await env.BUCKET.get(key);
		if (!o) return json({ error: "not found" }, 404);
		return new Response(o.body, { headers: { "content-type": "application/gzip", "cache-control": "no-store" } });
	}
	if (req.method === "PUT") {
		const len = Number(req.headers.get("content-length") ?? "0");
		if (!len || len > 64 * 1024 * 1024) return json({ error: "missing or oversized body" }, 413);
		const buf = await req.arrayBuffer();
		// gzip magic number: refuse to overwrite a checkpoint with anything else
		const b = new Uint8Array(buf, 0, 2);
		if (b[0] !== 0x1f || b[1] !== 0x8b) return json({ error: "not a gzip archive" }, 400);
		await env.BUCKET.put(key, buf, { httpMetadata: { contentType: "application/gzip" }, customMetadata: { written: new Date().toISOString() } });
		const pruned = object.startsWith("hourly/") ? await pruneHourly(env) : 0;
		return json({ ok: true, key, bytes: buf.byteLength, pruned });
	}
	return json({ error: "GET or PUT only" }, 405);
}

// ------------------------------------------------------------------ public API proxy

async function handleApi(req: Request, env: Env, url: URL, ctx: ExecutionContext) {
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: SECURITY_HEADERS });
  if (req.method !== "GET") {
    return json({ error: "This public world is observer-only: the clock and events run on their own so every visitor sees the same town." }, 403);
  }
  const ip = req.headers.get("cf-connecting-ip") ?? "unknown";
  const { success } = await env.RL.limit({ key: ip });
  if (!success) return json({ error: "Too many requests \u2014 slow down a little." }, 429, { "retry-after": "10" });
  const town = getContainer(env.AGENT, TOWN);
  const clean = new Request(`http://container${url.pathname}${url.search}`, { method: "GET" });
  if (url.pathname === "/api/state") {
    const store = budgetStateStore(env.BUDGET.get(env.BUDGET.idFromName("global")));
    const res = await townApi.handleState(town, clean, SECURITY_HEADERS, {
      store,
      waitUntil: ctx && typeof ctx.waitUntil === "function" ? ctx.waitUntil.bind(ctx) : void 0,
      realismPatch: realismPatchEnabled(env && env.LESSVILLE_REALISM_PATCH)
    });
    return new Response(res.body, { status: res.status, headers: res.headers });
  }
  const proxied = await townApi.handleProxied(town, clean, SECURITY_HEADERS);
  if (proxied.timedOut) {
    return new Response(proxied.body, { status: proxied.status, headers: proxied.headers });
  }
  const out = new Response(proxied.response.body, proxied.response);
  out.headers.delete("access-control-allow-origin");
  out.headers.delete("access-control-expose-headers");
  out.headers.set("cache-control", "no-store");
  for (const [k, v] of Object.entries(SECURITY_HEADERS)) out.headers.set(k, v);
  return out;
}
async function handleHealthz(env: Env) {
  const cap = Number(env.DAILY_NEURON_CAP) || 9500;
  const stub = env.BUDGET.get(env.BUDGET.idFromName("global"));
  let budget: unknown = null;
  try {
    budget = await stub.status(cap);
  } catch (e) {
    budget = { error: String(e).slice(0, 200) };
  }
  return json(await townApi.healthz(budget, budgetStateStore(stub)));
}

function simulationsDirectory() {
	const cards = SIMULATION_STAGING.map(
		(simulation) => `<article class="card"><div class="eyebrow">OBSERVER-ONLY STAGING</div><h2>${simulation.name}</h2><p>${simulation.label}</p><a href="${simulation.href}" target="_blank" rel="noopener noreferrer">Open simulation <span aria-hidden="true">↗</span></a></article>`,
	).join("");
	return withHeaders(
		new Response(
			`<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>LessVille research simulations</title><meta name="description" content="Observer-only LessVille research simulations for agent memory, trust and embodied AI."><style>:root{color-scheme:dark;--bg:#0c111a;--panel:#151e2c;--line:#34445d;--ink:#eef4ff;--muted:#aab8cc;--gold:#f2b84b;--cyan:#82dce5}*{box-sizing:border-box}body{margin:0;min-height:100vh;background:radial-gradient(circle at 20% 0,#243b54,#0c111a 58%);color:var(--ink);font:16px/1.55 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}.wrap{max-width:1120px;margin:auto;padding:clamp(24px,6vw,72px) 18px}.eyebrow{color:var(--cyan);font-size:11px;letter-spacing:.16em;font-weight:700}h1{font-size:clamp(34px,7vw,72px);line-height:1.02;letter-spacing:-.06em;margin:12px 0 18px}h2{margin:9px 0;font-size:22px}p{color:var(--muted);margin:0 0 18px}.intro{max-width:730px}.notice{border-left:3px solid var(--gold);background:#2b271c;color:#f4dfad;padding:13px 15px;margin:24px 0 28px;font-size:13px}.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:13px}.card{background:#151e2ceb;border:1px solid var(--line);border-radius:15px;padding:18px;min-height:190px;display:flex;flex-direction:column}.card p{font-size:14px}.card a{margin-top:auto;color:var(--gold);font-weight:700;text-decoration:none}.card a:hover{text-decoration:underline}.back{display:inline-block;margin-top:28px;color:var(--cyan);text-decoration:none}@media(max-width:520px){.wrap{padding-top:32px}.card{min-height:0}}</style></head><body><main class="wrap"><div class="eyebrow">LESSVILLE / LESSDB RESEARCH MAP</div><h1>Explore the living experiments.</h1><p class="intro">LessVille is the fictional town simulation. These companion surfaces show how the same agent-first ideas extend into security, verification, trust and embodied AI.</p><div class="notice"><strong>Staging boundary:</strong> the Security Lab, Reachy Mini and MicroDuck surfaces are temporary public research staging. They are observer-only, use deterministic or simulator-backed policies, and do not expose robot controls, actuator commands or public mutation routes.</div><section class="cards" aria-label="Research simulations">${cards}</section><a class="back" href="/">← Return to LessVille</a></main></body></html>`,
			{ headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-cache" } },
		),
	);
}

async function rootWithSimulationLink(req: Request, env: Env) {
	const response = await env.ASSETS.fetch(req);
	if (!response.ok) return withHeaders(response);
	const html = await response.text();
	const marker = '<section id="loading" role="status">';
	const discovery = '<div id="research-simulations-link" style="max-width:620px;justify-self:center;font-size:12px;line-height:1.45;color:#aab8cc"><a href="/simulations" style="color:#f2b84b;font-weight:700;text-decoration:none">Explore the live research simulations →</a><br><span>Security, Reachy Mini and MicroDuck are observer-only staging surfaces.</span></div>';
	const body = html.includes(marker) && !html.includes('id="research-simulations-link"') ? html.replace(marker, `${marker}${discovery}`) : html;
	const headers = new Headers(response.headers);
	headers.set("content-type", "text/html; charset=utf-8");
	headers.set("cache-control", "no-cache");
	return withHeaders(new Response(body, { status: response.status, headers }));
}

export default {
	async fetch(req: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
		const url = new URL(req.url);
		try {
			if (url.pathname === "/robots.txt") {
				return withHeaders(new Response(ROBOTS_TXT, { headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "public, max-age=3600" } }));
			}
			const ua = req.headers.get("user-agent") ?? "";
			if (!url.pathname.startsWith("/internal/") && (ua === "" || BLOCKED_UA.test(ua))) {
				return withHeaders(new Response("Automated access to this site is not permitted.\n", { status: 403, headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" } }));
			}
			if (url.pathname === "/internal/llm" || url.pathname.startsWith("/internal/ckpt/")) {
				if (!authorized(req, env)) return json({ error: "not found" }, 404);
				if (url.pathname === "/internal/llm") return handleLlm(req, env);
				return handleCkpt(req, env, url.pathname.slice("/internal/ckpt/".length));
			}
			if (url.pathname.startsWith("/internal/")) return json({ error: "not found" }, 404);
				if (url.pathname === "/api/healthz") return handleHealthz(env);
				if (url.pathname === "/api/budget") {
					const cap = Number(env.DAILY_NEURON_CAP) || 9500;
					return json(await env.BUDGET.get(env.BUDGET.idFromName("global")).status(cap));
				}
				if (url.pathname === "/simulations" || url.pathname === "/simulations.html") return simulationsDirectory();
				if (url.pathname.startsWith("/api/")) return handleApi(req, env, url, ctx);
				if (url.pathname.startsWith("/manus-storage/")) return handleBinary(req, env, url.pathname);
				if (url.pathname === "/") return rootWithSimulationLink(req, env);
				return withHeaders(await env.ASSETS.fetch(req));
		} catch (e) {
			console.error("request failed", url.pathname, e);
			return json({ error: "The town is waking up — try again in a few seconds." }, 503, { "retry-after": "5" });
		}
	},

	async scheduled(_c: ScheduledController, env: Env, ctx: ExecutionContext) {
		const store = budgetStateStore(env.BUDGET.get(env.BUDGET.idFromName("global")));
		ctx.waitUntil(townApi.runKeepalive(getContainer(env.AGENT, TOWN), console, store));
	},
} satisfies ExportedHandler<Env>;

// ------------------------------------------------------------------ game binaries

// The verified release references its engine and pack by content hash. They exceed the
// static-asset size limit, so they live in R2 under the same hash and are served immutably.
const BINARY = /^\/manus-storage\/game\/sha256\/([0-9a-f]{64})\.(wasm|pck)$/;

async function handleBinary(req: Request, env: Env, path: string) {
	const m = BINARY.exec(path);
	if (!m || (req.method !== "GET" && req.method !== "HEAD")) return json({ error: "not found" }, 404);
	const headers = new Headers(SECURITY_HEADERS);
	headers.set("content-type", m[2] === "wasm" ? "application/wasm" : "application/octet-stream");
	headers.set("cache-control", "public, max-age=31536000, immutable");
	const key = `lessville/game/sha256/${m[1]}.${m[2]}`;
	const obj = await env.BUCKET.get(key);
	if (!obj) return assembledBinary(req, env, key, m[1], headers);
	headers.set("etag", obj.httpEtag);
	headers.set("content-length", String(obj.size));
	return new Response(req.method === "HEAD" ? null : obj.body, { headers });
}

// Large packs may be uploaded as ordered parts plus a manifest (`<key>.parts.json`)
// when a single upload cannot complete. The parts are joined and served only if the
// result's sha256 equals the hash in the path, so content addressing still holds.
const MAX_ASSEMBLED = 64 * 1024 * 1024;
async function assembledBinary(req: Request, env: Env, key: string, hash: string, headers: Headers) {
	const cache = (caches as unknown as { default: Cache }).default;
	const cacheKey = new Request(new URL(req.url).toString(), { method: "GET" });
	const hit = await cache.match(cacheKey);
	if (hit) return req.method === "HEAD" ? new Response(null, hit) : hit;
	const man = await env.BUCKET.get(`${key}.parts.json`);
	if (!man) return json({ error: "not found" }, 404);
	let parts: string[] = [];
	try {
		const parsed = JSON.parse(await man.text()) as { parts?: unknown };
		if (Array.isArray(parsed.parts)) parts = parsed.parts.filter((p): p is string => typeof p === "string");
	} catch {
		return json({ error: "not found" }, 404);
	}
	if (parts.length === 0 || parts.length > 64 || parts.some((p) => !p.startsWith(`${key}.part-`))) return json({ error: "not found" }, 404);
	const buffers: ArrayBuffer[] = [];
	let total = 0;
	for (const p of parts) {
		const o = await env.BUCKET.get(p);
		if (!o) return json({ error: "not found" }, 404);
		const b = await o.arrayBuffer();
		total += b.byteLength;
		if (total > MAX_ASSEMBLED) return json({ error: "not found" }, 404);
		buffers.push(b);
	}
	const joined = new Uint8Array(total);
	let off = 0;
	for (const b of buffers) {
		joined.set(new Uint8Array(b), off);
		off += b.byteLength;
	}
	const digest = [...new Uint8Array(await crypto.subtle.digest("SHA-256", joined))].map((x) => x.toString(16).padStart(2, "0")).join("");
	if (digest !== hash) return json({ error: "integrity check failed" }, 502);
	headers.set("etag", `"${hash}"`);
	headers.set("content-length", String(total));
	const res = new Response(joined, { headers });
	await cache.put(cacheKey, res.clone());
	return req.method === "HEAD" ? new Response(null, { headers }) : res;
}

function withHeaders(res: Response) {
	const out = new Response(res.body, res);
	for (const [k, v] of Object.entries(SECURITY_HEADERS)) out.headers.set(k, v);
	return out;
}
