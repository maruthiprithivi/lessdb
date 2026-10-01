// LessDB's npm distribution — served from Cloudflare, not registry.npmjs.org.
//
//   GET /npm/lessdb                        -> the packument (registry metadata)
//   GET /npm/lessdb/-/lessdb-<ver>.tgz     -> the packed package tarball
//
// Both are stored in the lessdb-downloads R2 bucket under keys
// `npm/lessdb` and `npm/lessdb-<ver>.tgz` (written by scripts/package-npm.sh).
// Install with:
//
//   npm install -g lessdb --registry https://lessdb.dev/npm/
//   npm install -g https://lessdb.dev/npm/lessdb/-/lessdb-0.2.0.tgz
interface Env {
  DOWNLOADS: R2Bucket;
}

const PACKUMENT_KEYS = new Set(["lessdb", "lessdb.json", "lessdb/index.json"]);

export const onRequestGet: PagesFunction<Env> = async ({ params, env }) => {
  const parts = (params.path as string[]) || [];
  const key = parts.join("/");

  if (PACKUMENT_KEYS.has(key)) {
    const obj = await env.DOWNLOADS.get("npm/lessdb");
    if (obj === null) {
      return new Response('{"error":"not_found"}', {
        status: 404,
        headers: { "content-type": "application/json" },
      });
    }
    return new Response(obj.body as ReadableStream, {
      headers: {
        "content-type": "application/json",
        "cache-control": "public, max-age=300",
        "access-control-allow-origin": "*",
        etag: obj.httpEtag,
      },
    });
  }

  // Tarball: /npm/lessdb/-/lessdb-<ver>.tgz  (npm always fetches it from
  // dist.tarball, so any path ending in the package's .tgz name works).
  if (key.endsWith(".tgz")) {
    const file = parts[parts.length - 1];
    if (!/^[A-Za-z0-9._-]+\.tgz$/.test(file) || file.includes("..")) {
      return new Response("not found", { status: 404 });
    }
    const obj = await env.DOWNLOADS.get(`npm/${file}`);
    if (obj === null) {
      return new Response("not found", { status: 404 });
    }
    return new Response(obj.body as ReadableStream, {
      headers: {
        "content-type": "application/gzip",
        "cache-control": "public, max-age=604800",
        "access-control-allow-origin": "*",
        etag: obj.httpEtag,
      },
    });
  }

  return new Response("not found", { status: 404 });
};
