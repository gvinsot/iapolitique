import { RouterProvider, type RouterProviderProps } from "react-router";

import { AppProviders } from "@/app/providers";

interface AppProps {
  /** Browser router in production, memory router in tests. */
  router: RouterProviderProps["router"];
}

export function App({ router }: AppProps) {
  return (
    <AppProviders>
      <RouterProvider router={router} />
    </AppProviders>
  );
}
