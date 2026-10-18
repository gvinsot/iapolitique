import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ApiError } from "@/api/errors";
import { mockClipboard } from "@/test/browser-mocks";
import {
  acceptedIdeaResultFixture,
  hiddenIdeaFixture,
  mergedIdeaFixture,
  publishedIdeaFixture,
  rejectedIdeaFixture,
  rejectedIdeaResultFixture,
  reviewedReportFixture,
  TRACKING_TOKEN,
} from "@/test/fixtures/tracking";
import { renderComponent } from "@/test/render-component";

import { SubmissionOutcome } from "./submission-outcome";
import { TrackingActions } from "./tracking-actions";
import { TrackingStatus } from "./tracking-status";

describe("TrackingStatus", () => {
  it("states the reasons of a refusal (DSA art. 17)", () => {
    renderComponent(<TrackingStatus item={rejectedIdeaFixture} />);
    expect(screen.getByRole("heading", { level: 2, name: "Idée non retenue" })).toBeInTheDocument();
    const statement = screen.getByRole("region", { name: "Exposé des motifs" });
    expect(statement).toHaveTextContent("Votre idée n’est pas publiée.");
    expect(statement).toHaveTextContent(rejectedIdeaFixture.decision!.reason_message);
    expect(
      within(statement).getByRole("link", {
        name: "Règle 4.5 : Une mesure pour les finances publiques",
      }),
    ).toHaveAttribute("href", "/regles#critere-sujet");
    expect(statement).toHaveTextContent("Décision prise automatiquement par une IA");
    expect(statement).toHaveTextContent("second avis d’un modèle de Mistral AI");
    expect(statement).toHaveTextContent("Le texte sera supprimé le 17 novembre 2026.");
    expect(statement).toHaveTextContent("demander un réexamen humain");
    expect(statement).toHaveTextContent("saisir la juridiction française compétente");
    expect(
      within(statement).getByRole("link", { name: "Reformuler et proposer à nouveau" }),
    ).toHaveAttribute("href", "/proposer");
  });

  it("links a published idea and the idea a duplicate was merged into", () => {
    const { unmount } = renderComponent(<TrackingStatus item={publishedIdeaFixture} />);
    expect(screen.getByRole("link", { name: /Voir l’idée publiée/ })).toHaveAttribute(
      "href",
      "/idees/mutualiser-achats",
    );
    unmount();
    renderComponent(<TrackingStatus item={mergedIdeaFixture} />);
    expect(
      screen.getByRole("heading", { name: "Rattachée à une idée existante" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /rattachée/ })).toHaveAttribute(
      "href",
      "/idees/mutualiser-achats",
    );
  });

  it("explains a masking with the rules article on measures", () => {
    renderComponent(<TrackingStatus item={hiddenIdeaFixture} />);
    expect(screen.getByRole("heading", { level: 2, name: "Idée masquée" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Règle 7 : Mesures et durées" })).toHaveAttribute(
      "href",
      "/regles#mesures",
    );
    expect(screen.queryByRole("link", { name: /Voir l’idée/ })).not.toBeInTheDocument();
  });

  it.each([
    [{ status: "submitted" }, "En cours de filtrage"],
    [{ status: "second_opinion" }, "En cours de vérification"],
    [{ status: "analyzing" }, "Idée conservée"],
    [{ status: "deleted" }, "Supprimé à votre demande"],
    [{ status: "purged" }, "Idée non retenue"],
  ])("shows the %o phase", (patch, title) => {
    renderComponent(<TrackingStatus item={{ ...rejectedIdeaFixture, ...patch }} />);
    expect(screen.getByRole("heading", { level: 2, name: title })).toBeInTheDocument();
  });

  it("acknowledges a report and gives the review outcome", () => {
    renderComponent(<TrackingStatus item={reviewedReportFixture} />);
    expect(screen.getByText("Signalement envoyé le 19 octobre 2026")).toBeInTheDocument();
    expect(screen.getByText(/L’analyse de l’idée va être refaite/)).toBeInTheDocument();
  });

  it("shows a pending human review", () => {
    renderComponent(
      <TrackingStatus
        item={{
          ...rejectedIdeaFixture,
          review_request: { requested_on: "2026-10-19", status: "pending" },
        }}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "Réexamen humain demandé le 19 octobre 2026",
    );
  });
});

describe("TrackingActions", () => {
  it("confirms the deletion in a dialog, never with window.confirm", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    const onDelete = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <TrackingActions item={publishedIdeaFixture} onDelete={onDelete} onRequestReview={vi.fn()} />,
    );

    await user.click(screen.getByRole("button", { name: "Supprimer ma contribution" }));
    const dialog = screen.getByRole("dialog", { name: "Supprimer définitivement ?" });
    await user.click(within(dialog).getByRole("button", { name: "Annuler" }));
    expect(onDelete).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Supprimer ma contribution" }));
    await user.click(screen.getByRole("button", { name: "Supprimer définitivement" }));
    expect(onDelete).toHaveBeenCalledOnce();
    expect(await screen.findByText("Votre texte a été supprimé.")).toBeInTheDocument();
    expect(confirmSpy).not.toHaveBeenCalled();
  });

  it("keeps the dialog open with the API message when the deletion fails", async () => {
    const onDelete = vi.fn(() =>
      Promise.reject(new ApiError({ status: 503, code: "x", message: "Service indisponible." })),
    );
    const { user } = renderComponent(
      <TrackingActions item={publishedIdeaFixture} onDelete={onDelete} onRequestReview={vi.fn()} />,
    );
    await user.click(screen.getByRole("button", { name: "Supprimer ma contribution" }));
    await user.click(screen.getByRole("button", { name: "Supprimer définitivement" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Service indisponible.");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("requests a human review with an optional message, only for an automated decision", async () => {
    const onRequestReview = vi.fn(() => Promise.resolve());
    const { user, unmount } = renderComponent(
      <TrackingActions
        item={rejectedIdeaFixture}
        onDelete={vi.fn()}
        onRequestReview={onRequestReview}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Demander un réexamen humain" }));
    await user.type(
      screen.getByRole("textbox", { name: "Votre message (facultatif)" }),
      "  Mon idée concerne bien une recette.  ",
    );
    await user.click(screen.getByRole("button", { name: "Envoyer la demande" }));
    expect(onRequestReview).toHaveBeenCalledWith("Mon idée concerne bien une recette.");
    expect(
      await screen.findByText(/demande de réexamen humain est enregistrée/),
    ).toBeInTheDocument();
    unmount();

    renderComponent(
      <TrackingActions item={publishedIdeaFixture} onDelete={vi.fn()} onRequestReview={vi.fn()} />,
    );
    expect(screen.queryByRole("button", { name: "Demander un réexamen humain" })).toBeNull();
  });

  it("sends no message when the field is left empty", async () => {
    const onRequestReview = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <TrackingActions
        item={rejectedIdeaFixture}
        onDelete={vi.fn()}
        onRequestReview={onRequestReview}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Demander un réexamen humain" }));
    await user.click(screen.getByRole("button", { name: "Envoyer la demande" }));
    expect(onRequestReview).toHaveBeenCalledWith(null);
  });
});

describe("SubmissionOutcome", () => {
  it("announces an accepted idea and shows the tracking link once, with a strong warning", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    const { user } = renderComponent(
      <SubmissionOutcome kind="idea" result={acceptedIdeaResultFixture} linkLabel="Idée" />,
    );
    mockClipboard({ writeText });

    expect(
      screen.getByRole("heading", { level: 2, name: "Merci, c’est enregistré" }),
    ).toHaveFocus();
    expect(screen.getByText(/elle va être améliorée et chiffrée par IA/)).toBeInTheDocument();
    expect(screen.getByText(/il ne sera plus jamais affiché/)).toBeInTheDocument();
    const url = `http://localhost:3000/suivi#${TRACKING_TOKEN}`;
    expect(screen.getByText(url)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Copier le lien" }));
    expect(writeText).toHaveBeenCalledWith(url);
    expect(
      screen.getByRole("checkbox", { name: "Se souvenir de ce lien sur cet appareil" }),
    ).not.toBeChecked();
    expect(window.localStorage.length).toBe(0);
  });

  it("remembers the link only when asked", async () => {
    const { user } = renderComponent(
      <SubmissionOutcome kind="idea" result={acceptedIdeaResultFixture} linkLabel="Idée du jour" />,
    );
    const remember = screen.getByRole("checkbox", {
      name: "Se souvenir de ce lien sur cet appareil",
    });
    await user.click(remember);
    expect(window.localStorage.getItem("iapolitique-tracking-links")).toContain(TRACKING_TOKEN);
    await user.click(remember);
    expect(window.localStorage.getItem("iapolitique-tracking-links")).toBeNull();
  });

  it("gives the refusal reason, the rule and an invitation to rephrase", () => {
    renderComponent(
      <SubmissionOutcome kind="idea" result={rejectedIdeaResultFixture} linkLabel="Idée" />,
    );
    expect(
      screen.getByRole("heading", { name: "Votre idée n’a pas été retenue" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Votre texte permet de reconnaître une personne.")).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Règle 4.4 : Aucun particulier identifiable" }),
    ).toHaveAttribute("href", "/regles#critere-donnees-personnelles");
    expect(screen.getByText(/reformuler votre idée/)).toBeInTheDocument();
  });

  it("says when the verification is still running", () => {
    renderComponent(
      <SubmissionOutcome
        kind="idea"
        result={{ ...acceptedIdeaResultFixture, status: "second_opinion" }}
        linkLabel="Idée"
      />,
    );
    expect(screen.getByRole("heading", { name: "En cours de vérification" })).toBeInTheDocument();
  });
});
