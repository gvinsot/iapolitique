import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { renderComponent } from "@/test/render-component";

import { ContributionForm, type ContributionFormValues } from "./contribution-form";
import { ReportForm, type ReportFormValues } from "./report-form";

const CONTRIBUTION = "Ouvrir la centrale d’achat aux établissements publics locaux également.";

describe("ContributionForm", () => {
  function renderForm() {
    const onSubmit = vi.fn<(values: ContributionFormValues) => void>();
    const view = renderComponent(<ContributionForm onSubmit={onSubmit} altchaTestMode />);
    return { ...view, onSubmit };
  }

  it("requires a kind, a long enough text and both consents", async () => {
    const { onSubmit, user } = renderForm();
    await user.type(screen.getByRole("textbox", { name: "Votre contribution" }), "Trop court");
    await user.click(screen.getByRole("button", { name: "Envoyer ma contribution" }));

    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(/Choisissez s’il s’agit d’une amélioration/)).toHaveAttribute(
      "role",
      "alert",
    );
    expect(screen.getByText(/au moins 30 caractères \(actuellement 10\)/)).toBeInTheDocument();
    expect(screen.getByText(/Cochez les deux cases/)).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /J’accepte la licence/ })).not.toBeChecked();
  });

  it("submits an objection with its consents and anti-robot payload", async () => {
    const { onSubmit, user } = renderForm();
    await user.click(screen.getByRole("radio", { name: /Objection/ }));
    const textbox = screen.getByRole("textbox", { name: "Votre contribution" });
    expect(textbox).toHaveAccessibleDescription(
      /Ne signez pas, ne nommez aucun agent ni particulier/,
    );
    await user.type(textbox, CONTRIBUTION);
    expect(screen.getByText(/^\d+ \/ 2\s000 caractères$/)).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: /J’accepte la licence/ }));
    await user.click(screen.getByRole("checkbox", { name: /sera publié s’il est accepté/ }));
    await user.click(screen.getByRole("button", { name: "Envoyer ma contribution" }));

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith({
        kind: "objection",
        text: CONTRIBUTION,
        altcha: expect.any(String) as string,
        acceptedLicence: true,
        acceptedPublication: true,
      });
    });
    expect(screen.getByText(/évaluée automatiquement par une IA/)).toBeInTheDocument();
  });
});

describe("ReportForm", () => {
  function renderForm() {
    const onSubmit = vi.fn<(values: ReportFormValues) => void>();
    const view = renderComponent(<ReportForm onSubmit={onSubmit} altchaTestMode />);
    return { ...view, onSubmit };
  }

  it("lists the reasons, reminds the penalty and the automated processing", () => {
    renderForm();
    for (const reason of [
      "Erreur de chiffrage",
      "Source erronée ou absente",
      "Contenu problématique",
      "Doublon d’une autre idée",
      "Autre",
    ]) {
      expect(screen.getByRole("radio", { name: reason })).toBeInTheDocument();
    }
    expect(screen.getByText(/article 6, VII de la loi n° 2004-575/)).toBeInTheDocument();
    expect(screen.getByText(/d’abord traité automatiquement par une IA/)).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /de bonne foi/ })).not.toBeChecked();
  });

  it("requires a reason and the good-faith declaration, and checks the optional e-mail", async () => {
    const { onSubmit, user } = renderForm();
    await user.type(screen.getByRole("textbox", { name: "Adresse e-mail" }), "pas-une-adresse");
    await user.click(screen.getByRole("button", { name: "Envoyer le signalement" }));

    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText("Choisissez le motif du signalement.")).toBeInTheDocument();
    expect(screen.getByText(/Cochez la déclaration de bonne foi/)).toBeInTheDocument();
    expect(screen.getByText(/ne semble pas valide/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Adresse e-mail" })).toHaveAttribute(
      "aria-invalid",
      "true",
    );
  });

  it("submits with empty optional fields as null", async () => {
    const { onSubmit, user } = renderForm();
    await user.click(screen.getByRole("radio", { name: "Erreur de chiffrage" }));
    await user.type(screen.getByRole("textbox", { name: "Nom" }), "   ");
    await user.click(screen.getByRole("checkbox", { name: /de bonne foi/ }));
    await user.click(screen.getByRole("button", { name: "Envoyer le signalement" }));

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith({
        reason: "erreur_chiffrage",
        details: null,
        reporterName: null,
        reporterEmail: null,
        goodFaith: true,
        altcha: expect.any(String) as string,
      });
    });
  });

  it("submits the details and contact when given", async () => {
    const { onSubmit, user } = renderForm();
    await user.click(screen.getByRole("radio", { name: "Source erronée ou absente" }));
    await user.type(
      screen.getByRole("textbox", { name: "Précision (facultatif)" }),
      " La source [[S2] date de 2010. ",
    );
    await user.type(screen.getByRole("textbox", { name: "Nom" }), "Dominique");
    await user.type(
      screen.getByRole("textbox", { name: "Adresse e-mail" }),
      "dominique@example.org",
    );
    await user.click(screen.getByRole("checkbox", { name: /de bonne foi/ }));
    await user.click(screen.getByRole("button", { name: "Envoyer le signalement" }));

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({
          reason: "source_erronee",
          details: "La source [S2] date de 2010.",
          reporterName: "Dominique",
          reporterEmail: "dominique@example.org",
        }),
      );
    });
  });
});
