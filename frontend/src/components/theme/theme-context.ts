import { createContext, use } from "react";

import type { ResolvedTheme, ThemePreference } from "@/lib/theme";

export interface ThemeContextValue {
  /** What the visitor chose; "system" follows the operating system setting. */
  preference: ThemePreference;
  /** The theme actually displayed. */
  resolvedTheme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
  /** Switches between light and dark; returns to "system" when the choice matches the OS. */
  toggleTheme: () => void;
}

export const ThemeContext = createContext<ThemeContextValue | null>(null);

export function useTheme(): ThemeContextValue {
  const context = use(ThemeContext);
  if (!context) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}
