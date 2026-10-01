import { Outlet } from "react-router-dom";
import { Navigation } from "./Navigation";

export function AppLayout() {
  return (
    <div className="min-h-screen bg-background text-foreground">
      <Navigation />
      <main className="mx-auto max-w-7xl px-4 py-6"><Outlet /></main>
    </div>
  );
}
