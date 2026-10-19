// Runs before the first paint, from a same-origin file (the Content-Security-Policy allows no
// inline script):
// 1. applies the colour theme (no flash of the wrong theme); keep the storage key in sync with
//    src/lib/theme.ts;
// 2. hands the CSP nonce that nginx puts on this script tag (frontend/nginx.conf) to the
//    libraries that inject <style> elements at run time (react-style-singleton, used by the
//    Radix dialogs and selects, reads window.__webpack_nonce__). No nonce in development.
(function () {
  "use strict";
  var script = document.currentScript;
  if (script && script.nonce) {
    window.__webpack_nonce__ = script.nonce;
  }

  var preference = null;
  try {
    preference = window.localStorage.getItem("iapolitique-theme");
  } catch {
    preference = null;
  }
  var prefersDark =
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches;
  var dark = preference === "dark" || (preference !== "light" && prefersDark);
  var root = document.documentElement;
  root.classList.toggle("dark", dark);
  root.style.colorScheme = dark ? "dark" : "light";
})();
