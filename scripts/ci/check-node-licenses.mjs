import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const allowedLicenseIds = new Set([
  "0BSD",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "BlueOak-1.0.0",
  "CC-BY-4.0",
  "CC0-1.0",
  "ISC",
  "LLVM-exception",
  "MIT",
  "MIT-0",
  "MPL-2.0",
  "Python-2.0",
  "Unicode-3.0",
  "Unlicense",
  "Zlib",
]);

const listing = spawnSync("pnpm", ["list", "--recursive", "--depth", "Infinity", "--parseable"], {
  cwd: root,
  encoding: "utf8",
});

if (listing.status !== 0) {
  process.stderr.write(listing.stderr || "pnpm dependency listing failed.\n");
  process.exit(listing.status ?? 1);
}

const packagePaths = new Set(listing.stdout.split(/\r?\n/u).filter(Boolean));
const checked = new Map();
const violations = [];

for (const packagePath of packagePaths) {
  const workspaceRelativePath = relative(root, packagePath);
  if (!workspaceRelativePath.split(sep).includes("node_modules")) continue;

  const manifest = JSON.parse(readFileSync(resolve(packagePath, "package.json"), "utf8"));
  const identity = `${manifest.name ?? "<unnamed>"}@${manifest.version ?? "<unknown>"}`;
  const license = typeof manifest.license === "string" ? manifest.license.trim() : "";

  if (!license) {
    violations.push(`${identity}: missing string license field`);
    continue;
  }

  const identifiers = license.match(/[A-Za-z0-9.-]+/gu) ?? [];
  const licenseIds = identifiers.filter(
    (identifier) => !["AND", "OR", "WITH"].includes(identifier),
  );
  const denied = licenseIds.filter((identifier) => !allowedLicenseIds.has(identifier));

  if (licenseIds.length === 0 || denied.length > 0) {
    violations.push(`${identity}: ${license}`);
    continue;
  }

  checked.set(identity, license);
}

if (checked.size === 0) violations.push("no installed external Node dependencies were inspected");

if (violations.length > 0) {
  process.stderr.write(
    `Node license policy violations:\n${violations.map((item) => `- ${item}`).join("\n")}\n`,
  );
  process.exit(1);
}

const licenseCounts = new Map();
for (const license of checked.values()) {
  licenseCounts.set(license, (licenseCounts.get(license) ?? 0) + 1);
}

const summary = [...licenseCounts]
  .sort(([left], [right]) => left.localeCompare(right))
  .map(([license, count]) => `${license}=${count}`)
  .join(", ");
process.stdout.write(`Node license policy passed for ${checked.size} packages (${summary}).\n`);
