import { NavLink } from "react-router-dom";
import { NAV_ENTRIES } from "@/app/projectRegistry";

export function Navigation() {
  return (
    <header className="app-nav">
      <NavLink to="/" className="app-nav-brand">DarkBot</NavLink>
      <nav className="app-nav-links" aria-label="Projects">
        {NAV_ENTRIES.map(({ to, label, end }) => (
          <NavLink key={to} to={to} end={end} className={({ isActive }) => isActive ? "active" : undefined}>
            {label}
          </NavLink>
        ))}
      </nav>
    </header>
  );
}
