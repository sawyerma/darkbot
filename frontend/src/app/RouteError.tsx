import { isRouteErrorResponse, Link, useRouteError } from "react-router-dom";

// Router-Ebene (404, Loader-Fehler). Projektfehler fängt FeatureBoundary vorher.
export default function RouteError() {
  const err = useRouteError();
  const title = isRouteErrorResponse(err) ? `${err.status} ${err.statusText}` : "Navigation error";
  const detail = isRouteErrorResponse(err) ? err.data : err instanceof Error ? err.message : String(err);
  return (
    <main className="mx-auto mt-16 max-w-xl p-6 text-sm">
      <h1 className="text-base font-semibold">{title}</h1>
      <p className="mt-2 text-muted-foreground">{typeof detail === "string" ? detail : "Please return to the overview."}</p>
      <Link to="/" className="mt-4 inline-block underline">Back to Overview</Link>
    </main>
  );
}
