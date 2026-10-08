/* Project metrics are intentionally not fetched from a personal account. */
(() => {
  "use strict";
  for (const node of document.querySelectorAll("[data-github-stars]")) {
    node.textContent = "";
    node.setAttribute("aria-label", "Project metrics unavailable");
    node.title = "Project metrics unavailable";
  }
})();
