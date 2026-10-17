import js from "@eslint/js";
import globals from "globals";

// ESLint 9 flat config. TypeScript is checked by tsc and the dedicated
// project-boundary checker until a TypeScript-aware ESLint parser is pinned.
export default [
  {
    ignores: ["dist/**", "node_modules/**", "playwright-report/**", "test-results/**", "src/core/generated/**", "src/**/*.{ts,tsx}"],
  },
  js.configs.recommended,
  {
    files: ["**/*.{js,mjs,cjs}"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      globals: { ...globals.node, ...globals.browser },
    },
  },
];
