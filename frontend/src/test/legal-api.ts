import type { LegalInfo } from "@/lib/legal";

/** Answer of GET /api/legal used by the tests (fictitious publisher). */
export const legalInfoFixture: LegalInfo = {
  publisher: {
    name: "Camille Exemple",
    address: "1 rue de l’Exemple, 75000 Paris",
    phone: "01 23 45 67 89",
  },
  contact_email: "contact@example.org",
  privacy_email: "confidentialite@example.org",
  cgu_version: "1.1",
  pages: {
    mentions_legales: "2026-10-18",
    confidentialite: "2026-10-18",
    regles: "2026-10-20",
    licence: "2026-10-18",
  },
};

/** Request handler (see stubFetch) answering GET /api/legal, and nothing else. */
export function legalApi(info: unknown = legalInfoFixture, status = 200) {
  return (request: Request) => {
    const url = new URL(request.url);
    if (url.pathname !== "/api/legal") {
      throw new Error(`Unexpected request in test: ${request.method} ${url.pathname}`);
    }
    return Response.json(info, { status });
  };
}
