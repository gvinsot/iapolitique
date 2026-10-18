import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { stubFetch } from "@/test/fetch-mock";
import {
  publishedIdeaFixture,
  rejectedIdeaFixture,
  TRACKING_TOKEN,
} from "@/test/fixtures/tracking";
import { renderRoute } from "@/test/render-route";

interface Recorded {
  method: string;
  url: string;
  body: unknown;
  referrerPolicy: string;
}

/** Fake tracking API recording every request (URL, body) for privacy assertions. */
function trackingApi(item: unknown, status = 200) {
  const requests: Recorded[] = [];
  const fetch = stubFetch(async (request) => {
    const url = new URL(request.url);
    const text = await request.text();
    requests.push({
      method: request.method,
      url: request.url,
      body: text ? (JSON.parse(text) as unknown) : null,
      referrerPolicy: request.referrerPolicy,
    });
    if (url.pathname === "/api/track") return Response.json(item, { status });
    if (url.pathname === "/api/track/delete" || url.pathname === "/api/track/review-request") {
      return new Response(null, { status: 204 });
    }
    throw new Error(`Unexpected request in test: ${request.method} ${url.pathname}`);
  });
  return { fetch, requests };
}

describe("TrackPage", () => {
  it("reads the token from the fragment and sends it in a POST body only", async () => {
    const { requests } = trackingApi(publishedIdeaFixture);
    renderRoute(`/suivi#${TRACKING_TOKEN}`);

    expect(
      await screen.findByRole("heading", { level: 2, name: "Idée publiée" }),
    ).toBeInTheDocument();
    expect(requests).toHaveLength(1);
    expect(requests[0]).toMatchObject({
      method: "POST",
      body: { token: TRACKING_TOKEN },
      referrerPolicy: "no-referrer",
    });
    expect(requests[0]!.url).not.toContain(TRACKING_TOKEN);
  });

  it("asks search engines not to index the page and browsers not to send a referrer", () => {
    trackingApi(publishedIdeaFixture);
    renderRoute(`/suivi#${TRACKING_TOKEN}`);
    expect(document.head.querySelector('meta[name="robots"]')).toHaveAttribute(
      "content",
      "noindex, nofollow",
    );
    expect(document.head.querySelector('meta[name="referrer"]')).toHaveAttribute(
      "content",
      "no-referrer",
    );
  });

  it("rejects a truncated tracking link without calling the API", () => {
    const { fetch } = trackingApi(publishedIdeaFixture);
    renderRoute("/suivi#trop-court");
    expect(screen.getByText("Ce lien de suivi n’est pas valide")).toBeInTheDocument();
    expect(fetch).not.toHaveBeenCalled();
  });

  it("opens a pasted link by moving its token to the fragment", async () => {
    trackingApi(publishedIdeaFixture);
    const { router, user } = renderRoute("/suivi");
    await user.type(
      screen.getByRole("textbox", { name: "Votre lien de suivi" }),
      `https://iapolitique.fr/suivi/${TRACKING_TOKEN}`,
    );
    await user.click(screen.getByRole("button", { name: "Afficher le suivi" }));

    expect(
      await screen.findByRole("heading", { level: 2, name: "Idée publiée" }),
    ).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/suivi");
    expect(router.state.location.search).toBe("");
    expect(router.state.location.hash).toBe(`#${TRACKING_TOKEN}`);
  });

  it("redirects a former /suivi/<token> link to the fragment and warns about logs", async () => {
    trackingApi(publishedIdeaFixture);
    const { router } = renderRoute(`/suivi/${TRACKING_TOKEN}`);
    expect(await screen.findByText("Lien à l’ancien format")).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/suivi");
    expect(router.state.location.hash).toBe(`#${TRACKING_TOKEN}`);
    expect(
      await screen.findByRole("heading", { level: 2, name: "Idée publiée" }),
    ).toBeInTheDocument();
  });

  it("explains an unknown or erased link", async () => {
    trackingApi({ error: { code: "not_found", message: "Lien inconnu." } }, 404);
    renderRoute(`/suivi#${TRACKING_TOKEN}`);
    expect(await screen.findByText("Aucun envoi ne correspond à ce lien")).toBeInTheDocument();
  });

  it("deletes the contribution with the token in the body, after confirmation", async () => {
    const { requests } = trackingApi(publishedIdeaFixture);
    const { user } = renderRoute(`/suivi#${TRACKING_TOKEN}`);
    await user.click(await screen.findByRole("button", { name: "Supprimer ma contribution" }));
    await user.click(screen.getByRole("button", { name: "Supprimer définitivement" }));

    expect(await screen.findByText("Votre texte a été supprimé.")).toBeInTheDocument();
    const deletion = requests.find((request) => request.url.endsWith("/api/track/delete"));
    expect(deletion).toMatchObject({ method: "POST", body: { token: TRACKING_TOKEN } });
  });

  it("requests a human review of a refusal", async () => {
    const { requests } = trackingApi(rejectedIdeaFixture);
    const { user } = renderRoute(`/suivi#${TRACKING_TOKEN}`);
    await user.click(await screen.findByRole("button", { name: "Demander un réexamen humain" }));
    await user.type(screen.getByRole("textbox", { name: "Votre message (facultatif)" }), "Merci");
    await user.click(screen.getByRole("button", { name: "Envoyer la demande" }));

    expect(
      await screen.findByText(/demande de réexamen humain est enregistrée/),
    ).toBeInTheDocument();
    expect(
      requests.find((request) => request.url.endsWith("/api/track/review-request")),
    ).toMatchObject({ body: { token: TRACKING_TOKEN, message: "Merci" } });
  });

  it("remembers the link on this device only when asked, and lists it", async () => {
    trackingApi(publishedIdeaFixture);
    const { user, unmount } = renderRoute(`/suivi#${TRACKING_TOKEN}`);
    const remember = await screen.findByRole("checkbox", {
      name: "Se souvenir de ce lien sur cet appareil",
    });
    expect(remember).not.toBeChecked();
    expect(window.localStorage.getItem("iapolitique-tracking-links")).toBeNull();
    await user.click(remember);
    unmount();

    const view = renderRoute("/suivi");
    expect(screen.getByRole("link", { name: "Idée du 10 octobre 2026" })).toHaveAttribute(
      "href",
      `/suivi#${TRACKING_TOKEN}`,
    );
    await view.user.click(screen.getByRole("button", { name: /Oublier le lien/ }));
    expect(window.localStorage.getItem("iapolitique-tracking-links")).toBeNull();
  });
});
