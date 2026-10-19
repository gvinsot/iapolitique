// Stored-XSS check in real browsers (AGENTS.md §14.1, §14.8), LOCAL ONLY, run by
// devops/dev/security/run.sh in the Playwright image behind the local Traefik proxy.
//
// seed.py published an idea whose analysis the mock Mistral filled with payloads (title,
// summary, every list, sources with hostile http(s) URLs, contribution summaries), an idea whose
// visitor text carries HTML, a report (details, reporter name), an admin right of reply with
// payloads, and a second hostile idea escalated by a report, whose sealed copy (§7.5, D3) holds
// the payloads of its analysis. This script opens every page that renders them, public and
// admin (the sealed copy after the explicit click that its page requires), in Chromium and
// Firefox, and fails when:
//   - a payload runs: `window.__xss` is set (a trap records any write), a dialog opens, or the
//     CSP reports a violation (an injected inline handler or script would be blocked AND
//     reported, so a CSP report means the escaping failed even though the CSP held);
//   - an injected element exists in the DOM (img[src=x], svg[onload], iframe, on* attributes),
//     a <script> other than the bundle, theme-init.js or the JSON-LD block, or a javascript: link;
//   - the JSON-LD block is not valid JSON or contains a literal `</script`;
//   - a source link to the hostile host lacks https:, target=_blank or
//     rel="noopener noreferrer nofollow ugc";
// and checks that the payloads are visible as plain text (they reached the page, escaped).
//
// Environment: PUBLIC_ORIGIN, ADMIN_ORIGIN, ADMIN_TOKEN (required), SEED (seed.json path),
// BROWSERS (default "chromium,firefox").
import { readFileSync } from "node:fs";
import { chromium, firefox } from "playwright";

const PUBLIC_ORIGIN = process.env.PUBLIC_ORIGIN ?? "https://localhost";
const ADMIN_ORIGIN = process.env.ADMIN_ORIGIN ?? "https://admin.localhost";
const ADMIN_TOKEN = process.env.ADMIN_TOKEN ?? "";
const SEED = JSON.parse(readFileSync(process.env.SEED ?? "/work/seed.json", "utf8"));
const BROWSERS = (process.env.BROWSERS ?? "chromium,firefox").split(",").filter(Boolean);
if (!ADMIN_TOKEN) throw new Error("ADMIN_TOKEN is required");

const problems = [];
const passed = [];
let pages = 0;

const TRAP = () => {
  window.__cspViolations = [];
  window.__xssHits = [];
  document.addEventListener("securitypolicyviolation", (event) => {
    window.__cspViolations.push(
      `${event.effectiveDirective} blocked ${event.blockedURI || "inline"} ${event.sample ?? ""}`,
    );
  });
  Object.defineProperty(window, "__xss", {
    configurable: false,
    get: () => undefined,
    set: (value) => window.__xssHits.push(String(value)),
  });
  for (const name of ["alert", "confirm", "prompt", "print"]) {
    window[name] = (...args) => window.__xssHits.push(`${name}(${args.join(",")})`);
  }
};

async function inspect(page, browserName, where, expectTexts = []) {
  pages += 1;
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(500);
  const state = await page.evaluate(() => {
    const scripts = [...document.querySelectorAll("script")].map((s) => ({
      src: s.getAttribute("src"),
      type: s.getAttribute("type"),
      text: s.textContent ?? "",
    }));
    const injected = [
      ...document.querySelectorAll(
        'img[src="x"], svg[onload], iframe, object, embed, [onerror], [onload], [onfocus], ' +
          '[onclick], [onmouseover], a[href^="javascript:" i], [autofocus][onfocus]',
      ),
    ].map((el) => el.outerHTML.slice(0, 120));
    const links = [...document.querySelectorAll('a[href*="evil.example"]')].map((a) => ({
      href: a.getAttribute("href"),
      rel: a.getAttribute("rel") ?? "",
      target: a.getAttribute("target") ?? "",
    }));
    return {
      hits: window.__xssHits ?? [],
      violations: window.__cspViolations ?? [],
      scripts,
      injected,
      links,
      text: document.body.innerText,
    };
  });
  const local = [];
  for (const hit of state.hits) local.push(`payload executed: ${hit}`);
  for (const v of state.violations) local.push(`CSP violation: ${v}`);
  for (const el of state.injected) local.push(`injected element: ${el}`);
  for (const script of state.scripts) {
    if (script.type === "application/ld+json") {
      if (/<\/script/i.test(script.text)) local.push("JSON-LD contains a literal </script");
      try {
        JSON.parse(script.text);
      } catch {
        local.push("JSON-LD is not valid JSON");
      }
    } else if (!(script.src && /^\/(assets\/[\w.-]+\.js|theme-init\.js)$/.test(script.src))) {
      local.push(`unexpected script: src=${script.src} ${script.text.slice(0, 80)}`);
    }
  }
  for (const link of state.links) {
    const rel = new Set(link.rel.split(/\s+/));
    if (!/^https?:\/\//.test(link.href) || link.target !== "_blank" ||
        !["noopener", "noreferrer", "nofollow", "ugc"].every((r) => rel.has(r))) {
      local.push(`unsafe source link: ${JSON.stringify(link)}`);
    }
  }
  for (const text of expectTexts) {
    if (!state.text.includes(text)) local.push(`payload not shown as text: ${text}`);
  }
  for (const message of local) problems.push(`[${browserName}] ${where}: ${message}`);
  if (local.length === 0) {
    passed.push(`[${browserName}] ${where}: ${state.links.length} hostile link(s), ` +
      `${expectTexts.length} payload(s) shown as text`);
  }
}

