import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";

import { AUDIENCE_OPT_OUT_KEY } from "@/lib/audience";

import { AudienceOptOut } from "./audience-opt-out";

describe("AudienceOptOut", () => {
  afterEach(() => {
    window.localStorage.clear();
  });

  it("stores the key read by the Umami script, and removes it", async () => {
    const user = userEvent.setup();
    render(<AudienceOptOut />);
    const box = screen.getByRole("checkbox", { name: /refuser la mesure d’audience/i });
    expect(box).not.toBeChecked();

    await user.click(box);
    expect(window.localStorage.getItem(AUDIENCE_OPT_OUT_KEY)).toBe("1");
    expect(screen.getByText(/ne sont plus mesurées/)).toBeInTheDocument();

    await user.click(box);
    expect(window.localStorage.getItem(AUDIENCE_OPT_OUT_KEY)).toBeNull();
  });

  it("shows an earlier choice", () => {
    window.localStorage.setItem(AUDIENCE_OPT_OUT_KEY, "1");
    render(<AudienceOptOut />);
    expect(screen.getByRole("checkbox", { name: /refuser la mesure d’audience/i })).toBeChecked();
  });
});
