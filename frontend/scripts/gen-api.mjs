#!/usr/bin/env node
// Generates src/api/schema.d.ts from the backend OpenAPI spec (AGENTS.md §13.3).
//
//   npm run gen:api            regenerate the types
//   npm run gen:api -- --check fail if the committed types are stale (run by `npm run lint`)
//
// src/api/openapi.json is written by the backend (`iapolitique openapi`). While it does not
// exist, an empty placeholder schema is written so the typed client still compiles.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import openapiTS, { astToString, COMMENT_HEADER } from "openapi-typescript";

const specPath = fileURLToPath(new URL("../src/api/openapi.json", import.meta.url));
const schemaPath = fileURLToPath(new URL("../src/api/schema.d.ts", import.meta.url));
const check = process.argv.includes("--check");

const PLACEHOLDER = `${COMMENT_HEADER}// Placeholder: src/api/openapi.json does not exist yet. Run \`npm run gen:api\` once the
// backend has committed its OpenAPI spec.

export type paths = Record<string, never>;
export type webhooks = Record<string, never>;
export type components = Record<string, never>;
export type $defs = Record<string, never>;
export type operations = Record<string, never>;
`;

async function generate() {
  if (!existsSync(specPath)) return PLACEHOLDER;
  const spec = JSON.parse(readFileSync(specPath, "utf8"));
  const ast = await openapiTS(spec, { rootTypes: true, silent: true });
  return `${COMMENT_HEADER}${astToString(ast)}`;
}

const expected = await generate();

if (check) {
  const current = existsSync(schemaPath) ? readFileSync(schemaPath, "utf8") : "";
  if (current !== expected) {
    console.error(
      "src/api/schema.d.ts is out of date with src/api/openapi.json: run `npm run gen:api` and commit the result.",
    );
    process.exit(1);
  }
} else {
  writeFileSync(schemaPath, expected);
  console.warn(
    existsSync(specPath)
      ? "src/api/schema.d.ts generated from src/api/openapi.json"
      : "src/api/openapi.json not found: wrote a placeholder src/api/schema.d.ts",
  );
}
