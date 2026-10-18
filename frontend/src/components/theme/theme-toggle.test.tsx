import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { AppProviders } from "@/app/providers";
import { THEME_STORAGE_KEY } from "@/lib/theme";
import { mockColorScheme } from "@/test/browser-mocks";

import { ThemeToggle } from "./theme-toggle";

function renderToggle() {
  return render(
    <AppProviders>
      <ThemeToggle />
    </AppProviders>,
  );
}

describe("ThemeToggle", () => {
  it("switches to dark then back to light, persisting the choice", async () => {
    const user = userEvent.setup();
    renderToggle();
    const root = document.documentElement;
    expect(root).not.toHaveClass("dark");

    await user.click(screen.getByRole("button", { name: "Activer le thème sombre" }));
    expect(root).toHaveClass("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");

    await user.click(screen.getByRole("button", { name: "Activer le thème clair" }));
    expect(root).not.toHaveClass("dark");
    // Light matches the (mocked) system setting, so the explicit choice is dropped.
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBeNull();
  });

  it("starts from the stored preference", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "dark");
    renderToggle();
    expect(document.documentElement).toHaveClass("dark");
    expect(screen.getByRole("button", { name: "Activer le thème clair" })).toBeInTheDocument();
  });

  it("follows the operating system while no explicit choice is stored", () => {
    const scheme = mockColorScheme(false);
    renderToggle();
    expect(document.documentElement).not.toHaveClass("dark");

    act(() => {
      scheme.setDark(true);
    });
    expect(document.documentElement).toHaveClass("dark");
  });

  it("is keyboard operable", async () => {
    const user = userEvent.setup();
    renderToggle();
    await user.tab();
    expect(screen.getByRole("button", { name: "Activer le thème sombre" })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(document.documentElement).toHaveClass("dark");
  });
});
