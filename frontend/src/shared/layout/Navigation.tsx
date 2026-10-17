import { NavLink } from "react-router-dom";
import ThemeToggle from "@/shared/ui/theme-toggle";
import { NAV_ENTRIES } from "@/app/projectRegistry";

export function Navigation() {
  return (
    <header className="border-b border-border bg-background/95 backdrop-blur">
      <div className="mx-auto flex h-14 max-w-7xl items-center gap-6 px-4">
        <NavLink to="/" className="font-semibold tracking-tight">DarkBot</NavLink>
        <nav className="flex flex-1 gap-1" aria-label="Projects">
          {NAV_ENTRIES.map(({ to, label, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              className={({ isActive }) =>
                `rounded-md px-3 py-2 text-sm ${isActive ? "bg-secondary font-medium" : "text-muted-foreground hover:text-foreground"}`
              }
            >
              {label}
            </NavLink>
          ))}
        </nav>
        <ThemeToggle />
      </div>
    </header>
  );
}
