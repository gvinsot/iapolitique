import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter } from "react-router";

import { App } from "@/App";
import { routes } from "@/app/routes";

/** Renders the whole application (providers, layout, routes) at the given URL. */
export function renderRoute(path: string) {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  const user = userEvent.setup();
  const view = render(<App router={router} />);
  return { ...view, router, user };
}
