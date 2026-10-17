// Future ESLint rules only: no eslint.config.js currently exists in this repository.
// Enforced today by dependency-free frontend/scripts/check-boundaries.mjs.
// Mechanische Projektisolierung (Phasenplan §3). Nur ESLint-Core, keine Zusatzplugins:
// no-restricted-imports mit Dateimustern. Verstöße sind CI-Fehler.

const generatedPattern = {
  group: ["**/core/generated/**", "**/generated/darkbot/**"],
  message:
    "Generierte Protobuf-Typen nur in core/realtime/** und features/*/api/*Adapter.ts importieren. Alles andere über @/core/realtime.",
};

export default [
  // 1. Generierte Typen: überall verboten …
  {
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/core/realtime/**", "src/core/api/**", "src/features/*/api/*Adapter.ts", "src/core/generated/**"],
    rules: {
      "no-restricted-imports": ["error", { patterns: [generatedPattern] }],
    },
  },
  // 2. BSC darf nichts aus Project 0 importieren
  {
    files: ["src/features/bsc/**/*.{ts,tsx}"],
    ignores: ["src/features/bsc/api/*Adapter.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            generatedPattern,
            { group: ["**/features/project0/**", "**/project0/**"], message: "features/bsc darf nicht aus features/project0 importieren." },
            { group: ["**/features/solana/**"], message: "features/bsc darf nicht aus features/solana importieren." },
          ],
        },
      ],
    },
  },
  // 3. Project 0 darf nichts aus BSC importieren
  {
    files: ["src/features/project0/**/*.{ts,tsx}"],
    ignores: ["src/features/project0/api/*Adapter.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            generatedPattern,
            { group: ["**/features/bsc/**", "**/bsc/**"], message: "features/project0 darf nicht aus features/bsc importieren." },
            { group: ["**/features/solana/**"], message: "features/project0 darf nicht aus features/solana importieren." },
          ],
        },
      ],
    },
  },
  // 4. Overview kennt nur die gemeinsame Read-Summary, keine Projektinterna
  {
    files: ["src/features/overview/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            generatedPattern,
            { group: ["**/features/project0/**", "**/features/bsc/**", "**/features/solana/**"], message: "overview liest nur overview.aggregates und core/projects/readSummary." },
          ],
        },
      ],
    },
  },
  // 5. core/ kennt keine Features
  {
    files: ["src/core/**/*.{ts,tsx}"],
    ignores: ["src/core/generated/**"],
    rules: {
      "no-restricted-imports": [
        "error",
        { patterns: [{ group: ["**/features/**"], message: "core/ darf keine Features importieren." }] },
      ],
    },
  },
  // 6. Adapter sind die einzige Brücke: features/*/api/*Adapter.ts dürfen generated, aber keine anderen Features
  {
    files: ["src/features/*/api/*Adapter.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        { patterns: [{ group: ["**/features/**"], message: "Adapter importieren keine anderen Feature-Module." }] },
      ],
    },
  },
];
