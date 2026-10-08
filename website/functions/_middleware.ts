// Edge guard for lessdb.dev: refuse known AI-training crawlers and bulk
// scrapers before they reach static assets or download functions.
// robots.txt asks politely; this enforces it for bots that identify themselves.
// Bots that spoof a browser user agent are out of scope here and are handled by
// Cloudflare's zone-level bot protection.

const BLOCKED_UA = new RegExp(
  [
    "GPTBot", "ChatGPT-User", "OAI-SearchBot", "ClaudeBot", "Claude-Web",
    "anthropic-ai", "CCBot", "Google-Extended", "Applebot-Extended",
    "PerplexityBot", "Perplexity-User", "Bytespider", "Amazonbot",
    "meta-externalagent", "meta-externalfetcher", "FacebookBot", "Diffbot",
    "cohere-ai", "cohere-training-data-crawler", "AI2Bot", "Omgili",
    "ImagesiftBot", "Timpibot", "YouBot", "PetalBot", "SemrushBot",
    "AhrefsBot", "MJ12bot", "DotBot", "DataForSeoBot", "BLEXBot",
    "magpie-crawler", "img2dataset", "Scrapy", "python-requests",
    "aiohttp", "Go-http-client", "HeadlessChrome", "PhantomJS",
    "zgrab", "masscan", "nikto", "sqlmap", "Nuclei",
  ].join("|"),
  "i",
);

// curl/wget are allowed: the documented installer uses them.
export const onRequest: PagesFunction = async (ctx) => {
  const url = new URL(ctx.request.url);
  if (url.pathname === "/robots.txt") return ctx.next();
  const ua = ctx.request.headers.get("user-agent") || "";
  if (ua === "" || BLOCKED_UA.test(ua)) {
    return new Response("Automated access to this site is not permitted.\n", {
      status: 403,
      headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" },
    });
  }
  return ctx.next();
};
