import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderRoute } from "@/test/render-route";

const VALID_TOKEN = "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde";

describe("TrackPage", () => {
  it("rejects a truncated tracking link", () => {
    renderRoute("/suivi/trop-court");
    expect(screen.getByText("Ce lien de suivi n’est pas valide")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Étapes" })).not.toBeInTheDocument();
  });

  it("shows the follow-up steps for a well-formed link", () => {
    renderRoute(`/suivi/${VALID_TOKEN}`);
    expect(screen.getByRole("heading", { name: "Étapes" })).toBeInTheDocument();
    expect(screen.queryByText("Ce lien de suivi n’est pas valide")).not.toBeInTheDocument();
  });

  it("asks search engines not to index the private page", () => {
    renderRoute(`/suivi/${VALID_TOKEN}`);
    expect(document.head.querySelector('meta[name="robots"]')).toHaveAttribute(
      "content",
      "noindex, nofollow",
    );
  });
});
