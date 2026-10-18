import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { AppProviders } from "@/app/providers";

import { ProposeForm, type ProposeFormValues } from "./propose-form";

const LONG_ENOUGH =
  "Mutualiser les achats de fournitures des communes d’un même département pour obtenir de meilleurs prix.";

function renderForm() {
  const onSubmit = vi.fn<(values: ProposeFormValues) => void>();
  const user = userEvent.setup();
  render(
    <AppProviders>
      <ProposeForm onSubmit={onSubmit} />
    </AppProviders>,
  );
  return { onSubmit, user, textarea: screen.getByRole("textbox", { name: "Votre idée" }) };
}

describe("ProposeForm", () => {
  it("refuses a text that is too short and explains why", async () => {
    const { onSubmit, user, textarea } = renderForm();
    await user.type(textarea, "Trop court");
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));

    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent("au moins 50 caractères (actuellement 10)");
    expect(textarea).toHaveAttribute("aria-invalid", "true");
    expect(textarea).toHaveFocus();
  });

  it("clears the error once the text becomes valid", async () => {
    const { user, textarea } = renderForm();
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Décrivez votre idée");

    await user.type(textarea, LONG_ENOUGH);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(textarea).not.toHaveAttribute("aria-invalid");
  });

  it("counts characters", async () => {
    const { user, textarea } = renderForm();
    expect(screen.getByText(/^0 \/ 4\s000 caractères$/)).toBeInTheDocument();
    await user.type(textarea, "  Bonjour  ");
    expect(screen.getByText(/^7 \/ 4\s000 caractères$/)).toBeInTheDocument();
  });

  it("submits trimmed values, optional fields left empty", async () => {
    const { onSubmit, user, textarea } = renderForm();
    await user.type(textarea, `  ${LONG_ENOUGH}  `);
    await user.type(screen.getByRole("textbox", { name: /Territoire concerné/ }), "   ");
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));

    expect(onSubmit).toHaveBeenCalledWith({
      text: LONG_ENOUGH,
      adminLevel: null,
      territory: null,
    });
  });

  it("submits the chosen administrative level and territory", async () => {
    const { onSubmit, user, textarea } = renderForm();
    await user.type(textarea, LONG_ENOUGH);
    await user.click(screen.getByRole("combobox", { name: /Niveau administratif/ }));
    await user.click(await screen.findByRole("option", { name: "Département" }));
    await user.type(screen.getByRole("textbox", { name: /Territoire concerné/ }), "Gironde");
    await user.click(screen.getByRole("button", { name: "Envoyer mon idée" }));

    expect(onSubmit).toHaveBeenCalledWith({
      text: LONG_ENOUGH,
      adminLevel: "departement",
      territory: "Gironde",
    });
  });
});
