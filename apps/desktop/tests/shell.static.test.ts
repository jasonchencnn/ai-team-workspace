import { access, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const capabilityPath = fileURLToPath(
  new URL("../src-tauri/capabilities/main-window.json", import.meta.url),
);
const manifestPath = fileURLToPath(new URL("../src-tauri/Cargo.toml", import.meta.url));
const tauriConfigPath = fileURLToPath(new URL("../src-tauri/tauri.conf.json", import.meta.url));
const iconPath = fileURLToPath(new URL("../src-tauri/icons/icon.png", import.meta.url));

describe("desktop shell security boundary", () => {
  it("does not authorize frontend native APIs", async () => {
    const capability = JSON.parse(await readFile(capabilityPath, "utf8")) as {
      permissions: unknown[];
      windows: string[];
    };

    expect(capability.windows).toEqual(["main"]);
    expect(capability.permissions).toEqual([]);
  });

  it("does not include shell, filesystem, or external-navigation plugins", async () => {
    const manifest = await readFile(manifestPath, "utf8");

    expect(manifest).not.toContain("tauri-plugin-shell");
    expect(manifest).not.toContain("tauri-plugin-fs");
    expect(manifest).not.toContain("tauri-plugin-opener");
  });

  it("limits the configured development origin to the loopback interface", async () => {
    const config = JSON.parse(await readFile(tauriConfigPath, "utf8")) as {
      app: { security: { capabilities: string[] } };
      build: { devUrl: string };
    };

    expect(config.app.security.capabilities).toEqual(["main-window"]);
    expect(config.build.devUrl).toBe("http://127.0.0.1:1420");
  });

  it("includes the required temporary local build icon", async () => {
    await expect(access(iconPath)).resolves.toBeUndefined();
  });
});
