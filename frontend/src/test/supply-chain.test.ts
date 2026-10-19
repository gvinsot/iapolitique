/// <reference types="node" />
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

// Vite refuses to serve dotfiles: read them from disk (the test runs in Node).
const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const npmrc = read("../../.npmrc");
const lockText = read("../../package-lock.json");

/**
 * Supply-chain guards of AGENTS.md §14.7 (finding SUP-3): `npm ci` never runs the lifecycle
 * scripts of dependencies (preinstall, install, postinstall), which would execute as root in the
 * build container and could tamper with dist/. A dependency that really needs its install script
 * gets an explicit `npm rebuild <package>` in the Dockerfiles and an entry below, with its reason.
 */
const ALLOWED_INSTALL_SCRIPTS = new Map([
  ["node_modules/fsevents", "macOS file watcher, optional: never installed on Linux builders"],
]);

interface LockedPackage {
  hasInstallScript?: boolean;
  optional?: boolean;
  dev?: boolean;
}

const lock = JSON.parse(lockText) as { packages: Record<string, LockedPackage> };

describe("npm supply chain", () => {
  it("never runs dependency lifecycle scripts", () => {
    const settings = npmrc
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line !== "" && !line.startsWith("#") && !line.startsWith(";"));
    expect(settings).toContain("ignore-scripts=true");
  });

  it("locks no unexpected package with an install script", () => {
    const withScripts = Object.entries(lock.packages)
      .filter(([, pkg]) => pkg.hasInstallScript === true)
      .map(([path]) => path);
    expect(withScripts.filter((path) => !ALLOWED_INSTALL_SCRIPTS.has(path))).toEqual([]);
    for (const path of withScripts) {
      expect(lock.packages[path]?.optional, `${path} must stay optional`).toBe(true);
    }
  });
});
