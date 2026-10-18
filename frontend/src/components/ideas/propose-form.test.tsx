import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { renderComponent } from "@/test/render-component";

import { ProposeForm, type ProposeFormValues } from "./propose-form";

const LONG_ENOUGH =
  "Mutualiser les achats de fournitures des communes d’un même département pour obtenir de meilleurs prix.";

function renderForm({ altchaTestMode = true }: { altchaTestMode?: boolean } = {}) {
  const onSubmit = vi.fn<(values: ProposeFormValues) => void>();
  const view = renderComponent(<ProposeForm onSubmit={onSubmit} altchaTestMode={altchaTestMode} />);
  return { ...view, onSubmit, textarea: screen.getByRole("textbox", { name: "Votre idée" }) };
}

async function acceptConsents(user: ReturnType<typeof renderForm>["user"]) {
  await user.click(screen.getByRole("checkbox", { name: /J’accepte la licence/ }));
  await user.click(
    screen.getByRole("checkbox", {
      name: "Je comprends que mon texte sera publié s’il est accepté.",
    }),
  );
}

describe("ProposeForm", () => {
  it("refuses a text that is too short and explains why", async () => {
    const { onSubmit, user, textarea } = renderForm();
    await user.type(textarea, "Trop court");
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));

    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(/au moins 50 caractères \(actuellement 10\)/)).toHaveAttribute(
      "role",
      "alert",
    );
    expect(textarea).toHaveAttribute("aria-invalid", "true");
    expect(textarea).toHaveFocus();
  });

  it("clears the error once the text becomes valid", async () => {
    const { user, textarea } = renderForm();
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));
    expect(screen.getByText(/Décrivez votre idée/)).toHaveAttribute("role", "alert");

    await user.type(textarea, LONG_ENOUGH);
    expect(screen.queryByText(/Décrivez votre idée/)).not.toBeInTheDocument();
    expect(textarea).not.toHaveAttribute("aria-invalid");
  });

  it("counts characters", async () => {
    const { user, textarea } = renderForm();
    expect(screen.getByText(/^0 \/ 4\s000 caractères$/)).toBeInTheDocument();
    await user.type(textarea, "  Bonjour  ");
    expect(screen.getByText(/^7 \/ 4\s000 caractères$/)).toBeInTheDocument();
  });

  it("warns against personal data and explains the AI processing", () => {
    const { textarea } = renderForm();
    expect(textarea).toHaveAccessibleDescription(
      /Ne signez pas, ne nommez aucun agent ni particulier\./,
    );
    expect(
      screen.getByText(/évaluée automatiquement par une IA \(modèle local, puis Mistral AI\)/),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "règles du site" })).toHaveAttribute("href", "/regles");
    expect(screen.getByRole("link", { name: "politique de confidentialité" })).toHaveAttribute(
      "href",
      "/confidentialite",
    );
  });

  it("blocks the submission until both consents, never pre-checked, are given", async () => {
    const { onSubmit, user, textarea } = renderForm();
    const licence = screen.getByRole("checkbox", { name: /J’accepte la licence/ });
    const publication = screen.getByRole("checkbox", { name: /sera publié s’il est accepté/ });
    expect(licence).not.toBeChecked();
    expect(publication).not.toBeChecked();
    expect(screen.getByRole("link", { name: /licence de ma contribution/ })).toHaveAttribute(
      "href",
      "/regles#licence-contribution",
    );

    await user.type(textarea, LONG_ENOUGH);
    await user.click(licence);
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent("Cochez les deux cases");
    expect(publication).toHaveAttribute("aria-invalid", "true");

    await user.click(publication);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("submits trimmed values with the anti-robot payload and sets no cookie", async () => {
    const { onSubmit, user, textarea } = renderForm();
    await user.type(textarea, `  ${LONG_ENOUGH}  `);
    await user.type(screen.getByRole("textbox", { name: /Territoire concerné/ }), "   ");
    await acceptConsents(user);
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledOnce();
    });
    expect(onSubmit).toHaveBeenCalledWith({
      text: LONG_ENOUGH,
      adminLevel: null,
      territory: null,
      altcha: expect.any(String) as string,
      acceptedLicence: true,
      acceptedPublication: true,
    });
    expect(onSubmit.mock.calls[0]![0].altcha).not.toBe("");
    expect(document.cookie).toBe("");
  });

  it("submits the chosen administrative level and territory", async () => {
    const { onSubmit, user, textarea } = renderForm();
    await user.type(textarea, LONG_ENOUGH);
    await user.click(screen.getByRole("combobox", { name: /Niveau administratif/ }));
    await user.click(await screen.findByRole("option", { name: "Département" }));
    await user.type(screen.getByRole("textbox", { name: /Territoire concerné/ }), "Gironde");
    await acceptConsents(user);
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({ adminLevel: "departement", territory: "Gironde" }),
      );
    });
  });
});
