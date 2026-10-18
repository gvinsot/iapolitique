import "@testing-library/jest-dom/vitest";

import { cleanup, configure } from "@testing-library/react";
import { afterEach, beforeEach } from "vitest";

import { installBrowserMocks } from "./browser-mocks";

// findBy*/waitFor default to 1 s, too short on a loaded CI builder (see testTimeout in vite.config.ts).
configure({ asyncUtilTimeout: 10_000 });

beforeEach(() => {
  installBrowserMocks();
});

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(window.navigator, "clipboard");
  window.localStorage.clear();
  window.sessionStorage.clear();
  document.documentElement.className = "";
  document.documentElement.removeAttribute("style");
});
