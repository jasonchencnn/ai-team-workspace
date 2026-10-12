import { readFileSync, readdirSync } from "node:fs";
import { relative, resolve } from "node:path";

export function findGeneratedContractDrift(committed, generated) {
  const expected = generatedFiles(generated);
  const actual = generatedFiles(committed);
  const paths = [...new Set([...expected, ...actual])].sort();
  return paths.filter((path) => read(committed, path) !== read(generated, path));
}

function generatedFiles(directory) {
  return walk(directory)
    .map((path) => relative(directory, path))
    .filter((path) => path === "index.ts" || path.startsWith("schema/"));
}

function walk(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

function read(directory, path) {
  try {
    return readFileSync(resolve(directory, path), "utf8");
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "ENOENT")
      return null;
    throw error;
  }
}
