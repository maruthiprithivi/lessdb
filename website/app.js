/* ==========================================================================
   LESSDB — website interactions
   LUMO the firefly · pixel-art renderer + terminal/counter/tab animations.
   ========================================================================== */
(() => {
  "use strict";

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const finePointer = window.matchMedia("(pointer: fine)").matches;

  /* ------------------------------------------------------------------------
   PIXEL-ART MASCOT — "LUMO" the firefly
   21×24 grid. Keys:
     D dark frame · H head · h head shade · E eye · L antenna tip · S scarf
     W wing · B body · G glow abdomen · g glow core · . empty
   ------------------------------------------------------------------------ */
  const PIXEL_MAP = [
    ".....................",
    ".......L.....L.......",
    ".......D.....D.......",
    "......D.......D......",
    ".....DDDDDDDDDDD.....",
    "....DHHHHHHHHHHHD....",
    "....DHHHEHHHEHHHD....",
    "....DHHHEHHHEHHHD....",
    "....DHHHHHHHHHHHD....",
    ".....DhHHHHHHHHhD....",
    "......DDDDDDDDD......",
    ".......SSSSSSS.......",
    "......SSSSSSSSS......",
    ".WWW..DDDDDDDDD..WWW.",
    "WWWW.WDDDDDDDDDW.WWWW",
    "WWWWW.BBBBBBBBB.WWWWW",
    ".WWWW.BBBBBBBBB.WWWW.",
    "..WWW.BBBBBBBBB.WWW..",
    "......BBBBBBBBB......",
    ".....BBGGGGGGGBB.....",
    ".....BGgGGGGGgGB.....",
    ".....BGGgGGGgGGB.....",
    "......GGGGGGGGG......",
    "........ggg..........",
  ];

  const PIXEL_COLORS = {
    D: "#2e2e26", H: "#e8e6d8", h: "#9a988a", E: "#f6f09c",
    L: "#ffe14a", S: "#f25533", W: "#6fd7c0", B: "#3a3a32",
    G: "#ffe14a", g: "#fff8b0",
  };

  const PIXEL_ANIM = {
    E: "px-eye", L: "px-tip", G: "px-glow", g: "px-core", W: "px-wing",
  };

  const renderCount = { value: 0 };

  function renderMascot(targetId, cell) {
    const svg = document.getElementById(targetId);
    if (!svg) return;
    const NS = "http://www.w3.org/2000/svg";
    svg.setAttribute("shape-rendering", "crispEdges");

    if (renderCount.value === 0) {
      const style = document.createElementNS(NS, "style");
      style.textContent =
        ".px-eye{animation:eyeBlink 5.2s steps(1) infinite;}" +
        ".px-tip{animation:tipBlink 2.6s ease-in-out infinite;}" +
        ".px-glow{animation:abdomenPulse 2.4s ease-in-out infinite;}" +
        ".px-core{animation:coreFlicker 1.7s ease-in-out infinite;}" +
        ".px-wing{animation:wingFlap 3.4s ease-in-out infinite;}" +
        "@keyframes eyeBlink{0%,90.5%,100%{opacity:1}93%,96%{opacity:.1}}" +
        "@keyframes tipBlink{0%,100%{opacity:1}50%{opacity:.35}}" +
        "@keyframes abdomenPulse{0%,100%{opacity:.82}50%{opacity:1}}" +
        "@keyframes coreFlicker{0%,100%{opacity:1}40%{opacity:.75}60%{opacity:.95}}" +
        "@keyframes wingFlap{0%,100%{opacity:.55}50%{opacity:.92}}";
      svg.appendChild(style);
      renderCount.value = 1;
    }

    const width = PIXEL_MAP[0].length;
    const height = PIXEL_MAP.length;
    svg.setAttribute("viewBox", `0 0 ${width * cell} ${height * cell}`);

    PIXEL_MAP.forEach((row, y) => {
      [...row].forEach((key, x) => {
        if (key === "." || !PIXEL_COLORS[key]) return;
        const rect = document.createElementNS(NS, "rect");
        rect.setAttribute("x", x * cell);
        rect.setAttribute("y", y * cell);
        rect.setAttribute("width", cell);
        rect.setAttribute("height", cell);
        rect.setAttribute("fill", PIXEL_COLORS[key]);
        if (PIXEL_ANIM[key]) rect.classList.add(PIXEL_ANIM[key]);
        svg.appendChild(rect);
      });
    });
  }

  renderMascot("mascotLarge", 10); // 21×24 → 210×240
  renderMascot("mascotMini", 1);
  renderMascot("mascotBrand", 1);
  renderMascot("mascotFooter", 1);
  renderMascot("mascotCore", 1);

  /* ------------------------------------------------------------------------
   HERO — rotating word swap
   ------------------------------------------------------------------------ */
  const swapVisible = document.getElementById("swapVisible");
  const swapWords = [...document.querySelectorAll("#swapStack .swap-word")]
    .map((el) => el.textContent.trim())
    .filter(Boolean);

  if (swapVisible && swapWords.length > 1 && !reduceMotion) {
    let idx = 0;
    setInterval(() => {
      swapVisible.classList.add("is-out");
      setTimeout(() => {
        idx = (idx + 1) % swapWords.length;
        swapVisible.textContent = swapWords[idx];
        swapVisible.classList.remove("is-out");
      }, 230);
    }, 2300);
  }

  /* ------------------------------------------------------------------------
   HERO — firefly particles
   ------------------------------------------------------------------------ */
  const heroBg = document.getElementById("heroBg");
  if (heroBg && !reduceMotion) {
    const count = finePointer ? 16 : 9;
    for (let i = 0; i < count; i++) {
      const f = document.createElement("span");
      f.className = "firefly";
      const size = 2 + Math.random() * 3;
      f.style.width = size + "px";
      f.style.height = size + "px";
      f.style.left = 4 + Math.random() * 92 + "%";
      f.style.top = 8 + Math.random() * 80 + "%";
      f.style.animationDuration = 7 + Math.random() * 9 + "s";
      f.style.animationDelay = -Math.random() * 12 + "s";
      heroBg.appendChild(f);
    }
  }

  /* ------------------------------------------------------------------------
   HERO — parallax
   ------------------------------------------------------------------------ */
  const hero = document.getElementById("hero");
  const heroBot = document.getElementById("heroBot");
  const glows = document.querySelectorAll(".hero-glow");

  if (hero && heroBot && !reduceMotion && finePointer) {
    let raf = null;
    hero.addEventListener("mousemove", (e) => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        const r = hero.getBoundingClientRect();
        const x = (e.clientX - r.left) / r.width - 0.5;
        const y = (e.clientY - r.top) / r.height - 0.5;
        heroBot.style.transform = `translateY(-50%) translate(${x * -26}px, ${y * -18}px)`;
        glows.forEach((g, i) => {
          const f = i === 0 ? 24 : -16;
          g.style.translate = `${x * f}px ${y * f}px`;
        });
        raf = null;
      });
    });
  }

  /* ------------------------------------------------------------------------
   DEMO TERMINAL — loop the session lines
   ------------------------------------------------------------------------ */
  const demoBody = document.getElementById("demoBody");
  if (demoBody) {
    const lines = [...demoBody.querySelectorAll(".t-line")];
    const cursorLine = demoBody.querySelector(".t-cursor-line");
    const lineDelay = reduceMotion ? 80 : 520;
    const loopPause = reduceMotion ? 400 : 4200;

    function resetLines() {
      lines.forEach((l) => l.classList.remove("is-in"));
      if (cursorLine) cursorLine.style.opacity = "0";
    }

    function runSession() {
      resetLines();
      let t = 400;
      lines.forEach((line, i) => {
        t += lineDelay + line.textContent.length * 1.4;
        setTimeout(() => line.classList.add("is-in"), t);
      });
      setTimeout(() => {
        if (cursorLine) cursorLine.style.opacity = "1";
      }, t + lineDelay);
      setTimeout(runSession, t + lineDelay + loopPause);
    }

    runSession();
  }

  /* ------------------------------------------------------------------------
   STAT COUNTERS
   ------------------------------------------------------------------------ */
  const counters = document.querySelectorAll(".stat-count");
  const countObserver = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) return;
        const el = entry.target;
        countObserver.unobserve(el);
        animateCount(el);
      });
    },
    { threshold: 0.6 }
  );

  function animateCount(el) {
    const target = parseFloat(el.dataset.count);
    const decimals = parseInt(el.dataset.decimals || "0", 10);
    const duration = reduceMotion ? 0 : 1600;
    const start = performance.now();

    function frame(now) {
      const p = Math.min((now - start) / duration, 1);
      const eased = 1 - Math.pow(1 - p, 3);
      el.textContent = (target * eased).toFixed(decimals);
      if (p < 1) requestAnimationFrame(frame);
      else el.textContent = target.toFixed(decimals);
    }
    if (reduceMotion) {
      el.textContent = target.toFixed(decimals);
    } else {
      requestAnimationFrame(frame);
    }
  }
  counters.forEach((c) => countObserver.observe(c));

  /* ------------------------------------------------------------------------
   INSTALL TABS + COPY
   ------------------------------------------------------------------------ */
  const INSTALL_COMMANDS = {
    curl: "curl -fsSL https://lessdb.dev/install.sh | sh",
    brew: "brew tap lessdb/lessdb && brew install lessdb",
    npm: "npm install -g lessdb",
  };

  const installCommand = document.getElementById("installCommand");
  const copyBtn = document.getElementById("copyBtn");
  const tabs = document.querySelectorAll(".install-tab");

  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      tabs.forEach((t) => t.classList.toggle("is-active", t === tab));
      const cmd = INSTALL_COMMANDS[tab.dataset.method];
      if (installCommand && cmd) {
        installCommand.innerHTML =
          '<span class="term-prompt">$</span> ' + escapeHtml(cmd);
      }
    });
  });

  function escapeHtml(s) {
    return s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
  }

  if (copyBtn) {
    copyBtn.addEventListener("click", async () => {
      const active = document.querySelector(".install-tab.is-active");
      const cmd = active ? INSTALL_COMMANDS[active.dataset.method] : "";
      if (!cmd) return;
      try {
        await navigator.clipboard.writeText(cmd);
        copyBtn.classList.add("is-copied");
        copyBtn.querySelector("span").textContent = "COPIED";
        setTimeout(() => {
          copyBtn.classList.remove("is-copied");
          copyBtn.querySelector("span").textContent = "COPY";
        }, 1600);
      } catch (err) {
        /* clipboard unavailable — ignore */
      }
    });
  }

  /* ------------------------------------------------------------------------
   TICKER — duplicate chips for a seamless loop
   ------------------------------------------------------------------------ */
  const track = document.getElementById("agentTrack");
  if (track && !reduceMotion) {
    track.innerHTML += track.innerHTML;
  }

  /* ------------------------------------------------------------------------
   HEADER — scroll state + mobile nav
   ------------------------------------------------------------------------ */
  const header = document.getElementById("siteHeader");
  const navToggle = document.getElementById("navToggle");
  const mobileNav = document.getElementById("mobileNav");

  if (header) {
    const onScroll = () =>
      header.classList.toggle("is-scrolled", window.scrollY > 12);
    window.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
  }

  if (navToggle && mobileNav) {
    navToggle.addEventListener("click", () => {
      const open = mobileNav.classList.toggle("is-open");
      navToggle.classList.toggle("is-open", open);
      navToggle.setAttribute("aria-expanded", String(open));
    });
    mobileNav.querySelectorAll("a").forEach((a) =>
      a.addEventListener("click", () => {
        mobileNav.classList.remove("is-open");
        navToggle.classList.remove("is-open");
        navToggle.setAttribute("aria-expanded", "false");
      })
    );
  }

  /* ------------------------------------------------------------------------
   REVEAL ON SCROLL
   ------------------------------------------------------------------------ */
  const revealEls = document.querySelectorAll(".reveal");
  if ("IntersectionObserver" in window) {
    const revealObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            revealObserver.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.08 }
    );
    revealEls.forEach((el, i) => {
      el.style.setProperty("--reveal-delay", (i % 6) * 60 + "ms");
      revealObserver.observe(el);
    });
  } else {
    revealEls.forEach((el) => el.classList.add("is-visible"));
  }

  // Belt-and-braces: anything still hidden while inside the viewport after
  // load gets revealed, so above-the-fold content can never stay invisible.
  window.addEventListener("load", () => {
    requestAnimationFrame(() => {
      revealEls.forEach((el) => {
        if (el.classList.contains("is-visible")) return;
        const r = el.getBoundingClientRect();
        if (r.top < window.innerHeight && r.bottom > 0) {
          el.classList.add("is-visible");
        }
      });
    });
  });
})();
