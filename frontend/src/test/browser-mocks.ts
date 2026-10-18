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

export function installBrowserMocks(): void {
  mockColorScheme(false);
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  window.scrollTo = vi.fn();
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => false);
  Element.prototype.releasePointerCapture = vi.fn();
}
