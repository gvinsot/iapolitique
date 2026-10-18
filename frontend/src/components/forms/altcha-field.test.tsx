import { screen, waitFor } from "@testing-library/react";
import altchaBundle from "altcha/external?raw";
import { createRef } from "react";
import { describe, expect, it } from "vitest";

import { stubFetch } from "@/test/fetch-mock";
import { renderComponent } from "@/test/render-component";

import { AltchaField, type AltchaHandle } from "./altcha-field";

async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function renderField() {
  const ref = createRef<AltchaHandle>();
  const view = renderComponent(<AltchaField ref={ref} />);
  // The widget chunk loads asynchronously, then mounts a tick after insertion.
  await waitFor(() => {
    expect(document.querySelector("altcha-widget")).not.toBeNull();
  });
  await waitFor(() => {
    expect(typeof (document.querySelector("altcha-widget") as { verify?: unknown }).verify).toBe(
      "function",
    );
  });
  return { ...view, ref };
}

describe("AltchaField", () => {
  it("solves the challenge served by our API, without credentials nor cookie", async () => {
    const challenge = {
      algorithm: "SHA-256",
      challenge: await sha256Hex("sel-de-test42"),
      salt: "sel-de-test",
      signature: "signature-du-serveur",
      maxnumber: 100,
    };
    const fetch = stubFetch((request) => {
      expect(new URL(request.url).pathname).toBe("/api/altcha/challenge");
      return Response.json(challenge);
    });
    const { ref } = await renderField();

    const payload = await ref.current!.solve();

    expect(payload).not.toBeNull();
    expect(JSON.parse(atob(payload!))).toMatchObject({
      algorithm: "SHA-256",
      number: 42,
      salt: "sel-de-test",
      signature: "signature-du-serveur",
    });
    expect(fetch).toHaveBeenCalledOnce();
    expect(fetch.mock.calls[0]![1]).toMatchObject({ credentials: "omit" });
    expect(document.cookie).toBe("");
    // The payload is kept until used, then forgotten on reset.
    expect(await ref.current!.solve()).toBe(payload);
  });

  it("returns null when the challenge cannot be fetched", async () => {
    stubFetch(() => Promise.reject(new TypeError("Failed to fetch")));
    const { ref } = await renderField();
    expect(await ref.current!.solve()).toBeNull();
  });

  it("shows the error given by the form", () => {
    renderComponent(<AltchaField error="La vérification anti-robot a échoué." />);
    expect(screen.getByRole("alert")).toHaveTextContent("La vérification anti-robot a échoué.");
  });

  it("ships a widget that never touches cookies or storage nor calls a third party", () => {
    expect(altchaBundle).not.toMatch(/document\.cookie/);
    expect(altchaBundle).not.toMatch(/localStorage|sessionStorage|indexedDB/);
    // XML namespaces, documentation links of error messages and the (hidden) footer link only.
    const allowedHosts = ["altcha.org", "developer.mozilla.org", "svelte.dev", "www.w3.org"];
    for (const match of altchaBundle.matchAll(/https?:\/\/([a-z0-9.-]+)/gi)) {
      expect(allowedHosts).toContain(match[1]);
    }
    expect(altchaBundle).not.toMatch(/fetch\(\s*["'`]https?:/);
  });
});
