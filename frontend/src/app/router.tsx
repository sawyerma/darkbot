import { Suspense } from "react";
import { createBrowserRouter, useLocation, type RouteObject } from "react-router-dom";
import { AppLayout } from "@/shared/layout/AppLayout";
import { FeatureBoundary } from "@/shared/ui/FeatureBoundary";
import { PROJECT_REGISTRY } from "./projectRegistry";
import RouteError from "./RouteError";
import { RouteTopics } from "./RouteTopics";

// Routen werden aus der Registry erzeugt. Jede Projektseite: eigener Lazy-Chunk,
// eigene Error Boundary, eigene Basis-Topics. URLs bleiben /, /project-0, /bsc, /solana.

function PageFallback() {
  return <div className="p-6 text-sm text-muted-foreground">Loading…</div>;
}

type RegistryEntry = (typeof PROJECT_REGISTRY)[number];

function RouteShell({ reg }: { reg: RegistryEntry }) {
  const { pathname } = useLocation();
  const Page = reg.page;
  return (
    <FeatureBoundary name={reg.label} resetKey={pathname}>
      <Suspense fallback={<PageFallback />}>
        <RouteTopics topics={reg.topics.base}>
          <Page />
        </RouteTopics>
      </Suspense>
    </FeatureBoundary>
  );
}

function toRoute(reg: RegistryEntry): RouteObject {
  const element = <RouteShell reg={reg} />;
  return reg.route === "/" ? { index: true, element } : { path: reg.route.replace(/^\//, ""), element };
}

export const router = createBrowserRouter([
  {
    path: "/",
    element: <AppLayout />,
    errorElement: <RouteError />,
    children: PROJECT_REGISTRY.map(toRoute),
  },
]);
