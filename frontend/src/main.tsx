import "@fontsource-variable/inter";
import "@/styles/globals.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter } from "react-router";

import { App } from "@/App";
import { routes } from "@/app/routes";

const container = document.getElementById("root");
if (!container) {
  throw new Error("Root element #root is missing from index.html");
}

const router = createBrowserRouter(routes);

createRoot(container).render(
  <StrictMode>
    <App router={router} />
  </StrictMode>,
);
