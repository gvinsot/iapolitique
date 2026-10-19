/// <reference types="node" />
import { readFileSync } from "node:fs";

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import "sonner";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const indexHtml = read("../../index.html");
const nginxConf = read("../../nginx.conf");
const themeInit = read("../../public/theme-init.js");

/**
 * The Content-Security-Policy of frontend/nginx.conf (AGENTS.md §14.2) allows the <style>
 * elements that libraries inject at run time by hash (fixed contents) or by nonce (contents that
 * vary). This test fails when a library update changes a hashed content: copy the new hash it
 * prints into nginx.conf and devops/check_frontend_headers.sh.
 */
const csp = /add_header Content-Security-Policy "([^"]+)" always;/.exec(nginxConf)?.[1] ?? "";
const directives = new Map(
  csp.split(";").map((directive) => {
    const [name = "", ...values] = directive.trim().split(/\s+/);
    return [name, values] as const;
  }),
);

async function cspHash(text: string): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)),
  );
  return `'sha256-${btoa(String.fromCharCode(...digest))}'`;
}

function injectedStyle(marker: string): string {
  const style = [...document.querySelectorAll("style")].find((element) =>
    element.textContent.includes(marker),
  );
  if (!style) throw new Error(`no <style> containing ${marker}`);
  return style.textContent;
}

describe("Content-Security-Policy of nginx.conf", () => {
  it("is the strict policy of AGENTS.md §14.2", () => {
    expect([...directives.keys()]).toEqual([
      "default-src",
      "script-src",
      "style-src",
      "style-src-attr",
      "img-src",
      "font-src",
      "connect-src",
      "worker-src",
      "manifest-src",
      "base-uri",
      "form-action",
      "frame-ancestors",
      "object-src",
      "upgrade-insecure-requests",
    ]);
    expect(directives.get("default-src")).toEqual(["'none'"]);
    expect(directives.get("script-src")).toEqual(["'self'"]);
    expect(directives.get("style-src-attr")).toEqual(["'unsafe-inline'"]);
    expect(directives.get("worker-src")).toEqual(["'self'"]);
    expect(directives.get("base-uri")).toEqual(["'none'"]);
    expect(directives.get("form-action")).toEqual(["'self'"]);
    expect(directives.get("frame-ancestors")).toEqual(["'none'"]);
    expect(directives.get("object-src")).toEqual(["'none'"]);
    expect(csp).not.toMatch(/unsafe-eval|unsafe-hashes|\*|https?:|blob:/);
    expect(directives.get("style-src")).not.toContain("'unsafe-inline'");
  });

  it("gives every response its own nonce for styles", () => {
    expect(directives.get("style-src")).toContain("'nonce-$request_id'");
  });

  it("allows sonner's stylesheet by hash (inserted empty, then filled)", async () => {
    const styleSrc = directives.get("style-src") ?? [];
    expect(styleSrc).toContain(await cspHash(injectedStyle("[data-sonner-toaster]")));
    expect(styleSrc).toContain(await cspHash(""));
  });

  it("allows the viewport style of Radix Select by hash", async () => {
    render(
      <Select open value="a">
        <SelectTrigger aria-label="Choix">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="a">A</SelectItem>
        </SelectContent>
      </Select>,
    );
    const style = injectedStyle("[data-radix-select-viewport]");
    expect(directives.get("style-src")).toContain(await cspHash(style));
  });

  it("allows no other hash than these three", () => {
    const hashes = (directives.get("style-src") ?? []).filter((value) => value.startsWith("'sha"));
    expect(hashes).toHaveLength(3);
  });
});

describe("CSP nonce hand-over", () => {
  it("is put by nginx on the exact theme-init.js tag of index.html", () => {
    const tag = '<script src="/theme-init.js"></script>';
    expect(indexHtml).toContain(tag);
    expect(nginxConf).toContain(
      `sub_filter '${tag}' '<script nonce="$request_id" src="/theme-init.js"></script>';`,
    );
  });

  it("is copied by theme-init.js to window.__webpack_nonce__ for react-style-singleton", () => {
    const script = document.createElement("script");
    script.nonce = "0123456789abcdef0123456789abcdef";
    Object.defineProperty(document, "currentScript", { configurable: true, value: script });
    try {
      // Runs the public file as the browser does.
      // eslint-disable-next-line @typescript-eslint/no-implied-eval
      const run = new Function(themeInit) as () => void;
      run();
      expect((window as { __webpack_nonce__?: string }).__webpack_nonce__).toBe(script.nonce);
    } finally {
      Reflect.deleteProperty(document, "currentScript");
      Reflect.deleteProperty(window, "__webpack_nonce__");
    }
  });
});
