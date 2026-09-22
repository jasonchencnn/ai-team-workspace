import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    projects: [
      { test: { include: ["tests/unit/**/*.test.ts"], name: "unit" } },
      { test: { include: ["tests/integration/**/*.test.ts"], name: "integration" } },
      { test: { include: ["tests/contract/**/*.test.ts"], name: "contract" } },
      { test: { include: ["tests/e2e/**/*.test.ts"], name: "e2e" } },
    ],
  },
});
