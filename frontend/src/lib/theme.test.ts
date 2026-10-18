import { describe, expect, it, vi } from "vitest";

import { mockColorScheme } from "@/test/browser-mocks";

import {
  applyTheme,
  getSystemTheme,
  readThemePreference,
  resolveTheme,
  THEME_STORAGE_KEY,
  writeThemePreference,
} from "./theme";

describe("theme preference storage", () => {
  it("defaults to the system preference", () => {
    expect(readThemePreference()).toBe("system");
  });

  it("round-trips an explicit choice and forgets it when back to system", () => {
    writeThemePreference("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
    expect(readThemePreference()).toBe("dark");

    writeThemePreference("system");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBeNull();
    expect(readThemePreference()).toBe("system");
  });

  it("ignores unexpected stored values", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "purple");
    expect(readThemePreference()).toBe("system");
  });

  it("survives a storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError");
    });

    expect(readThemePreference()).toBe("system");
    expect(() => {
      writeThemePreference("dark");
    }).not.toThrow();
  });
});

describe("theme resolution", () => {
  it("follows the operating system when no choice was made", () => {
    mockColorScheme(true);
    expect(getSystemTheme()).toBe("dark");
    expect(resolveTheme("system", "dark")).toBe("dark");
    expect(resolveTheme("light", "dark")).toBe("light");
  });

  it("applies the dark class and color-scheme on the root element", () => {
    const root = document.createElement("html");
    applyTheme("dark", root);
    expect(root).toHaveClass("dark");
    expect(root.style.colorScheme).toBe("dark");

    applyTheme("light", root);
    expect(root).not.toHaveClass("dark");
    expect(root.style.colorScheme).toBe("light");
  });
});
