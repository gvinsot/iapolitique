#!/usr/bin/env node
// Fails when the production build (dist/) names a third-party origin that is not on the list
// below (AGENTS.md §14.1, §14.2): the site loads nothing from another origin (no CDN, font,
// analytics or captcha service; the CSP would block it anyway) and the only absolute URLs allowed
// in the bundle are identifiers and documentation links that the browser never fetches by itself.
// Run after `npm run build` (`npm run check:dist`, devops/Dockerfile.test).
//
// Usage: node scripts/check-dist-urls.mjs [DIST_DIR]
import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

/** Host -> why it may appear in the bundle. Any new entry needs the same justification. */
export const ALLOWED_HOSTS = new Map([
  ["www.w3.org", "XML namespaces (SVG, XHTML, MathML) used by React and the icons"],
  ["schema.org", "JSON-LD @context of the AI marking (AGENTS.md §7.5)"],
  ["cv.iptc.org", "IPTC digital source type of the AI marking (AGENTS.md §7.5)"],
  ["iapolitique.fr", "the site's own canonical address (licence attribution)"],
  ["www.data.gouv.fr", "Licence Ouverte 2.0 text, linked from /licence and the exports"],
  ["creativecommons.org", "CC BY 4.0 compatibility link of /licence"],
  ["www.cnil.fr", "complaint channel linked from /confidentialite"],
  ["www.internet-signalement.gouv.fr", "PHAROS, linked from the admin escalations"],
  ["react.dev", "links in React's minified error messages"],
  ["reactrouter.com", "links in React Router's warnings"],
  ["github.com", "links in library error messages"],
  ["altcha.org", "ALTCHA widget: hidden footer link and error documentation"],
  ["developer.mozilla.org", "ALTCHA widget: error documentation"],
  ["svelte.dev", "ALTCHA widget (Svelte runtime): error documentation"],
]);

const SCANNED = new Set([".html", ".js", ".mjs", ".css", ".json", ".svg", ".txt", ".webmanifest"]);
// Absolute (http:, https:) and protocol-relative (//host) URLs.
const URL_PATTERN = /(?:\bhttps?:)?\/\/([a-z0-9-]+(?:\.[a-z0-9-]+)+)(?::\d+)?/gi;

function* files(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) yield* files(path);
    else if (SCANNED.has(extname(name))) yield path;
  }
}

/** Third-party hosts named in `text`, with their first occurrence. */
export function unexpectedHosts(text, allowed = ALLOWED_HOSTS) {
  const found = new Map();
  for (const match of text.matchAll(URL_PATTERN)) {
    const host = match[1].toLowerCase();
    if (!allowed.has(host) && !found.has(host)) {
      found.set(host, text.slice(Math.max(0, match.index - 40), match.index + 80));
    }
  }
  return found;
}

function main() {
  const dist = process.argv[2] ?? fileURLToPath(new URL("../dist", import.meta.url));
  const problems = [];
  let scanned = 0;
  for (const path of files(dist)) {
    scanned += 1;
    for (const [host, context] of unexpectedHosts(readFileSync(path, "utf8"))) {
      problems.push(`${relative(dist, path)}: ${host}  …${context.replace(/\s+/g, " ")}…`);
    }
  }
  if (scanned === 0) {
    console.error(`no file to scan in ${dist}: run npm run build first`);
    process.exit(1);
  }
  if (problems.length > 0) {
    console.error(problems.join("\n"));
    console.error(
      `${problems.length} third-party origin(s) in the build: remove them, or add the host to ` +
        "ALLOWED_HOSTS in scripts/check-dist-urls.mjs with the reason it is never fetched.",
    );
    process.exit(1);
  }
  console.log(`ok    ${scanned} built files name no unexpected third-party origin`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
