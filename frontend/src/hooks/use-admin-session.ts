import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";

import {
  clearAdminToken,
  onAdminTokenRejected,
  readAdminToken,
  storeAdminToken,
} from "@/api/admin-token";
import { $api, createApiClient } from "@/api/client";
import { ApiError } from "@/api/errors";

export type AdminSessionState =
  | { status: "signed-out"; rejected: boolean }
  | { status: "checking" }
  | { status: "wrong-host" }
  | { status: "error"; error: unknown; retry: () => void }
  /** Too many wrong tokens: the admin API refuses every request for a few minutes (429). */
  | { status: "locked"; message: string; retryAfterSeconds: number | null }
  | { status: "authenticated" };

const SESSION_QUERY_KEY = ["get", "/api/admin/session"] as const;
const PROBE_QUERY_KEY = ["admin-api-probe"] as const;

/** Never attaches the admin token, whatever sessionStorage holds. */
const probeClient = createApiClient({ getAdminToken: () => null });

type ProbeResult = "reachable" | "wrong-host";

/**
 * Token-less GET /api/admin/session (findings ADM-1, FE-5). The admin port answers 401 to a
 * request without `Authorization` (AGENTS.md §14.4), or 429 while locked; the public port, which
 * has no admin route, answers 404. Only a reachable admin API ever receives the token: on the
 * public host (`TRAEFIK_HOST/admin`), the form is not even shown.
 */
async function probeAdminApi(signal: AbortSignal): Promise<ProbeResult> {
  try {
    await probeClient.GET("/api/admin/session", { signal });
    return "reachable";
  } catch (error) {
    if (error instanceof ApiError) {
      if (error.status === 401 || error.status === 429) return "reachable";
      if (error.status === 404) return "wrong-host";
    }
    throw error;
  }
}

/**
 * Admin access: first a token-less probe proves that this origin serves the admin API, then the
 * token typed by the administrator is kept in sessionStorage and checked against
 * GET /api/admin/session, which answers 401 for a wrong token. On the public host the probe (or
 * a stored token) gets 404: the token is dropped and the page says where the admin lives.
 */
export function useAdminSession() {
  const queryClient = useQueryClient();
  const [token, setToken] = useState<string | null>(readAdminToken);
  /** The last token was refused by the admin API (401): it is already forgotten. */
  const [rejected, setRejected] = useState(false);

  const probe = useQuery({
    queryKey: PROBE_QUERY_KEY,
    queryFn: ({ signal }) => probeAdminApi(signal),
    retry: false,
    staleTime: Infinity,
  });
  const reachable = probe.data === "reachable";

  const session = $api.useQuery(
    "get",
    "/api/admin/session",
    {},
    { enabled: token !== null && reachable, retry: false, staleTime: 5 * 60_000 },
  );

  const wrongHost =
    probe.data === "wrong-host" ||
    (session.error instanceof ApiError && session.error.status === 404);

  // The token never stays in the public origin's storage (the state below ignores it already).
  useEffect(() => {
    if (wrongHost) clearAdminToken();
  }, [wrongHost]);

  // Finding AUTH-3: any admin call answered 401 drops the stored token (client middleware); the
  // page then asks for a new one and forgets every cached admin response.
  useEffect(
    () =>
      onAdminTokenRejected(() => {
        setToken(null);
        setRejected(true);
        queryClient.removeQueries({
          predicate: (query) => query.queryKey[0] !== PROBE_QUERY_KEY[0],
        });
      }),
    [queryClient],
  );

  const signIn = useCallback(
    (value: string) => {
      storeAdminToken(value);
      setRejected(false);
      setToken(value.trim());
      void queryClient.resetQueries({ queryKey: SESSION_QUERY_KEY });
    },
    [queryClient],
  );

  const signOut = useCallback(() => {
    clearAdminToken();
    setRejected(false);
    setToken(null);
    // Drop every cached admin response along with the token; keep the (token-less) probe.
    queryClient.removeQueries({
      predicate: (query) => query.queryKey[0] !== PROBE_QUERY_KEY[0],
    });
  }, [queryClient]);

  let state: AdminSessionState;
  if (wrongHost) {
    state = { status: "wrong-host" };
  } else if (probe.isPending) {
    state = { status: "checking" };
  } else if (probe.isError) {
    state = {
      status: "error",
      error: probe.error,
      retry: () => {
        void probe.refetch();
      },
    };
  } else if (token === null) {
    state = { status: "signed-out", rejected };
  } else if (session.isPending) {
    state = { status: "checking" };
  } else if (session.isError) {
    const error = session.error;
    if (error instanceof ApiError && error.status === 401) {
      state = { status: "signed-out", rejected: true };
    } else if (error instanceof ApiError && error.status === 429) {
      state = {
        status: "locked",
        message: error.message,
        retryAfterSeconds: error.retryAfterSeconds,
      };
    } else {
      state = {
        status: "error",
        error,
        retry: () => {
          void session.refetch();
        },
      };
    }
  } else {
    state = session.data.authenticated
      ? { status: "authenticated" }
      : { status: "signed-out", rejected: true };
  }

  return { state, signIn, signOut, hasToken: token !== null && !wrongHost };
}
