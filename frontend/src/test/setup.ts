import "@testing-library/jest-dom/vitest";

import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach } from "vitest";

import { installBrowserMocks } from "./browser-mocks";

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