async function visit(page, browserName, url, expectTexts) {
  await page.goto(url);
  await inspect(page, browserName, url.replace(/#.*/, "#…"), expectTexts);
}

async function runBrowser(browserType) {
  const browserName = browserType.name();
  const browser = await browserType.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, locale: "fr-FR" });
  await context.addInitScript(TRAP);
  const page = await context.newPage();
  page.on("dialog", (dialog) => {
    problems.push(`[${browserName}] dialog opened: ${dialog.type()} ${dialog.message()}`);
    dialog.dismiss().catch(() => {});
  });
  page.on("pageerror", (error) => problems.push(`[${browserName}] page error: ${error.message}`));
  try {
    const hostile = SEED.hostile_slug;
    await visit(page, browserName, `${PUBLIC_ORIGIN}/idees/${hostile}`, [
      "<script>window.__xss=2</script>",
      '<img src=x onerror="window.__xss=1">',
      "=HYPERLINK(",
    ]);
    if (SEED.visitor_html_idea?.slug) {
      await visit(page, browserName, `${PUBLIC_ORIGIN}/idees/${SEED.visitor_html_idea.slug}`, [
        '<img src=x onerror="window.__xss=10">',
      ]);
    }
    await visit(page, browserName, `${PUBLIC_ORIGIN}/`, ["=HYPERLINK("]);
    await visit(page, browserName,
      `${PUBLIC_ORIGIN}/?q=${encodeURIComponent('<img src=x onerror="window.__xss=20">')}`, []);
    await visit(page, browserName,
      `${PUBLIC_ORIGIN}/idees/${encodeURIComponent('<svg onload="window.__xss=21">')}`, []);
    if (SEED.hostile_tracking_token) {
      await visit(page, browserName, `${PUBLIC_ORIGIN}/suivi#${SEED.hostile_tracking_token}`,
        ["=HYPERLINK("]);
    }
    await visit(page, browserName, `${PUBLIC_ORIGIN}/methodologie`, []);

    // Administration: the token is typed once, then kept in sessionStorage.
    await page.goto(`${ADMIN_ORIGIN}/admin`);
    await page.getByLabel("Jeton d’administration").fill(ADMIN_TOKEN);
    await page.getByRole("button", { name: "Se connecter" }).click();
    await page.waitForLoadState("networkidle");
    const ideas = await (await page.request.get(
      `${ADMIN_ORIGIN}/api/admin/ideas?q=${encodeURIComponent(hostile)}`,
      { headers: { Authorization: `Bearer ${ADMIN_TOKEN}` } },
    )).json();
    const ideaId = ideas.items?.[0]?.id;
    const adminPages = [
      ["/admin", []],
      ["/admin/idees", ["=HYPERLINK("]],
      ["/admin/signalements", SEED.report?.status === 201 ? ["<script>window.__xss=13</script>"] : []],
      ["/admin/escalades", []],
      ["/admin/reexamens", []],
      ["/admin/decisions", []],
      ["/admin/jobs", []],
      ["/admin/journal", []],
      ["/admin/moderation", []],
    ];
    if (ideaId) {
      adminPages.push([`/admin/idees/${ideaId}/reponses`, ['<img src=x onerror="window.__xss=14">']]);
    }
    adminPages.push(["/admin/copies-scellees", []]);
    for (const [path, texts] of adminPages) await visit(page, browserName, ADMIN_ORIGIN + path, texts);

    // Sealed copy of the escalated hostile idea: fetched only after the explicit click (each
    // read is journaled), then rendered as plain text.
    const escalationId = SEED.escalation?.escalation_id;
    if (!escalationId) throw new Error("seed.json has no escalation (re-run seed.py)");
    const sealUrl = `${ADMIN_ORIGIN}/admin/escalades/${encodeURIComponent(escalationId)}/copie-scellee`;
    await page.goto(sealUrl);
    await inspect(page, browserName, `${sealUrl} (before the click)`, ["Consultation journalisée"]);
    await page.getByRole("button", { name: "Afficher la copie scellée" }).click();
    await page.getByRole("article", { name: "Copie scellée" }).waitFor();
    await inspect(page, browserName, `${sealUrl} (revealed)`, [
      "=HYPERLINK(",
      "<script>window.__xss=2</script>",
      '<img src=x onerror="window.__xss=1">',
    ]);
  } catch (error) {
    problems.push(`[${browserName}] ${String(error.message ?? error).split("\n")[0]}`);
  } finally {
    await browser.close();
  }
}

const types = { chromium, firefox };
for (const name of BROWSERS) await runBrowser(types[name]);
for (const line of passed) console.log(`ok    ${line}`);
if (problems.length > 0) {
  console.error(problems.join("\n"));
  console.error(`FAIL  ${problems.length} stored-XSS problem(s)`);
  process.exit(1);
}
console.log(`ok    ${pages} pages in ${BROWSERS.join(", ")}: no payload executed, no injected ` +
  "element, no CSP violation, payloads rendered as text");
