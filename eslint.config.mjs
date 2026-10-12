import eslint from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "node_modules/**",
      "target/**",
      "coverage/**",
      "**/*.d.mts",
      "apps/desktop/dist/**",
      "apps/desktop/src-tauri/gen/**",
      // Immutable audit reproduction; active scripts and adjacent evidence stay linted.
      "docs/reviews/evidence/deep-audit-2026-10-08/replay_env_manifest.mjs",
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
    },
  },
  {
    files: ["**/*.mjs"],
    ...tseslint.configs.disableTypeChecked,
    languageOptions: {
      globals: globals.node,
    },
  },
);
