// Real-browser check of the frontend image behind a Traefik-like HTTPS proxy (AGENTS.md §14.2),
// run by devops/dev/browser_check.sh in the Playwright image. Fails on:
//   - any Content-Security-Policy violation (documents and workers), page error or failed
//     same-origin request, on every public and admin route;
//   - a document without the expected security headers, or not cross-origin isolated
//     (COOP same-origin + COEP require-corp);
//   - a <style> injected at run time that the CSP blocked (sonner, Radix Select, the scrollbar
//     compensation of react-remove-scroll with the per-response nonce).
// It also submits an idea through the form (ALTCHA solved by its worker), opens the tracking
// page and a dialog, signs in to the administration and visits every admin page.
//
// Environment: PUBLIC_ORIGIN (https://localhost), ADMIN_ORIGIN (https://admin.localhost),
// ADMIN_TOKEN (required), BROWSERS (comma separated, default "chromium,firefox").
import { chromium, firefox } from "playwright";

const PUBLIC_ORIGIN = process.env.PUBLIC_ORIGIN ?? "https://localhost";
const ADMIN_ORIGIN = process.env.ADMIN_ORIGIN ?? "https://admin.localhost";
const ADMIN_TOKEN = process.env.ADMIN_TOKEN ?? "";
const BROWSERS = (process.env.BROWSERS ?? "chromium,firefox").split(",").filter(Boolean);
if (!ADMIN_TOKEN) throw new Error("ADMIN_TOKEN is required");

const PUBLIC_ROUTES = [
  "/",
  "/proposer",
  "/methodologie",
  "/a-propos",
  "/mentions-legales",
  "/confidentialite",
  "/regles",
  "/licence",
  "/cgu",
  "/suivi",
  "/page-inconnue",
];
const ADMIN_ROUTES = [
  "/admin",
  "/admin/idees",
  "/admin/signalements",
  "/admin/escalades",
  "/admin/reexamens",
  "/admin/decisions",
  "/admin/jobs",
  "/admin/couts",
  "/admin/reglages",
  "/admin/journal",
  "/admin/moderation",
];
const EXPECTED_HEADERS = {
  "strict-transport-security": "max-age=63072000; includeSubDomains",
  "x-content-type-options": "nosniff",
  "x-frame-options": "DENY",
  "referrer-policy": "no-referrer",
  "cross-origin-opener-policy": "same-origin",
  "cross-origin-resource-policy": "same-origin",
  "cross-origin-embedder-policy": "require-corp",
};
const IDEA_TEXT =
  "Contrôle navigateur : regrouper les achats de papier des écoles d’une même commune " +
  "permettrait d’obtenir de meilleurs prix auprès des fournisseurs et de réduire les frais " +
  "de livraison, sans changer la qualité du papier utilisé par les élèves.";

const problems = [];
let checkedPages = 0;

function problem(browserName, where, message) {
  problems.push(`[${browserName}] ${where}: ${message}`);
}

// Collects violations reported inside the documents (workers report through the console).
const VIOLATION_LISTENER = () => {
  window.__cspViolations = [];
  document.addEventListener("securitypolicyviolation", (event) => {
    window.__cspViolations.push(
      `${event.effectiveDirective} blocked ${event.blockedURI || "inline"} ` +
        `(${event.sourceFile ?? ""}:${event.lineNumber ?? ""}) ${event.sample ?? ""}`,
    );
  });
};

function watch(page, browserName, where) {
  page.on("console", (message) => {
    const text = message.text();
    if (/content.security.policy|Content-Security-Policy/i.test(text)) {
      problem(browserName, where(), `console: ${text}`);
    } else if (message.type() === "error" && !/status of 4\d\d/.test(text)) {
      problem(browserName, where(), `console error: ${text}`);
    } else if (/permissions.policy/i.test(text)) {
      problem(browserName, where(), `console: ${text}`);
    }
  });
  page.on("pageerror", (error) => problem(browserName, where(), `page error: ${error.message}`));
  page.on("requestfailed", (request) => {
    const failure = request.failure()?.errorText ?? "";
    // Navigations aborted by the next goto are not failures of the page.
    if (!/ABORTED|NS_BINDING_ABORTED|cancelled/i.test(failure)) {
      problem(browserName, where(), `request failed: ${request.url()} ${failure}`);
    }
  });
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (!["https://localhost", "https://admin.localhost"].includes(url.origin) && url.protocol !== "data:") {
      problem(browserName, where(), `third-party request: ${request.url()}`);
    }
  });
}

