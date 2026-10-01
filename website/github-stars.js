/* Public repository stars: one unauthenticated request, no dependencies. */
(() => {
  "use strict";
  const nodes = document.querySelectorAll("[data-github-stars]");
  if (!nodes.length) return;
  const key = "lessdb-github-stars-v1";
  const ttl = 60 * 60 * 1000;
  function render(count, cached) {
    for (const node of nodes) {
      node.textContent = `★ ${count.toLocaleString("en-US")}`;
      node.setAttribute("aria-label", `${count} GitHub stars${cached ? " (cached)" : ""}`);
      node.title = cached ? "GitHub stars (cached for up to one hour)" : "GitHub stars";
    }
  }
  function valid(count) { return Number.isSafeInteger(count) && count >= 0; }
  try {
    const cached = JSON.parse(localStorage.getItem(key));
    if (cached && valid(cached.count) && Number.isFinite(cached.at) &&
        Date.now() >= cached.at && Date.now() - cached.at < ttl) {
      render(cached.count, true);
      return;
    }
  } catch (_) { /* Storage may be disabled; the link still works. */ }
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 3000);
  fetch("https://api.github.com/repos/lessdb/lessdb", {
    signal: controller.signal,
    credentials: "omit",
    headers: { Accept: "application/vnd.github+json" },
  }).then(response => {
    if (!response.ok) throw new Error("GitHub unavailable");
    return response.json();
  }).then(repo => {
    if (repo.private !== false || repo.full_name !== "lessdb/lessdb" || !valid(repo.stargazers_count)) {
      throw new Error("Unexpected repository response");
    }
    render(repo.stargazers_count, false);
    try { localStorage.setItem(key, JSON.stringify({count: repo.stargazers_count, at: Date.now()})); }
    catch (_) { /* No storage required. */ }
  }).catch(() => {
    for (const node of nodes) {
      node.textContent = "";
      node.setAttribute("aria-label", "Star count unavailable");
    }
  }).finally(() => clearTimeout(timeout));
})();
