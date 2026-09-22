import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { color, radius, spacing, theme, typography } from "../index.js";

const css = readFileSync(new URL("../theme.css", import.meta.url), "utf8");
const cssToken = (name: string) => {
  const value = css.match(new RegExp(`--${name}:\\s*([^;]+);`))?.[1];
  if (!value) throw new Error(`Missing CSS token: --${name}`);
  return value.trim();
};

const hex = (value: string) =>
  (() => {
    const parts = value.slice(1).match(/.{2}/g);
    if (!parts || parts.length !== 3) throw new Error(`Invalid color: ${value}`);
    return parts.map((part) => parseInt(part, 16) / 255);
  })();
const linear = (channel: number) =>
  channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
const luminance = (value: string) => {
  const channels = hex(value).map(linear);
  const r = channels[0];
  const g = channels[1];
  const b = channels[2];
  if (r === undefined || g === undefined || b === undefined) {
    throw new Error(`Invalid color channels: ${value}`);
  }
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const contrast = (foreground: string, background: string) => {
  const light = Math.max(luminance(foreground), luminance(background));
  const dark = Math.min(luminance(foreground), luminance(background));
  return (light + 0.05) / (dark + 0.05);
};

describe("design token contract", () => {
  it("exports the frozen Figma primitives consistently", () => {
    expect(theme.color).toBe(color);
    expect(spacing[6]).toBe("24px");
    expect(radius.control).toBe("8px");
    expect(typography.body.lineHeight).toBe("21px");
  });

  it("keeps CSS theme values synchronized with TypeScript tokens", () => {
    expect(cssToken("color-canvas")).toBe(color.canvas.toLowerCase());
    expect(cssToken("color-surface")).toBe(color.surface.toLowerCase());
    expect(cssToken("color-ink")).toBe(color.ink.toLowerCase());
    expect(cssToken("color-muted")).toBe(color.muted.toLowerCase());
    expect(cssToken("color-line")).toBe(color.line.toLowerCase());
    expect(cssToken("color-brand")).toBe(color.brand.toLowerCase());
    expect(cssToken("font-family-sans")).toBe(typography.fontFamily);
    expect(cssToken("radius-control")).toBe(radius.control);
    expect(cssToken("radius-panel")).toBe(radius.panel);
    expect(cssToken("radius-pill")).toBe(radius.pill);
    expect(cssToken("space-section")).toBe(spacing[6]);
    expect(cssToken("space-content")).toBe(spacing[8]);
    expect(cssToken("font-size-body")).toBe(typography.body.fontSize);
    expect(cssToken("line-height-body")).toBe(typography.body.lineHeight);
    expect(css).toContain("@media (min-width: 768px)");
    expect(css).toContain("@media (min-width: 1280px)");
  });

  it("keeps normal text at WCAG AA contrast", () => {
    expect(contrast(color.ink, color.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.muted, color.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.status.info, color.status.infoSurface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.status.warning, color.status.warningSurface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.status.success, color.status.successSurface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color.status.danger, color.status.dangerSurface)).toBeGreaterThanOrEqual(4.5);
  });

  it("uses the required CJK font fallback chain", () => {
    expect(typography.fontFamily).toContain("Noto Sans SC");
    expect(typography.fontFamily).toContain("system-ui");
    expect(typography.body.lineHeight).toBe("21px");
  });
});
