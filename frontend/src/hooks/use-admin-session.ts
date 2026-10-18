import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";

import { clearAdminToken, readAdminToken, storeAdminToken } from "@/api/admin-token";
import { $api } from "@/api/client";
import { ApiError } from "@/api/errors";

export type AdminSessionState =
  | { status: "signed-out"; rejected: boolean }
  | { status: "checking" }
  | { status: "error"; error: unknown; retry: () => void }
  | { status: "authenticated" };

const SESSION_QUERY_KEY = ["get", "/api/admin/session"] as const;

/**
 * Admin access: the token typed by the administrator is kept in sessionStorage and checked
 * against GET /api/admin/session, which answers 401 for a wrong token.
 */
export function useAdminSession() {
  const queryClient = useQueryClient();
  const [token, setToken] = useState<string | null>(readAdminToken);

  const session = $api.useQuery(
    "get",
    "/api/admin/session",
    {},
    { enabled: token !== null, retry: false, staleTime: 5 * 60_000 },
  );

  const signIn = useCallback(
    (value: string) => {
      storeAdminToken(value);
      setToken(value.trim());
      void queryClient.resetQueries({ queryKey: SESSION_QUERY_KEY });
    },
    [queryClient],
  );

  const signOut = useCallback(() => {
    clearAdminToken();
    setToken(null);
    // Drop every cached admin response along with the token.
    queryClient.clear();
  }, [queryClient]);

  let state: AdminSessionState;
  if (token === null) {
    state = { status: "signed-out", rejected: false };
  } else if (session.isPending) {
    state = { status: "checking" };
  } else if (session.isError) {
    state =
      session.error instanceof ApiError && session.error.status === 401
        ? { status: "signed-out", rejected: true }
        : {
            status: "error",
            error: session.error,
            retry: () => {
              void session.refetch();
            },
          };
  } else {
    state = session.data.authenticated
      ? { status: "authenticated" }
      : { status: "signed-out", rejected: true };
  }

  return { state, signIn, signOut, hasToken: token !== null };
}