async function settle(page) {
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(300);
}

async function checkDocument(page, browserName, response, where) {
  checkedPages += 1;
  if (!response) {
    problem(browserName, where, "no response");
    return;
  }
  const headers = response.headers();
  for (const [name, value] of Object.entries(EXPECTED_HEADERS)) {
    if (headers[name] !== value) problem(browserName, where, `${name}: ${headers[name] ?? "<absent>"}`);
  }
  const csp = headers["content-security-policy"] ?? "";
  if (!/^default-src 'none'; script-src 'self'; style-src 'self' 'nonce-[0-9a-f]{32}' /.test(csp)) {
    problem(browserName, where, `unexpected CSP: ${csp}`);
  }
  const state = await page.evaluate(() => ({
    isolated: window.crossOriginIsolated,
    violations: window.__cspViolations ?? [],
    // A <style> blocked by the CSP has no stylesheet.
    blockedStyles: [...document.querySelectorAll("style")]
      .filter((style) => style.textContent !== "" && style.sheet === null)
      .map((style) => style.textContent.slice(0, 80)),
  }));
  if (!state.isolated) problem(browserName, where, "not cross-origin isolated");
  for (const violation of state.violations) problem(browserName, where, `CSP violation: ${violation}`);
  for (const style of state.blockedStyles) problem(browserName, where, `blocked <style>: ${style}`);
}

async function visit(page, browserName, url, context) {
  context.where = url;
  const response = await page.goto(url);
  await settle(page);
  await checkDocument(page, browserName, response, url);
}

/** Opens and closes a Radix Select: its viewport style (hash) and the scroll lock (nonce). */
async function exerciseSelect(page, browserName, where) {
  const trigger = page.locator('[data-slot="select-trigger"]').first();
  if ((await trigger.count()) === 0) {
    problem(browserName, where, "no select to open");
    return;
  }
  await trigger.click();
  await page.locator('[data-slot="select-content"]').waitFor();
  await checkScrollLock(page, browserName, `${where} (select open)`);
  await page.keyboard.press("Escape");
}

/** react-remove-scroll locks the body with a nonce-carrying <style>: it must apply. */
async function checkScrollLock(page, browserName, where) {
  const lock = await page.evaluate(() => ({
    locked: document.body.hasAttribute("data-scroll-locked"),
    overflow: getComputedStyle(document.body).overflow,
    nonced: [...document.querySelectorAll("style")].some((style) => style.nonce),
  }));
  if (!lock.locked || lock.overflow !== "hidden" || !lock.nonced) {
    problem(browserName, where, `scroll lock style not applied: ${JSON.stringify(lock)}`);
  }
}

async function runBrowser(browserType) {
  const browserName = browserType.name();
  const browser = await browserType.launch();
  const context = await browser.newContext({ ignoreHTTPSErrors: true, locale: "fr-FR" });
  await context.addInitScript(VIOLATION_LISTENER);
  const page = await context.newPage();
  const state = { where: "", api: [] };
  watch(page, browserName, () => state.where);
  // The last API answers, reported when a step fails.
  page.on("response", (response) => {
    const url = new URL(response.url());
    if (url.pathname.startsWith("/api/")) {
      state.api.push(`${response.request().method()} ${url.pathname} ${response.status()}`);
    }
  });
  try {
    await browse(page, browserName, state);
  } catch (error) {
    const alerts = await page.getByRole("alert").allInnerTexts().catch(() => []);
    const text = `alerts: ${JSON.stringify(alerts)}; api: ${state.api.slice(-5).join(", ")}`;
    const message = String(error.message ?? error).split("\n")[0];
    problem(browserName, state.where, `${message}\n  page: ${text.replace(/\s+/g, " ").slice(-1200)}`);
  } finally {
    await browser.close();
  }
}

