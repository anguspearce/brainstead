// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// The tutorials: scroll reveals, the list of steps following along, and the Mac or Windows switch.
(() => {
  // A screenshot's frame shimmers until its image has loaded.
  document.querySelectorAll(".window img").forEach((img) => {
    const frame = img.closest(".window");
    const done = () => frame.classList.add("loaded");
    if (img.complete && img.naturalWidth) done();
    else {
      img.addEventListener("load", done, { once: true });
      img.addEventListener("error", done, { once: true });
    }
  });

  // Fade things in as they scroll into view.
  const reveals = new IntersectionObserver(
    (entries) => {
      for (const e of entries)
        if (e.isIntersecting) {
          e.target.classList.add("in");
          reveals.unobserve(e.target);
        }
    },
    { rootMargin: "0px 0px -8% 0px" },
  );
  document.querySelectorAll(".reveal").forEach((el) => reveals.observe(el));

  // The step being read is marked in the list beside it, and the bar under the list fills up.
  const links = [...document.querySelectorAll(".toc a")];
  const lessons = links.map((a) => document.querySelector(a.getAttribute("href")));
  const bar = document.querySelector(".toc .bar i");
  if (links.length) {
    let ticking = false;
    const frame = () => {
      ticking = false;
      const mid = innerHeight * 0.35;
      let at = 0;
      lessons.forEach((l, i) => {
        if (l.getBoundingClientRect().top < mid) at = i;
      });
      links.forEach((a, i) => a.classList.toggle("on", i === at));
      const first = lessons[0].getBoundingClientRect().top;
      const last = lessons[lessons.length - 1].getBoundingClientRect().bottom;
      const p = Math.min(1, Math.max(0, (mid - first) / (last - first)));
      bar.style.width = `${p * 100}%`;
    };
    const onScroll = () => {
      if (!ticking) {
        ticking = true;
        requestAnimationFrame(frame);
      }
    };
    addEventListener("scroll", onScroll, { passive: true });
    addEventListener("resize", onScroll);
    frame();
  }

  // Mac or Windows, chosen at the top: the install steps for it, and every key (and the odd
  // word, such as menu bar) as it is there. It starts as the computer you're on, and a choice is remembered for next time.
  const seg = document.querySelector(".pick .seg");
  if (seg) {
    const buttons = [...seg.querySelectorAll("button")];
    const keys = [...document.querySelectorAll("[data-win]")];
    keys.forEach((k) => (k.dataset.mac = k.textContent));
    const pick = (os) => {
      document.documentElement.dataset.os = os;
      buttons.forEach((b) => b.setAttribute("aria-selected", String(b.dataset.os === os)));
      seg.dataset.mode = os === "win" ? "1" : "0";
      keys.forEach((k) => (k.textContent = os === "win" ? k.dataset.win : k.dataset.mac));
    };
    let saved = null;
    try {
      saved = localStorage.getItem("tutorial-os");
    } catch {
      // No storage (a private window): start from the computer you're on.
    }
    pick(saved || (/Windows/.test(navigator.userAgent) ? "win" : "mac"));
    buttons.forEach((b) =>
      b.addEventListener("click", () => {
        pick(b.dataset.os);
        try {
          localStorage.setItem("tutorial-os", b.dataset.os);
        } catch {
          // No storage: the choice holds for this visit only.
        }
      }),
    );
  }
})();
