import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { stubFetch } from "@/test/fetch-mock";
import {
  acceptedIdeaResultFixture,
  rejectedIdeaResultFixture,
  TRACKING_TOKEN,
} from "@/test/fixtures/tracking";
import { renderRoute } from "@/test/render-route";

const IDEA =
  "Mutualiser les achats de fournitures des communes d’un même département pour obtenir de meilleurs prix.";

async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Fake API: a small ALTCHA challenge, then the given answer to POST /api/ideas. */
async function ideasApi(answer: unknown, status = 200) {
  const challenge = {
    algorithm: "SHA-256",
    challenge: await sha256Hex("sel-de-test7"),
    salt: "sel-de-test",
    signature: "signature",
    maxnumber: 50,
  };
  const bodies: unknown[] = [];
  stubFetch(async (request) => {
    const { pathname } = new URL(request.url);
    if (pathname === "/api/altcha/challenge") return Response.json(challenge);
    if (pathname === "/api/ideas" && request.method === "POST") {
      bodies.push(await request.json());
      return Response.json(answer, { status });
    }
    throw new Error(`Unexpected request in test: ${request.method} ${pathname}`);
  });
  return bodies;
}

async function fillAndSend(user: ReturnType<typeof renderRoute>["user"]) {
  await screen.findByText(/Je ne suis pas un robot|Chargement de la vérification/);
  await user.type(screen.getByRole("textbox", { name: "Votre idée" }), IDEA);
  await user.click(screen.getByRole("checkbox", { name: /J’accepte la licence/ }));
  await user.click(screen.getByRole("checkbox", { name: /sera publié s’il est accepté/ }));
  await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));
}

describe("ProposePage", () => {
  it("sends the idea with its consents and shows the tracking link once", async () => {
    const bodies = await ideasApi(acceptedIdeaResultFixture);
    const { user } = renderRoute("/proposer");
    await fillAndSend(user);

    expect(
      await screen.findByRole("heading", { level: 2, name: "Merci, c’est enregistré" }),
    ).toBeInTheDocument();
    expect(bodies).toEqual([
      {
        text: IDEA,
        admin_level: null,
        territory: null,
        altcha: expect.any(String) as string,
        accept_licence: true,
        accept_publication: true,
      },
    ]);
    expect(screen.getByText(`http://localhost:3000/suivi#${TRACKING_TOKEN}`)).toBeInTheDocument();
    expect(document.cookie).toBe("");

    await user.click(
      screen.getByRole("button", { name: "J’ai conservé mon lien : proposer une autre idée" }),
    );
    expect(screen.queryByText(/suivi#/)).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Votre idée" })).toHaveValue("");
  });

  it("shows the refusal reason with the tracking link", async () => {
    await ideasApi(rejectedIdeaResultFixture);
    const { user } = renderRoute("/proposer");
    await fillAndSend(user);
    expect(
      await screen.findByRole("heading", { name: "Votre idée n’a pas été retenue" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/suivi#/)).toBeInTheDocument();
  });

  it("keeps the text and shows the API message when sending fails", async () => {
    await ideasApi(
      { error: { code: "rate_limited", message: "Trop de demandes en peu de temps." } },
      429,
    );
    const { user } = renderRoute("/proposer");
    await fillAndSend(user);
    expect(await screen.findByText("Votre idée n’a pas pu être envoyée")).toBeInTheDocument();
    expect(screen.getByText("Trop de demandes en peu de temps.")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Votre idée" })).toHaveValue(IDEA);
  });
});
