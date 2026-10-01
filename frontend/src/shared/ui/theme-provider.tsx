import { ReactNode, createContext, useContext, useEffect, useState } from "react";

type Theme = "light" | "dark" | "system";

type ThemeProviderContextType = {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  actualTheme: "light" | "dark";
};

const ThemeProviderContext = createContext<ThemeProviderContextType | undefined>(undefined);

export const useTheme = () => {
  const context = useContext(ThemeProviderContext);
  if (context === undefined) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
};

const getSystemTheme = (): "light" | "dark" => {
  if (typeof window !== "undefined") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return "light";
};

interface ThemeProviderProps {
  children: ReactNode;
}

const ThemeProvider = ({ children }: ThemeProviderProps) => {
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window !== "undefined") {
      const savedTheme = localStorage.getItem("theme") as Theme;
      return savedTheme || "system";
    }
    return "system";
  });

  const [systemTheme, setSystemTheme] = useState<"light" | "dark">(getSystemTheme);
  const actualTheme = theme === "system" ? systemTheme : theme;

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => setSystemTheme(getSystemTheme());
    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    localStorage.setItem("theme", theme);
    const root = window.document.documentElement;
    root.classList.remove("light", "dark");
    root.classList.add(actualTheme);
  }, [theme, actualTheme]);

  return (
    <ThemeProviderContext.Provider value={{ theme, setTheme, actualTheme }}>
      {children}
    </ThemeProviderContext.Provider>
  );
};

export default ThemeProvider;
