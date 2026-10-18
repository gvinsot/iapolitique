import { vi } from "vitest";

/**
 * jsdom lacks a few browser APIs used by Radix primitives and the theme logic.
 * Installed once from setup.ts; tests can override matchMedia with `mockColorScheme`.
 */

type Listener = (event: MediaQueryListEvent) => void;

export interface ColorSchemeMock {
  /** Simulates the operating system switching its colour scheme. */
  setDark: (dark: boolean) => void;
}

export function mockColorScheme(initiallyDark = false): ColorSchemeMock {
  let dark = initiallyDark;
  const listeners = new Set<Listener>();

  const matchMedia = vi.fn((query: string): MediaQueryList => {
    const isDarkQuery = query.includes("prefers-color-scheme: dark");
    const list = {
      get matches() {
        return isDarkQuery ? dark : false;
      },
      media: query,
      onchange: null,
      addEventListener: (_type: string, listener: Listener) => {
        if (isDarkQuery) listeners.add(listener);
      },
      removeEventListener: (_type: string, listener: Listener) => {
        listeners.delete(listener);
      },
      addListener: (listener: Listener) => {
        if (isDarkQuery) listeners.add(listener);
      },
      removeListener: (listener: Listener) => {
        listeners.delete(listener);
      },
      dispatchEvent: () => true,
    };
    return list as unknown as MediaQueryList;
  });

  vi.stubGlobal("matchMedia", matchMedia);

  return {
    setDark(next: boolean) {
      dark = next;
      for (const listener of listeners) {
        listener({ matches: next, media: "(prefers-color-scheme: dark)" } as MediaQueryListEvent);
      }
    },
  };
}

/**
 * Replaces navigator.clipboard (undefined simulates an insecure context). Call it after
 * userEvent.setup(), which installs its own clipboard stub. Removed after each test (setup.ts).
 */
export function mockClipboard(clipboard: Pick<Clipboard, "writeText"> | undefined): void {
  Object.defineProperty(window.navigator, "clipboard", {
    value: clipboard,
    configurable: true,
    writable: true,
  });
}

class ResizeObserverStub implements ResizeObserver {
  observe(): void {
    // jsdom has no layout: nothing to observe.
  }
  unobserve(): void {
    // no-op
  }
  disconnect(): void {
    // no-op
  }
}

/** Default network: tests never reach a real server. Use stubFetch (fetch-mock.ts) to answer. */
function offlineFetch(input: RequestInfo | URL): Promise<Response> {
  const url = input instanceof Request ? input.url : String(input);
  return Promise.reject(new TypeError(`Network disabled in tests (${url})`));
}

/**
 * jsdom's crypto has no `subtle`: expose Node's Web Crypto implementation instead (used by the
 * ALTCHA widget, which hashes in the main thread when Web Workers are unavailable, as in jsdom).
 */
function installWebCrypto(): void {
  if ("subtle" in globalThis.crypto) return;
  const nodeProcess = (globalThis as { process?: { getBuiltinModule?: (id: string) => unknown } })
    .process;
  const nodeCrypto = nodeProcess?.getBuiltinModule?.("node:crypto") as
    { webcrypto?: Crypto } | undefined;
  if (nodeCrypto?.webcrypto) vi.stubGlobal("crypto", nodeCrypto.webcrypto);
}

export function installBrowserMocks(): void {
  installWebCrypto();
  mockColorScheme(false);
  vi.stubGlobal("fetch", vi.fn(offlineFetch));
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  window.scrollTo = vi.fn();
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => false);
  Element.prototype.releasePointerCapture = vi.fn();
}
