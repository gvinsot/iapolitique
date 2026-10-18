/**
 * Colour theme handling. The preference lives in localStorage only (never a cookie) and every
 * storage access is guarded: private browsing or blocked storage must never break the page.
 *
 * Keep THEME_STORAGE_KEY in sync with public/theme-init.js, which applies the theme before the
 * first paint to avoid a flash of the wrong theme.
 */

export const THEME_STORAGE_KEY = "iapolitique-theme";

export type ResolvedTheme = "light" | "dark";
export type ThemePreference = ResolvedTheme | "system";

const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";

function isResolvedTheme(value: unknown): value is ResolvedTheme {
  return value === "light" || value === "dark";
}

export function readThemePreference(): ThemePreference {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isResolvedTheme(stored) ? stored : "system";
  } catch {
    return "system";
  }
}

export function writeThemePreference(preference: ThemePreference): void {
  try {
    if (preference === "system") {
      window.localStorage.removeItem(THEME_STORAGE_KEY);
    } else {
      window.localStorage.setItem(THEME_STORAGE_KEY, preference);
    }
  } catch {
    // Storage unavailable (private mode, blocked site data): the choice lasts for this page only.
  }
}

function darkSchemeQuery(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia(DARK_SCHEME_QUERY) : null;
}

export function getSystemTheme(): ResolvedTheme {
  return darkSchemeQuery()?.matches ? "dark" : "light";
}

export function subscribeToSystemTheme(onChange: () => void): () => void {
  const query = darkSchemeQuery();
  if (!query) return () => undefined;
  query.addEventListener("change", onChange);
  return () => {
    query.removeEventListener("change", onChange);
  };
}

export function resolveTheme(preference: ThemePreference, system: ResolvedTheme): ResolvedTheme {
  return preference === "system" ? system : preference;
}

export function applyTheme(theme: ResolvedTheme, root: HTMLElement = document.documentElement) {
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme;
}

/** Reads a preference written by another tab (storage event payload). */
export function parseStoredPreference(value: string | null): ThemePreference {
  return isResolvedTheme(value) ? value : "system";
}
