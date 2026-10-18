// Applies the colour theme before the first paint (no flash of the wrong theme).
// Loaded as a same-origin file so the Content-Security-Policy needs no inline script.
// Keep the storage key in sync with src/lib/theme.ts.
(function () {
  "use strict";
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