async function browse(page, browserName, state) {
  // --- Public routes ---------------------------------------------------------------------------
  for (const route of PUBLIC_ROUTES) await visit(page, browserName, PUBLIC_ORIGIN + route, state);

  const ideas = await (await page.request.get(`${PUBLIC_ORIGIN}/api/ideas`)).json();
  const slug = ideas.items?.[0]?.slug;
  if (slug) {
    await visit(page, browserName, `${PUBLIC_ORIGIN}/idees/${slug}`, state);
  } else {
    problem(browserName, "/api/ideas", "no published idea to open");
  }

  await visit(page, browserName, PUBLIC_ORIGIN + "/", state);
  await exerciseSelect(page, browserName, "/ (filters)");

  // --- Submission: ALTCHA proof of work in its worker, then the tracking page ------------------
  await visit(page, browserName, PUBLIC_ORIGIN + "/proposer", state);
  await exerciseSelect(page, browserName, "/proposer");
  await page.getByLabel("Votre idée").fill(`${IDEA_TEXT} (${browserName})`);
  await page.getByRole("checkbox", { name: /J’accepte la/ }).check();
  await page.getByRole("checkbox", { name: /Je comprends que mon texte/ }).check();
  await page.getByRole("button", { name: "Envoyer mon idée" }).click();
  const linkText = await page
    .getByText(/\/suivi#[A-Za-z0-9_-]{43}/)
    .first()
    .textContent({ timeout: 60_000 });
  await settle(page);
  await checkDocument(page, browserName, await page.request.get(page.url()), "/proposer (sent)");
  const trackingUrl = linkText.trim();

  await visit(page, browserName, trackingUrl, state);
  const deleteButton = page.getByRole("button", { name: "Supprimer ma contribution" });
  if ((await deleteButton.count()) > 0) {
    await deleteButton.click();
    await page.getByRole("dialog").waitFor();
    await checkScrollLock(page, browserName, "/suivi (dialog open)");
    await page.getByRole("button", { name: "Annuler" }).click();
  } else {
    problem(browserName, "/suivi", "no delete button on the tracking page");
  }
  await checkDocument(page, browserName, await page.request.get(trackingUrl), "/suivi (after dialog)");

  // --- Mobile menu --------------------------------------------------------------------------------
  await page.setViewportSize({ width: 390, height: 844 });
  await visit(page, browserName, PUBLIC_ORIGIN + "/", state);
  await page.getByRole("button", { name: "Ouvrir le menu" }).click();
  await settle(page);
  await checkDocument(page, browserName, await page.request.get(PUBLIC_ORIGIN + "/"), "/ (mobile menu)");
  await page.setViewportSize({ width: 1280, height: 900 });

  // --- Administration -----------------------------------------------------------------------------
  await visit(page, browserName, ADMIN_ORIGIN + "/admin", state);
  await page.getByLabel("Jeton d’administration").fill(ADMIN_TOKEN);
  await page.getByRole("button", { name: "Se connecter" }).click();
  await settle(page);
  const stored = await page.evaluate(() => ({
    local: window.localStorage.length,
    session: window.sessionStorage.length,
  }));
  if (stored.local !== 0) problem(browserName, "/admin", "the admin token reached localStorage");
  for (const route of ADMIN_ROUTES) await visit(page, browserName, ADMIN_ORIGIN + route, state);
  await visit(page, browserName, ADMIN_ORIGIN + "/admin/reglages", state);
  const adminSelect = page.locator('[data-slot="select-trigger"]');
  if ((await adminSelect.count()) > 0) await exerciseSelect(page, browserName, "/admin/reglages");

}

const types = { chromium, firefox };
for (const name of BROWSERS) await runBrowser(types[name]);

if (problems.length > 0) {
  console.error(problems.join("\n"));
  console.error(`FAIL  ${problems.length} browser problem(s)`);
  process.exit(1);
}
console.log(`ok    ${checkedPages} pages in ${BROWSERS.join(", ")}: no CSP violation, headers and isolation as expected`);
