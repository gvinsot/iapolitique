import { Navigate, useLocation } from "react-router";

/**
 * Client-side redirect that keeps the URL fragment (`#article` anchors, tracking tokens): a
 * loader redirect would lose it, since the fragment never reaches a request.
 */
export function RedirectWithHash({ to }: { to: string }) {
  const { hash } = useLocation();
  return <Navigate to={{ pathname: to, hash }} replace />;
}
