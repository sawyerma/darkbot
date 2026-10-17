import { Outlet } from "react-router-dom";
import { Navigation } from "./Navigation";
import "./layout.css";

export function AppLayout() {
  return (
    <div className="app-shell">
      <Navigation />
      <main className="app-main"><Outlet /></main>
    </div>
  );
}
