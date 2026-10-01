import { useTheme } from "./theme-provider";

const ThemeToggle = () => {
  const { theme, setTheme } = useTheme();

  const toggleTheme = () => {
    switch (theme) {
      case "light":
        setTheme("dark");
        break;
      case "dark":
        setTheme("system");
        break;
      case "system":
        setTheme("light");
        break;
    }
  };

  const getIcon = () => {
    switch (theme) {
      case "light":
        return (
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <circle
              cx="12"
              cy="12"
              r="5"
              stroke="currentColor"
              strokeWidth="2"
            />
            <path
              d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"
              stroke="currentColor"
              strokeWidth="2"
            />
          </svg>
        );
      case "dark":
        return (
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <path
              d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"
              stroke="currentColor"
              strokeWidth="2"
              fill="currentColor"
            />
          </svg>
        );
      case "system":
        return (
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <rect
              x="2"
              y="4"
              width="20"
              height="14"
              rx="2"
              stroke="currentColor"
              strokeWidth="2"
            />
            <path d="M8 20h8" stroke="currentColor" strokeWidth="2" />
            <path d="M12 16v4" stroke="currentColor" strokeWidth="2" />
          </svg>
        );
    }
  };

  const getTooltip = () => {
    switch (theme) {
      case "light":
        return "Hell-Modus";
      case "dark":
        return "Dunkel-Modus";
      case "system":
        return "System-Modus";
    }
  };

  return (
    <button
      onClick={toggleTheme}
      className={`px-3 py-1.5 rounded font-medium transition-colors ${"text-foreground hover:bg-muted"}`}
      title={getTooltip()}
      aria-label={`Aktueller Modus: ${getTooltip()}. Klicken zum Wechseln.`}
    >
      {getIcon()}
    </button>
  );
};

export default ThemeToggle;
