import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router";

import { AppProviders } from "@/app/providers";

/** Renders one component with the app providers and a router (links, location hooks). */
export function renderComponent(ui: ReactElement, { path = "/" }: { path?: string } = {}) {
  const user = userEvent.setup();
  const view = render(
    <AppProviders>
      <MemoryRouter initialEntries={[path]}>{ui}</MemoryRouter>
    </AppProviders>,
  );
  return { ...view, user };
}
