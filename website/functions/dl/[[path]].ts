// Serves release artifacts from the lessdb-downloads R2 bucket at /dl/<key>.
interface Env {
  DOWNLOADS: R2Bucket;
}

const TYPES: Record<string, string> = {
  ".tar.gz": "application/gzip",
  ".sha256": "text/plain",
  ".json": "application/json",
};

export const onRequestGet: PagesFunction<Env> = async ({ params, env }) => {
  const key = (params.path as string[]).join("/");
  if (!key || key.includes("..")) {
    return new Response("not found", { status: 404 });
  }
  const obj = await env.DOWNLOADS.get(key);
  if (obj === null) {
    return new Response("not found", { status: 404 });
  }
  const headers = new Headers();
  const ext = Object.keys(TYPES).find((e) => key.endsWith(e));
  if (ext) headers.set("content-type", TYPES[ext]);
  headers.set("cache-control", "public, max-age=604800");
  headers.set("access-control-allow-origin", "*");
  headers.set("etag", obj.httpEtag);
  return new Response(obj.body as ReadableStream, { headers });
};
